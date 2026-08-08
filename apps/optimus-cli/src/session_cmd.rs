//! Shared session-command plumbing for the CLI (spec-034): the
//! `parse_session`/`open_session` pair is the allowlisted core-state
//! entry; `with_session` keeps the goal and children subcommands to a
//! single arm each.

use std::path::Path;

use optimus_kernel::Kernel;

use crate::open_session;

/// Parse the optional `--session` argument.
pub(crate) fn parse_session(
    session: Option<String>,
) -> Result<Option<uuid::Uuid>, Box<dyn std::error::Error>> {
    match session {
        None => Ok(None),
        Some(s) => Ok(Some(uuid::Uuid::parse_str(&s)?)),
    }
}

/// Open the given session and run `body` against its kernel.
pub(crate) fn with_session<R>(
    home: &Path,
    session: Option<String>,
    body: impl FnOnce(&mut Kernel) -> Result<R, Box<dyn std::error::Error>>,
) -> Result<R, Box<dyn std::error::Error>> {
    let sid = parse_session(session)?;
    let mut kernel = open_session(home, sid)?;
    body(&mut kernel)
}
