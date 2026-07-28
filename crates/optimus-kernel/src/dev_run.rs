//! Session entry point for one engineering run (ADR-0052 §2).

use std::path::Path;

use uuid::Uuid;

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
    pub fn open_dev_run_session(
        home: impl AsRef<Path>,
        config: KernelConfig,
        session_id: Option<Uuid>,
        project_id: &str,
        worktree: &Path,
    ) -> Result<Self> {
        Self::open_session_with_project(home, config, session_id, Some(project_id), Some(worktree))
    }
}
