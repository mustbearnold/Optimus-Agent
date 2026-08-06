//! Serve capability-probe tests (spec-015 A5/R1/R8) — the named executor
//! for `optimus serve`'s exit-code and diagnostic pins.
//!
//! This is an integration test of the package that DEFINES the `optimus`
//! bin: Cargo sets `CARGO_BIN_EXE_optimus` only for tests of the bin's own
//! package, so a test inside optimus-host cannot read it. The suite:
//!   - pins the probe premise: the built binary's `serve --help` exits 0;
//!   - spawns the built binary against (i) an occupied port → exit 2,
//!     (iia) an http-holder home → exit 3 + the HTTP-mode refusal
//!     diagnostic, (iib) a ws-holder home → exit 3 + the ws-mode refusal
//!     diagnostic, (iii) a record-write failure (a directory pre-created at
//!     the record path) → exit 2, (iv) a fresh home → binds and writes
//!     record v2/ws within the readiness bound, (v) a stdio shell-kind
//!     hello without the process secret → stderr diagnostic + exit 2.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The built `optimus` bin (this package's `[[bin]] optimus`): Cargo sets
/// `CARGO_BIN_EXE_optimus` at test RUNTIME for tests of the bin's own
/// package — a test inside optimus-host cannot read it, which is exactly
/// why this suite lives here.
fn built_bin() -> String {
    std::env::var("CARGO_BIN_EXE_optimus").expect("CARGO_BIN_EXE_optimus at runtime")
}

fn serve_command(home: &Path, port: u16, stdio: bool) -> Command {
    let mut command = Command::new(built_bin());
    command
        .arg("serve")
        .arg("--home")
        .arg(home)
        .arg("--port")
        .arg(port.to_string())
        .env_remove("OPTIMUS_NATIVE_SELECTION_TOKEN")
        .env_remove("OPTIMUS_OFFLINE_LATENCY_MS")
        .env_remove("OPTIMUS_SERVE_HELLO_TIMEOUT_MS");
    if stdio {
        command.arg("--stdio");
    }
    command
}

fn spawn(home: &Path, port: u16, stdio: bool, stdin: Stdio) -> Child {
    let mut command = serve_command(home, port, stdio);
    command
        .stdin(stdin)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command.spawn().expect("spawn optimus serve")
}

fn stderr_of(child: &mut Child) -> String {
    let mut buffer = String::new();
    child
        .stderr
        .take()
        .expect("stderr piped")
        .read_to_string(&mut buffer)
        .expect("read stderr");
    buffer
}

fn wait_exit(child: &mut Child, bound: Duration) -> i32 {
    let deadline = Instant::now() + bound;
    loop {
        if let Some(status) = child.try_wait().expect("wait") {
            return status.code().expect("exit code");
        }
        assert!(
            Instant::now() < deadline,
            "serve did not exit within {bound:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_record(home: &Path, bound: Duration) {
    let deadline = Instant::now() + bound;
    loop {
        if optimus_host::record_path(home).exists() {
            return;
        }
        assert!(Instant::now() < deadline, "no record within {bound:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// One-request loopback server speaking just enough HTTP for the health
/// probe (the `host_runtime.rs` test pattern): accepts once, reads the
/// request head, answers 200 with `{"ok":true}`.
fn scripted_health_server() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut head = [0u8; 4096];
            let _ = stream.read(&mut head);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"ok\":true}",
            );
            let _ = stream.flush();
        }
    });
    port
}

/// The probe premise (R8): `serve --help` exits 0 — the capability probe
/// is exit-code based, never text-matched.
#[test]
fn serve_help_exits_zero() {
    let output = Command::new(built_bin())
        .args(["serve", "--help"])
        .output()
        .expect("run serve --help");
    assert_eq!(output.status.code(), Some(0), "serve --help must exit 0");
}

/// (i) An occupied port: bind-failure → exit 2 (ADR-0083: bind-2 is a
/// change from the old HTTP mode's exit 1).
#[test]
fn occupied_port_exits_2() {
    let holder = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = holder.local_addr().expect("addr").port();
    let home = tempfile::tempdir().expect("tempdir");
    let mut child = spawn(home.path(), port, false, Stdio::null());
    let code = wait_exit(&mut child, Duration::from_secs(10));
    let stderr = stderr_of(&mut child);
    assert_eq!(code, 2, "occupied port → exit 2; stderr: {stderr}");
}

