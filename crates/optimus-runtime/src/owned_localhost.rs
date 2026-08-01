//! Runtime-owned localhost lease registry (ADR-0060).
//!
//! A serializable [`OwnedLocalhostBinding`] is correlation data, not authority.
//! Authority exists only while the exact binding is present in this in-memory
//! registry, its trusted execution context still matches, its generation and
//! expiry are current, and the retained process owner still proves the exact
//! listener live. Process restart invalidates this in-memory authority by
//! construction; orphan-process discovery/reaping remains a later product seam.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use optimus_graph::GraphError;
use optimus_policy::{OwnedLocalhostBinding, OwnedLocalhostScheme};
use optimus_store::Store;
use uuid::Uuid;

use crate::{Result, Runtime, RuntimeError};

mod global;
mod listener_proof;
mod serve;

const MAX_LEASE_SECONDS: u64 = 300;
const USE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Opaque execution identity that a later kernel/runtime bridge must mint only
/// after validating durable session, run, job, and attempt provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedLocalhostExecutionContext {
    project_scope_id: String,
    project_root_hash: String,
    session_id: Uuid,
    run_id: Uuid,
    owner_job_id: Uuid,
    owner_attempt_id: Uuid,
}

impl OwnedLocalhostExecutionContext {
    fn is_well_formed_for(&self, workspace_sha256: &str) -> bool {
        !self.project_scope_id.trim().is_empty()
            && self.project_scope_id.trim() == self.project_scope_id
            && self.project_scope_id.len() <= 256
            && self.project_root_hash == workspace_sha256
            && self.project_root_hash.len() == 64
            && self
                .project_root_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            && !self.session_id.is_nil()
            && !self.run_id.is_nil()
            && !self.owner_job_id.is_nil()
            && !self.owner_attempt_id.is_nil()
    }

    fn matches(&self, binding: &OwnedLocalhostBinding) -> bool {
        self.project_scope_id == binding.project_scope_id
            && self.project_root_hash == binding.project_root_hash
            && self.session_id == binding.session_id
            && self.run_id == binding.run_id
            && self.owner_job_id == binding.owner_job_id
            && self.owner_attempt_id == binding.owner_attempt_id
    }
}

/// Opaque retained ownership proof.
///
/// The production constructor will live beside platform containment in
/// `process_ownership`; there is deliberately no public constructor here.
/// Until that path exists, no product code can activate a lease.
pub(crate) trait RetainedOwnedServer: Send {
    /// Implementations must bound their platform probe internally.
    fn process_tree_id(&self) -> &str;
    fn listener_is_live(&mut self, host: IpAddr, port: u16) -> std::io::Result<bool>;
    /// Implementations must bound process-tree settlement internally.
    fn terminate_and_confirm(&mut self) -> std::io::Result<()>;
}

/// Opaque proof that a platform containment backend retained and verified the
/// exact listener.
///
/// The type is public so the later structured-serve effect can pass it through
/// the runtime waist, but its field and constructor stay private. Serializable
/// data and external callers therefore cannot manufacture one.
pub struct VerifiedOwnedServer {
    owner: Option<Box<dyn RetainedOwnedServer>>,
    quarantine_key: Option<String>,
}

impl VerifiedOwnedServer {
    #[cfg(test)]
    pub(crate) fn from_retained(owner: Box<dyn RetainedOwnedServer>) -> Self {
        Self {
            owner: Some(owner),
            quarantine_key: None,
        }
    }

    fn owner_mut(&mut self) -> Result<&mut (dyn RetainedOwnedServer + 'static)> {
        self.owner
            .as_deref_mut()
            .ok_or_else(|| refused("verified owned-server proof was already consumed"))
    }

    fn terminate_and_confirm(&mut self) -> std::io::Result<()> {
        let Some(owner) = self.owner.as_deref_mut() else {
            return Ok(());
        };
        owner.terminate_and_confirm()?;
        self.owner.take();
        Ok(())
    }
}

impl Drop for VerifiedOwnedServer {
    fn drop(&mut self) {
        if self.terminate_and_confirm().is_err() {
            if let Some(owner) = self.owner.take() {
                global::retain_failed_owner(
                    self.quarantine_key
                        .take()
                        .unwrap_or_else(|| "unscoped".into()),
                    owner,
                );
            }
        }
    }
}

