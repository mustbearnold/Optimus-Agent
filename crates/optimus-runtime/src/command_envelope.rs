//! Command capability envelope builders (Linux bwrap profiles, platform gates).

use std::path::{Path, PathBuf};

use optimus_graph::CommandFsEnvelope;

/// Paths that must exist and be ro-bound when present for a usable userspace.
const LINUX_RO_CANDIDATES: &[&str] = &[
    "/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/opt", "/nix",
];

/// Build parent directories that must exist inside the sandbox for `workspace`
/// to be reachable at the same absolute path.
pub fn parent_dirs_for_bind(workspace: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut cur = workspace.to_path_buf();
    while let Some(parent) = cur.parent() {
        if parent.as_os_str().is_empty() || parent == Path::new("/") {
            break;
        }
        dirs.push(parent.to_path_buf());
        cur = parent.to_path_buf();
    }
    dirs.reverse();
    dirs
}

/// Linux bwrap argument vector **after** `/usr/bin/bwrap` and **before** `-- program`.
///
/// Confined modes: workspace is the only host path bound read-write. System
/// trees are ro-bind when present. No full-root `--bind / /`.
pub fn linux_bwrap_args(workspace: &Path, envelope: CommandFsEnvelope) -> Vec<String> {
    let mut args: Vec<String> = vec!["--die-with-parent".into(), "--unshare-pid".into()];
    if envelope.linux_unshare_net() {
        args.push("--unshare-net".into());
    }

    match envelope {
        CommandFsEnvelope::UnrestrictedHost => {
            args.extend([
                "--bind".into(),
                "/".into(),
                "/".into(),
                "--dev-bind".into(),
                "/dev".into(),
                "/dev".into(),
                "--proc".into(),
                "/proc".into(),
            ]);
        }
        CommandFsEnvelope::Confined | CommandFsEnvelope::ConfinedNoNetwork => {
            for path in LINUX_RO_CANDIDATES {
                let p = Path::new(path);
                if p.exists() {
                    args.extend(["--ro-bind".into(), path.to_string(), path.to_string()]);
                }
            }
            args.extend([
                "--dev".into(),
                "/dev".into(),
                "--proc".into(),
                "/proc".into(),
                "--tmpfs".into(),
                "/tmp".into(),
                "--tmpfs".into(),
                "/var".into(),
                "--tmpfs".into(),
                "/run".into(),
            ]);
            for dir in parent_dirs_for_bind(workspace) {
                args.push("--dir".into());
                args.push(dir.to_string_lossy().into_owned());
            }
            let ws = workspace.to_string_lossy().into_owned();
            args.extend(["--bind".into(), ws.clone(), ws]);
        }
    }

    args.push("--chdir".into());
    args.push(workspace.to_string_lossy().into_owned());
    args
}

/// Whether this host can spawn RunCommand under the given envelope.
pub fn command_envelope_supported(os: &str, envelope: CommandFsEnvelope) -> Result<(), String> {
    match (os, envelope) {
        ("linux", _) => Ok(()),
        ("windows", CommandFsEnvelope::ConfinedNoNetwork) => Err(
            "command envelope confined_no_network is fail-closed on Windows: \
             no AppContainer FS/network sandbox is implemented; use Shared/ProjectBound \
             (confined process-tree only) or explicit unrestricted_host break-glass"
                .into(),
        ),
        ("windows", CommandFsEnvelope::Confined | CommandFsEnvelope::UnrestrictedHost) => Ok(()),
        (other, CommandFsEnvelope::ConfinedNoNetwork) => Err(format!(
            "command envelope confined_no_network is fail-closed on {other}: \
             only Linux provides network-namespace unshare for commands"
        )),
        (other, CommandFsEnvelope::Confined) => Err(format!(
            "command FS confinement (workspace-only writable) is not implemented on {other}; \
             refuse spawn rather than claim a false envelope"
        )),
        (_, CommandFsEnvelope::UnrestrictedHost) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use optimus_graph::CommandFsEnvelope;
    use std::path::PathBuf;

    #[test]
    fn parent_dirs_are_outer_to_inner() {
        let ws = PathBuf::from("/home/u/proj/ws");
        let dirs = parent_dirs_for_bind(&ws);
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/home"),
                PathBuf::from("/home/u"),
                PathBuf::from("/home/u/proj"),
            ]
        );
    }

    #[test]
    fn confined_profile_never_full_root_bind() {
        let ws = PathBuf::from("/tmp/optimus-ws-test");
        let args = linux_bwrap_args(&ws, CommandFsEnvelope::Confined);
        let joined = args.join(" ");
        assert!(!joined.contains("--bind / /"), "{joined}");
        assert!(args.windows(3).any(|w| {
            w[0] == "--bind" && w[1].contains("optimus-ws-test") && w[2].contains("optimus-ws-test")
        }));
        assert!(args.iter().any(|a| a == "--ro-bind"));
    }

    #[test]
    fn no_network_adds_unshare_net() {
        let ws = PathBuf::from("/tmp/ws");
        let args = linux_bwrap_args(&ws, CommandFsEnvelope::ConfinedNoNetwork);
        assert!(args.iter().any(|a| a == "--unshare-net"));
    }

    #[test]
    fn unrestricted_host_uses_full_root_bind() {
        let ws = PathBuf::from("/tmp/ws");
        let args = linux_bwrap_args(&ws, CommandFsEnvelope::UnrestrictedHost);
        assert!(args.windows(3).any(|w| w == ["--bind", "/", "/"]));
    }

    #[test]
    fn windows_no_network_fail_closed() {
        assert!(command_envelope_supported("windows", CommandFsEnvelope::Confined).is_ok());
        assert!(
            command_envelope_supported("windows", CommandFsEnvelope::ConfinedNoNetwork).is_err()
        );
        assert!(command_envelope_supported("linux", CommandFsEnvelope::ConfinedNoNetwork).is_ok());
    }
}
