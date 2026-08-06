//! The shell-kind staging relay (spec-015 A4/A3, R7/R12): the shell
//! calls `project_root_stage_native` over ITS OWN client-kind:"shell"
//! wire connection, presenting the process secret (ADR-0084) instead of
//! the renderer's dial ticket. Serve-side, the shell-gated class
//! injects the process secret into the stage call (dispatch.rs), so
//! `os.rs:88-92`'s per-call constant-time check passes unchanged.
//!
//! The renderer flow is untouched: pick_folder still returns the grant
//! token; only the staging step moves from the in-process store write to
//! the wire.

use std::time::Duration;

use serde_json::{json, Value};
use tungstenite::client::IntoClientRequest;
use tungstenite::Message;

const HELLO_REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// Stage a native project root over the shell-kind wire connection.
/// `secret` is the shell-minted process secret (the same value delivered
/// to serve via `PROCESS_SECRET_ENV` at spawn, ADR-0084).
pub fn stage_native_root(port: u16, secret: &str, path: &str) -> Result<Value, String> {
    let url = format!("ws://127.0.0.1:{port}/ws");
    let request = url
        .into_client_request()
        .map_err(|error| format!("relay request: {error}"))?;
    let (mut socket, _) =
        tungstenite::connect(request).map_err(|error| format!("relay connect: {error}"))?;

    // Hello as the shell: the process secret is the shell credential
    // (NOT the dial ticket — those classes are distinct, ADR-0084).
    socket
        .send(Message::Text(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "hello",
                "params": {
                    "protocol_version": 1,
                    "client_kind": "shell",
                    "ticket": secret,
                },
            })
            .to_string()
            .into(),
        ))
        .map_err(|error| format!("relay hello send: {error}"))?;

    // The hello reply settles the handshake (a 4001/4002/4003 close
    // means the secret was rejected).
    let hello = read_reply(&mut socket, 1)?;
    if let Some(error) = hello.get("error") {
        return Err(format!("shell hello rejected: {error}"));
    }

    socket
        .send(Message::Text(
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "project_root_stage_native",
                "params": { "path": path },
            })
            .to_string()
            .into(),
        ))
        .map_err(|error| format!("relay stage send: {error}"))?;

    let reply = read_reply(&mut socket, 2)?;
    match reply.get("error") {
        Some(error) => Err(format!("stage failed over the wire: {error}")),
        None => Ok(reply
            .get("result")
            .cloned()
            .unwrap_or_else(|| json!({ "ok": true }))),
    }
}

/// Read frames until the reply with `id` arrives (the serve may push
/// pings and events between frames; pings must be answered like a real
/// client).
fn read_reply(
    socket: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    id: u64,
) -> Result<Value, String> {
    let deadline = std::time::Instant::now() + HELLO_REPLY_TIMEOUT;
    loop {
        if std::time::Instant::now() >= deadline {
            return Err("relay reply timeout".into());
        }
        let message = socket
            .read()
            .map_err(|error| format!("relay read: {error}"))?;
        match message {
            Message::Text(text) => {
                let value: Value = serde_json::from_str(&text)
                    .map_err(|error| format!("relay frame parse: {error}"))?;
                if value.get("id").and_then(Value::as_u64) == Some(id) {
                    return Ok(value);
                }
                // Notifications (events, pings) are ignored here; the
                // stage call has no stream.
            }
            Message::Ping(payload) => {
                socket
                    .send(Message::Pong(payload))
                    .map_err(|error| format!("relay pong: {error}"))?;
            }
            Message::Close(_) => return Err("relay closed by serve".into()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The relay's end-to-end shape against a REAL `optimus serve`:
    /// shell-kind hello with the process secret is accepted, and the
    /// stage call round-trips. Skipped when the CLI binary is absent
    /// (the verify pipeline builds it before the gate tier).
    #[test]
    fn shell_kind_relay_stages_over_the_wire() {
        let cli = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("debug")
            .join("optimus");
        if !cli.is_file() {
            eprintln!("skipping: optimus CLI not built");
            return;
        }
        let secret = format!("relay-secret-{}-{}", std::process::id(), "x".repeat(40));
        let root_dir = std::env::temp_dir().join(format!("relay-root-{}", std::process::id()));
        std::fs::create_dir_all(&root_dir).expect("staging target");
        let home = std::env::temp_dir().join(format!("relay-home-{}", std::process::id()));
        std::fs::create_dir_all(&home).ok();
        let port = 0; // ephemeral
        let mut child = std::process::Command::new(&cli)
            .arg("serve")
            .arg("--home")
            .arg(&home)
            .arg("--port")
            .arg(port.to_string())
            .env("OPTIMUS_NATIVE_SELECTION_TOKEN", &secret)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn serve");

        // Wait for the record (the dial ticket lives there; the shell
        // uses the secret, but the record proves readiness).
        let record_path = home.join("host-runtime.json");
        let mut record: Option<Value> = None;
        for _ in 0..100 {
            if let Ok(raw) = std::fs::read_to_string(&record_path) {
                if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                    record = Some(value);
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let record = record.expect("serve record within the ready bound");
        let serve_port = record
            .get("port")
            .and_then(Value::as_u64)
            .expect("record port") as u16;

        let result = stage_native_root(serve_port, &secret, &root_dir.to_string_lossy());
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&root_dir);

        match result {
            Ok(value) => {
                // The stage store write succeeds and returns the grant
                // shape (the grant token is what the renderer's
                // authorize call consumes).
                assert!(
                    value.get("grant_token").and_then(Value::as_str).is_some(),
                    "stage result must carry the grant token: {value}"
                );
            }
            Err(error) => panic!("relay stage failed: {error}"),
        }
    }

    #[test]
    fn shell_kind_hello_without_the_secret_is_rejected() {
        let cli = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("debug")
            .join("optimus");
        if !cli.is_file() {
            eprintln!("skipping: optimus CLI not built");
            return;
        }
        let secret = format!(
            "relay-secret-wrong-{}-{}",
            std::process::id(),
            "y".repeat(40)
        );
        let home = std::env::temp_dir().join(format!("relay-home-wrong-{}", std::process::id()));
        std::fs::create_dir_all(&home).ok();
        let mut child = std::process::Command::new(&cli)
            .arg("serve")
            .arg("--home")
            .arg(&home)
            .arg("--port")
            .arg("0")
            .env("OPTIMUS_NATIVE_SELECTION_TOKEN", &secret)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn serve");
        let record_path = home.join("host-runtime.json");
        let mut serve_port: Option<u16> = None;
        for _ in 0..100 {
            if let Ok(raw) = std::fs::read_to_string(&record_path) {
                if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                    serve_port = value.get("port").and_then(Value::as_u64).map(|p| p as u16);
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let serve_port = serve_port.expect("serve record within the ready bound");
        let result = stage_native_root(serve_port, "a-wrong-secret", "/tmp/x");
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&home);
        assert!(result.is_err(), "wrong secret must be rejected");
    }
}