struct ActiveLease {
    binding: OwnedLocalhostBinding,
    owner: VerifiedOwnedServer,
    uses: Arc<UseCounter>,
}

#[derive(Default)]
pub(crate) struct OwnedLocalhostLeaseRegistry {
    scope_key: String,
    workspace_sha256: String,
    active: HashMap<Uuid, ActiveLease>,
    cleanup_pending: HashMap<Uuid, ActiveLease>,
    next_generation: HashMap<(IpAddr, u16), u64>,
}

struct LeaseActivation {
    context: OwnedLocalhostExecutionContext,
    host: IpAddr,
    port: u16,
    ttl_seconds: u64,
    now_unix: u64,
    proof: VerifiedOwnedServer,
}

#[derive(Debug, Clone, Copy)]
pub enum OwnedLocalhostRevocationReason {
    Explicit,
    RunSettled,
    SessionClosed,
    Expired,
    ListenerLost,
    RuntimeDrop,
}

impl OwnedLocalhostRevocationReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::RunSettled => "run_settled",
            Self::SessionClosed => "session_closed",
            Self::Expired => "expired",
            Self::ListenerLost => "listener_lost",
            Self::RuntimeDrop => "runtime_drop",
        }
    }
}

#[derive(Default)]
struct UseCounter {
    state: Mutex<UseState>,
    drained: Condvar,
}

#[derive(Default)]
struct UseState {
    revoking: bool,
    active: usize,
}

impl UseCounter {
    fn acquire(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| refused("owned localhost use counter lock was poisoned"))?;
        if state.revoking {
            return Err(refused("owned localhost lease is revoking"));
        }
        state.active = state
            .active
            .checked_add(1)
            .ok_or_else(|| refused("owned localhost active-use count overflowed"))?;
        Ok(())
    }

    fn release(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.active = state.active.saturating_sub(1);
            if state.active == 0 {
                self.drained.notify_all();
            }
        }
    }

    fn revoke_and_wait(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| refused("owned localhost use counter lock was poisoned"))?;
        state.revoking = true;
        let deadline = Instant::now() + USE_DRAIN_TIMEOUT;
        while state.active != 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(refused(
                    "owned localhost uses did not drain before the bounded deadline",
                ));
            }
            let (next, timed_out) = self
                .drained
                .wait_timeout(state, remaining)
                .map_err(|_| refused("owned localhost use counter lock was poisoned"))?;
            state = next;
            if timed_out.timed_out() && state.active != 0 {
                return Err(refused(
                    "owned localhost uses did not drain before the bounded deadline",
                ));
            }
        }
        Ok(())
    }
}

/// Non-serializable live-use guard.
///
/// Acquire one guard per network request. Revocation first removes the registry
/// entry so no new guard can be issued, then waits up to the bounded drain
/// deadline before fail-safe process termination. Cleanup evidence records
/// whether the drain completed.
pub struct OwnedLocalhostUse {
    binding: OwnedLocalhostBinding,
    counter: Arc<UseCounter>,
}

impl OwnedLocalhostUse {
    pub fn binding(&self) -> &OwnedLocalhostBinding {
        &self.binding
    }
}

impl Drop for OwnedLocalhostUse {
    fn drop(&mut self) {
        self.counter.release();
    }
}

impl OwnedLocalhostLeaseRegistry {
    fn new(scope_key: String, workspace_sha256: String) -> Self {
        Self {
            scope_key,
            workspace_sha256,
            ..Self::default()
        }
    }

