//! Spec-015 B2 acceptance: the CLI `chat` prompt is a client of `serve` by
//! default (attach-or-spawn via the B1 host client), and `--embedded` keeps
//! the in-process kernel path byte-for-byte unchanged.
//!
//! DoD (issue #137):
//! 1. A CLI prompt answers a question through a spawned serve.
//! 2. A second CLI attaches to the same serve with the record token.
//! 3. `--embedded` works unchanged in CI and headless (no serve spawned).
//!
//! Every test drives the shipped binary (`CARGO_BIN_EXE_optimus`) on a temp
//! home with API keys stripped, so `auto` resolves to the offline provider:
//! deterministic, no credentials, no network.

use std::net::TcpStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::tempdir;

/// A CLI command template: `optimus --home H <subcommand...>` with every
/// provider credential removed so routing settles on `offline`.
fn optimus_command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_optimus"));
    command
        .arg("--home")
        .arg(home)
        .env("OPTIMUS_SERVE_PORT", "0") // ephemeral; parallel tests never collide
        .env_remove("OPTIMUS_API_KEY")
        .env_remove("OPTIMUS_OPENAI_BASE_URL")
        .env_remove("OPTIMUS_OPENAI_API_KEY")
        .env_remove("OPENAI_API_KEY");
    command
}

fn run_chat(home: &Path, extra_args: &[&str]) -> std::process::Output {
    optimus_command(home)
        .args(["chat", "hello"])
        .args(extra_args)
        .output()
        .expect("run optimus chat")
}

/// The serve's runtime record, when one was written.
fn record_exists(home: &Path) -> bool {
    home.join("host-runtime.json").exists()
}

/// Wait for a healthy serve record: TCP probe the recorded port until the
/// serve answers or the deadline passes. `connect` polls the same record.
fn wait_for_healthy_record(home: &Path, deadline: Instant) -> Result<(), String> {
    let path = home.join("host-runtime.json");
    while Instant::now() < deadline {
        let Ok(text) = std::fs::read_to_string(&path) else {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };
        let Ok(record) = serde_json::from_str::<serde_json::Value>(&text) else {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };
        let Some(port) = record.get("port").and_then(serde_json::Value::as_u64) else {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };
        if probe(port as u16) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("serve record never became healthy".to_string())
}

/// Minimal TCP connect to the serve's port: the health probe's essence.
fn probe(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}

/// Test 1 (DoD): a CLI prompt answers through a spawned serve. No serve was
/// running, so the client spawned `optimus serve --stdio`, spoke the wire,
/// and the answer carries the offline echo. The leftover record proves a
/// serve actually ran — the embedded path writes none.
#[test]
fn cli_prompt_answers_through_a_spawned_serve() {
    let home = tempdir().expect("temporary Optimus home");
    let output = run_chat(home.path(), &[]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("offline echo: hello"), "{stdout}");
    assert!(stdout.contains("[provider=offline "), "{stdout}");
    assert!(
        record_exists(home.path()),
        "a spawned serve writes the runtime record; none was found in {}",
        home.path().display()
    );
}

/// Test 2 (DoD): a second CLI attaches to the same serve with the record
/// token. A plain `optimus serve` already holds the home; the CLI's spawn
/// attempt is refused (exit 3) and the B1 client falls back to a WebSocket
/// attach presenting the record token. The original serve must still be the
/// one serving — the record's pid is unchanged and the process is alive —
/// and the post-hello connections.log line proves the dial AND the
/// credential handshake completed.
#[test]
fn second_cli_attaches_to_the_same_serve_with_the_record_token() {
    let home = tempdir().expect("temporary Optimus home");
    let mut serve = Command::new(env!("CARGO_BIN_EXE_optimus"))
        .args([
            "serve",
            "--home",
            home.path().to_str().unwrap(),
            "--port",
            "0",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn optimus serve");
    let deadline = Instant::now() + Duration::from_secs(10);
    wait_for_healthy_record(home.path(), deadline).expect("plain serve must become healthy");

    // The CLI's spawned serve child sees the healthy holder and exits 3; the
    // client then attaches over WebSocket with the record token.
    let output = run_chat(home.path(), &[]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("offline echo: hello"), "{stdout}");

    // Attach, not spawn-and-replace: the recorded pid is still the original
    // serve, and the process is still alive.
    let text =
        std::fs::read_to_string(home.path().join("host-runtime.json")).expect("record must exist");
    let record: serde_json::Value = serde_json::from_str(&text).expect("record must parse");
    let pid = record.get("pid").and_then(serde_json::Value::as_u64);
    assert_eq!(
        pid,
        Some(serve.id() as u64),
        "record pid must be the original serve"
    );
    assert!(
        serve.try_wait().expect("try_wait").is_none(),
        "the original serve must still be running after an attach"
    );

    // Post-hello connections.log line: dial + credential handshake, R8.
    let log = home.path().join("logs").join("connections.log");
    let lines = std::fs::read_to_string(&log).expect("connections.log must exist");
    assert!(
        !lines.trim().is_empty(),
        "attach must log an accepted connection"
    );

    let _ = serve.kill();
    let _ = serve.wait();
}

/// Test 3 (DoD): `--embedded` keeps the in-process kernel path unchanged.
/// Same offline echo, no serve spawned (no runtime record written).
#[test]
fn embedded_flag_keeps_the_in_process_kernel() {
    let home = tempdir().expect("temporary Optimus home");
    let output = optimus_command(home.path())
        .args(["--embedded", "chat", "hello"])
        .output()
        .expect("run optimus chat --embedded");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("offline echo: hello"), "{stdout}");
    assert!(
        !record_exists(home.path()),
        "embedded mode opens the kernel in process; no serve record may appear"
    );
}
