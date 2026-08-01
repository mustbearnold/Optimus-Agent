//! ADR-0060 end-to-end: the structured project-serve effect is the only path
//! that mints an owned-localhost lease, and the lease dies with its run.
//!
//! These tests deliberately use real sockets and a real contained process tree.
//! The claim under test is physical — *this* listener belongs to *that* tree —
//! and a mocked owner would assert the mock rather than the claim.

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

use optimus_graph::{
    AutonomyProfile, Effect, JobId, JobSpec, JobStatus, NodeSpec, PolicyMode, RuntimeConfig,
};
use optimus_runtime::{ApprovalGrant, Runtime, RuntimeError};
use tempfile::TempDir;

/// A port the kernel just handed out and that nobody now holds.
///
/// Binding and releasing is the only way to name a free port without racing a
/// hardcoded one against whatever else runs on the machine.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

/// Arguments for a server that binds the port and then does nothing.
///
/// `python3` rather than a purpose-built fixture binary: it is already a hard
/// dependency of this repository's own gates, and the confined envelope
/// ro-binds `/usr`, so it is reachable inside the sandbox without widening it.
fn listen_forever(port: u16) -> Vec<String> {
    vec![
        "-c".into(),
        format!(
            "import socket, time; s = socket.socket(); \
             s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); \
             s.bind(('127.0.0.1', {port})); s.listen(8); time.sleep(120)"
        ),
    ]
}

fn runtime(profile: AutonomyProfile) -> (TempDir, Runtime) {
    let dir = TempDir::new().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let runtime = Runtime::open_with_config(
        &dir.path().join("o.db"),
        &ws,
        RuntimeConfig {
            policy: PolicyMode::SmartDeny,
            autonomy_profile: profile,
            ..Default::default()
        },
    )
    .unwrap();
    (dir, runtime)
}

fn serve_effect(root_hash: String, port: u16) -> Effect {
    Effect::ProjectServe {
        workspace_sha256: root_hash,
        program: "/usr/bin/python3".into(),
        args: listen_forever(port),
        port,
        ttl_seconds: 60,
    }
}

/// The receipt of the most recent effect that succeeded in this job.
///
/// Read from the store rather than from `StepOutcome`, because the receipt is
/// what a later session or an audit actually gets to see.
fn latest_succeeded_receipt(db: &Path, job: JobId) -> serde_json::Value {
    let conn = rusqlite::Connection::open(db).unwrap();
    let raw: String = conn
        .query_row(
            "SELECT receipt_json FROM effect_attempts
             WHERE job_id=?1 AND status='succeeded'
             ORDER BY attempt_no DESC LIMIT 1",
            rusqlite::params![job.0.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn reachable(port: u16) -> bool {
    TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(500),
    )
    .is_ok()
}

/// Wait briefly for the listener to disappear.
///
/// Revocation confirms the cgroup is empty before returning, so this normally
/// settles on the first poll; the deadline exists so a failure reports "still
/// reachable" instead of hanging.
fn wait_until_unreachable(port: u16) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if !reachable(port) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn read_only_refuses_a_serve_before_any_process_starts() {
    let (_dir, rt) = runtime(AutonomyProfile::ReadOnly);
    let port = free_port();
    let job = rt
        .create_job(JobSpec {
            label: "serve".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "dev server".into(),
                effect: serve_effect(rt.workspace_sha256(), port),
            }],
        })
        .unwrap();

    // Denial is terminal and loud: there is no approval that rescues a serve
    // under a profile that forbids command execution outright, so the step
    // errors rather than parking the job.
    let refusal = rt.run_all(job).unwrap_err();
    assert!(
        matches!(&refusal, RuntimeError::PolicyDenied { code, .. } if code == "read_only_profile"),
        "expected a read-only denial, got {refusal:?}"
    );
    assert!(!reachable(port), "a denied serve must never have started");
}

#[cfg(target_os = "linux")]
#[test]
fn review_changes_pauses_a_serve_and_leases_it_only_after_approval() {
    let (dir, rt) = runtime(AutonomyProfile::ReviewChanges);
    let port = free_port();
    let job = rt
        .create_job(JobSpec {
            label: "serve".into(),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "dev server".into(),
                effect: serve_effect(rt.workspace_sha256(), port),
            }],
        })
        .unwrap();

    assert_eq!(rt.run_all(job).unwrap(), JobStatus::AwaitingApproval);
    assert!(
        !reachable(port),
        "a serve awaiting approval must not have started its process"
    );

    rt.grant_approval(ApprovalGrant::for_job(job)).unwrap();
    assert_eq!(rt.run_all(job).unwrap(), JobStatus::Succeeded);

    let receipt = latest_succeeded_receipt(&dir.path().join("o.db"), job);
    assert_eq!(receipt["owned_localhost"]["port"], port);
    // A null authority id is not "unauthorized". Under Review changes the
    // broker answers `Ask` for the lease round too, and that ask was already
    // discharged by the approval that let this exact program, port and TTL
    // start at all — asking twice for one decision has no second answer.
    assert!(
        receipt["authority_id"].is_null(),
        "review changes discharges the lease ask through the effect approval"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn standard_leases_a_proven_listener_and_revokes_it_when_the_run_settles() {
    let (dir, rt) = runtime(AutonomyProfile::Standard);
    let port = free_port();
    let root_hash = rt.workspace_sha256();
    // A second node keeps the run alive past the serve, which is the only
    // window in which a lease is meant to be usable.
    let job = rt
        .create_job(JobSpec {
            label: "serve".into(),
            budget: Default::default(),
            nodes: vec![
                NodeSpec {
                    label: "dev server".into(),
                    effect: serve_effect(root_hash.clone(), port),
                },
                NodeSpec {
                    label: "note".into(),
                    effect: Effect::ProjectWriteFile {
                        workspace_sha256: root_hash.clone(),
                        relative_path: "served.txt".into(),
                        contents: "ok\n".into(),
                    },
                },
            ],
        })
        .unwrap();

    rt.run_next(job).unwrap();
    // Pending, not terminal: the run that owns the lease is still going, which
    // is precisely the condition under which the lease is meant to hold.
    assert_eq!(rt.job_status(job).unwrap(), JobStatus::Pending);
    assert!(
        reachable(port),
        "the leased listener must actually answer while its run is live"
    );

    let receipt = latest_succeeded_receipt(&dir.path().join("o.db"), job);
    let binding = &receipt["owned_localhost"];
    assert_eq!(binding["scheme"], "http");
    assert_eq!(binding["host"], "127.0.0.1");
    assert_eq!(binding["port"], port);
    assert_eq!(binding["project_root_hash"], root_hash);
    // The process tree id is the transient unit whose cgroup was proven to hold
    // the listening socket, not a pid the child reported about itself.
    let tree = binding["process_tree_id"].as_str().unwrap();
    assert!(
        tree.starts_with("optimus-command-") && tree.ends_with(".service"),
        "lease is bound to a retained transient unit, got {tree}"
    );
    assert!(
        receipt["authority_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "standard allows the lease outright and must record which authority did"
    );

    assert_eq!(rt.run_all(job).unwrap(), JobStatus::Succeeded);
    assert!(
        wait_until_unreachable(port),
        "a settled run must take its listener down with it"
    );
}