    fn activate(
        &mut self,
        store: &Store,
        activation: LeaseActivation,
    ) -> Result<OwnedLocalhostBinding> {
        let LeaseActivation {
            context,
            host,
            port,
            ttl_seconds,
            now_unix,
            mut proof,
        } = activation;
        let quarantine_key = format!("{}:{host}:{port}", self.scope_key);
        proof.quarantine_key = Some(quarantine_key.clone());
        if global::has_failed_owner(&quarantine_key) {
            return Err(refused(
                "the exact localhost origin has an owner awaiting cleanup confirmation",
            ));
        }
        if !context.is_well_formed_for(&self.workspace_sha256) {
            return Err(refused("invalid trusted execution context"));
        }
        if host != IpAddr::V4(Ipv4Addr::LOCALHOST) || port == 0 {
            return Err(refused(
                "owned localhost requires exact 127.0.0.1 and a nonzero port",
            ));
        }
        if ttl_seconds == 0 || ttl_seconds > MAX_LEASE_SECONDS {
            return Err(refused(
                "owned localhost lease duration is outside the bounded limit",
            ));
        }
        let process_tree_id = proof.owner_mut()?.process_tree_id().to_string();
        if process_tree_id.trim().is_empty()
            || process_tree_id.trim() != process_tree_id
            || process_tree_id.len() > 256
        {
            return Err(refused("owned process-tree identity is invalid"));
        }
        if self
            .active
            .values()
            .chain(self.cleanup_pending.values())
            .any(|lease| lease.binding.host == host && lease.binding.port == port)
        {
            return Err(refused(
                "the exact localhost origin already has an active lease",
            ));
        }
        if !proof
            .owner_mut()?
            .listener_is_live(host, port)
            .map_err(|error| refused(format!("listener ownership proof failed: {error}")))?
        {
            return Err(refused(
                "listener is not live in the retained owned process tree",
            ));
        }

        let previous_generation = self
            .next_generation
            .get(&(host, port))
            .copied()
            .unwrap_or(0);
        let generation = previous_generation
            .checked_add(1)
            .ok_or_else(|| refused("localhost lease generation overflowed"))?;
        let expires_unix = now_unix
            .checked_add(ttl_seconds)
            .ok_or_else(|| refused("localhost lease expiry overflowed"))?;
        let binding = OwnedLocalhostBinding {
            lease_id: Uuid::new_v4(),
            scheme: OwnedLocalhostScheme::Http,
            host,
            port,
            project_scope_id: context.project_scope_id,
            project_root_hash: context.project_root_hash,
            session_id: context.session_id,
            run_id: context.run_id,
            owner_job_id: context.owner_job_id,
            owner_attempt_id: context.owner_attempt_id,
            process_tree_id,
            generation,
            expires_unix,
        };
        let active = ActiveLease {
            binding: binding.clone(),
            owner: proof,
            uses: Arc::new(UseCounter::default()),
        };
        if let Err(error) = append_audit(
            store,
            &binding,
            "owned_localhost_lease_issued",
            serde_json::json!({"expires_unix": expires_unix}),
        ) {
            let mut owner = active.owner;
            if let Err(cleanup) = owner.terminate_and_confirm() {
                return Err(refused(format!(
                    "{error}; activation cleanup was not confirmed: {cleanup}"
                )));
            }
            return Err(error);
        }
        self.next_generation.insert((host, port), generation);
        self.active.insert(binding.lease_id, active);
        Ok(binding)
    }

    fn authorize_use(
        &mut self,
        store: &Store,
        presented: &OwnedLocalhostBinding,
        context: &OwnedLocalhostExecutionContext,
        now_unix: u64,
    ) -> Result<OwnedLocalhostUse> {
        let Some(active) = self.active.get_mut(&presented.lease_id) else {
            append_unbound_refusal(store, presented, "lease_not_active")?;
            return Err(refused("owned localhost lease is not active"));
        };
        if active.binding != *presented
            || !context.is_well_formed_for(&self.workspace_sha256)
            || !context.matches(&active.binding)
        {
            append_refusal(store, &active.binding, "lease_context_mismatch")?;
            return Err(refused("owned localhost lease context does not match"));
        }
        if now_unix >= active.binding.expires_unix {
            let lease_id = active.binding.lease_id;
            self.revoke(store, lease_id, OwnedLocalhostRevocationReason::Expired)?;
            return Err(refused("owned localhost lease expired"));
        }
        let live = active
            .owner
            .owner_mut()?
            .listener_is_live(active.binding.host, active.binding.port)
            .map_err(|error| error.to_string());
        if !matches!(live, Ok(true)) {
            let lease_id = active.binding.lease_id;
            self.revoke(
                store,
                lease_id,
                OwnedLocalhostRevocationReason::ListenerLost,
            )?;
            return Err(refused(match live {
                Ok(false) => "owned localhost listener is no longer live".into(),
                Err(error) => format!("listener liveness check failed: {error}"),
                Ok(true) => unreachable!(),
            }));
        }
        active.uses.acquire()?;
        let use_guard = OwnedLocalhostUse {
            binding: active.binding.clone(),
            counter: Arc::clone(&active.uses),
        };
        append_audit(
            store,
            &active.binding,
            "owned_localhost_lease_use_allowed",
            serde_json::json!({}),
        )?;
        Ok(use_guard)
    }

