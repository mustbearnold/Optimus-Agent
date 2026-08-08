//! Kernel child operations (spec-034): the `session_spawn`,
//! `session_cancel_child`, `session_delete_child`, and
//! `session_children` tool surface, plus the admission ceremony.
//!
//! Split out of `lib.rs` under the module-size ratchet and hosted
//! under the `session` module. The registry record, the status
//! machine, and the `session_children` tables live in
//! `session/children.rs`; this file owns how the kernel exposes the
//! surface. The daemon bridge is `ChildCoordinator` (config.rs); a
//! kernel without the bridge refuses spawn with a diagnostic (A9).

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::session::children::ChildStatus;
use crate::{Kernel, KernelError, Result, ToolCall};

/// The task bound mirrors `MAX_TASK_BYTES` in the agent crate (R1).
const MAX_TASK_BYTES: usize = 64 * 1024;

fn policy_mode_str(mode: optimus_graph::PolicyMode) -> &'static str {
    match mode {
        optimus_graph::PolicyMode::SmartDeny => "smart_deny",
        optimus_graph::PolicyMode::Unrestricted => "unrestricted",
    }
}

/// The admission handle (R1): returned at once, before the child
/// executes. The handle never waits for the answer.
#[derive(Debug, Clone, Serialize)]
pub struct ChildAdmission {
    pub child_session_id: Uuid,
    pub depth: u32,
    pub status: &'static str,
    pub created_at: String,
}

impl Kernel {
    /// The running manifest of this session, if a turn is in flight.
    /// The child attribution links to it (R7).
    fn running_parent_manifest(&self) -> Result<Option<Uuid>> {
        self.executions
            .running_manifest_for_session(self.session_id)
    }

    /// Spawn a child kernel session with one typed task prompt (R1).
    /// Validates the task bound, the depth limit, and the daemon
    /// bridge; admits the registry row; hands the run to the daemon.
    /// Returns the admission handle at once.
    pub fn session_spawn(
        &mut self,
        task_prompt: String,
        provider: Option<String>,
        model: Option<String>,
    ) -> Result<ChildAdmission> {
        let coordinator = self.config.children.clone().ok_or_else(|| {
            KernelError::Tool(
                "session_spawn requires a daemon-backed kernel; embedded kernels refuse child spawn (spec-034 R4)"
                    .into(),
            )
        })?;
        let task = task_prompt.trim().to_string();
        if task.is_empty() || task.len() > MAX_TASK_BYTES {
            return Err(KernelError::Tool(format!(
                "session_spawn task must be 1 to {MAX_TASK_BYTES} bytes"
            )));
        }
        // Depth limit (R3): a child of a root session is depth 1. The
        // limit names itself in the diagnostic.
        let parent_depth = self.sessions.child_depth(self.session_id)?.unwrap_or(0);
        let depth = parent_depth.saturating_add(1);
        let limit = self.config.children_max_depth;
        if depth > limit {
            return Err(KernelError::Tool(format!(
                "depth limit reached: child depth {depth} exceeds children_max_depth {limit}"
            )));
        }
        // A child is a full kernel session with its own store context.
        // The task prompt becomes the first user message; the runner
        // injects it, so the transcript carries it exactly once.
        let title = format!("child of {}", self.session_title);
        let child_session_id = self.sessions.create(&title)?;
        let task_sha256 = format!("{:x}", Sha256::digest(task.as_bytes()));
        let parent_manifest_id = self.running_parent_manifest()?;
        let request = ChildSpawnRequest {
            parent_session_id: self.session_id,
            child_session_id,
            depth,
            task_prompt: task.clone(),
            provider: provider.clone(),
            model: model.clone(),
            effect_policy: policy_mode_str(self.config.effect_policy).to_string(),
            autonomy_profile: self.config.autonomy_profile.as_str().to_string(),
            command_fs_envelope: self
                .config
                .command_fs_envelope
                .map(|e| e.as_str().to_string()),
            children_max_depth: self.config.children_max_depth,
            parent_manifest_id,
        };
        // The registry row is durable before the handle returns (R2).
        self.sessions
            .child_admit(&crate::session::children::NewChild {
                parent_session_id: self.session_id,
                child_session_id,
                depth,
                task_sha256,
                provider,
                model: request.model.clone(),
                effect_policy: request.effect_policy.clone(),
                autonomy_profile: request.autonomy_profile.clone(),
                command_fs_envelope: request.command_fs_envelope.clone(),
                children_max_depth: request.children_max_depth,
                parent_manifest_id,
            })?;
        // Hand the run to the daemon. On failure the admission is
        // undone: the child session row goes away and the registry row
        // and its events cascade with it (no stranded `spawned` rows).
        if let Err(error) = coordinator.spawn(request) {
            let _ = self.sessions.delete(child_session_id);
            return Err(KernelError::Tool(error));
        }
        let created_at = self
            .sessions
            .child_get(child_session_id)?
            .map(|row| row.created_at)
            .unwrap_or_default();
        Ok(ChildAdmission {
            child_session_id,
            depth,
            status: ChildStatus::Spawned.as_str(),
            created_at,
        })
    }

