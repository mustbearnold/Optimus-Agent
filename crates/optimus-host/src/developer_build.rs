//! Building a development Optimus binary for the supervisor.
//!
//! Split out of `developer.rs` under the ADR-0049 module-size ratchet. The
//! build runs in the granted workspace and its output path is re-checked
//! against the grant before the supervisor is allowed to launch it.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use optimus_policy::DeveloperAccessGrant;

use crate::developer::{
    assert_in_scope, rotate_log_if_needed, supervisor_dir, validate_surface, BUILD_LOG_FILE,
};

pub(crate) fn build_development_binary(
    home: &Path,
    workspace: &Path,
    grant: &DeveloperAccessGrant,
    surface: &str,
) -> Result<String, String> {
    validate_surface(surface)?;
    let (package, binary_name, output_name) = match surface {
        "host" => ("optimus-desktop", "optimus-desktop", "optimus-desktop"),
        "desktop" => ("optimus-tauri", "optimus-agent", "optimus-agent"),
        _ => unreachable!("surface was validated above"),
    };
    let started = Instant::now();
    let mut command = Command::new("cargo");
    command.args(["build", "--locked", "-p", package, "--bin", binary_name]);
    if surface == "desktop" {
        // The child must be independently launchable after the current app
        // remains in place, so embed the already-built React bundle instead
        // of relying on a dev server owned by the current window.
        command.args(["--features", "optimus-tauri/custom-protocol"]);
    }
    let output = command
        .current_dir(workspace)
        .env("CARGO_TERM_COLOR", "never")
        .output();
    let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            append_build_log(
                home,
                workspace,
                surface,
                duration_ms,
                None,
                "",
                &format!("could not start cargo: {error}"),
            )?;
            return Err(format!("could not start development build: {error}"));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    append_build_log(
        home,
        workspace,
        surface,
        duration_ms,
        output.status.code(),
        &stdout,
        &stderr,
    )?;
    if !output.status.success() {
        return Err(format!(
            "development build failed with {}; see {}",
            output.status,
            supervisor_dir(home).join(BUILD_LOG_FILE).display()
        ));
    }

    let raw_binary = workspace
        .join("target")
        .join("debug")
        .join(if cfg!(windows) {
            format!("{output_name}.exe")
        } else {
            output_name.to_owned()
        });
    let binary = fs::canonicalize(&raw_binary)
        .map_err(|error| format!("development build did not produce {output_name}: {error}"))?;
    if !binary.is_file() {
        return Err(format!(
            "development build did not produce a regular {output_name} binary"
        ));
    }
    assert_in_scope(grant, &binary, "development binary")?;
    Ok(binary.display().to_string())
}

pub(crate) fn append_build_log(
    home: &Path,
    workspace: &Path,
    surface: &str,
    duration_ms: u64,
    status: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<(), String> {
    let dir = supervisor_dir(home);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(BUILD_LOG_FILE);
    rotate_log_if_needed(&path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    writeln!(
        file,
        "build workspace={} surface={surface} status={status:?} duration_ms={duration_ms}",
        workspace.display(),
    )
    .map_err(|error| error.to_string())?;
    if !stdout.trim().is_empty() {
        writeln!(file, "stdout:\n{}", stdout.trim_end()).map_err(|error| error.to_string())?;
    }
    if !stderr.trim().is_empty() {
        writeln!(file, "stderr:\n{}", stderr.trim_end()).map_err(|error| error.to_string())?;
    }
    Ok(())
}