    fn revoke(
        &mut self,
        store: &Store,
        lease_id: Uuid,
        reason: OwnedLocalhostRevocationReason,
    ) -> Result<bool> {
        let Some(mut active) = self.active.remove(&lease_id) else {
            return Ok(false);
        };
        let reason = reason.as_str();
        let revoke_audit = append_audit(
            store,
            &active.binding,
            "owned_localhost_lease_revoked",
            serde_json::json!({"reason": reason}),
        );
        let drain = active.uses.revoke_and_wait();
        let cleanup = active.owner.terminate_and_confirm();
        let cleanup_audit = match cleanup {
            Ok(()) => append_audit(
                store,
                &active.binding,
                "owned_localhost_process_cleaned",
                serde_json::json!({
                    "reason": reason,
                    "revocation_audit_persisted": revoke_audit.is_ok(),
                    "use_drain_completed": drain.is_ok(),
                }),
            ),
            Err(error) => {
                let audit = append_audit(
                    store,
                    &active.binding,
                    "owned_localhost_process_cleanup_failed",
                    serde_json::json!({"reason": reason, "error": error.to_string()}),
                );
                self.cleanup_pending.insert(lease_id, active);
                audit?;
                return Err(refused(format!(
                    "owned process-tree cleanup was not confirmed and is retained for retry: {error}"
                )));
            }
        };
        revoke_audit?;
        drain?;
        cleanup_audit?;
        Ok(true)
    }