    /// Cancel a child and its descendants (R6). Parent-scoped: the
    /// child must be a direct child of this session.
    pub fn session_cancel_child(&mut self, child_session_id: Uuid) -> Result<()> {
        let coordinator = self.config.children.clone().ok_or_else(|| {
            KernelError::Tool(
                "session_cancel_child requires a daemon-backed kernel (spec-034 R4)".into(),
            )
        })?;
        let row = self
            .sessions
            .child_of_parent(self.session_id, child_session_id)?
            .ok_or_else(|| {
                KernelError::Tool(format!(
                    "{child_session_id} is not a direct child of this session"
                ))
            })?;
        if row.deleted_at.is_some() {
            return Err(KernelError::Tool(format!(
                "child {child_session_id} is already deleted"
            )));
        }
        coordinator
            .cancel(child_session_id, "parent requested")
            .map_err(KernelError::Tool)
    }

    /// Delete a child (R6): serialize with the run, settle when the
    /// runner is gone, write the durable tombstone.
    pub fn session_delete_child(&mut self, child_session_id: Uuid) -> Result<()> {
        let coordinator = self.config.children.clone().ok_or_else(|| {
            KernelError::Tool(
                "session_delete_child requires a daemon-backed kernel (spec-034 R4)".into(),
            )
        })?;
        self.sessions
            .child_of_parent(self.session_id, child_session_id)?
            .ok_or_else(|| {
                KernelError::Tool(format!(
                    "{child_session_id} is not a direct child of this session"
                ))
            })?;
        coordinator
            .delete(child_session_id)
            .map_err(KernelError::Tool)
    }

    /// Direct children of this session, oldest first (R2). The parent
    /// keeps a registry of direct children only.
    pub fn session_children(&self) -> Result<Vec<crate::session::children::ChildRegistryRow>> {
        self.sessions.child_children(self.session_id)
    }

