//! Work Graph executor with crash-resume, policy, budgets, and bounded commands.

mod campaign;
mod command_envelope;

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
#[cfg(target_os = "linux")]
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
#[cfg(windows)]
use std::os::windows::process::CommandExt;

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use optimus_graph::{
    create_job, create_job_with_id as graph_create_job_with_id, interrupt_running_nodes_for_job,
    mark_job_running, mark_job_terminal, mark_node_awaiting_approval, mark_node_failed,
    mark_node_running, mark_node_succeeded, next_runnable_node, recompute_job_status, GraphError,
    NewActionApproval, PreparedAttemptDisposition, Store, StoreError,
};
use optimus_skills::{Permission, SkillError, SkillRegistry};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

pub use campaign::{
    Campaign, CampaignDiagnostic, CampaignError, CampaignHealthReport, CampaignRepairReport,
    CampaignStatus, CampaignStep, CampaignStepSpec, CampaignStore, CampaignView, StepKind,
    StepStatus, CAMPAIGN_SCHEMA_VERSION,
};
pub use command_envelope::{command_envelope_supported, linux_bwrap_args, parent_dirs_for_bind};
pub use optimus_graph::{
    CommandFsEnvelope, Effect, JobBudget, JobId, JobSpec, JobStatus, NodeSpec, NodeStatus,
    PolicyMode, RuntimeConfig,
};

/// Construct a [`JobId`] from a raw UUID (stable API for CLI/desktop).
pub fn job_id(id: Uuid) -> JobId {
    JobId(id)
}

/// Cooperative cancellation token shared by kernel turns, agent invocations,
/// and specialist verticals.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, AtomicOrdering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(AtomicOrdering::SeqCst)
    }
}

/// Soft cap per stream so tool results stay context-safe.
pub const MAX_CAPTURE_BYTES: usize = 32 * 1024;