    fn revoke_many(
        &mut self,
        store: &Store,
        lease_ids: Vec<Uuid>,
        reason: OwnedLocalhostRevocationReason,
    ) -> Result<usize> {
        let mut revoked = 0;
        let mut first_error = None;
        for lease_id in lease_ids {
            match self.revoke(store, lease_id, reason) {
                Ok(did_revoke) => revoked += usize::from(did_revoke),
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(revoked),
        }
    }

    fn revoke_session(
        &mut self,
        store: &Store,
        session_id: Uuid,
        reason: OwnedLocalhostRevocationReason,
    ) -> Result<usize> {
        let lease_ids: Vec<Uuid> = self
            .active
            .values()
            .filter(|lease| lease.binding.session_id == session_id)
            .map(|lease| lease.binding.lease_id)
            .collect();
        self.revoke_many(store, lease_ids, reason)
    }

    fn sweep_expired(&mut self, store: &Store, now_unix: u64) -> Result<usize> {
        let lease_ids: Vec<Uuid> = self
            .active
            .values()
            .filter(|lease| now_unix >= lease.binding.expires_unix)
            .map(|lease| lease.binding.lease_id)
            .collect();
        self.revoke_many(store, lease_ids, OwnedLocalhostRevocationReason::Expired)
    }

    fn revoke_run(
        &mut self,
        store: &Store,
        run_id: Uuid,
        reason: OwnedLocalhostRevocationReason,
    ) -> Result<usize> {
        let lease_ids: Vec<Uuid> = self
            .active
            .values()
            .filter(|lease| lease.binding.run_id == run_id)
            .map(|lease| lease.binding.lease_id)
            .collect();
        self.revoke_many(store, lease_ids, reason)
    }

    fn revoke_all(
        &mut self,
        store: &Store,
        reason: OwnedLocalhostRevocationReason,
    ) -> Result<usize> {
        let lease_ids: Vec<Uuid> = self.active.keys().copied().collect();
        let result = self.revoke_many(store, lease_ids, reason);
        let retry_result = self.retry_pending_cleanup(store);
        match (result, retry_result) {
            (Ok(revoked), Ok(_)) => Ok(revoked),
            (Err(error), _) | (_, Err(error)) => Err(error),
        }
    }

    fn retry_pending_cleanup(&mut self, store: &Store) -> Result<usize> {
        let pending_ids: Vec<Uuid> = self.cleanup_pending.keys().copied().collect();
        let mut cleaned = 0;
        let mut first_error = None;
        for lease_id in pending_ids {
            let Some(mut pending) = self.cleanup_pending.remove(&lease_id) else {
                continue;
            };
            match pending.owner.terminate_and_confirm() {
                Ok(()) => {
                    cleaned += 1;
                    if let Err(error) = append_audit(
                        store,
                        &pending.binding,
                        "owned_localhost_process_cleaned",
                        serde_json::json!({"reason": "cleanup_retry"}),
                    ) {
                        if first_error.is_none() {
                            first_error = Some(error);
                        }
                    }
                }
                Err(error) => {
                    self.cleanup_pending.insert(lease_id, pending);
                    if first_error.is_none() {
                        first_error = Some(refused(format!(
                            "owned process-tree cleanup retry was not confirmed: {error}"
                        )));
                    }
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(cleaned),
        }
    }
}

fn append_audit(
    store: &Store,
    binding: &OwnedLocalhostBinding,
    kind: &str,
    detail: serde_json::Value,
) -> Result<()> {
    store
        .append_event(
            Some(binding.owner_job_id),
            None,
            kind,
            &serde_json::json!({
                "lease_id": binding.lease_id,
                "project_scope_id": binding.project_scope_id,
                "project_root_hash": binding.project_root_hash,
                "session_id": binding.session_id,
                "run_id": binding.run_id,
                "owner_job_id": binding.owner_job_id,
                "owner_attempt_id": binding.owner_attempt_id,
                "process_tree_id": binding.process_tree_id,
                "host": binding.host.to_string(),
                "port": binding.port,
                "generation": binding.generation,
                "expires_unix": binding.expires_unix,
                "detail": detail,
            }),
        )
        .map_err(GraphError::from)?;
    Ok(())
}

fn append_refusal(store: &Store, binding: &OwnedLocalhostBinding, reason: &str) -> Result<()> {
    append_audit(
        store,
        binding,
        "owned_localhost_lease_use_refused",
        serde_json::json!({"reason": reason}),
    )
}

fn append_unbound_refusal(
    store: &Store,
    binding: &OwnedLocalhostBinding,
    reason: &str,
) -> Result<()> {
    store
        .append_event(
            None,
            None,
            "owned_localhost_lease_use_refused",
            &serde_json::json!({
                "lease_id": binding.lease_id,
                "run_id": binding.run_id,
                "host": binding.host.to_string(),
                "port": binding.port,
                "generation": binding.generation,
                "detail": {"reason": reason},
            }),
        )
        .map_err(GraphError::from)?;
    Ok(())
}

fn refused(message: impl Into<String>) -> RuntimeError {
    RuntimeError::PolicyDenied {
        code: "owned_localhost_lease_refused".into(),
        reason: message.into(),
    }
}

impl Runtime {
    /// Activate authority from an opaque platform ownership proof.
    ///
    /// The only production caller is the structured serve effect in
    /// [`serve`](self::serve), which builds its [`VerifiedOwnedServer`] from a
    /// retained process tree it started and then proved holds the listener
    /// (ADR-0060 clause 3). The proof type has no public constructor, so no
    /// other crate can reach this with an unproven one.
    pub fn issue_verified_owned_localhost(
        &self,
        context: OwnedLocalhostExecutionContext,
        host: IpAddr,
        port: u16,
        ttl_seconds: u64,
        proof: VerifiedOwnedServer,
    ) -> Result<OwnedLocalhostBinding> {
        let now_unix = Self::now_unix()?;
        self.owned_localhost
            .lock()
            .map_err(|_| refused("owned localhost registry lock was poisoned"))?
            .activate(
                &self.store,
                LeaseActivation {
                    context,
                    host,
                    port,
                    ttl_seconds,
                    now_unix,
                    proof,
                },
            )
    }

    /// Resolve a serializable binding back to live runtime authority.
    ///
    /// This is intentionally stricter than shape validation in the pure policy
    /// broker. A copied, expired, revoked, foreign-context, stale-generation, or
    /// dead-listener binding is refused.
    pub fn authorize_owned_localhost_use(
        &self,
        binding: &OwnedLocalhostBinding,
        context: &OwnedLocalhostExecutionContext,
    ) -> Result<OwnedLocalhostUse> {
        let now_unix = Self::now_unix()?;
        self.owned_localhost
            .lock()
            .map_err(|_| refused("owned localhost registry lock was poisoned"))?
            .authorize_use(&self.store, binding, context, now_unix)
    }

    /// Revoke every lease belonging to one execution before its process handles
    /// are released.
    pub fn revoke_owned_localhost_run(&self, run_id: Uuid) -> Result<usize> {
        self.owned_localhost
            .lock()
            .map_err(|_| refused("owned localhost registry lock was poisoned"))?
            .revoke_run(
                &self.store,
                run_id,
                OwnedLocalhostRevocationReason::RunSettled,
            )
    }

    /// Revoke every lease owned by one session.
    pub fn revoke_owned_localhost_session(&self, session_id: Uuid) -> Result<usize> {
        self.owned_localhost
            .lock()
            .map_err(|_| refused("owned localhost registry lock was poisoned"))?
            .revoke_session(
                &self.store,
                session_id,
                OwnedLocalhostRevocationReason::SessionClosed,
            )
    }

    /// Deterministically reap leases whose deadline has passed.
    pub fn sweep_expired_owned_localhost(&self) -> Result<usize> {
        let now_unix = Self::now_unix()?;
        self.owned_localhost
            .lock()
            .map_err(|_| refused("owned localhost registry lock was poisoned"))?
            .sweep_expired(&self.store, now_unix)
    }

    /// Explicit single-lease revocation. Idempotent when already absent.
    pub fn revoke_owned_localhost_lease(&self, lease_id: Uuid) -> Result<bool> {
        self.owned_localhost
            .lock()
            .map_err(|_| refused("owned localhost registry lock was poisoned"))?
            .revoke(
                &self.store,
                lease_id,
                OwnedLocalhostRevocationReason::Explicit,
            )
    }

    /// Retry owners retained after a failed cleanup confirmation.
    pub fn retry_owned_localhost_cleanup(&self) -> Result<usize> {
        let retained = self
            .owned_localhost
            .lock()
            .map_err(|_| refused("owned localhost registry lock was poisoned"))?
            .retry_pending_cleanup(&self.store)?;
        Ok(retained + global::retry_failed_owners())
    }
}

pub(crate) fn registry_for(
    scope_key: String,
    workspace_sha256: String,
) -> Arc<Mutex<OwnedLocalhostLeaseRegistry>> {
    global::registry_for(scope_key, workspace_sha256)
}

pub(crate) fn revoke_all_on_drop(runtime: &mut Runtime) {
    global::release_registry(
        &runtime.owned_localhost_scope,
        &runtime.owned_localhost,
        &runtime.store,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex as StdMutex};

    struct FakeOwner {
        tree: String,
        live: bool,
        events: Arc<StdMutex<Vec<String>>>,
    }

    impl RetainedOwnedServer for FakeOwner {
        fn process_tree_id(&self) -> &str {
            &self.tree
        }

        fn listener_is_live(&mut self, _host: IpAddr, _port: u16) -> std::io::Result<bool> {
            Ok(self.live)
        }

        fn terminate_and_confirm(&mut self) -> std::io::Result<()> {
            self.events.lock().unwrap().push("cleaned".into());
            self.live = false;
            Ok(())
        }
    }

    struct FlakyCleanupOwner {
        attempts: Arc<AtomicUsize>,
    }

    impl RetainedOwnedServer for FlakyCleanupOwner {
        fn process_tree_id(&self) -> &str {
            "optimus-flaky.service"
        }

        fn listener_is_live(&mut self, _host: IpAddr, _port: u16) -> std::io::Result<bool> {
            Ok(true)
        }

        fn terminate_and_confirm(&mut self) -> std::io::Result<()> {
            if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(std::io::Error::other("injected cleanup failure"))
            } else {
                Ok(())
            }
        }
    }

    fn context(root: &str) -> OwnedLocalhostExecutionContext {
        OwnedLocalhostExecutionContext {
            project_scope_id: "project-a".into(),
            project_root_hash: root.into(),
            session_id: Uuid::from_u128(1),
            run_id: Uuid::from_u128(2),
            owner_job_id: Uuid::from_u128(3),
            owner_attempt_id: Uuid::from_u128(4),
        }
    }

    fn setup() -> (
        tempfile::TempDir,
        Store,
        OwnedLocalhostLeaseRegistry,
        String,
        Arc<StdMutex<Vec<String>>>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("runtime.db")).unwrap();
        optimus_graph::create_job_with_id(
            &store,
            crate::JobId(Uuid::from_u128(3)),
            optimus_graph::JobSpec {
                label: "owned localhost registry".into(),
                budget: Default::default(),
                nodes: vec![optimus_graph::NodeSpec {
                    label: "proof".into(),
                    effect: optimus_graph::Effect::AssertFileEquals {
                        relative_path: "proof".into(),
                        expected: String::new(),
                    },
                }],
            },
        )
        .unwrap();
        let root = "a".repeat(64);
        let registry =
            OwnedLocalhostLeaseRegistry::new(format!("test:{}", Uuid::new_v4()), root.clone());
        (
            dir,
            store,
            registry,
            root,
            Arc::new(StdMutex::new(Vec::new())),
        )
    }

    fn owner(events: Arc<StdMutex<Vec<String>>>) -> VerifiedOwnedServer {
        VerifiedOwnedServer::from_retained(Box::new(FakeOwner {
            tree: "optimus-owned-1.service".into(),
            live: true,
            events,
        }))
    }

    fn activation(
        context: OwnedLocalhostExecutionContext,
        host: IpAddr,
        now_unix: u64,
        proof: VerifiedOwnedServer,
    ) -> LeaseActivation {
        LeaseActivation {
            context,
            host,
            port: 4173,
            ttl_seconds: 30,
            now_unix,
            proof,
        }
    }

    #[test]
    fn active_membership_context_expiry_and_generation_are_fenced() {
        let (_dir, store, mut registry, root, events) = setup();
        let ctx = context(&root);
        let first = registry
            .activate(
                &store,
                activation(
                    ctx.clone(),
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    100,
                    owner(events.clone()),
                ),
            )
            .unwrap();
        assert_eq!(first.generation, 1);
        assert_eq!(
            registry
                .authorize_use(&store, &first, &ctx, 129)
                .unwrap()
                .binding(),
            &first
        );

        let mut foreign = ctx.clone();
        foreign.run_id = Uuid::from_u128(99);
        assert!(registry
            .authorize_use(&store, &first, &foreign, 129)
            .is_err());
        assert!(registry.authorize_use(&store, &first, &ctx, 130).is_err());
        assert_eq!(events.lock().unwrap().as_slice(), ["cleaned"]);
        assert!(registry.authorize_use(&store, &first, &ctx, 129).is_err());

        let second = registry
            .activate(
                &store,
                activation(
                    ctx.clone(),
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    200,
                    owner(events),
                ),
            )
            .unwrap();
        assert_eq!(second.generation, 2);
        assert_ne!(second.lease_id, first.lease_id);
        assert!(registry.authorize_use(&store, &first, &ctx, 201).is_err());
        assert!(registry.authorize_use(&store, &second, &ctx, 201).is_ok());
    }

    #[test]
    fn activation_fails_closed_for_foreign_root_dead_listener_and_bad_origin() {
        let (_dir, store, mut registry, root, events) = setup();
        let mut foreign = context(&root);
        foreign.project_root_hash = "b".repeat(64);
        assert!(registry
            .activate(
                &store,
                activation(
                    foreign,
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    100,
                    owner(events.clone()),
                ),
            )
            .is_err());
        assert!(registry
            .activate(
                &store,
                activation(
                    context(&root),
                    IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                    100,
                    owner(events.clone()),
                ),
            )
            .is_err());
        assert!(registry
            .activate(
                &store,
                activation(
                    context(&root),
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    100,
                    VerifiedOwnedServer::from_retained(Box::new(FakeOwner {
                        tree: "foreign.service".into(),
                        live: false,
                        events: events.clone(),
                    })),
                ),
            )
            .is_err());
        assert!(registry.active.is_empty());
        assert_eq!(
            events.lock().unwrap().as_slice(),
            ["cleaned", "cleaned", "cleaned"],
            "every refused proof must clean its retained owner"
        );
    }

    #[test]
    fn revoke_removes_authority_before_cleanup_and_audits_order() {
        let (_dir, store, mut registry, root, events) = setup();
        let binding = registry
            .activate(
                &store,
                activation(
                    context(&root),
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    100,
                    owner(events.clone()),
                ),
            )
            .unwrap();
        assert!(registry
            .revoke(
                &store,
                binding.lease_id,
                OwnedLocalhostRevocationReason::RunSettled,
            )
            .unwrap());
        assert!(!registry
            .revoke(
                &store,
                binding.lease_id,
                OwnedLocalhostRevocationReason::Explicit,
            )
            .unwrap());
        assert!(registry.active.is_empty());
        assert_eq!(events.lock().unwrap().as_slice(), ["cleaned"]);
        let kinds: Vec<String> = store
            .list_events(Some(binding.owner_job_id))
            .unwrap()
            .into_iter()
            .map(|event| event.kind)
            .filter(|kind| kind.starts_with("owned_localhost"))
            .collect();
        assert_eq!(
            kinds,
            [
                "owned_localhost_lease_issued",
                "owned_localhost_lease_revoked",
                "owned_localhost_process_cleaned",
            ]
        );
    }

    #[test]
    fn revocation_fences_new_uses_and_waits_for_existing_guard() {
        let counter = Arc::new(UseCounter::default());
        counter.acquire().unwrap();
        let worker = Arc::clone(&counter);
        let reaper = std::thread::spawn(move || worker.revoke_and_wait());

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if counter.state.lock().unwrap().revoking {
                break;
            }
            assert!(Instant::now() < deadline, "revocation did not begin");
            std::thread::yield_now();
        }
        assert!(counter.acquire().is_err(), "new use crossed revocation");
        assert!(!reaper.is_finished(), "revocation ignored the live use");
        counter.release();
        reaper.join().unwrap().unwrap();
    }