/// (iia) An http-holder home (scripted health server + v1/http record):
/// refusal → exit 3 with the HTTP-mode diagnostic (R1/R8).
#[test]
fn http_holder_refusal_exits_3_with_http_diagnostic() {
    let port = scripted_health_server();
    let home = tempfile::tempdir().expect("tempdir");
    let record = serde_json::json!({
        "version": 1,
        "port": port,
        "pid": 1234,
        "token": "t".repeat(40),
    });
    std::fs::write(
        optimus_host::record_path(home.path()),
        serde_json::to_vec_pretty(&record).expect("record json"),
    )
    .expect("write v1/http record");

    let mut child = spawn(home.path(), 0, false, Stdio::null());
    let code = wait_exit(&mut child, Duration::from_secs(10));
    let stderr = stderr_of(&mut child);
    assert_eq!(code, 3, "http holder → exit 3; stderr: {stderr}");
    assert!(
        stderr.contains("a host is already serving this home in HTTP mode"),
        "http refusal diagnostic: {stderr}"
    );
}

/// (iib) A ws-holder home: the natural (iv)→(iib) sequence — the first
/// serve binds and writes record v2/ws; a second serve on the same home
/// refuses with the ws-mode diagnostic.
#[test]
fn ws_holder_refusal_exits_3_with_ws_diagnostic() {
    let home = tempfile::tempdir().expect("tempdir");
    let mut holder = spawn(home.path(), 0, false, Stdio::null());
    wait_for_record(home.path(), Duration::from_secs(10));
    let record = optimus_host::read_record(home.path()).expect("record");
    assert_eq!(record.version, 2);
    assert_eq!(record.transport_label(), "ws");

    let mut child = spawn(home.path(), 0, false, Stdio::null());
    let code = wait_exit(&mut child, Duration::from_secs(10));
    let stderr = stderr_of(&mut child);
    assert_eq!(code, 3, "ws holder → exit 3; stderr: {stderr}");
    assert!(
        stderr.contains("a host is already serving this home in ws mode"),
        "ws refusal diagnostic: {stderr}"
    );
    let _ = holder.kill();
    let _ = holder.wait();
}

/// (iii) A record-write failure (a directory pre-created at the record
/// path, so the atomic rename fails after bind) → exit 2.
#[test]
fn record_write_failure_exits_2() {
    let home = tempfile::tempdir().expect("tempdir");
    let record_dir = optimus_host::record_path(home.path());
    std::fs::create_dir_all(&record_dir).expect("pre-create record path as directory");

    let mut child = spawn(home.path(), 0, false, Stdio::null());
    let code = wait_exit(&mut child, Duration::from_secs(10));
    let stderr = stderr_of(&mut child);
    assert_eq!(code, 2, "record-write failure → exit 2; stderr: {stderr}");
    assert!(
        stderr.contains("cannot write host-runtime record"),
        "record-write diagnostic: {stderr}"
    );
}

/// (iv) A fresh home: serve binds and writes record v2/ws within the
/// readiness bound, and the record's port answers the Bearer-gated health
/// probe (the serve is ipso facto capable, R8).
#[test]
fn fresh_home_binds_and_writes_record_v2_ws() {
    let home = tempfile::tempdir().expect("tempdir");
    let mut child = spawn(home.path(), 0, false, Stdio::null());
    wait_for_record(home.path(), Duration::from_secs(10));
    let record = optimus_host::read_record(home.path()).expect("record");
    assert_eq!(record.version, 2, "record v2 on a fresh home");
    assert_eq!(record.transport_label(), "ws", "record transport ws");
    assert!(
        child.try_wait().expect("try_wait").is_none(),
        "serve keeps serving"
    );

    let mut stream = std::net::TcpStream::connect(("127.0.0.1", record.port)).expect("connect");
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let request = format!(
        "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\
         Authorization: Bearer {}\r\n\
         Origin: http://127.0.0.1:{}\r\n\
         Connection: close\r\n\r\n",
        record.port, record.token, record.port
    );
    stream.write_all(request.as_bytes()).expect("write probe");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read probe");
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "health 200: {response}"
    );
    assert!(response.contains("\"ok\":true"), "health ok: {response}");

    let _ = child.kill();
    let _ = child.wait();
}

/// (v) A stdio shell-kind hello without the process secret: stderr
/// diagnostic + exit 2 (R5/R7 — pipe ownership is not a credential).
#[test]
fn stdio_shell_kind_without_secret_exits_2() {
    let home = tempfile::tempdir().expect("tempdir");
    let mut child = spawn(home.path(), 0, true, Stdio::piped());
    let mut stdin = child.stdin.take().expect("stdin piped");
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"hello","params":{{"protocol_version":1,"client_kind":"shell","ticket":"whatever"}}}}"#
    )
    .expect("write hello");
    drop(stdin);

    let code = wait_exit(&mut child, Duration::from_secs(10));
    let stderr = stderr_of(&mut child);
    assert_eq!(
        code, 2,
        "shell-kind over stdio without secret → exit 2; stderr: {stderr}"
    );
    assert!(
        stderr.contains("shell-kind hello rejected over stdio"),
        "stdio rejection diagnostic: {stderr}"
    );
}