/// True for secret or credential basenames denied by filesystem tools.
///
/// This is the single policy authority shared by runtime effects and kernel
/// filesystem-root callers.
pub fn is_secret_basename(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(lower.as_str(), ".env" | "auth.json" | "id_rsa" | ".netrc") || lower.ends_with(".pem")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CommandCapture {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub truncated_stdout: bool,
    pub truncated_stderr: bool,
    pub timed_out: bool,
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Graph(#[from] GraphError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("effector: {0}")]
    Effector(String),
    #[error("path escape denied: {0}")]
    PathEscape(String),
    #[error("job not runnable: {0}")]
    NotRunnable(String),
    #[error("needs approval for job {job_id} node {node_index}")]
    NeedsApproval { job_id: JobId, node_index: u32 },
    #[error("budget exceeded for job {job_id}: {reason}")]
    BudgetExceeded { job_id: JobId, reason: String },
    #[error("job {job_id} cancelled")]
    Cancelled { job_id: JobId },
    #[error("skill bridge: {0}")]
    Skill(#[from] SkillError),
    /// Command finished with non-zero exit or timeout; capture is still available.
    #[error("command failed exit={exit_code:?} timed_out={}", .capture.timed_out)]
    CommandFailed {
        exit_code: Option<i32>,
        capture: CommandCapture,
    },
}

pub type Result<T> = std::result::Result<T, RuntimeError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepOutcome {
    pub job_id: JobId,
    pub node_id: Uuid,
    pub node_index: u32,
    pub node_status: NodeStatus,
    pub capture: Option<CommandCapture>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectOutcome {
    pub attempt_id: Uuid,
    pub job_id: JobId,
    pub node_id: Uuid,
    pub effect_hash: String,
    pub status: String,
    pub receipt_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ApprovalGrant {
    pub job_id: JobId,
    actor: String,
    ttl_seconds: u64,
}

impl ApprovalGrant {
    pub fn for_job(job_id: JobId) -> Self {
        Self::for_job_by(job_id, "local_operator", 300)
    }

    pub fn for_job_by(job_id: JobId, actor: impl Into<String>, ttl_seconds: u64) -> Self {
        Self {
            job_id,
            actor: actor.into(),
            ttl_seconds,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub job_id: JobId,
    pub job_label: String,
    pub job_status: JobStatus,
    pub node_id: Option<Uuid>,
    pub node_index: Option<u32>,
    pub node_label: Option<String>,
    pub effect_json: String,
    pub has_grant: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSummary {
    pub job_id: JobId,
    pub label: String,
    pub status: JobStatus,
    pub steps_executed: u32,
    pub max_steps: u32,
}

struct KillOnDrop {
    child: Option<Child>,
    #[cfg(target_os = "linux")]
    linux_unit: Option<String>,
    #[cfg(windows)]
    job: Option<OwnedHandle>,
}

const CHILD_SETTLE_TIMEOUT: Duration = Duration::from_secs(2);

fn ensure_owned_command_containment(os: &str) -> std::io::Result<()> {
    if matches!(os, "linux" | "windows") {
        return Ok(());
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!("run_command is disabled on {os}: durable process-tree ownership is unavailable"),
    ))
}

fn wait_child_bounded(child: &mut Child) -> std::io::Result<()> {
    let deadline = Instant::now() + CHILD_SETTLE_TIMEOUT;
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::other(format!(
                "child process {} did not exit within {:?}",
                child.id(),
                CHILD_SETTLE_TIMEOUT
            )));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn kill_child_and_confirm_exit(child: &mut Child) -> std::io::Result<()> {
    if child.try_wait()?.is_none() {
        if let Err(kill_error) = child.kill() {
            if child.try_wait()?.is_none() {
                return Err(kill_error);
            }
        }
    }
    wait_child_bounded(child)
}

fn settle_child_after_signal(
    child: &mut Child,
    signal_result: std::io::Result<()>,
) -> std::io::Result<()> {
    match signal_result {
        Ok(()) => match wait_child_bounded(child) {
            Ok(()) => Ok(()),
            Err(settle_error) => {
                if let Err(kill_error) = kill_child_and_confirm_exit(child) {
                    return Err(std::io::Error::other(format!(
                        "{settle_error}; direct child cleanup also failed: {kill_error}"
                    )));
                }
                Err(settle_error)
            }
        },
        Err(signal_error) => match kill_child_and_confirm_exit(child) {
            Ok(()) => Err(signal_error),
            Err(kill_error) => Err(std::io::Error::other(format!(
                "{signal_error}; direct child cleanup also failed: {kill_error}"
            ))),
        },
    }
}

impl KillOnDrop {
    #[cfg(target_os = "linux")]
    fn spawn(command: &mut Command, linux_unit: String) -> std::io::Result<Self> {
        let mut child = command.spawn()?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match Self::linux_unit_is_active(&linux_unit) {
                Ok(true) => {
                    return Ok(Self {
                        child: Some(child),
                        linux_unit: Some(linux_unit),
                    });
                }
                Ok(false) => {
                    if child.try_wait()?.is_some() {
                        // Short-lived commands can finish and have their unit
                        // collected before the first status probe.
                        return Ok(Self {
                            child: Some(child),
                            linux_unit: Some(linux_unit),
                        });
                    }
                }
                Err(error) => {
                    let _ = kill_child_and_confirm_exit(&mut child);
                    return Err(error);
                }
            }
            if Instant::now() >= deadline {
                let _ = kill_child_and_confirm_exit(&mut child);
                return Err(std::io::Error::other(format!(
                    "systemd user service {linux_unit} did not become active"
                )));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    fn spawn(command: &mut Command, _fail_before_assignment: bool) -> std::io::Result<Self> {
        let _ = command;
        ensure_owned_command_containment(std::env::consts::OS)?;
        unreachable!("non-Linux Unix must fail the containment gate")
    }

    #[cfg(not(any(windows, unix)))]
    fn spawn(command: &mut Command, _fail_before_assignment: bool) -> std::io::Result<Self> {
        let _ = command;
        ensure_owned_command_containment(std::env::consts::OS)?;
        unreachable!("platforms without durable process-tree ownership must fail closed")
    }

    #[cfg(windows)]
    fn spawn(command: &mut Command, fail_before_assignment: bool) -> std::io::Result<Self> {
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;

        #[link(name = "ntdll")]
        extern "system" {
            fn NtResumeProcess(process_handle: windows_sys::Win32::Foundation::HANDLE) -> i32;
        }

        command.creation_flags(CREATE_SUSPENDED);
        let mut child = command.spawn()?;
        if fail_before_assignment {
            let _ = kill_child_and_confirm_exit(&mut child);
            return Err(std::io::Error::other(
                "injected failure before Job Object assignment",
            ));
        }

        let raw_job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw_job.is_null() {
            let error = std::io::Error::last_os_error();
            let _ = kill_child_and_confirm_exit(&mut child);
            return Err(error);
        }
        let job = unsafe { OwnedHandle::from_raw_handle(raw_job) };
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
        if configured == 0 {
            let error = std::io::Error::last_os_error();
            let _ = kill_child_and_confirm_exit(&mut child);
            return Err(error);
        }
        let assigned =
            unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) };
        if assigned == 0 {
            let error = std::io::Error::last_os_error();
            let _ = kill_child_and_confirm_exit(&mut child);
            return Err(error);
        }
        let resume_status = unsafe { NtResumeProcess(child.as_raw_handle()) };
        if resume_status < 0 {
            let terminated = if unsafe { TerminateJobObject(job.as_raw_handle(), 1) } == 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            };
            let _ = settle_child_after_signal(&mut child, terminated);
            return Err(std::io::Error::other(format!(
                "NtResumeProcess failed with NTSTATUS 0x{:08x}",
                resume_status as u32
            )));
        }
        Ok(Self {
            child: Some(child),
            job: Some(job),
        })
    }

    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        if let Some(child) = self.child.as_mut() {
            child.try_wait()
        } else {
            Ok(None)
        }
    }

    #[cfg(windows)]
    fn terminate_job_and_confirm_empty(&mut self) -> std::io::Result<()> {
        use windows_sys::Win32::System::JobObjects::{
            JobObjectBasicAccountingInformation, QueryInformationJobObject, TerminateJobObject,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        };

        let Some(job) = self.job.as_ref() else {
            return Err(std::io::Error::other("missing process Job Object"));
        };
        if unsafe { TerminateJobObject(job.as_raw_handle(), 1) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION =
                unsafe { std::mem::zeroed() };
            let queried = unsafe {
                QueryInformationJobObject(
                    job.as_raw_handle(),
                    JobObjectBasicAccountingInformation,
                    std::ptr::addr_of_mut!(accounting).cast(),
                    std::mem::size_of_val(&accounting) as u32,
                    std::ptr::null_mut(),
                )
            };
            if queried == 0 {
                return Err(std::io::Error::last_os_error());
            }
            if accounting.ActiveProcesses == 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::other(format!(
                    "Job Object still owns {} active process(es)",
                    accounting.ActiveProcesses
                )));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_unit_is_active(linux_unit: &str) -> std::io::Result<bool> {
        let output = Command::new("/usr/bin/systemctl")
            .args([
                "--user",
                "show",
                linux_unit,
                "--property=LoadState",
                "--property=ActiveState",
            ])
            .output()?;
        if !output.status.success() {
            return Err(std::io::Error::other(format!(
                "systemctl show {linux_unit}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let state = String::from_utf8_lossy(&output.stdout);
        let mut load_state = None;
        let mut active_state = None;
        for line in state.lines() {
            if let Some(value) = line.strip_prefix("LoadState=") {
                load_state = Some(value);
            }
            if let Some(value) = line.strip_prefix("ActiveState=") {
                active_state = Some(value);
            }
        }
        if load_state == Some("not-found") || matches!(active_state, Some("inactive" | "failed")) {
            return Ok(false);
        }
        if matches!(
            active_state,
            Some("active" | "activating" | "deactivating" | "reloading")
        ) {
            return Ok(true);
        }
        Err(std::io::Error::other(format!(
            "unexpected state for systemd user service {linux_unit}: {}",
            state.trim()
        )))
    }

    #[cfg(target_os = "linux")]
    fn signal_linux_unit(&mut self) -> std::io::Result<()> {
        let Some(linux_unit) = self.linux_unit.as_deref() else {
            return Err(std::io::Error::other("missing systemd user service"));
        };
        if !Self::linux_unit_is_active(linux_unit)? {
            self.linux_unit = None;
            return Ok(());
        }
        let output = Command::new("/usr/bin/systemctl")
            .args([
                "--user",
                "kill",
                "--signal=SIGKILL",
                "--kill-whom=all",
                linux_unit,
            ])
            .output()?;
        if output.status.success() {
            return Ok(());
        }
        if !Self::linux_unit_is_active(linux_unit)? {
            self.linux_unit = None;
            return Ok(());
        }
        Err(std::io::Error::other(format!(
            "systemctl kill {linux_unit}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }

    #[cfg(target_os = "linux")]
    fn confirm_linux_unit_empty(&mut self) -> std::io::Result<()> {
        let Some(linux_unit) = self.linux_unit.as_deref() else {
            return Ok(());
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if !Self::linux_unit_is_active(linux_unit)? {
                self.linux_unit = None;
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::other(format!(
                    "systemd user service {linux_unit} is still active"
                )));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(target_os = "linux")]
    fn terminate_linux_unit_and_confirm_empty(&mut self) -> std::io::Result<()> {
        self.signal_linux_unit()?;
        self.confirm_linux_unit_empty()
    }

    fn kill_and_reap(&mut self) -> std::io::Result<()> {
        #[cfg(windows)]
        {
            if let Some(mut child) = self.child.take() {
                let tree_result = self.terminate_job_and_confirm_empty();
                let child_result = settle_child_after_signal(&mut child, tree_result);
                self.job.take();
                child_result?;
            }
        }

        #[cfg(target_os = "linux")]
        {
            let mut wait_error = None;
            if let Some(mut child) = self.child.take() {
                let signalled = self.signal_linux_unit();
                if let Err(error) = settle_child_after_signal(&mut child, signalled) {
                    wait_error = Some(error);
                }
            }

            // A normal root exit may already have reaped `child` while owned
            // descendants remain in the cgroup. Retry unit settlement
            // independently so Drop never loses the containment handle.
            if self.linux_unit.is_some() {
                self.terminate_linux_unit_and_confirm_empty()?;
            }
            if let Some(error) = wait_error {
                return Err(error);
            }
        }

        #[cfg(all(unix, not(target_os = "linux")))]
        if let Some(mut child) = self.child.take() {
            // This is unreachable through `spawn`, which fails before creating
            // a child on platforms without durable process-tree ownership.
            kill_child_and_confirm_exit(&mut child)?;
        }

        #[cfg(not(any(windows, unix)))]
        if let Some(mut child) = self.child.take() {
            kill_child_and_confirm_exit(&mut child)?;
        }

        Ok(())
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.kill_and_reap();
    }
}

#[cfg(target_os = "linux")]
static NEXT_LINUX_UNIT_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(target_os = "linux")]
fn linux_contained_command(
    program: &str,
    args: &[String],
    workspace: &Path,
    envelope: CommandFsEnvelope,
) -> (Command, String) {
    let sequence = NEXT_LINUX_UNIT_ID.fetch_add(1, AtomicOrdering::Relaxed);
    let unit_base = format!("optimus-command-{}-{sequence}", std::process::id());
    let linux_unit = format!("{unit_base}.service");
    let mut command = Command::new("/usr/bin/systemd-run");
    command
        .args([
            "--user",
            "--quiet",
            "--collect",
            "--wait",
            "--pipe",
            "--service-type=exec",
            "--property=KillMode=control-group",
            "--property=NoNewPrivileges=yes",
            "--property=RestrictSUIDSGID=yes",
        ])
        .arg(format!("--unit={unit_base}"))
        .arg("--")
        .arg("/usr/bin/bwrap");
    for arg in command_envelope::linux_bwrap_args(workspace, envelope) {
        command.arg(arg);
    }
    // UnrestrictedHost still binds host `/`; mask session bus + tmpfs the
    // systemd runtime dir. Confined profiles already replace `/run` with tmpfs.
    if matches!(envelope, CommandFsEnvelope::UnrestrictedHost) {
        let runtime_dir = format!("/run/user/{}", unsafe { libc::geteuid() });
        command
            .arg("--ro-bind")
            .arg("/sys/fs/cgroup")
            .arg("/sys/fs/cgroup")
            .arg("--ro-bind")
            .arg("/dev/null")
            .arg(format!("{runtime_dir}/bus"))
            .arg("--tmpfs")
            .arg(format!("{runtime_dir}/systemd"));
    }
    command
        .arg("--")
        .arg(program)
        .args(args)
        .current_dir(workspace)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    (command, linux_unit)
}

pub struct Runtime {
    store: Store,
    workspace: PathBuf,
    workspace_dir: Dir,
    config: RuntimeConfig,
}

impl Runtime {
    pub fn open(db_path: &Path, workspace: &Path) -> Result<Self> {
        Self::open_with_config(db_path, workspace, RuntimeConfig::default())
    }

    pub fn open_with_config(
        db_path: &Path,
        workspace: &Path,
        config: RuntimeConfig,
    ) -> Result<Self> {
        fs::create_dir_all(workspace)?;
        let workspace = fs::canonicalize(workspace)?;
        let workspace_dir = Dir::open_ambient_dir(&workspace, ambient_authority())?;
        let store = Store::open(db_path).map_err(GraphError::from)?;
        Ok(Self {
            store,
            workspace,
            workspace_dir,
            config,
        })
    }

    pub fn workspace_path(&self) -> &Path {
        &self.workspace
    }

    /// Stable identity used by project-bound effects to prevent cross-root replay.
    pub fn workspace_sha256(&self) -> String {
        Self::path_sha256(&self.workspace)
    }

    pub fn canonical_workspace_sha256(path: &Path) -> Result<String> {
        Ok(Self::path_sha256(&fs::canonicalize(path)?))
    }

    pub fn create_job(&self, spec: JobSpec) -> Result<JobId> {
        let created = create_job(&self.store, spec)?;
        Ok(created.id)
    }

    pub fn create_job_with_id(&self, job_id: JobId, spec: JobSpec) -> Result<JobId> {
        let created = graph_create_job_with_id(&self.store, job_id, spec)?;
        Ok(created.id)
    }

    pub fn grant_approval(&self, grant: ApprovalGrant) -> Result<()> {
        let (target_id, effect_hash, now, expires) = self.approval_identity(&grant)?;
        self.store
            .insert_action_approval(&NewActionApproval {
                id: Uuid::new_v4(),
                job_id: grant.job_id.0,
                node_id: target_id,
                effect_hash,
                actor: grant.actor,
                created_unix: now,
                expires_unix: expires,
            })
            .map_err(GraphError::from)?;
        Ok(())
    }

    pub fn deny_approval(&self, grant: ApprovalGrant, reason: &str) -> Result<()> {
        if reason.is_empty() {
            return Err(RuntimeError::NotRunnable(
                "approval denial reason is required".into(),
            ));
        }
        let (target_id, effect_hash, now, expires) = self.approval_identity(&grant)?;
        self.store
            .insert_action_denial(
                &NewActionApproval {
                    id: Uuid::new_v4(),
                    job_id: grant.job_id.0,
                    node_id: target_id,
                    effect_hash,
                    actor: grant.actor,
                    created_unix: now,
                    expires_unix: expires,
                },
                reason,
            )
            .map_err(GraphError::from)?;
        Ok(())
    }

    pub fn revoke_approval(&self, job_id: JobId, actor: &str, reason: &str) -> Result<()> {
        let (target_id, effect_hash) = self.approval_target(job_id)?;
        let revoked = self
            .store
            .revoke_action_approval(
                job_id.0,
                target_id,
                &effect_hash,
                actor,
                Self::now_unix()?,
                reason,
            )
            .map_err(GraphError::from)?;
        if !revoked {
            return Err(RuntimeError::NotRunnable(format!(
                "job {job_id} has no active exact approval to revoke"
            )));
        }
        Ok(())
    }

    fn approval_identity(&self, grant: &ApprovalGrant) -> Result<(Uuid, String, u64, u64)> {
        if grant.actor.is_empty() || grant.ttl_seconds == 0 {
            return Err(RuntimeError::NotRunnable(
                "approval actor and positive TTL are required".into(),
            ));
        }
        let (target_id, effect_hash) = self.approval_target(grant.job_id)?;
        let now = Self::now_unix()?;
        let expires = now
            .checked_add(grant.ttl_seconds)
            .ok_or_else(|| RuntimeError::NotRunnable("approval expiry overflow".into()))?;
        Ok((target_id, effect_hash, now, expires))
    }

    fn approval_target(&self, job_id: JobId) -> Result<(Uuid, String)> {
        let nodes = self.store.list_nodes(job_id.0).map_err(GraphError::from)?;
        let target = nodes
            .iter()
            .find(|node| node.status == NodeStatus::AwaitingApproval)
            .or_else(|| {
                nodes.iter().find(|node| {
                    serde_json::from_str::<Effect>(&node.effect_json)
                        .is_ok_and(|effect| effect.is_high_risk())
                        && !matches!(
                            node.status,
                            NodeStatus::Succeeded | NodeStatus::Failed | NodeStatus::Cancelled
                        )
                })
            })
            .ok_or_else(|| {
                RuntimeError::NotRunnable(format!("job {job_id} has no approvable high-risk node"))
            })?;
        Ok((target.id, Self::effect_hash(&target.effect_json)))
    }

    fn effect_hash(effect_json: &str) -> String {
        format!("{:x}", Sha256::digest(effect_json.as_bytes()))
    }

    fn now_unix() -> Result<u64> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .map_err(|error| RuntimeError::Effector(format!("system clock before epoch: {error}")))
    }

    /// Issue a durable job approval grant when the skill's closed permission set
    /// covers the pending high-risk effect class.
    ///
    /// - Write effects require `FsWorkspace`
    /// - Command effects require `Terminal`
    pub fn grant_from_skill(
        &self,
        job_id: JobId,
        skills: &SkillRegistry,
        skill_id: Uuid,
    ) -> Result<()> {
        let (target_id, effect_hash) = self.approval_target(job_id)?;
        let nodes = self.store.list_nodes(job_id.0).map_err(GraphError::from)?;
        let node = nodes
            .iter()
            .find(|node| node.id == target_id)
            .ok_or_else(|| {
                RuntimeError::NotRunnable(format!("job {job_id} approval target node missing"))
            })?;
        let effect: Effect = serde_json::from_str(&node.effect_json).map_err(GraphError::Serde)?;
        if Self::effect_hash(&node.effect_json) != effect_hash {
            return Err(RuntimeError::NotRunnable(
                "approval target effect hash drifted".into(),
            ));
        }
        let (required, permission_label) = match &effect {
            Effect::WriteFile { .. } | Effect::ProjectWriteFile { .. } => {
                (Permission::FsWorkspace, "fs_workspace")
            }
            Effect::RunCommand { .. } | Effect::ProjectRunCommand { .. } => {
                (Permission::Terminal, "terminal")
            }
            Effect::AssertFileEquals { .. } => {
                return Err(RuntimeError::NotRunnable(
                    "assert effects do not require skill approval grants".into(),
                ));
            }
        };
        skills.authorize(skill_id, &[required])?;
        self.grant_approval(ApprovalGrant::for_job_by(
            job_id,
            format!("skill:{skill_id}"),
            300,
        ))?;
        self.store
            .append_event(
                Some(job_id.0),
                None,
                "approval_from_skill",
                &serde_json::json!({
                    "skill_id": skill_id,
                    "permission": permission_label,
                }),
            )
            .map_err(GraphError::from)?;
        Ok(())
    }

    pub fn node_statuses(&self, job_id: JobId) -> Result<Vec<NodeStatus>> {
        let nodes = self.store.list_nodes(job_id.0).map_err(GraphError::from)?;
        Ok(nodes.into_iter().map(|n| n.status).collect())
    }

    pub fn job_status(&self, job_id: JobId) -> Result<JobStatus> {
        Ok(self
            .store
            .get_job(job_id.0)
            .map_err(GraphError::from)?
            .status)
    }

    pub fn job_status_optional(&self, job_id: JobId) -> Result<Option<JobStatus>> {
        match self.store.get_job(job_id.0) {
            Ok(job) => Ok(Some(job.status)),
            Err(StoreError::NotFound(_)) => Ok(None),
            Err(error) => Err(GraphError::from(error).into()),
        }
    }

    pub fn cancel_job(&self, job_id: JobId) -> Result<JobStatus> {
        Ok(self
            .store
            .request_job_cancellation(job_id.0, "operator_request")
            .map_err(GraphError::from)?)
    }

    pub fn recover_crashed_running(&self) -> Result<Vec<JobId>> {
        let mut recovered = Vec::new();
        for job in self.store.list_jobs().map_err(GraphError::from)? {
            let id = JobId(job.id);
            if self.recover_crashed_job(id)? {
                recovered.push(id);
            }
        }
        Ok(recovered)
    }

    pub fn recover_crashed_job(&self, job_id: JobId) -> Result<bool> {
        let attempts = self
            .store
            .list_prepared_effect_attempts(job_id.0)
            .map_err(GraphError::from)?;
        for attempt in &attempts {
            let effect: Effect =
                serde_json::from_str(&attempt.intent_json).map_err(GraphError::from)?;
            let (disposition, reason) = match effect {
                Effect::RunCommand { .. } | Effect::ProjectRunCommand { .. } => (
                    PreparedAttemptDisposition::Ambiguous,
                    "process outcome is unknown after runtime interruption",
                ),
                Effect::WriteFile { .. }
                | Effect::ProjectWriteFile { .. }
                | Effect::AssertFileEquals { .. } => (
                    PreparedAttemptDisposition::Interrupted,
                    "replay-safe effect interrupted before receipt persistence",
                ),
            };
            self.store
                .resolve_prepared_effect_attempt(attempt, disposition, reason)
                .map_err(GraphError::from)?;
        }
        let legacy = interrupt_running_nodes_for_job(&self.store, job_id)?;
        if !attempts.is_empty() {
            let _ = recompute_job_status(&self.store, job_id)?;
        }
        Ok(legacy || !attempts.is_empty())
    }

    pub fn list_resumable_jobs(&self) -> Result<Vec<JobId>> {
        let jobs = self.store.list_jobs().map_err(GraphError::from)?;
        let mut out = Vec::new();
        for job in jobs {
            if matches!(
                job.status,
                JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled
            ) {
                continue;
            }
            let id = JobId(job.id);
            if self
                .store
                .job_has_ambiguous_effect(job.id)
                .map_err(GraphError::from)?
            {
                continue;
            }
            if next_runnable_node(&self.store, id)?.is_some() {
                out.push(id);
            }
        }
        Ok(out)
    }

    /// Jobs blocked on SmartDeny (node or job status awaiting approval).
    pub fn list_pending_approvals(&self) -> Result<Vec<PendingApproval>> {
        let jobs = self.store.list_jobs().map_err(GraphError::from)?;
        let mut out = Vec::new();
        for job in jobs {
            let nodes = self.store.list_nodes(job.id).map_err(GraphError::from)?;
            let awaiting_nodes: Vec<_> = nodes
                .iter()
                .filter(|n| n.status == NodeStatus::AwaitingApproval)
                .cloned()
                .collect();
            if job.status == JobStatus::AwaitingApproval || !awaiting_nodes.is_empty() {
                let node = awaiting_nodes.first();
                let has_grant = match node {
                    Some(node) => self
                        .store
                        .action_has_valid_approval(
                            job.id,
                            node.id,
                            &Self::effect_hash(&node.effect_json),
                            Self::now_unix()?,
                        )
                        .map_err(GraphError::from)?,
                    None => false,
                };
                out.push(PendingApproval {
                    job_id: JobId(job.id),
                    job_label: job.label.clone(),
                    job_status: job.status,
                    node_id: node.map(|n| n.id),
                    node_index: node.map(|n| n.idx),
                    node_label: node.map(|n| n.label.clone()),
                    effect_json: node.map(|n| n.effect_json.clone()).unwrap_or_default(),
                    has_grant,
                });
            }
        }
        Ok(out)
    }

    /// Grant job-scoped approval and continue execution.
    pub fn grant_and_resume(&self, job_id: JobId) -> Result<JobStatus> {
        self.grant_approval(ApprovalGrant::for_job(job_id))?;
        match self.resume(job_id) {
            Ok(s) => Ok(s),
            Err(RuntimeError::NeedsApproval { .. }) => Ok(JobStatus::AwaitingApproval),
            Err(e) => Err(e),
        }
    }

    pub fn list_jobs_summary(&self) -> Result<Vec<JobSummary>> {
        let jobs = self.store.list_jobs().map_err(GraphError::from)?;
        let mut out = Vec::new();
        for job in jobs {
            out.push(JobSummary {
                job_id: JobId(job.id),
                label: job.label,
                status: job.status,
                steps_executed: job.steps_executed,
                max_steps: job.max_steps,
            });
        }
        Ok(out)
    }

    pub fn resume_all(&self) -> Result<Vec<(JobId, JobStatus)>> {
        self.recover_crashed_running()?;
        let mut results = Vec::new();
        for job_id in self.list_resumable_jobs()? {
            match self.resume(job_id) {
                Ok(status) => results.push((job_id, status)),
                Err(RuntimeError::NeedsApproval { .. }) => {
                    results.push((job_id, JobStatus::AwaitingApproval));
                }
                Err(e) => return Err(e),
            }
        }
        Ok(results)
    }

    pub fn run_next(&self, job_id: JobId) -> Result<StepOutcome> {
        let job = self.store.get_job(job_id.0).map_err(GraphError::from)?;
        if self
            .store
            .job_cancellation_requested(job_id.0)
            .map_err(GraphError::from)?
        {
            self.store
                .finalize_job_cancellation(job_id.0, "observed_before_effect")
                .map_err(GraphError::from)?;
            return Err(RuntimeError::Cancelled { job_id });
        }
        if job.steps_executed >= job.max_steps {
            mark_job_terminal(&self.store, job_id, JobStatus::Failed)?;
            self.store
                .append_event(
                    Some(job_id.0),
                    None,
                    "budget_exceeded",
                    &serde_json::json!({ "reason": "max_steps", "max_steps": job.max_steps }),
                )
                .map_err(GraphError::from)?;
            return Err(RuntimeError::BudgetExceeded {
                job_id,
                reason: format!("max_steps {}", job.max_steps),
            });
        }
        if job.consecutive_failures >= job.max_consecutive_failures {
            mark_job_terminal(&self.store, job_id, JobStatus::Failed)?;
            return Err(RuntimeError::BudgetExceeded {
                job_id,
                reason: format!("max_consecutive_failures {}", job.max_consecutive_failures),
            });
        }

        let node = next_runnable_node(&self.store, job_id)?
            .ok_or_else(|| RuntimeError::NotRunnable(format!("no runnable node on {job_id}")))?;

        if node.status == NodeStatus::Running {
            return Err(RuntimeError::NotRunnable(
                "node is running; recover_crashed_running required".into(),
            ));
        }

        let effect: Effect = serde_json::from_str(&node.effect_json).map_err(GraphError::Serde)?;
        let effect_hash = Self::effect_hash(&node.effect_json);

        // Reject illegal path shapes before asking the user to approve.
        self.preflight_effect(&effect)?;

        if effect.is_high_risk()
            && self.config.policy == PolicyMode::SmartDeny
            && !self
                .store
                .action_has_valid_approval(job_id.0, node.id, &effect_hash, Self::now_unix()?)
                .map_err(GraphError::from)?
        {
            mark_node_awaiting_approval(&self.store, job_id, node.id)?;
            return Err(RuntimeError::NeedsApproval {
                job_id,
                node_index: node.idx,
            });
        }

        mark_job_running(&self.store, job_id)?;
        let attempt_id = mark_node_running(&self.store, job_id, node.id)?;

        if self
            .store
            .job_cancellation_requested(job_id.0)
            .map_err(GraphError::from)?
        {
            self.store
                .finalize_job_cancellation(job_id.0, "observed_before_effect")
                .map_err(GraphError::from)?;
            return Err(RuntimeError::Cancelled { job_id });
        }

        let timeout = Duration::from_millis(u64::from(job.command_timeout_ms));
        match self.execute_effect(&effect, timeout, attempt_id, job_id) {
            Ok((capture, receipt)) => {
                if self
                    .store
                    .job_cancellation_requested(job_id.0)
                    .map_err(GraphError::from)?
                {
                    self.store
                        .finalize_job_cancellation(job_id.0, "observed_after_effect")
                        .map_err(GraphError::from)?;
                    return Err(RuntimeError::Cancelled { job_id });
                }
                mark_node_succeeded(&self.store, job_id, node.id, attempt_id, &receipt)?;
                if let Some(ref cap) = capture {
                    self.store
                        .append_event(
                            Some(job_id.0),
                            Some(node.id),
                            "command_output",
                            &serde_json::to_value(cap).unwrap_or_default(),
                        )
                        .map_err(GraphError::from)?;
                }
                self.store
                    .bump_steps_executed(job_id.0)
                    .map_err(GraphError::from)?;
                self.store
                    .set_consecutive_failures(job_id.0, 0)
                    .map_err(GraphError::from)?;
                let _ = recompute_job_status(&self.store, job_id)?;
                Ok(StepOutcome {
                    job_id,
                    node_id: node.id,
                    node_index: node.idx,
                    node_status: NodeStatus::Succeeded,
                    capture,
                })
            }
            Err(e) => {
                if matches!(e, RuntimeError::Cancelled { .. }) {
                    self.store
                        .finalize_job_cancellation(job_id.0, "process_tree_terminated")
                        .map_err(GraphError::from)?;
                    return Err(e);
                }
                let msg = e.to_string();
                if let RuntimeError::CommandFailed { capture, .. } = &e {
                    let _ = self.store.append_event(
                        Some(job_id.0),
                        Some(node.id),
                        "command_output",
                        &serde_json::to_value(capture).unwrap_or_default(),
                    );
                }
                let receipt = match &e {
                    RuntimeError::CommandFailed { capture, .. } => serde_json::json!({
                        "error": msg,
                        "capture": capture,
                    }),
                    _ => serde_json::json!({ "error": msg }),
                };
                mark_node_failed(&self.store, job_id, node.id, attempt_id, &receipt)?;
                self.store
                    .bump_steps_executed(job_id.0)
                    .map_err(GraphError::from)?;
                let fails = job.consecutive_failures.saturating_add(1);
                self.store
                    .set_consecutive_failures(job_id.0, fails)
                    .map_err(GraphError::from)?;
                if fails >= job.max_consecutive_failures {
                    mark_job_terminal(&self.store, job_id, JobStatus::Failed)?;
                    self.store
                        .append_event(
                            Some(job_id.0),
                            Some(node.id),
                            "circuit_open",
                            &serde_json::json!({ "consecutive_failures": fails }),
                        )
                        .map_err(GraphError::from)?;
                } else {
                    let _ = recompute_job_status(&self.store, job_id)?;
                }
                Err(e)
            }
        }
    }

    pub fn run_all(&self, job_id: JobId) -> Result<JobStatus> {
        loop {
            let status = self.job_status(job_id)?;
            if matches!(
                status,
                JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled
            ) {
                return Ok(status);
            }
            // AwaitingApproval is terminal only until an exact live action grant exists.
            if status == JobStatus::AwaitingApproval {
                let Some(node) = next_runnable_node(&self.store, job_id)? else {
                    return Ok(status);
                };
                let has_grant = self
                    .store
                    .action_has_valid_approval(
                        job_id.0,
                        node.id,
                        &Self::effect_hash(&node.effect_json),
                        Self::now_unix()?,
                    )
                    .map_err(GraphError::from)?;
                if !has_grant {
                    return Ok(status);
                }
            }
            let nodes = self.store.list_nodes(job_id.0).map_err(GraphError::from)?;
            if nodes.iter().any(|n| n.status == NodeStatus::Running) {
                return Err(RuntimeError::NotRunnable(
                    "running node present; recover first".into(),
                ));
            }
            if next_runnable_node(&self.store, job_id)?.is_none() {
                return Ok(recompute_job_status(&self.store, job_id)?);
            }
            match self.run_next(job_id) {
                Ok(_) => {}
                Err(RuntimeError::NeedsApproval { .. }) => {
                    return Ok(JobStatus::AwaitingApproval);
                }
                Err(e) => {
                    if matches!(self.job_status(job_id)?, JobStatus::Failed) {
                        return Ok(JobStatus::Failed);
                    }
                    return Err(e);
                }
            }
        }
    }

    pub fn resume(&self, job_id: JobId) -> Result<JobStatus> {
        if self
            .store
            .job_has_ambiguous_effect(job_id.0)
            .map_err(GraphError::from)?
        {
            return Err(RuntimeError::NotRunnable(
                "ambiguous command outcome requires operator resolution".into(),
            ));
        }
        if self
            .store
            .list_nodes(job_id.0)
            .map_err(GraphError::from)?
            .iter()
            .any(|n| n.status == NodeStatus::Running)
        {
            return Err(RuntimeError::NotRunnable(
                "running nodes remain; call recover_crashed_running".into(),
            ));
        }
        self.run_all(job_id)
    }

    pub fn begin_node_and_crash(&self, job_id: JobId) -> Result<()> {
        mark_job_running(&self.store, job_id)?;
        let node = next_runnable_node(&self.store, job_id)?
            .ok_or_else(|| RuntimeError::NotRunnable(format!("no runnable node on {job_id}")))?;
        if node.status == NodeStatus::Running {
            return Ok(());
        }
        mark_node_running(&self.store, job_id, node.id)?;
        Ok(())
    }

    fn execute_effect(
        &self,
        effect: &Effect,
        timeout: Duration,
        attempt_id: Uuid,
        job_id: JobId,
    ) -> Result<(Option<CommandCapture>, serde_json::Value)> {
        match effect {
            Effect::ProjectWriteFile {
                workspace_sha256,
                relative_path,
                contents,
            } => {
                self.verify_workspace_sha256(workspace_sha256)?;
                self.execute_effect(
                    &Effect::WriteFile {
                        relative_path: relative_path.clone(),
                        contents: contents.clone(),
                    },
                    timeout,
                    attempt_id,
                    job_id,
                )
            }
            Effect::WriteFile {
                relative_path,
                contents,
            } => {
                let relative = self.safe_relative_path(relative_path)?;
                if let Some(parent) = relative.parent() {
                    self.workspace_dir.create_dir_all(parent).map_err(|error| {
                        RuntimeError::PathEscape(format!("{relative_path}: {error}"))
                    })?;
                }
                let file_name = relative.file_name().ok_or_else(|| {
                    RuntimeError::PathEscape(format!("{relative_path}: missing file name"))
                })?;
                let temp_name =
                    format!(".{}.optimus-tmp-{attempt_id}", file_name.to_string_lossy());
                let temporary = relative
                    .parent()
                    .map(|parent| parent.join(&temp_name))
                    .unwrap_or_else(|| PathBuf::from(&temp_name));
                let mut file = self.workspace_dir.create(&temporary).map_err(|error| {
                    RuntimeError::PathEscape(format!("{relative_path}: {error}"))
                })?;
                if let Err(error) = file
                    .write_all(contents.as_bytes())
                    .and_then(|_| file.sync_all())
                {
                    let _ = self.workspace_dir.remove_file(&temporary);
                    return Err(error.into());
                }
                drop(file);
                if let Err(error) =
                    self.workspace_dir
                        .rename(&temporary, &self.workspace_dir, &relative)
                {
                    let _ = self.workspace_dir.remove_file(&temporary);
                    return Err(RuntimeError::Effector(format!(
                        "atomic replace {relative_path}: {error}"
                    )));
                }
                Ok((
                    None,
                    serde_json::json!({
                        "effect": "write_file",
                        "relative_path": relative_path,
                        "bytes": contents.len(),
                    }),
                ))
            }
            Effect::AssertFileEquals {
                relative_path,
                expected,
            } => {
                let relative = self.safe_relative_path(relative_path)?;
                let mut file = self.workspace_dir.open(&relative).map_err(|error| {
                    RuntimeError::PathEscape(format!("{relative_path}: {error}"))
                })?;
                let mut actual = String::new();
                file.read_to_string(&mut actual).map_err(|error| {
                    RuntimeError::Effector(format!("read {relative_path}: {error}"))
                })?;
                if actual != *expected {
                    return Err(RuntimeError::Effector(format!(
                        "assert failed for {}: expected {:?}, got {:?}",
                        relative_path, expected, actual
                    )));
                }
                Ok((
                    None,
                    serde_json::json!({
                        "effect": "assert_file_equals",
                        "relative_path": relative_path,
                        "matched": true,
                    }),
                ))
            }
            Effect::RunCommand { program, args } => {
                let cap = self.run_command_bounded(program, args, timeout, job_id)?;
                let receipt = serde_json::json!({
                    "effect": "run_command",
                    "capture": cap,
                });
                Ok((Some(cap), receipt))
            }
            Effect::ProjectRunCommand {
                workspace_sha256,
                program,
                args,
            } => {
                self.verify_workspace_sha256(workspace_sha256)?;
                self.execute_effect(
                    &Effect::RunCommand {
                        program: program.clone(),
                        args: args.clone(),
                    },
                    timeout,
                    attempt_id,
                    job_id,
                )
            }
        }
    }

    fn run_command_bounded(
        &self,
        program: &str,
        args: &[String],
        timeout: Duration,
        job_id: JobId,
    ) -> Result<CommandCapture> {
        ensure_owned_command_containment(std::env::consts::OS)
            .map_err(|error| RuntimeError::Effector(error.to_string()))?;
        command_envelope::command_envelope_supported(
            std::env::consts::OS,
            self.config.command_fs_envelope,
        )
        .map_err(RuntimeError::Effector)?;
        #[cfg(target_os = "linux")]
        let (mut command, linux_unit) = linux_contained_command(
            program,
            args,
            &self.workspace,
            self.config.command_fs_envelope,
        );
        #[cfg(not(target_os = "linux"))]
        let mut command = Command::new(program);
        #[cfg(not(target_os = "linux"))]
        command
            .args(args)
            .current_dir(&self.workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        sanitize_command_environment(&mut command);
        #[cfg(target_os = "linux")]
        let mut guard = KillOnDrop::spawn(&mut command, linux_unit)
            .map_err(|e| RuntimeError::Effector(format!("spawn {program}: {e}")))?;
        #[cfg(not(target_os = "linux"))]
        let mut guard = KillOnDrop::spawn(&mut command, false)
            .map_err(|e| RuntimeError::Effector(format!("spawn {program}: {e}")))?;

        let child = guard.child.as_mut().expect("contained child is present");
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let out_h = thread::spawn(move || read_limited(stdout_pipe));
        let err_h = thread::spawn(move || read_limited(stderr_pipe));

        let start = Instant::now();
        let mut timed_out = false;
        let mut cancelled = false;
        let cleanup_error;
        let wait_status = loop {
            match guard.try_wait() {
                Ok(Some(status)) => {
                    #[cfg(windows)]
                    {
                        cleanup_error = guard.terminate_job_and_confirm_empty().err();
                    }
                    #[cfg(target_os = "linux")]
                    {
                        cleanup_error = guard.terminate_linux_unit_and_confirm_empty().err();
                    }
                    #[cfg(not(any(windows, target_os = "linux")))]
                    {
                        cleanup_error = None;
                    }
                    guard.child = None;
                    break Some(status);
                }
                Ok(None) => {
                    if self
                        .store
                        .job_cancellation_requested(job_id.0)
                        .map_err(GraphError::from)?
                    {
                        cleanup_error = guard.kill_and_reap().err();
                        cancelled = true;
                        break None;
                    }
                    if start.elapsed() >= timeout {
                        cleanup_error = guard.kill_and_reap().err();
                        timed_out = true;
                        break None;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    let _ = guard.kill_and_reap();
                    return Err(RuntimeError::Effector(format!("wait {program}: {e}")));
                }
            }
        };

        let (stdout, trunc_out) = out_h.join().unwrap_or_else(|_| (String::new(), false));
        let (stderr, trunc_err) = err_h.join().unwrap_or_else(|_| (String::new(), false));

        if let Some(error) = cleanup_error {
            return Err(RuntimeError::Effector(format!(
                "process-tree cleanup for {program} was not confirmed: {error}"
            )));
        }
        if cancelled {
            return Err(RuntimeError::Cancelled { job_id });
        }

        let exit_code = wait_status.and_then(|s| s.code());
        let capture = CommandCapture {
            stdout,
            stderr,
            exit_code,
            truncated_stdout: trunc_out,
            truncated_stderr: trunc_err,
            timed_out,
        };

        if timed_out {
            return Err(RuntimeError::CommandFailed {
                exit_code: None,
                capture,
            });
        }
        if wait_status.map(|s| s.success()).unwrap_or(false) {
            return Ok(capture);
        }
        Err(RuntimeError::CommandFailed { exit_code, capture })
    }

    /// Latest command_output event for a job (if any).
    pub fn latest_command_capture(&self, job_id: JobId) -> Result<Option<CommandCapture>> {
        let events = self
            .store
            .list_events(Some(job_id.0))
            .map_err(GraphError::from)?;
        for ev in events.into_iter().rev() {
            if ev.kind == "command_output" {
                let cap: CommandCapture = serde_json::from_str(&ev.payload_json)
                    .map_err(|e| RuntimeError::Effector(format!("bad command_output: {e}")))?;
                return Ok(Some(cap));
            }
        }
        Ok(None)
    }

    pub fn latest_effect_outcome(&self, job_id: JobId) -> Result<Option<EffectOutcome>> {
        let Some(outcome) = self
            .store
            .latest_effect_attempt_outcome(job_id.0)
            .map_err(GraphError::from)?
        else {
            return Ok(None);
        };
        Ok(Some(EffectOutcome {
            attempt_id: outcome.attempt_id,
            job_id: JobId(outcome.job_id),
            node_id: outcome.node_id,
            effect_hash: Self::effect_hash(&outcome.intent_json),
            status: outcome.status,
            receipt_hash: outcome.receipt_json.as_deref().map(Self::effect_hash),
        }))
    }

    /// Validate effect shape before SmartDeny wait or execution.
    fn preflight_effect(&self, effect: &Effect) -> Result<()> {
        match effect {
            Effect::WriteFile { relative_path, .. }
            | Effect::AssertFileEquals { relative_path, .. }
            | Effect::ProjectWriteFile { relative_path, .. } => {
                self.safe_relative_path(relative_path)?;
            }
            Effect::RunCommand { program, .. } | Effect::ProjectRunCommand { program, .. } => {
                if program.trim().is_empty() {
                    return Err(RuntimeError::Effector("empty command program".into()));
                }
            }
        }
        Ok(())
    }

    fn safe_relative_path(&self, relative: &str) -> Result<PathBuf> {
        if relative.is_empty() {
            return Err(RuntimeError::PathEscape("empty path".into()));
        }
        let rel = Path::new(relative);
        if rel.is_absolute() {
            return Err(RuntimeError::PathEscape(relative.into()));
        }
        for comp in rel.components() {
            if !matches!(comp, std::path::Component::Normal(_)) {
                return Err(RuntimeError::PathEscape(relative.into()));
            }
        }
        if rel
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_secret_basename)
        {
            return Err(RuntimeError::PathEscape(relative.into()));
        }
        Ok(rel.to_path_buf())
    }

    fn verify_workspace_sha256(&self, expected: &str) -> Result<()> {
        if expected.len() != 64 || expected != self.workspace_sha256() {
            return Err(RuntimeError::PathEscape(
                "project effect workspace identity does not match the active runtime root".into(),
            ));
        }
        Ok(())
    }

    fn path_sha256(path: &Path) -> String {
        format!("{:x}", Sha256::digest(path.to_string_lossy().as_bytes()))
    }
}

/// Strip loader and dynamic-link injection variables from child process env.
///
/// Linux FS confinement is applied by [`command_envelope::linux_bwrap_args`].
/// Non-Linux hosts retain process-tree ownership only unless the envelope
/// fail-closes (see [`command_envelope_supported`]).
fn sanitize_command_environment(command: &mut Command) {
    const STRIP: &[&str] = &[
        "LD_PRELOAD",
        "LD_LIBRARY_PATH",
        "LD_AUDIT",
        "LD_DEBUG",
        "LD_DYNAMIC_WEAK",
        "LD_USE_LOAD_BIAS",
        "DYLD_INSERT_LIBRARIES",
        "DYLD_LIBRARY_PATH",
        "DYLD_FRAMEWORK_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "DYLD_IMAGE_SUFFIX",
        "DYLD_PRINT_TO_FILE",
    ];
    for key in STRIP {
        command.env_remove(key);
    }
}

fn read_limited(pipe: Option<impl Read>) -> (String, bool) {
    let Some(mut r) = pipe else {
        return (String::new(), false);
    };
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut truncated = false;
    loop {
        match r.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() >= MAX_CAPTURE_BYTES {
                    truncated = true;
                    continue;
                }
                let room = MAX_CAPTURE_BYTES - buf.len();
                if n > room {
                    buf.extend_from_slice(&chunk[..room]);
                    truncated = true;
                } else {
                    buf.extend_from_slice(&chunk[..n]);
                }
            }
            Err(_) => break,
        }
    }
    let s = String::from_utf8_lossy(&buf).into_owned();
    (s, truncated)
}

#[cfg(all(test, unix))]
mod bounded_child_cleanup_tests {
    use super::{ensure_owned_command_containment, settle_child_after_signal};
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn failed_containment_signal_kills_child_without_unbounded_wait() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 30"])
            .spawn()
            .expect("spawn long-lived child");
        let started = Instant::now();

        let error = settle_child_after_signal(
            &mut child,
            Err(std::io::Error::other("injected containment failure")),
        )
        .expect_err("injected signal failure must be reported");

        assert!(error.to_string().contains("injected containment failure"));
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(child.try_wait().expect("poll child").is_some());
    }

    #[test]
    fn run_command_fails_closed_without_durable_process_tree_ownership() {
        assert!(ensure_owned_command_containment("linux").is_ok());
        assert!(ensure_owned_command_containment("windows").is_ok());
        for unsupported in ["macos", "freebsd", "netbsd", "openbsd", "unknown"] {
            let error = ensure_owned_command_containment(unsupported).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
        }
    }
}

#[cfg(all(test, windows))]
mod windows_process_tests {
    use super::KillOnDrop;
    use std::os::windows::io::AsRawHandle;
    use std::process::Command;

    #[test]
    fn suspended_launch_failure_cannot_execute_first_instruction() {
        let root = tempfile::tempdir().expect("tempdir");
        let marker = root.path().join("escaped-before-assignment.txt");
        let mut command = Command::new("cmd");
        command
            .current_dir(root.path())
            .args(["/C", "echo escaped>escaped-before-assignment.txt"]);

        let error = match KillOnDrop::spawn(&mut command, true) {
            Ok(mut guard) => {
                let _ = guard.kill_and_reap();
                panic!("injected failure unexpectedly succeeded");
            }
            Err(error) => error,
        };

        assert!(error.to_string().contains("before Job Object assignment"));
        assert!(
            !marker.exists(),
            "suspended process executed before ownership"
        );
    }

    #[test]
    fn resumed_child_is_member_of_owned_job_object() {
        use windows_sys::Win32::System::JobObjects::IsProcessInJob;

        let mut command = Command::new("cmd");
        command.args(["/C", "ping -n 3 127.0.0.1 >nul"]);
        let mut guard = KillOnDrop::spawn(&mut command, false).expect("contained spawn");
        let child = guard.child.as_ref().expect("child");
        let job = guard.job.as_ref().expect("job");
        let mut in_job = 0;
        let queried = unsafe {
            IsProcessInJob(
                child.as_raw_handle(),
                job.as_raw_handle(),
                std::ptr::addr_of_mut!(in_job),
            )
        };

        assert_ne!(
            queried,
            0,
            "IsProcessInJob failed: {}",
            std::io::Error::last_os_error()
        );
        assert_ne!(in_job, 0, "child is not owned by the expected Job Object");
        guard.kill_and_reap().expect("terminate contained tree");
    }
}
