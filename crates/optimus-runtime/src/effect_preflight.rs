//! Effect shape validation that runs before SmartDeny and before execution.
//!
//! This is deliberately upstream of the capability broker: an effect whose
//! shape is illegal must never reach a human as an approval prompt, because
//! approving it would be approving something the runtime will refuse anyway.
//!
//! Split out of `lib.rs` so the runtime waist stays under the 800-line module
//! law (AGENTS.md law 21) instead of growing its grandfathered baseline.

use std::path::{Path, PathBuf};

use crate::{is_secret_basename, Effect, Result, Runtime, RuntimeError};

impl Runtime {
    /// Validate effect shape before SmartDeny wait or execution.
    pub(crate) fn preflight_effect(&self, effect: &Effect) -> Result<()> {
        match effect {
            Effect::WriteFile { relative_path, .. }
            | Effect::AssertFileEquals { relative_path, .. }
            | Effect::ProjectWriteFile { relative_path, .. }
            | Effect::Mkdir { relative_path, .. }
            | Effect::ProjectMkdir { relative_path, .. }
            | Effect::DeletePath { relative_path, .. }
            | Effect::ProjectDeletePath { relative_path, .. }
            | Effect::PatchFile { relative_path, .. }
            | Effect::ProjectPatchFile { relative_path, .. } => {
                self.safe_relative_path(relative_path)?;
            }
            Effect::RenamePath {
                from_relative_path,
                to_relative_path,
                ..
            }
            | Effect::ProjectRenamePath {
                from_relative_path,
                to_relative_path,
                ..
            } => {
                self.safe_relative_path(from_relative_path)?;
                self.safe_relative_path(to_relative_path)?;
            }
            Effect::RunCommand { program, .. } | Effect::ProjectRunCommand { program, .. } => {
                if program.trim().is_empty() {
                    return Err(RuntimeError::Effector("empty command program".into()));
                }
            }
            Effect::ProjectServe { program, port, .. } => {
                if program.trim().is_empty() {
                    return Err(RuntimeError::Effector("empty command program".into()));
                }
                // Port 0 means "whatever the kernel gives you", which cannot be
                // stated in advance and so cannot be what a human approved. It
                // is also rejected by `OwnedLocalhostBinding::is_valid_for`, so
                // catching it here keeps a doomed serve from ever starting.
                if *port == 0 {
                    return Err(RuntimeError::Effector(
                        "project serve requires an explicit port".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn safe_relative_path(&self, relative: &str) -> Result<PathBuf> {
        if relative.is_empty() {
            return Err(RuntimeError::PathEscape("empty path".into()));
        }
        let rel = Path::new(relative);
        if rel.is_absolute() {
            return Err(RuntimeError::PathEscape(relative.into()));
        }
        for comp in rel.components() {
            if !matches!(comp, std::path::Component::Normal(_)) {
                return Err(RuntimeError::PathEscape(relative.into()));
            }
        }
        if rel
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_secret_basename)
        {
            return Err(RuntimeError::PathEscape(relative.into()));
        }
        Ok(rel.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Effect, Runtime, RuntimeError};

    fn runtime() -> (tempfile::TempDir, Runtime) {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Runtime::open(&dir.path().join("g.db"), &dir.path().join("ws")).unwrap();
        (dir, runtime)
    }

    fn serve(program: &str, port: u16) -> Effect {
        Effect::ProjectServe {
            workspace_sha256: "0".repeat(64),
            program: program.into(),
            args: Vec::new(),
            port,
            ttl_seconds: 30,
        }
    }

    #[test]
    fn a_serve_without_a_program_is_refused_before_policy() {
        let (_dir, runtime) = runtime();
        assert!(matches!(
            runtime.preflight_effect(&serve("   ", 4173)),
            Err(RuntimeError::Effector(_))
        ));
    }

    #[test]
    fn a_serve_on_port_zero_is_refused_before_policy() {
        let (_dir, runtime) = runtime();
        assert!(matches!(
            runtime.preflight_effect(&serve("node", 0)),
            Err(RuntimeError::Effector(_))
        ));
    }

    #[test]
    fn a_well_formed_serve_passes_preflight() {
        let (_dir, runtime) = runtime();
        runtime.preflight_effect(&serve("node", 4173)).unwrap();
    }
}
