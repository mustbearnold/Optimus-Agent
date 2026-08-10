//! Leased child-agent campaign steps (S7.3–S7.5).
//!
//! Provides fail-closed leases, cancel propagation to child invocations, and a
//! hard parallel fan-out budget N≤k.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use optimus_runtime::CancellationToken;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChildLeaseError {
    #[error("lease not found: {0}")]
    NotFound(String),
    #[error("lease owned by another principal")]
    OwnerMismatch,
    #[error("lease expired")]
    Expired,
    #[error("parallel fan-out limit exceeded: {active} >= {max}")]
    FanOutLimit { active: usize, max: usize },
    #[error("child already terminal")]
    AlreadyTerminal,
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, ChildLeaseError>;

/// Default max concurrent leased children (S7.5).
pub const DEFAULT_MAX_PARALLEL_CHILDREN: usize = 4;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChildStatus {
    Leased,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl ChildStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChildLease {
    pub lease_id: Uuid,
    pub campaign_id: String,
    pub specialist_id: String,
    pub owner: String,
    pub generation: u64,
    pub token: Uuid,
    pub deadline_unix: u64,
    pub status: ChildStatus,
    pub handoff_artifact: Option<String>,
    pub cancel_token: String,
}

#[derive(Debug, Default)]
struct Inner {
    leases: BTreeMap<Uuid, ChildLease>,
    /// campaign_id -> active (non-terminal) lease count
    active_by_campaign: BTreeMap<String, usize>,
}

/// In-process leased child coordinator (product S7 MVP).
#[derive(Debug, Clone, Default)]
pub struct ChildLeaseCoordinator {
    inner: Arc<Mutex<Inner>>,
    max_parallel: usize,
}

impl ChildLeaseCoordinator {
    pub fn new(max_parallel: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            max_parallel: max_parallel.max(1),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_MAX_PARALLEL_CHILDREN)
    }

    pub fn max_parallel(&self) -> usize {
        self.max_parallel
    }

    pub fn lease_child(
        &self,
        campaign_id: &str,
        specialist_id: &str,
        owner: &str,
        lease_secs: u64,
    ) -> Result<(ChildLease, CancellationToken)> {
        let mut g = self
            .inner
            .lock()
            .map_err(|e| ChildLeaseError::Msg(e.to_string()))?;
        let active = g.active_by_campaign.get(campaign_id).copied().unwrap_or(0);
        if active >= self.max_parallel {
            return Err(ChildLeaseError::FanOutLimit {
                active,
                max: self.max_parallel,
            });
        }
        let now = now_unix();
        let cancel = CancellationToken::new();
        let lease = ChildLease {
            lease_id: Uuid::new_v4(),
            campaign_id: campaign_id.into(),
            specialist_id: specialist_id.into(),
            owner: owner.into(),
            generation: 1,
            token: Uuid::new_v4(),
            deadline_unix: now.saturating_add(lease_secs.max(1)),
            status: ChildStatus::Leased,
            handoff_artifact: None,
            cancel_token: format!("{:p}", &cancel as *const _),
        };
        g.leases.insert(lease.lease_id, lease.clone());
        *g.active_by_campaign.entry(campaign_id.into()).or_insert(0) += 1;
        Ok((lease, cancel))
    }

    pub fn mark_running(&self, lease_id: Uuid, owner: &str, token: Uuid) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|e| ChildLeaseError::Msg(e.to_string()))?;
        let lease = g
            .leases
            .get_mut(&lease_id)
            .ok_or_else(|| ChildLeaseError::NotFound(lease_id.to_string()))?;
        self.authorize(lease, owner, token)?;
        if lease.status.is_terminal() {
            return Err(ChildLeaseError::AlreadyTerminal);
        }
        lease.status = ChildStatus::Running;
        Ok(())
    }

    pub fn complete(
        &self,
        lease_id: Uuid,
        owner: &str,
        token: Uuid,
        handoff_artifact: Option<String>,
    ) -> Result<ChildLease> {
        self.finish(
            lease_id,
            owner,
            token,
            ChildStatus::Succeeded,
            handoff_artifact,
        )
    }

    pub fn fail(&self, lease_id: Uuid, owner: &str, token: Uuid) -> Result<ChildLease> {
        self.finish(lease_id, owner, token, ChildStatus::Failed, None)
    }

    pub fn cancel(
        &self,
        lease_id: Uuid,
        owner: &str,
        token: Uuid,
        cancel: &CancellationToken,
    ) -> Result<ChildLease> {
        cancel.cancel();
        self.finish(lease_id, owner, token, ChildStatus::Cancelled, None)
    }

    /// Cancel all active children for a campaign (parent cancel propagation).
    pub fn cancel_campaign(
        &self,
        campaign_id: &str,
        tokens: &BTreeMap<Uuid, CancellationToken>,
    ) -> Result<usize> {
        let mut g = self
            .inner
            .lock()
            .map_err(|e| ChildLeaseError::Msg(e.to_string()))?;
        let mut n = 0;
        for lease in g.leases.values_mut() {
            if lease.campaign_id != campaign_id || lease.status.is_terminal() {
                continue;
            }
            if let Some(t) = tokens.get(&lease.lease_id) {
                t.cancel();
            }
            lease.status = ChildStatus::Cancelled;
            n += 1;
        }
        g.active_by_campaign.insert(campaign_id.into(), 0);
        Ok(n)
    }

    pub fn get(&self, lease_id: Uuid) -> Result<ChildLease> {
        let g = self
            .inner
            .lock()
            .map_err(|e| ChildLeaseError::Msg(e.to_string()))?;
        g.leases
            .get(&lease_id)
            .cloned()
            .ok_or_else(|| ChildLeaseError::NotFound(lease_id.to_string()))
    }

    pub fn active_count(&self, campaign_id: &str) -> usize {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        g.active_by_campaign.get(campaign_id).copied().unwrap_or(0)
    }

    fn finish(
        &self,
        lease_id: Uuid,
        owner: &str,
        token: Uuid,
        status: ChildStatus,
        handoff: Option<String>,
    ) -> Result<ChildLease> {
        let mut g = self
            .inner
            .lock()
            .map_err(|e| ChildLeaseError::Msg(e.to_string()))?;
        let lease = g
            .leases
            .get_mut(&lease_id)
            .ok_or_else(|| ChildLeaseError::NotFound(lease_id.to_string()))?;
        self.authorize(lease, owner, token)?;
        if lease.status.is_terminal() {
            return Err(ChildLeaseError::AlreadyTerminal);
        }
        let was_active = !lease.status.is_terminal();
        lease.status = status;
        lease.handoff_artifact = handoff;
        let campaign = lease.campaign_id.clone();
        let out = lease.clone();
        if was_active {
            if let Some(c) = g.active_by_campaign.get_mut(&campaign) {
                *c = c.saturating_sub(1);
            }
        }
        Ok(out)
    }

    fn authorize(&self, lease: &ChildLease, owner: &str, token: Uuid) -> Result<()> {
        if lease.owner != owner || lease.token != token {
            return Err(ChildLeaseError::OwnerMismatch);
        }
        if now_unix() > lease.deadline_unix && !lease.status.is_terminal() {
            return Err(ChildLeaseError::Expired);
        }
        Ok(())
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_complete_with_handoff_and_fanout_limit() {
        let coord = ChildLeaseCoordinator::new(2);
        let (a, _ca) = coord
            .lease_child("camp-1", "workspace_writer", "owner", 60)
            .unwrap();
        let (b, _cb) = coord
            .lease_child("camp-1", "workspace_reader", "owner", 60)
            .unwrap();
        assert!(coord
            .lease_child("camp-1", "workspace_writer", "owner", 60)
            .is_err());
        coord.mark_running(a.lease_id, "owner", a.token).unwrap();
        let done = coord
            .complete(a.lease_id, "owner", a.token, Some("sha256:abc".into()))
            .unwrap();
        assert_eq!(done.status, ChildStatus::Succeeded);
        assert_eq!(done.handoff_artifact.as_deref(), Some("sha256:abc"));
        // After complete, a third lease fits
        let (_c, _) = coord
            .lease_child("camp-1", "workspace_writer", "owner", 60)
            .unwrap();
        // Wrong owner cannot complete b
        assert!(coord.complete(b.lease_id, "other", b.token, None).is_err());
    }

    #[test]
    fn cancel_propagates_to_child_token() {
        let coord = ChildLeaseCoordinator::with_defaults();
        let (lease, token) = coord
            .lease_child("camp-x", "workspace_writer", "owner", 30)
            .unwrap();
        assert!(!token.is_cancelled());
        coord
            .cancel(lease.lease_id, "owner", lease.token, &token)
            .unwrap();
        assert!(token.is_cancelled());
        let got = coord.get(lease.lease_id).unwrap();
        assert_eq!(got.status, ChildStatus::Cancelled);
    }

    #[test]
    fn a_terminal_lease_cannot_be_rerun_or_recompleted() {
        // Fail-closed invariant: once a child settles, no owner can move it
        // again. Re-running a completed child would double-execute its side
        // effects, and re-completing it would let a stale attempt claim a
        // fresh success (double-charging handoff artifacts downstream). Both
        // must report `AlreadyTerminal`, never a silent Ok.
        let coord = ChildLeaseCoordinator::with_defaults();
        let (lease, _token) = coord
            .lease_child("camp-t", "workspace_writer", "owner", 60)
            .unwrap();
        coord
            .complete(lease.lease_id, "owner", lease.token, None)
            .unwrap();

        // A stale re-run is refused even though owner + token are still valid...
        assert_eq!(
            coord.mark_running(lease.lease_id, "owner", lease.token),
            Err(ChildLeaseError::AlreadyTerminal)
        );
        // ...and so is a re-completion claiming another success.
        assert_eq!(
            coord.complete(lease.lease_id, "owner", lease.token, None),
            Err(ChildLeaseError::AlreadyTerminal)
        );
        // The lease stays settled and the campaign frees its slot.
        assert_eq!(
            coord.get(lease.lease_id).unwrap().status,
            ChildStatus::Succeeded
        );
        assert_eq!(coord.active_count("camp-t"), 0);
    }

    #[test]
    fn cancel_campaign_clears_all_active() {
        let coord = ChildLeaseCoordinator::new(4);
        let (a, ta) = coord.lease_child("c", "w", "o", 30).unwrap();
        let (b, tb) = coord.lease_child("c", "r", "o", 30).unwrap();
        let mut map = BTreeMap::new();
        map.insert(a.lease_id, ta);
        map.insert(b.lease_id, tb);
        let n = coord.cancel_campaign("c", &map).unwrap();
        assert_eq!(n, 2);
        assert!(map[&a.lease_id].is_cancelled());
        assert!(map[&b.lease_id].is_cancelled());
        assert_eq!(coord.active_count("c"), 0);
    }
}
