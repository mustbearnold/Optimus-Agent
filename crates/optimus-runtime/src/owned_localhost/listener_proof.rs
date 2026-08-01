//! Platform proof that a listener belongs to an Optimus-owned process tree
//! (ADR-0060 clause 4).
//!
//! The claim a lease rests on is narrow and physical: *this* socket, in LISTEN
//! on `127.0.0.1:<port>`, is held open by a file descriptor in a process that
//! is a member of the cgroup of the transient unit this runtime started. Each
//! link in that chain is read from the kernel rather than inferred:
//!
//!   1. `systemctl --user show <unit> --property=ControlGroup` — the unit's cgroup
//!   2. `<cgroup>/cgroup.procs` — the host pids it currently owns
//!   3. `/proc/net/tcp` — the inode of the socket listening on that address
//!   4. `/proc/<pid>/fd/*` — which of those pids holds that inode
//!
//! Nothing here trusts a port number, a pid the child reported, or the fact
//! that a connect() succeeded: another process can win the port after ours dies
//! and would answer a connection test identically. Ownership is the whole point
//! of the lease, so ownership is what gets proven.
//!
//! Non-Linux hosts have no equivalent proof implemented and fail closed.

use super::RetainedOwnedServer;

#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::io;
use std::net::IpAddr;
#[cfg(target_os = "linux")]
use std::net::Ipv4Addr;
use std::path::Path;
#[cfg(target_os = "linux")]
use std::process::Command;

use optimus_graph::CommandFsEnvelope;

/// Start a contained long-running server and retain ownership of its tree.
///
/// The returned owner kills the tree when dropped, so every failure path
/// between here and lease activation reaps the process rather than orphaning it.
#[cfg(target_os = "linux")]
pub(super) fn spawn_retained_serve(
    program: &str,
    args: &[String],
    workspace: &Path,
    envelope: CommandFsEnvelope,
) -> io::Result<Box<dyn RetainedOwnedServer>> {
    let (mut command, unit) =
        crate::process_ownership::linux_contained_serve_command(program, args, workspace, envelope);
    crate::sanitize_command_environment(&mut command);
    let guard = crate::process_ownership::KillOnDrop::spawn(&mut command, unit.clone())?;
    Ok(Box::new(LinuxRetainedServer { unit, guard }))
}

#[cfg(not(target_os = "linux"))]
pub(super) fn spawn_retained_serve(
    program: &str,
    args: &[String],
    workspace: &Path,
    envelope: CommandFsEnvelope,
) -> std::io::Result<Box<dyn RetainedOwnedServer>> {
    let _ = (program, args, workspace, envelope);
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!(
            "project serve is disabled on {}: no listener-ownership proof is implemented, \
             and an unproven lease is exactly what ADR-0060 forbids",
            std::env::consts::OS
        ),
    ))
}

#[cfg(target_os = "linux")]
struct LinuxRetainedServer {
    unit: String,
    guard: crate::process_ownership::KillOnDrop,
}

#[cfg(target_os = "linux")]
impl RetainedOwnedServer for LinuxRetainedServer {
    fn process_tree_id(&self) -> &str {
        &self.unit
    }

    fn listener_is_live(&mut self, host: IpAddr, port: u16) -> io::Result<bool> {
        // ADR-0060 leases are IPv4 loopback only; anything else has no proof
        // path here and must not be reported as owned.
        let IpAddr::V4(host) = host else {
            return Ok(false);
        };
        let Some(control_group) = unit_control_group(&self.unit)? else {
            return Ok(false);
        };
        let pids = cgroup_pids(&control_group)?;
        if pids.is_empty() {
            return Ok(false);
        }
        let inodes = listening_socket_inodes(host, port)?;
        if inodes.is_empty() {
            return Ok(false);
        }
        Ok(pids.iter().any(|pid| pid_holds_socket(*pid, &inodes)))
    }

    fn terminate_and_confirm(&mut self) -> io::Result<()> {
        self.guard.kill_and_reap()
    }
}

