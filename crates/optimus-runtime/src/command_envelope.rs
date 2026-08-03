//! Command capability envelope builders (Linux bwrap profiles, platform gates).

use std::path::{Path, PathBuf};

use optimus_graph::CommandFsEnvelope;

/// Paths that must exist and be ro-bound when present for a usable userspace.
const LINUX_RO_CANDIDATES: &[&str] = &[
    "/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/opt", "/nix",
];

/// Trees the confined profile replaces with a fresh tmpfs.
///
/// Whatever the child still needs from one of these has to be re-bound *after*
/// the tmpfs is mounted, because the tmpfs hides everything underneath it.
const LINUX_TMPFS_TREES: &[&str] = &["/tmp", "/var", "/run"];

/// Re-bind arguments for the resolver configuration, or empty if unnecessary.
///
/// `/etc` is ro-bound, so a `/etc/resolv.conf` that is a plain file arrives
/// intact and this does nothing. The common Linux case is a symlink into
/// `/run` (systemd-resolved's stub) — and the confined profile mounts a tmpfs
/// over `/run`, which erases the target.
///
/// The failure that causes is quiet and expensive. `Confined` shares the
/// network namespace, so the child *has* a network and every hostname fails to
/// resolve: `curl` exits non-zero into a pipeline that ends in `head`, the
/// pipeline exits 0, and the effect is recorded as succeeded with empty stdout.
/// A model reading that receipt is told the command worked.
fn linux_resolver_bind(target: Option<&Path>) -> Vec<String> {
    let Some(target) = target else {
        // A resolver path that will not canonicalise is already broken on the
        // host; the sandbox is not the thing to fix in that case.
        return Vec::new();
    };
    if !LINUX_TMPFS_TREES
        .iter()
        .any(|tree| target.starts_with(tree))
    {
        return Vec::new();
    }
    let path = target.to_string_lossy().into_owned();
    vec!["--ro-bind".into(), path.clone(), path]
}

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
    linux_bwrap_args_with_roots(workspace, envelope, &[])
}

/// Build a confined envelope with additional explicitly selected writable
/// roots. The workspace remains the current directory; extra roots only widen
/// the bind set for Developer Full Access and are never used for ordinary
/// profiles.
pub fn linux_bwrap_args_with_roots(
    workspace: &Path,
    envelope: CommandFsEnvelope,
    extra_roots: &[PathBuf],
) -> Vec<String> {
    let mut args: Vec<String> = vec!["--die-with-parent".into(), "--unshare-pid".into()];
    let explicit_whole_machine_root = extra_roots.iter().any(|root| root == Path::new("/"));
    if envelope.linux_unshare_net() {
        args.push("--unshare-net".into());
    }

    match envelope {
        CommandFsEnvelope::UnrestrictedHost if explicit_whole_machine_root => {
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
        CommandFsEnvelope::Confined | CommandFsEnvelope::ConfinedNoNetwork
            if explicit_whole_machine_root =>
        {
            // Entire-local-machine Developer Full Access with networking
            // disabled uses the confined envelope solely for its network
            // namespace. The explicit `/` root remains an intentional full
            // filesystem bind.
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
            // After the tmpfs, never before: the mount would hide it. Skipped
            // when the network is unshared, where there is nothing to resolve.
            if !envelope.linux_unshare_net() {
                args.extend(linux_resolver_bind(
                    std::fs::canonicalize("/etc/resolv.conf").ok().as_deref(),
                ));
            }
            for dir in parent_dirs_for_bind(workspace) {
                args.push("--dir".into());
                args.push(dir.to_string_lossy().into_owned());
            }
            let ws = workspace.to_string_lossy().into_owned();
            args.extend(["--bind".into(), ws.clone(), ws]);
            for root in extra_roots {
                if root == workspace || !root.is_absolute() || root == Path::new("/") {
                    continue;
                }
                for dir in parent_dirs_for_bind(root) {
                    args.push("--dir".into());
                    args.push(dir.to_string_lossy().into_owned());
                }
                let root = root.to_string_lossy().into_owned();
                args.extend(["--bind".into(), root.clone(), root]);
            }
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

impl crate::Runtime {
    /// How far a spawned command can reach. Read by the kernel so the system
    /// prompt can state it: an agent that does not know its commands have no
    /// network spends a whole turn rediscovering it, one approval at a time.
    ///
    /// Lives here rather than beside the other `Runtime` accessors because
    /// `lib.rs` is at its module-size baseline and may only shrink, and this is
    /// the module that owns what the answer means.
    pub fn command_fs_envelope(&self) -> CommandFsEnvelope {
        self.config.command_fs_envelope
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
    fn explicit_machine_root_preserves_full_fs_when_network_is_disabled() {
        let ws = PathBuf::from("/tmp/ws");
        let args = linux_bwrap_args_with_roots(
            &ws,
            CommandFsEnvelope::ConfinedNoNetwork,
            &[PathBuf::from("/")],
        );
        assert!(args.iter().any(|arg| arg == "--unshare-net"));
        assert!(args.windows(3).any(|w| w == ["--bind", "/", "/"]));
    }

    #[test]
    fn a_resolver_under_a_tmpfs_tree_is_rebound() {
        let args = linux_resolver_bind(Some(Path::new("/run/systemd/resolve/stub-resolv.conf")));
        assert_eq!(
            args,
            vec![
                "--ro-bind".to_string(),
                "/run/systemd/resolve/stub-resolv.conf".to_string(),
                "/run/systemd/resolve/stub-resolv.conf".to_string(),
            ]
        );
    }

    #[test]
    fn a_resolver_that_survives_the_profile_is_left_alone() {
        // A plain /etc/resolv.conf arrives via the /etc ro-bind; re-binding it
        // would be noise, and noise in an argv is a thing to get wrong later.
        assert!(linux_resolver_bind(Some(Path::new("/etc/resolv.conf"))).is_empty());
        assert!(linux_resolver_bind(None).is_empty());
    }

    #[test]
    fn a_tmpfs_prefix_match_is_by_component_not_by_text() {
        assert!(linux_resolver_bind(Some(Path::new("/tmpfoo/resolv.conf"))).is_empty());
    }

    #[test]
    fn the_resolver_rebind_lands_after_the_tmpfs_that_would_hide_it() {
        // Order is the whole fix. bwrap applies operations in argv order, so a
        // ro-bind of /run/... placed before `--tmpfs /run` is erased by it and
        // the child silently loses DNS again.
        let ws = PathBuf::from("/tmp/ws");
        let args = linux_bwrap_args(&ws, CommandFsEnvelope::Confined);
        let Some(resolver) = args.iter().position(|a| a.starts_with("/run/")) else {
            // Hosts whose resolv.conf is a plain file have nothing to re-bind.
            return;
        };
        let tmpfs_run = args
            .windows(2)
            .position(|w| w[0] == "--tmpfs" && w[1] == "/run")
            .expect("the confined profile mounts a tmpfs over /run");
        assert!(
            resolver > tmpfs_run,
            "the resolver bind must follow the tmpfs, got {args:?}"
        );
    }

    #[test]
    fn an_unshared_network_gets_no_resolver() {
        // Nothing to resolve, so binding a resolver would only widen the
        // profile past what the envelope promises.
        let ws = PathBuf::from("/tmp/ws");
        let args = linux_bwrap_args(&ws, CommandFsEnvelope::ConfinedNoNetwork);
        assert!(
            !args.iter().any(|a| a.contains("resolv.conf")),
            "got {args:?}"
        );
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
