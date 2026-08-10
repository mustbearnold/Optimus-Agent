//! Session-scoped capability consent (spec-014 R7, ADR-0081).
//!
//! These runtime methods surface the store's `capability_grants` API to the
//! host routes and the settlement auto-grant path. The capability and scope
//! are always derived server-side: the class discriminator maps to exactly
//! one capability, and the scope is pinned to `workspace_sha256()` so a
//! grant minted in one project can never authorize effects in another.

use optimus_policy::CommandClass;

use crate::{GraphError, Result, Runtime, RuntimeError};

impl Runtime {
    /// Grant (or renew) "always allow <class> in this project (this
    /// session)". The capability is derived from the class server-side; the
    /// scope is pinned to `workspace_sha256()`.
    pub fn grant_session_consent(
        &self,
        session_id: &str,
        command_class: &str,
        ttl_secs: u64,
    ) -> Result<optimus_store::CapabilityGrantRow> {
        let class = CommandClass::parse(command_class).ok_or_else(|| {
            RuntimeError::NotRunnable(format!("unknown command class: {command_class}"))
        })?;
        let scope = self.workspace_sha256();
        Ok(self
            .store
            .grant_capability(
                session_id,
                class.capability().as_str(),
                class.as_str(),
                &scope,
                ttl_secs,
                Self::now_unix()?,
            )
            .map_err(GraphError::from)?)
    }

    /// Soft-revoke one live session consent. Returns true if a live grant
    /// existed for the exact key.
    pub fn revoke_session_consent(&self, session_id: &str, command_class: &str) -> Result<bool> {
        let class = CommandClass::parse(command_class).ok_or_else(|| {
            RuntimeError::NotRunnable(format!("unknown command class: {command_class}"))
        })?;
        let scope = self.workspace_sha256();
        Ok(self
            .store
            .revoke_capability(
                session_id,
                class.capability().as_str(),
                class.as_str(),
                &scope,
                "operator",
                Self::now_unix()?,
                "session_consent_revoke",
            )
            .map_err(GraphError::from)?)
    }

    /// Revoke every live consent for a session (the settings affordance).
    /// Returns the number of grants revoked.
    pub fn revoke_session_consents(&self, session_id: &str) -> Result<u64> {
        Ok(self
            .store
            .revoke_session_capabilities(session_id, "operator", Self::now_unix()?, "settings")
            .map_err(GraphError::from)?)
    }

    /// All consent rows for a session (live and revoked), newest first.
    pub fn list_session_consents(
        &self,
        session_id: &str,
    ) -> Result<Vec<optimus_store::CapabilityGrantRow>> {
        Ok(self
            .store
            .list_capability_grants(session_id)
            .map_err(GraphError::from)?)
    }

    /// Exact-effect audit rows, newest first (ADR-0044 law 6 evidence).
    pub fn list_action_approvals(
        &self,
        limit: usize,
    ) -> Result<Vec<optimus_store::ActionApprovalRow>> {
        Ok(self
            .store
            .list_action_approvals(limit)
            .map_err(GraphError::from)?)
    }
}