/// The unit's cgroup path, or `None` once systemd no longer has one for it.
#[cfg(target_os = "linux")]
fn unit_control_group(unit: &str) -> io::Result<Option<String>> {
    let output = Command::new("/usr/bin/systemctl")
        .args(["--user", "show", unit, "--property=ControlGroup"])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "systemctl show {unit}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let shown = String::from_utf8_lossy(&output.stdout);
    let Some(value) = shown
        .lines()
        .find_map(|line| line.strip_prefix("ControlGroup="))
    else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() || !value.starts_with('/') || value.contains("..") {
        return Ok(None);
    }
    Ok(Some(value.to_string()))
}

/// Host pids currently in the unit's cgroup.
///
/// A vanished cgroup means the tree is gone, which is a dead listener rather
/// than a probe failure.
#[cfg(target_os = "linux")]
fn cgroup_pids(control_group: &str) -> io::Result<Vec<u32>> {
    let path = format!(
        "/sys/fs/cgroup{}/cgroup.procs",
        control_group.trim_end_matches('/')
    );
    match fs::read_to_string(&path) {
        Ok(listed) => Ok(listed
            .lines()
            .filter_map(|line| line.trim().parse::<u32>().ok())
            .collect()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

/// Socket inodes in LISTEN on the exact address.
///
/// `/proc/net/tcp` renders the local address as little-endian hex, so
/// `127.0.0.1:4173` is `0100007F:104D`.
#[cfg(target_os = "linux")]
fn listening_socket_inodes(host: Ipv4Addr, port: u16) -> io::Result<Vec<u64>> {
    const TCP_LISTEN: &str = "0A";
    const INODE_FIELDS_AFTER_STATE: usize = 5;

    let octets = host.octets();
    let wanted = format!(
        "{:02X}{:02X}{:02X}{:02X}:{port:04X}",
        octets[3], octets[2], octets[1], octets[0]
    );
    let table = fs::read_to_string("/proc/net/tcp")?;
    let mut inodes = Vec::new();
    for line in table.lines().skip(1) {
        let mut fields = line.split_whitespace();
        let (Some(_index), Some(local), Some(_remote), Some(state)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if local != wanted || state != TCP_LISTEN {
            continue;
        }
        if let Some(inode) = fields
            .nth(INODE_FIELDS_AFTER_STATE)
            .and_then(|field| field.parse::<u64>().ok())
        {
            inodes.push(inode);
        }
    }
    Ok(inodes)
}

/// Whether this pid has one of those sockets open.
///
/// A pid that disappears mid-scan is not an owner, so unreadable `/proc` entries
/// are simply not matches.
#[cfg(target_os = "linux")]
fn pid_holds_socket(pid: u32, inodes: &[u64]) -> bool {
    let Ok(entries) = fs::read_dir(format!("/proc/{pid}/fd")) else {
        return false;
    };
    entries.flatten().any(|entry| {
        fs::read_link(entry.path())
            .ok()
            .and_then(|target| {
                target
                    .to_str()?
                    .strip_prefix("socket:[")?
                    .strip_suffix(']')?
                    .parse::<u64>()
                    .ok()
            })
            .is_some_and(|inode| inodes.contains(&inode))
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn loopback_addresses_render_as_little_endian_proc_hex() {
        // Exercised through the public probe on a real listener in
        // tests/owned_localhost_serve.rs; this pins the encoding itself.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let inodes = listening_socket_inodes(Ipv4Addr::new(127, 0, 0, 1), port).unwrap();
        assert!(
            !inodes.is_empty(),
            "own listener must appear in /proc/net/tcp"
        );
        assert!(
            pid_holds_socket(std::process::id(), &inodes),
            "the test process holds the socket it just bound"
        );
    }

    #[test]
    fn an_unbound_port_has_no_listening_inode() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(listening_socket_inodes(Ipv4Addr::new(127, 0, 0, 1), port)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_missing_cgroup_reads_as_an_empty_tree_not_an_error() {
        assert!(cgroup_pids("/optimus/does-not-exist").unwrap().is_empty());
    }

    #[test]
    fn a_collected_unit_has_no_control_group() {
        assert_eq!(
            unit_control_group("optimus-command-absent-0.service").unwrap(),
            None
        );
    }
}
