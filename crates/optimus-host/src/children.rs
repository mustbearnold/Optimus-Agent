//! Daemon-backed child execution (spec-034 R4).
//!
//! The daemon owns the children. A spawned child runs as a kernel
//! turn loop on the worker pool, with its own store context and a
//! cancellation token. The parent client may detach; the daemon
//! keeps running the child to its single terminal outcome.
//!
//! The module also implements the crash-window rules: cancellation
//! and deletion write durable markers first, wait a bounded time for
//! the runner, and settle a runner-less child directly (spec-034 R6).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use optimus_kernel::{CancellationToken, ChildCoordinator, ChildSpawnRequest};
use optimus_workflow::children::{AdoptionAction, ChildStatus, ChildSupervisor};
use serde_json::json;
use uuid::Uuid;

use crate::chat::chat_turn_inner;
use crate::dispatch::PoolJob;

/// How long a cancel or delete waits for the runner to settle.
const SETTLE_WAIT: Duration = Duration::from_secs(10);
/// How often the wait polls the registry.
const SETTLE_POLL: Duration = Duration::from_millis(100);

/// The full run inputs for one child turn.
#[derive(Debug, Clone)]
pub struct ChildRunSpec {
    pub home: PathBuf,
    pub child_session_id: Uuid,
    /// The task prompt. `None` reads the transcript's last user
    /// message (the adoption path, R4).
    pub task_prompt: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub effect_policy: String,
    pub autonomy_profile: String,
    pub command_fs_envelope: Option<String>,
    pub children_max_depth: u32,
    pub parent_manifest_id: Option<Uuid>,
}

/// The daemon-owned child coordinator. Injected into every kernel the
/// daemon opens, so parent kernels and child kernels share one
/// supervisor surface.
#[derive(Debug)]
pub struct ChildrenRuntime {
    home: PathBuf,
    tx: mpsc::SyncSender<PoolJob>,
    live: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
}

impl ChildrenRuntime {
    pub fn new(
        home: PathBuf,
        tx: mpsc::SyncSender<PoolJob>,
        live: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
    ) -> Self {
        Self { home, tx, live }
    }

    /// A fresh supervisor connection set. The registry connections are
    /// not shareable across threads, so each operation opens its own.
    fn supervisor(&self) -> Result<ChildSupervisor, String> {
        ChildSupervisor::open(&self.home).map_err(|e| e.to_string())
    }

    /// Wait for the child's terminal outcome. Exits early when the
    /// runner is gone (the token is no longer registered): the crash
    /// window leaves no live runner, so the caller settles the child
    /// itself (R6). The bounded wait covers a stuck-but-registered
    /// runner.
    fn wait_terminal(&self, sup: &ChildSupervisor, child: Uuid) -> Result<(), String> {
        let deadline = Instant::now() + SETTLE_WAIT;
        loop {
            let terminal = match sup.row(child).map_err(|e| e.to_string()) {
                Ok(Some(row)) => row.status.is_terminal(),
                Ok(None) => true,
                Err(e) => return Err(e),
            };
            if terminal {
                return Ok(());
            }
            let runner_live = self
                .live
                .lock()
                .map_err(|e| e.to_string())?
                .contains_key(&child);
            if !runner_live {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("child did not settle within the wait bound".into());
            }
            std::thread::sleep(SETTLE_POLL);
        }
    }
}

impl ChildCoordinator for ChildrenRuntime {
    fn spawn(&self, request: ChildSpawnRequest) -> Result<(), String> {
        let spec = ChildRunSpec {
            home: self.home.clone(),
            child_session_id: request.child_session_id,
            task_prompt: Some(request.task_prompt),
            provider: request.provider,
            model: request.model,
            effect_policy: request.effect_policy,
            autonomy_profile: request.autonomy_profile,
            command_fs_envelope: request.command_fs_envelope,
            children_max_depth: request.children_max_depth,
            parent_manifest_id: request.parent_manifest_id,
        };
        let token = CancellationToken::new();
        self.live
            .lock()
            .map_err(|e| e.to_string())?
            .insert(spec.child_session_id, token.clone());
        // The child kernel gets its own coordinator arc so it can
        // spawn grandchildren (R3 depth permitting).
        let child_arc: Arc<dyn ChildCoordinator> = Arc::new(ChildrenRuntime {
            home: self.home.clone(),
            tx: self.tx.clone(),
            live: Arc::clone(&self.live),
        });
        self.tx
            .try_send(PoolJob::ChildRun {
                spec,
                token,
                live: Arc::clone(&self.live),
                children: Some(child_arc),
            })
            .map_err(|e| format!("child spawn enqueue failed: {e}"))?;
        Ok(())
    }