    /// Direct children with attributed usage (R7/R8), for the context
    /// tree and the CLI.
    pub fn session_children_with_usage(&self) -> Result<Vec<crate::home_ops::ChildSummary>> {
        let rows = self.sessions.child_children(self.session_id)?;
        let child_ids = rows
            .iter()
            .map(|row| row.child_session_id)
            .collect::<Vec<_>>();
        let usage = self.executions.child_usage(&child_ids)?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let (total, input, output, reasoning) = usage
                    .get(&row.child_session_id)
                    .copied()
                    .unwrap_or((0, 0, 0, 0));
                crate::home_ops::ChildSummary {
                    child_session_id: row.child_session_id,
                    depth: row.depth,
                    status: row.status.as_str().to_string(),
                    total_tokens: total,
                    input_tokens: input,
                    output_tokens: output,
                    reasoning_tokens: reasoning,
                    created_at: row.created_at,
                    terminal_at: row.terminal_at,
                }
            })
            .collect())
    }

    /// Route one of the four named children tools (spec-034). The action
    /// comes from the invocation variant, not from a generic `action`
    /// argument — `session_spawn` is the spawn surface, not a switch.
    pub(crate) fn dispatch_children(&mut self, call: &ToolCall, action: &str) -> Result<String> {
        match action {
            "spawn" => {
                let task = call
                    .arguments
                    .get("task")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("children spawn requires task".into()))?;
                let provider = call
                    .arguments
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let model = call
                    .arguments
                    .get("model")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let admission = self.session_spawn(task.to_string(), provider, model)?;
                Ok(serde_json::to_string(&admission)?)
            }
            "cancel" => {
                let child = call
                    .arguments
                    .get("child_session_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .ok_or_else(|| {
                        KernelError::Tool("children cancel requires child_session_id".into())
                    })?;
                self.session_cancel_child(child)?;
                Ok("cancelled".into())
            }
            "delete" => {
                let child = call
                    .arguments
                    .get("child_session_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .ok_or_else(|| {
                        KernelError::Tool("children delete requires child_session_id".into())
                    })?;
                self.session_delete_child(child)?;
                Ok("deleted".into())
            }
            "list" => {
                let children = self.session_children()?;
                Ok(serde_json::to_string(&children)?)
            }
            other => Err(KernelError::Tool(format!(
                "children action must be spawn|cancel|delete|list, got {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::children::{ChildRegistryRow, NewChild};
    use crate::session::SessionStore;
    use crate::{ChildCoordinator, KernelConfig};
    use std::sync::Arc;
    use std::sync::Mutex;

    /// The daemon-bridge stand-in (spec-034 A1: admission returns at
    /// once; the run itself is not this unit's concern).
    #[derive(Debug, Default)]
    struct FakeCoordinator {
        spawned: Mutex<Vec<ChildSpawnRequest>>,
    }

    impl ChildCoordinator for FakeCoordinator {
        fn spawn(&self, request: ChildSpawnRequest) -> std::result::Result<(), String> {
            self.spawned.lock().unwrap().push(request);
            Ok(())
        }
        fn cancel(
            &self,
            _child_session_id: Uuid,
            _reason: &str,
        ) -> std::result::Result<(), String> {
            Ok(())
        }
        fn delete(&self, _child_session_id: Uuid) -> std::result::Result<(), String> {
            Ok(())
        }
    }

    fn kernel_with(
        home: &std::path::Path,
        max_depth: u32,
        children: Option<Arc<dyn ChildCoordinator>>,
    ) -> Kernel {
        Kernel::open(
            home,
            KernelConfig {
                children_max_depth: max_depth,
                children,
                ..KernelConfig::default()
            },
        )
        .expect("kernel opens")
    }

    fn admit_as_child(store: &SessionStore, parent: Uuid, child: Uuid, depth: u32) {
        store
            .child_admit(&NewChild {
                parent_session_id: parent,
                child_session_id: child,
                depth,
                task_sha256: "a".repeat(64),
                provider: None,
                model: None,
                effect_policy: "smart_deny".into(),
                autonomy_profile: "review_changes".into(),
                command_fs_envelope: None,
                children_max_depth: 1,
                parent_manifest_id: None,
            })
            .unwrap();
    }

    /// A9: an embedded kernel (no daemon bridge) refuses spawn with a
    /// diagnostic that names the requirement.
    #[test]
    fn embedded_kernel_refuses_spawn_with_the_daemon_diagnostic() {
        let dir = tempfile::tempdir().unwrap();
        let mut kernel = kernel_with(dir.path(), 1, None);
        let err = kernel
            .session_spawn("summarize the roadmap".into(), None, None)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("daemon-backed kernel"),
            "the refusal must name the daemon requirement, got: {err}"
        );
    }

    /// R3: the depth limit is enforced with a self-naming diagnostic.
    /// A depth-1 child spawning at the default limit 1 fails clearly.
    #[test]
    fn depth_limit_one_refuses_a_child_of_a_child() {
        let dir = tempfile::tempdir().unwrap();
        let mut kernel = kernel_with(dir.path(), 1, Some(Arc::new(FakeCoordinator::default())));
        let store = SessionStore::open(dir.path().join("sessions.db")).unwrap();
        let parent = store.create("grandparent").unwrap();
        admit_as_child(&store, parent, kernel.session_id, 1);
        drop(store);

        let err = kernel
            .session_spawn("summarize the roadmap".into(), None, None)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("depth limit") && err.contains("children_max_depth 1"),
            "the diagnostic must name the limit, got: {err}"
        );
    }

    /// R1/R3: raising the limit lets a depth-1 child spawn at depth 2.
    /// The admission handle returns at once with the spawned status.
    #[test]
    fn raised_limit_admits_a_grandchild_and_returns_the_handle() {
        let dir = tempfile::tempdir().unwrap();
        let coordinator = Arc::new(FakeCoordinator::default());
        let mut kernel = kernel_with(dir.path(), 2, Some(coordinator.clone()));
        let store = SessionStore::open(dir.path().join("sessions.db")).unwrap();
        let parent = store.create("grandparent").unwrap();
        admit_as_child(&store, parent, kernel.session_id, 1);
        drop(store);

        let admission = kernel
            .session_spawn("summarize the roadmap".into(), Some("offline".into()), None)
            .expect("admission returns at once");
        assert_eq!(admission.depth, 2);
        assert_eq!(admission.status, "spawned");

        // The registry row is durable and the coordinator received the
        // full request (policy snapshot, depth, prompt).
        let store = SessionStore::open(dir.path().join("sessions.db")).unwrap();
        let row: ChildRegistryRow = store
            .child_get(admission.child_session_id)
            .unwrap()
            .expect("registry row durable before the handle returns");
        assert_eq!(row.status, ChildStatus::Spawned);
        assert_eq!(row.depth, 2);
        drop(store);

        let spawned = coordinator.spawned.lock().unwrap();
        assert_eq!(spawned.len(), 1);
        assert_eq!(spawned[0].task_prompt, "summarize the roadmap");
        assert_eq!(spawned[0].depth, 2);
        assert_eq!(spawned[0].effect_policy, "smart_deny");
        assert_eq!(spawned[0].autonomy_profile, "review_changes");
        assert_eq!(spawned[0].children_max_depth, 2);
    }

    /// R1: the task bound refuses empty and oversized prompts.
    #[test]
    fn task_bound_refuses_empty_and_oversized_prompts() {
        let dir = tempfile::tempdir().unwrap();
        let mut kernel = kernel_with(dir.path(), 1, Some(Arc::new(FakeCoordinator::default())));
        let err = kernel
            .session_spawn("   ".into(), None, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("1 to"), "empty task must refuse, got: {err}");

        let huge = "x".repeat(MAX_TASK_BYTES + 1);
        let err = kernel
            .session_spawn(huge, None, None)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("1 to"),
            "oversized task must refuse, got: {err}"
        );
    }

    /// R2: when the daemon bridge fails, the admission is undone — no
    /// stranded `spawned` row survives.
    #[test]
    fn coordinator_failure_undoes_the_admission() {
        let dir = tempfile::tempdir().unwrap();
        let coordinator = Arc::new(RejectingCoordinator);
        let mut kernel = kernel_with(dir.path(), 1, Some(coordinator));
        let err = kernel
            .session_spawn("summarize the roadmap".into(), None, None)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("busy"),
            "the coordinator error must surface, got: {err}"
        );
        let store = SessionStore::open(dir.path().join("sessions.db")).unwrap();
        let children = store.child_children(kernel.session_id).unwrap();
        assert!(
            children.is_empty(),
            "a failed admission must not leave a stranded child"
        );
    }

    #[derive(Debug)]
    struct RejectingCoordinator;
    impl ChildCoordinator for RejectingCoordinator {
        fn spawn(&self, _request: ChildSpawnRequest) -> std::result::Result<(), String> {
            Err("server busy".into())
        }
        fn cancel(
            &self,
            _child_session_id: Uuid,
            _reason: &str,
        ) -> std::result::Result<(), String> {
            Ok(())
        }
        fn delete(&self, _child_session_id: Uuid) -> std::result::Result<(), String> {
            Ok(())
        }
    }

    /// R6: cancel and delete are parent-scoped — a session cannot touch
    /// a child it does not own.
    #[test]
    fn cancel_and_delete_are_parent_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let mut kernel = kernel_with(dir.path(), 1, Some(Arc::new(FakeCoordinator::default())));
        let store = SessionStore::open(dir.path().join("sessions.db")).unwrap();
        let other_parent = store.create("other parent").unwrap();
        let child = store.create("child of other").unwrap();
        admit_as_child(&store, other_parent, child, 1);
        drop(store);

        let err = kernel.session_cancel_child(child).unwrap_err().to_string();
        assert!(
            err.contains("not a direct child"),
            "the refusal must name the parent scope, got: {err}"
        );
        let err = kernel.session_delete_child(child).unwrap_err().to_string();
        assert!(
            err.contains("not a direct child"),
            "the refusal must name the parent scope, got: {err}"
        );
    }
}