    #[test]
    fn registries_are_process_shared_for_the_same_workspace() {
        let root = format!("{:064x}", Uuid::new_v4().as_u128());
        let scope = format!("test:{root}");
        let first = registry_for(scope.clone(), root.clone());
        let second = registry_for(scope, root);
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn generation_and_expiry_overflow_fail_closed_and_clean_owner() {
        let (_dir, store, mut registry, root, events) = setup();
        registry
            .next_generation
            .insert((IpAddr::V4(Ipv4Addr::LOCALHOST), 4173), u64::MAX);
        assert!(registry
            .activate(
                &store,
                activation(
                    context(&root),
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    100,
                    owner(events.clone()),
                ),
            )
            .is_err());
        registry.next_generation.clear();
        assert!(registry
            .activate(
                &store,
                activation(
                    context(&root),
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    u64::MAX,
                    owner(events.clone()),
                ),
            )
            .is_err());
        assert!(registry.active.is_empty());
        assert_eq!(events.lock().unwrap().as_slice(), ["cleaned", "cleaned"]);
    }

    #[test]
    fn cleanup_failure_is_non_authorizable_and_retained_for_retry() {
        let (_dir, store, mut registry, root, events) = setup();
        let attempts = Arc::new(AtomicUsize::new(0));
        let binding = registry
            .activate(
                &store,
                activation(
                    context(&root),
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    100,
                    VerifiedOwnedServer::from_retained(Box::new(FlakyCleanupOwner {
                        attempts: Arc::clone(&attempts),
                    })),
                ),
            )
            .unwrap();
        assert!(registry
            .revoke(
                &store,
                binding.lease_id,
                OwnedLocalhostRevocationReason::Explicit,
            )
            .is_err());
        assert!(!registry.active.contains_key(&binding.lease_id));
        assert!(registry.cleanup_pending.contains_key(&binding.lease_id));
        assert!(registry
            .activate(
                &store,
                activation(
                    context(&root),
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    110,
                    owner(events.clone()),
                ),
            )
            .is_err());
        assert_eq!(events.lock().unwrap().as_slice(), ["cleaned"]);
        assert_eq!(registry.retry_pending_cleanup(&store).unwrap(), 1);
        assert!(registry.cleanup_pending.is_empty());
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert!(registry
            .activate(
                &store,
                activation(
                    context(&root),
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    120,
                    owner(events),
                ),
            )
            .is_ok());
    }

    #[test]
    fn invalid_activation_cleanup_failure_is_quarantined_for_retry() {
        let (_dir, store, mut registry, root, _events) = setup();
        let attempts = Arc::new(AtomicUsize::new(0));
        let mut invalid = context(&root);
        invalid.run_id = Uuid::nil();
        assert!(registry
            .activate(
                &store,
                activation(
                    invalid,
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    100,
                    VerifiedOwnedServer::from_retained(Box::new(FlakyCleanupOwner {
                        attempts: Arc::clone(&attempts),
                    })),
                ),
            )
            .is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        let rejected_events = Arc::new(StdMutex::new(Vec::new()));
        assert!(registry
            .activate(
                &store,
                activation(
                    context(&root),
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    101,
                    owner(Arc::clone(&rejected_events)),
                ),
            )
            .is_err());
        assert_eq!(rejected_events.lock().unwrap().as_slice(), ["cleaned"]);
        assert_eq!(global::retry_failed_owners(), 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }
}
