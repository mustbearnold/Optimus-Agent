//! Session entry point for one engineering run (ADR-0052 §2).

use std::path::Path;

use optimus_graph::AutonomyProfile;
use uuid::Uuid;

use crate::project_trust::ProjectTrustStore;
use crate::{Kernel, KernelConfig, Result};

impl Kernel {
    /// Open a session bound to one engineering run's isolated checkout.
    ///
    /// The worktree replaces the project's roots outright, so a run's tools
    /// reach their own checkout and nothing else — not a sibling run, and not
    /// the main checkout the human is using.
    ///
    /// The worktree must already lie strictly inside a root the user
    /// authorized for `project_id`. A run cannot nominate its own boundary,
    /// which is what would let a run record widen the authority it was given.
    ///
    /// If the project carries a live trust grant, the run adopts its profile.
    /// This is the one place that happens automatically, and deliberately so:
    /// "routine engineering work inside a worktree the user already authorized"
    /// is exactly the case a standing grant is *for*. Chat sessions keep
    /// choosing per turn.
    pub fn open_dev_run_session(
        home: impl AsRef<Path>,
        mut config: KernelConfig,
        session_id: Option<Uuid>,
        project_id: &str,
        worktree: &Path,
    ) -> Result<Self> {
        let home = home.as_ref();
        if let Some(profile) = granted_profile(home, project_id)? {
            config.autonomy_profile = profile;
        }
        Self::open_session_with_project(home, config, session_id, Some(project_id), Some(worktree))
    }
}

/// The profile a durable grant allows this project, if any.
///
/// A store that cannot be read is not treated as "no grant": that would turn a
/// corrupt or unreadable file into a silent authority change, and the direction
/// of the change would depend on which way the caller happened to default. The
/// error propagates instead.
fn granted_profile(home: &Path, project_id: &str) -> Result<Option<AutonomyProfile>> {
    ProjectTrustStore::open(home)?.effective_profile(project_id)
}
