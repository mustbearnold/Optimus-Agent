//! Owned process trees: containment, spawn, and cancellation reaping.
//!
//! Split out of `lib.rs` under architectural law 21. Verbatim move.
//!
//! This is the machinery that makes cancellation total. A job's command runs
//! inside a systemd transient scope on Linux (and a Job object on Windows), so
//! cancelling reaps the entire owned tree through its cgroup rather than only
//! the direct child — including a `setsid` escapee, which the cancellation
//! suite asserts.
//!
//! Two consequences worth knowing before changing anything here:
//!
//!  * The host's systemd user manager is a shared global resource, so these
//!    paths cannot run concurrently across test processes. That is why
//!    `optimus-runtime` is serialised in `.config/nextest.toml`.
//!  * `CHILD_SETTLE_TIMEOUT` bounds unit settlement. Contention surfaces as a
//!    uniform ~2s failure, not as a logic error.

use super::*;

pub(crate) struct KillOnDrop {
    pub(crate) child: Option<Child>,
    #[cfg(target_os = "linux")]
    pub(crate) linux_unit: Option<String>,
    #[cfg(windows)]
    pub(crate) job: Option<OwnedHandle>,
}

pub(crate) const CHILD_SETTLE_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) fn ensure_owned_command_containment(os: &str) -> std::io::Result<()> {
    if matches!(os, "linux" | "windows") {
        return Ok(());
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!("run_command is disabled on {os}: durable process-tree ownership is unavailable"),
    ))
}

