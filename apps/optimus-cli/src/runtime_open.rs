//! Runtime opener honouring the global `--yolo` flag (ADR-0044).
//!
//! Lives outside `main.rs` because that module is the largest entry in
//! `docs/architecture/module-size-baseline.json` and may only shrink.

use std::path::Path;

use optimus_runtime::{AutonomyProfile, PolicyMode, Runtime, RuntimeConfig};

/// Open a runtime for a CLI subcommand.
///
/// Without `--yolo` this is the ordinary SmartDeny runtime. With it, the
/// invocation runs under `UnrestrictedHost` *and* releases any approval that is
/// already open — typing yolo at a pause is a request to unblock the thing on
/// screen. Each release still writes a durable receipt naming the yolo actor, so
/// the exact-action trail survives even though the human gate does not.
pub fn open_runtime(
    db: &Path,
    workspace: &Path,
    yolo: bool,
) -> Result<Runtime, Box<dyn std::error::Error>> {
    if !yolo {
        return Ok(Runtime::open(db, workspace)?);
    }
    let runtime = Runtime::open_with_config(
        db,
        workspace,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            autonomy_profile: AutonomyProfile::UnrestrictedHost,
            ..Default::default()
        },
    )?;
    let released = runtime.release_open_approvals_under_yolo()?;
    if released > 0 {
        eprintln!("yolo: released {released} open approval(s)");
    }
    Ok(runtime)
}