    fn cancel(&self, child_session_id: Uuid, reason: &str) -> Result<(), String> {
        let sup = self.supervisor()?;
        // Durable markers first (R6): adoption settles a marked child
        // to `cancelled` instead of re-running it.
        let mut targets = sup
            .descendants(child_session_id)
            .map_err(|e| e.to_string())?;
        targets.insert(0, child_session_id);
        for id in &targets {
            let _ = sup.cancel_request(*id, reason);
        }
        // Then the live tokens, all the way down the hierarchy.
        {
            let live = self.live.lock().map_err(|e| e.to_string())?;
            for id in &targets {
                if let Some(token) = live.get(id) {
                    token.cancel();
                }
            }
        }
        // Bounded wait for the child's own terminal.
        let _ = self.wait_terminal(&sup, child_session_id);
        // Runner-lost settle (the crash-window rule): a child with no
        // live runner settles at the cancel call.
        for id in &targets {
            if let Some(row) = sup.row(*id).map_err(|e| e.to_string())? {
                if !row.status.is_terminal() {
                    let _ = sup.settle(
                        *id,
                        ChildStatus::Cancelled,
                        Some("runner_lost"),
                        0,
                        row.parent_manifest_id,
                    );
                }
            }
        }
        Ok(())
    }

    fn delete(&self, child_session_id: Uuid) -> Result<(), String> {
        let sup = self.supervisor()?;
        let row = sup
            .row(child_session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no such child session".to_string())?;
        if row.status.is_terminal() {
            sup.tombstone(child_session_id).map_err(|e| e.to_string())?;
            return Ok(());
        }
        // Serialize with the run (R6): request cancellation, wait for
        // the `cancelled` terminal, then write the tombstone.
        self.cancel(child_session_id, "delete")?;
        sup.tombstone(child_session_id).map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Execute one child turn to its single terminal outcome. Runs on a
/// worker-pool thread; never returns a result to a client.
pub(crate) fn run_child_turn(
    spec: ChildRunSpec,
    cancellation: CancellationToken,
    live: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
    children: Option<Arc<dyn ChildCoordinator>>,
) {
    let supervisor = match ChildSupervisor::open(&spec.home) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("children: supervisor open failed: {e}");
            return;
        }
    };
    // The task prompt: explicit (live spawn) or the transcript's last
    // user message (adoption re-run).
    let message = match spec.task_prompt.clone() {
        Some(message) => message,
        None => match supervisor.task_prompt(spec.child_session_id) {
            Ok(Some(message)) => message,
            Ok(None) => {
                eprintln!("children: no task prompt in transcript");
                let _ = supervisor.settle(
                    spec.child_session_id,
                    ChildStatus::Failed,
                    Some("no_task_prompt"),
                    0,
                    spec.parent_manifest_id,
                );
                return;
            }
            Err(e) => {
                eprintln!("children: task prompt read failed: {e}");
                let _ = supervisor.settle(
                    spec.child_session_id,
                    ChildStatus::Failed,
                    Some("task_prompt_read_failed"),
                    0,
                    spec.parent_manifest_id,
                );
                return;
            }
        },
    };
    // Resolve the route for the provider/model snapshot, then record
    // the running transition before any model work (R4 ordering).
    let snapshot = optimus_kernel::resolve_route(
        &spec.home,
        &optimus_kernel::RouteRequest::standard(
            optimus_kernel::RouteSurface::Desktop,
            spec.provider.as_deref().unwrap_or("auto"),
            spec.model.clone(),
        ),
    )
    .ok()
    .map(|route| (route.provider.as_str().to_string(), route.model));
    let running = supervisor.mark_running(
        spec.child_session_id,
        true,
        snapshot.as_ref().map(|s| s.0.as_str()),
        snapshot.as_ref().map(|s| s.1.as_str()),
    );
    if let Err(e) = running {
        eprintln!("children: mark_running failed: {e}");
        // Exactly-one-terminal: a child whose run could not even start
        // settles failed here, so no `spawned` row survives the daemon.
        let _ = supervisor.settle(
            spec.child_session_id,
            ChildStatus::Failed,
            Some("mark_running_failed"),
            0,
            spec.parent_manifest_id,
        );
        return;
    }
    live.lock()
        .map_err(|e| e.to_string())
        .map(|mut map| map.insert(spec.child_session_id, cancellation.clone()))
        .ok();
    let started = Instant::now();
    let params = json!({
        "message": message,
        "provider": spec.provider,
        "model": spec.model,
        "session": spec.child_session_id.to_string(),
        "access": spec.autonomy_profile,
        "effect_policy": spec.effect_policy,
        "command_fs_envelope": spec.command_fs_envelope,
        "children_max_depth": spec.children_max_depth,
    });
    let outcome = chat_turn_inner(&spec.home, params, None, &cancellation, children, true);
    live.lock()
        .map_err(|e| e.to_string())
        .map(|mut map| map.remove(&spec.child_session_id))
        .ok();
    let duration_ms = started.elapsed().as_millis() as u64;
    let (status, reason): (ChildStatus, Option<String>) = if cancellation.is_cancelled() {
        (ChildStatus::Cancelled, Some("cancelled".into()))
    } else {
        match outcome {
            Ok(_) => (ChildStatus::Succeeded, None),
            Err(e) => (ChildStatus::Failed, Some(e.chars().take(200).collect())),
        }
    };
    if let Err(e) = supervisor.settle(
        spec.child_session_id,
        status,
        reason.as_deref(),
        duration_ms,
        spec.parent_manifest_id,
    ) {
        eprintln!("children: settle {} failed: {e}", spec.child_session_id);
    }
}

/// The daemon-start adoption sweep (R4): re-run never-started
/// children, settle interrupted and cancel-requested children.
/// Returns the number of children acted on.
pub(crate) fn adopt_children(
    home: &Path,
    tx: &mpsc::SyncSender<PoolJob>,
    live: &Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
    children: &Arc<dyn ChildCoordinator>,
) -> Result<usize, String> {
    let supervisor = ChildSupervisor::open(home).map_err(|e| e.to_string())?;
    let plan = supervisor.adoption_plan().map_err(|e| e.to_string())?;
    let mut count = 0;
    for action in plan {
        match action {
            AdoptionAction::Run {
                child_session_id,
                provider,
                model,
                effect_policy,
                autonomy_profile,
                command_fs_envelope,
                children_max_depth,
                parent_manifest_id,
            } => {
                let spec = ChildRunSpec {
                    home: home.to_path_buf(),
                    child_session_id,
                    task_prompt: None,
                    provider,
                    model,
                    effect_policy,
                    autonomy_profile,
                    command_fs_envelope,
                    children_max_depth,
                    parent_manifest_id,
                };
                let token = CancellationToken::new();
                live.lock()
                    .map_err(|e| e.to_string())?
                    .insert(child_session_id, token.clone());
                tx.try_send(PoolJob::ChildRun {
                    spec,
                    token,
                    live: Arc::clone(live),
                    children: Some(Arc::clone(children)),
                })
                .map_err(|e| format!("adoption enqueue failed: {e}"))?;
                count += 1;
            }
            AdoptionAction::Settle {
                child_session_id,
                status,
                reason,
            } => {
                supervisor
                    .settle(child_session_id, status, Some(reason), 0, None)
                    .map_err(|e| e.to_string())?;
                count += 1;
            }
        }
    }
    Ok(count)
}

/// The surface-turn refusal (R4): only the daemon may run a child
/// session's turns. A chat stream targeting a child session refuses
/// with a diagnostic that names the session.
pub(crate) fn is_child_session(home: &Path, session_id: Uuid) -> Result<bool, String> {
    let conn = rusqlite::Connection::open(home.join("sessions.db")).map_err(|e| e.to_string())?;
    let has_table: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'session_children')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !has_table {
        return Ok(false);
    }
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM session_children WHERE child_session_id = ?1)",
        [session_id.to_string()],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}