pub(crate) fn wait_child_bounded(child: &mut Child) -> std::io::Result<()> {
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

pub(crate) fn kill_child_and_confirm_exit(child: &mut Child) -> std::io::Result<()> {
    if child.try_wait()?.is_none() {
        if let Err(kill_error) = child.kill() {
            if child.try_wait()?.is_none() {
                return Err(kill_error);
            }
        }
    }
    wait_child_bounded(child)
}

pub(crate) fn settle_child_after_signal(
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
    pub(crate) fn spawn(command: &mut Command, linux_unit: String) -> std::io::Result<Self> {
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
    pub(crate) fn spawn(
        command: &mut Command,
        _fail_before_assignment: bool,
    ) -> std::io::Result<Self> {
        let _ = command;
        ensure_owned_command_containment(std::env::consts::OS)?;
        unreachable!("non-Linux Unix must fail the containment gate")
    }

    #[cfg(not(any(windows, unix)))]
    pub(crate) fn spawn(
        command: &mut Command,
        _fail_before_assignment: bool,
    ) -> std::io::Result<Self> {
        let _ = command;
        ensure_owned_command_containment(std::env::consts::OS)?;
        unreachable!("platforms without durable process-tree ownership must fail closed")
    }

    #[cfg(windows)]
    pub(crate) fn spawn(
        command: &mut Command,
        fail_before_assignment: bool,
    ) -> std::io::Result<Self> {
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

    pub(crate) fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        if let Some(child) = self.child.as_mut() {
            child.try_wait()
        } else {
            Ok(None)
        }
    }

    #[cfg(windows)]
    pub(crate) fn terminate_job_and_confirm_empty(&mut self) -> std::io::Result<()> {
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
    pub(crate) fn linux_unit_is_active(linux_unit: &str) -> std::io::Result<bool> {
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
    pub(crate) fn signal_linux_unit(&mut self) -> std::io::Result<()> {
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
    pub(crate) fn confirm_linux_unit_empty(&mut self) -> std::io::Result<()> {
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
    pub(crate) fn terminate_linux_unit_and_confirm_empty(&mut self) -> std::io::Result<()> {
        self.signal_linux_unit()?;
        self.confirm_linux_unit_empty()
    }

    pub(crate) fn kill_and_reap(&mut self) -> std::io::Result<()> {
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

/// Whether the caller waits out the contained process or outlives it.
///
/// [`LinuxRunMode::Detached`] exists for ADR-0060 project serves. A serve is
/// still running when the runtime needs to talk to it, so `--wait` (which
/// blocks until the unit exits) and `--pipe` (whose pipes deadlock once nobody
/// drains them) are both wrong there. The unit still owns the process tree
/// either way; only the runtime's relationship to it changes.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinuxRunMode {
    WaitPiped,
    Detached,
}

#[cfg(target_os = "linux")]
pub(crate) fn linux_contained_command(
    program: &str,
    args: &[String],
    workspace: &Path,
    envelope: CommandFsEnvelope,
    developer_roots: &[std::path::PathBuf],
    sandbox: &crate::toolchain::CommandSandbox<'_>,
) -> (Command, String) {
    linux_contained_command_in_mode(
        program,
        args,
        workspace,
        envelope,
        developer_roots,
        sandbox,
        LinuxRunMode::WaitPiped,
    )
}

/// Same containment as [`linux_contained_command`], for a process that keeps
/// running after the spawn call returns (ADR-0060).
///
/// Output goes to the journal rather than to pipes: an undrained pipe stalls a
/// serve at 64 KiB, and there is no reader here by construction.
#[cfg(target_os = "linux")]
pub(crate) fn linux_contained_serve_command(
    program: &str,
    args: &[String],
    workspace: &Path,
    envelope: CommandFsEnvelope,
) -> (Command, String) {
    linux_contained_command_in_mode(
        program,
        args,
        workspace,
        envelope,
        &[],
        &crate::toolchain::CommandSandbox::default(),
        LinuxRunMode::Detached,
    )
}

#[cfg(target_os = "linux")]
fn linux_contained_command_in_mode(
    program: &str,
    args: &[String],
    workspace: &Path,
    envelope: CommandFsEnvelope,
    developer_roots: &[std::path::PathBuf],
    sandbox: &crate::toolchain::CommandSandbox<'_>,
    mode: LinuxRunMode,
) -> (Command, String) {
    let sequence = NEXT_LINUX_UNIT_ID.fetch_add(1, AtomicOrdering::Relaxed);
    let unit_base = format!("optimus-command-{}-{sequence}", std::process::id());
    let linux_unit = format!("{unit_base}.service");
    let mut command = Command::new("/usr/bin/systemd-run");
    command.args([
        "--user",
        "--quiet",
        "--collect",
        "--service-type=exec",
        "--property=KillMode=control-group",
        "--property=NoNewPrivileges=yes",
        "--property=RestrictSUIDSGID=yes",
    ]);
    match mode {
        LinuxRunMode::WaitPiped => {
            command.args(["--wait", "--pipe"]);
        }
        LinuxRunMode::Detached => {
            command.args([
                "--property=StandardOutput=journal",
                "--property=StandardError=journal",
            ]);
        }
    }
    if !sandbox.bind_path.is_empty() {
        // Bind-derived sandbox PATH (spec-014 R2, ADR-0080): the bwrap child
        // resolves bare toolchain names the same way host-side normalization
        // did. HOME is pinned to the open-time home of the bind set, never
        // read from the ambient environment at spawn.
        command.args([
            "--property=Environment=PATH=".to_owned() + sandbox.bind_path,
            "--property=Environment=HOME=".to_owned() + sandbox.home,
        ]);
    }
    command
        .arg(format!("--unit={unit_base}"))
        .arg("--")
        .arg("/usr/bin/bwrap");
    for arg in command_envelope::linux_bwrap_args_classed(
        workspace,
        envelope,
        developer_roots,
        sandbox.toolchain,
    ) {
        command.arg(arg);
    }
    // UnrestrictedHost still binds host `/`; mask session bus + tmpfs the
    // systemd runtime dir. Confined profiles already replace `/run` with tmpfs.
    if matches!(envelope, CommandFsEnvelope::UnrestrictedHost)
        || developer_roots.iter().any(|root| root == Path::new("/"))
    {
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
        .current_dir(workspace);
    match mode {
        LinuxRunMode::WaitPiped => {
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
        }
        LinuxRunMode::Detached => {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
        }
    }
    (command, linux_unit)
}