use std::fmt::Debug;

use uuid::Uuid;

/// The child run request the kernel hands to the daemon at admission.
/// The daemon enqueues the run and returns at once; the admission
/// handle never waits for the answer (R1).
#[derive(Debug, Clone)]
pub struct ChildSpawnRequest {
    pub parent_session_id: Uuid,
    pub child_session_id: Uuid,
    /// Registry depth of the child (R3).
    pub depth: u32,
    /// The one typed task prompt. Becomes the first user message of
    /// the child transcript (R1).
    pub task_prompt: String,
    /// Explicit provider and model selection, or inheritance (R5).
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Inherited or explicit policy snapshot (R5): effect policy,
    /// autonomy profile, command FS envelope.
    pub effect_policy: String,
    pub autonomy_profile: String,
    pub command_fs_envelope: Option<String>,
    /// The depth limit the child kernel inherits (R3). Default 1.
    pub children_max_depth: u32,
    /// The running parent turn manifest, for usage attribution (R7).
    pub parent_manifest_id: Option<Uuid>,
}

/// The host-owned coordinator. All methods return fast; the bounded
/// waits (R4 gate) happen inside `cancel` and `delete` only.
pub trait ChildCoordinator: Send + Sync + Debug {
    /// Enqueue the child run. Returns at once. The child executes in
    /// the daemon and survives parent client detach (R4).
    fn spawn(&self, request: ChildSpawnRequest) -> std::result::Result<(), String>;

    /// Cancel a child and its descendants (R6). Records the durable
    /// marker, cancels the live token when present, and waits for the
    /// terminal within a bound. A child with no live runner settles
    /// to `cancelled` with the reason `runner_lost`.
    fn cancel(&self, child_session_id: Uuid, reason: &str) -> std::result::Result<(), String>;

    /// Delete a child (R6): serialize with the run, settle
    /// `runner_lost` when the runner is gone, then write the durable
    /// tombstone. The terminal status never changes.
    fn delete(&self, child_session_id: Uuid) -> std::result::Result<(), String>;
}
