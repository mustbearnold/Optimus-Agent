//! The structured project-serve effect (ADR-0060 clause 3).
//!
//! This is the only production path that mints an owned-localhost lease. It
//! exists as a child module of `owned_localhost` so it can build the two opaque
//! types the registry requires — [`OwnedLocalhostExecutionContext`] and
//! [`VerifiedOwnedServer`] — without either growing a public constructor that
//! serializable data or another crate could reach.
//!
//! Two authority decisions happen for one serve, and they are not redundant:
//!
//!   * the **effect gate** in `run_next` decides whether this process may start
//!     at all. It runs before a listener exists, so it maps to
//!     `ProcessProjectExecute` and has no binding to present.
//!   * the **lease round** below decides whether the proven binding may be
//!     handed out as `ProcessProjectServe`. It is the first moment a binding
//!     exists, and therefore the first moment that question is answerable.
//!
//! Ordering matters for containment, not just for policy: the process is
//! spawned before either the proof or the lease round can happen, so every
//! failure path from spawn onwards drops the retained owner, and dropping it
//! kills the tree.

use std::net::{IpAddr, Ipv4Addr};
use std::thread;
use std::time::{Duration, Instant};

use optimus_graph::CommandFsEnvelope;
use uuid::Uuid;

use super::{listener_proof, refused, OwnedLocalhostExecutionContext, VerifiedOwnedServer};
use crate::{
    command_envelope_supported, CommandCapture, Effect, JobId, Result, Runtime, RuntimeError,
};

const READINESS_POLL: Duration = Duration::from_millis(25);

impl Runtime {
    pub(crate) fn execute_project_serve(
        &self,
        effect: &Effect,
        timeout: Duration,
        attempt_id: Uuid,
        job_id: JobId,
    ) -> Result<(Option<CommandCapture>, serde_json::Value)> {
        let Effect::ProjectServe {
            workspace_sha256,
            program,
            args,
            port,
            ttl_seconds,
        } = effect
        else {
            return Err(RuntimeError::Effector(
                "execute_project_serve requires a project serve effect".into(),
            ));
        };
        self.verify_workspace_sha256(workspace_sha256)?;
        let envelope = self.command_fs_envelope();
        command_envelope_supported(std::env::consts::OS, envelope)
            .map_err(RuntimeError::Effector)?;
        if envelope == CommandFsEnvelope::ConfinedNoNetwork {
            // The lease's entire value is that something can reach the
            // listener. Under an unshared network namespace nothing outside the
            // sandbox can, so a lease here would be authority over an
            // unreachable socket.
            return Err(refused(
                "project serve is incoherent under the confined_no_network envelope: \
                 the listener would be unreachable from outside its network namespace",
            ));
        }

        // Every node's `effect_json` is `serde_json::to_string(&effect)`
        // (`optimus_graph::create_job`), so re-serializing here reproduces the
        // exact bytes the effect gate hashed — and with them the approval that
        // covers this program, arguments, port and TTL.
        let effect_hash = Self::effect_hash(&serde_json::to_string(effect).map_err(|error| {
            RuntimeError::Effector(format!("project serve effect is not serializable: {error}"))
        })?);
        let host = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let context = OwnedLocalhostExecutionContext {
            project_scope_id: self.owned_localhost_scope.clone(),
            project_root_hash: workspace_sha256.clone(),
            session_id: self.session_id,
            // A Work Graph job is the unit of execution today, so the run and
            // the job share an identity. They stay separate fields because
            // revocation is defined per run, and a run may outgrow one job.
            run_id: job_id.0,
            owner_job_id: job_id.0,
            owner_attempt_id: attempt_id,
        };

        let mut owner =
            listener_proof::spawn_retained_serve(program, args, &self.workspace, envelope)
                .map_err(|error| refused(format!("project serve could not start: {error}")))?;
        let readiness = timeout.min(Duration::from_secs(*ttl_seconds));
        let deadline = Instant::now() + readiness;
        loop {
            match owner.listener_is_live(host, *port) {
                Ok(true) => break,
                Ok(false) => {}
                Err(error) => {
                    return Err(refused(format!(
                        "listener ownership on 127.0.0.1:{port} could not be probed: {error}"
                    )));
                }
            }
            if Instant::now() >= deadline {
                return Err(refused(format!(
                    "no owned listener on 127.0.0.1:{port} within {readiness:?}"
                )));
            }
            thread::sleep(READINESS_POLL);
        }

        let proof = VerifiedOwnedServer {
            owner: Some(owner),
            quarantine_key: None,
        };
        let binding =
            self.issue_verified_owned_localhost(context, host, *port, *ttl_seconds, proof)?;
        let summary = format!(
            "lease 127.0.0.1:{port} to {} for {ttl_seconds}s",
            binding.process_tree_id
        );
        let authority_id =
            match self.authorize_owned_localhost_lease(&effect_hash, summary, &binding) {
                Ok(authority_id) => authority_id,
                Err(error) => {
                    // Revocation terminates the process tree, so a refused lease
                    // leaves no listener behind for anyone to reach.
                    let _ = self.revoke_owned_localhost_lease(binding.lease_id);
                    return Err(error);
                }
            };

        Ok((
            None,
            serde_json::json!({
                "owned_localhost": binding,
                "authority_id": authority_id,
            }),
        ))
    }
}
