//! Deterministic HTTP gateway smoke (local only).
//!
//! Spawns `optimus gateway serve --max-requests N` on an ephemeral-ish fixed port,
//! posts inbound, drains, asserts outbox. No wall sleeps for readiness — polls health.

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const TOKEN: &str = "gateway-test-token-32-characters-minimum";

fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

fn wait_health(port: u16, timeout: Duration) {
    let url = format!("http://127.0.0.1:{port}/health");
    let start = Instant::now();
    loop {
        if let Ok(resp) = ureq::get(&url)
            .set("Authorization", &format!("Bearer {TOKEN}"))
            .timeout(Duration::from_millis(200))
            .call()
        {
            if resp.status() == 200 {
                return;
            }
        }
        if start.elapsed() > timeout {
            panic!("gateway health timeout on {url}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn webhook_inbound_drain_outbox() {
    let port = free_port();
    let home = std::env::temp_dir().join(format!("optimus-gw-http-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();

    let exe = env!("CARGO_BIN_EXE_optimus");
    let child = Command::new(exe)
        .args([
            "--home",
            home.to_str().unwrap(),
            "gateway",
            "serve",
            "--port",
            &port.to_string(),
            // health + unauthorized probe + inbound + drain + outbox = 5
            "--max-requests",
            "5",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("OPTIMUS_GATEWAY_TOKEN", TOKEN)
        .spawn()
        .expect("spawn gateway");
    let _g = ChildGuard(child);

    wait_health(port, Duration::from_secs(10));

    let unauthorized = ureq::get(&format!("http://127.0.0.1:{port}/health"))
        .call()
        .unwrap_err();
    assert_eq!(unauthorized.into_response().unwrap().status(), 401);

    let inbound = ureq::post(&format!("http://127.0.0.1:{port}/inbound"))
        .set("Authorization", &format!("Bearer {TOKEN}"))
        .set("Content-Type", "application/json")
        .send_string(r#"{"text":"hello webhook","channel":"http-test","provider":"offline"}"#)
        .expect("inbound");
    assert_eq!(inbound.status(), 200);
    let body: serde_json::Value = inbound.into_json().unwrap();
    assert_eq!(body["ok"], true);

    let drain = ureq::post(&format!("http://127.0.0.1:{port}/drain"))
        .set("Authorization", &format!("Bearer {TOKEN}"))
        .send_string("")
        .expect("drain");
    assert_eq!(drain.status(), 200);
    let d: serde_json::Value = drain.into_json().unwrap();
    assert_eq!(d["ok"], true);
    assert!(d["drained"]["reply_preview"]
        .as_str()
        .unwrap_or("")
        .contains("hello webhook"));

    let outbox = ureq::get(&format!("http://127.0.0.1:{port}/outbox"))
        .set("Authorization", &format!("Bearer {TOKEN}"))
        .call()
        .expect("outbox");
    let o: serde_json::Value = outbox.into_json().unwrap();
    assert!(!o["messages"].as_array().unwrap().is_empty());
}
