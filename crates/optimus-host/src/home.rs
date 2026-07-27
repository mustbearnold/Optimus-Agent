//! Where a surface keeps its durable state, resolved the same way everywhere.
//!
//! The credential store lives under this directory, so two surfaces that
//! disagree about it are two surfaces that need separate logins. Resolution
//! lives in the host rather than in each binary for exactly that reason: the
//! desktop and the TUI cannot drift apart if they cannot each hold an opinion.
//!
//! A relative path is resolved against the working directory, which is what
//! makes an unqualified default dangerous — `.optimus` follows the shell around
//! and a `cd` silently becomes a different install with no saved login. The
//! default is therefore absolute and user-scoped.

use std::path::{Path, PathBuf};

/// Resolve the home directory for a surface, given whatever the caller passed.
///
/// In precedence order: an explicit argument, then `OPTIMUS_HOME`, then the
/// platform's local data directory. An explicit relative argument is still
/// honoured — a caller that asks for a working-directory home has said so — and
/// only the *absent* case is user-scoped.
pub fn resolve_home(explicit: Option<&str>) -> PathBuf {
    if let Some(path) = explicit.map(str::trim).filter(|path| !path.is_empty()) {
        return absolutise(Path::new(path));
    }
    if let Some(env) = std::env::var("OPTIMUS_HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return absolutise(Path::new(env.trim()));
    }
    // The fallback is only reached where the platform reports no data
    // directory at all. It is relative, and so carries the drift this module
    // exists to prevent — but a surface that cannot start is worse.
    dirs::data_local_dir().map_or_else(|| PathBuf::from(".optimus"), |data| data.join("optimus"))
}

/// Anchor a relative path to the working directory, so what is stored is never
/// re-resolved later against a directory the process has since left.
fn absolutise(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(path)
}

#[cfg(test)]
mod tests {
    use super::{absolutise, resolve_home};
    use std::path::{Path, PathBuf};

    #[test]
    fn an_explicit_absolute_path_is_taken_as_given() {
        assert_eq!(
            resolve_home(Some("/srv/optimus")),
            PathBuf::from("/srv/optimus")
        );
    }

    #[test]
    fn an_explicit_relative_path_is_anchored_not_left_floating() {
        // Honoured, because asking for it is saying so — but pinned now, so it
        // does not re-resolve if the process later changes directory.
        let resolved = resolve_home(Some("scratch-home"));
        assert!(resolved.is_absolute(), "{resolved:?} must be anchored");
        assert!(resolved.ends_with("scratch-home"));
    }

    #[test]
    fn blank_arguments_are_treated_as_absent() {
        // A shell expanding an unset variable produces an empty argument; that
        // is an omission, not a request for the working directory.
        assert_eq!(resolve_home(Some("   ")), resolve_home(None));
    }

    #[test]
    fn the_default_is_absolute_so_it_cannot_follow_the_shell() {
        // The whole point: a `cd` must not change which install you are in, or
        // which credential store your login was written to.
        let home = resolve_home(None);
        assert!(
            home.is_absolute(),
            "an unqualified home must not be working-directory relative: {home:?}"
        );
    }

    #[test]
    fn absolutising_an_absolute_path_leaves_it_alone() {
        assert_eq!(
            absolutise(Path::new("/var/lib/optimus")),
            PathBuf::from("/var/lib/optimus")
        );
    }
}
