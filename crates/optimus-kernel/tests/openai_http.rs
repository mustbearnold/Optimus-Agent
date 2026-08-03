//! Live HTTP round-trip against a local mock OpenAI server (no internet).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use optimus_kernel::{
    CompletionRequest, Message, ModelProvider, OpenAiCompatConfig, OpenAiCompatModel, Role,
};
use optimus_packs::CapabilitySession;
use serde_json::json;

fn handle_one(mut stream: TcpStream, status: u16, body: &str) {
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(3))).ok();
    let mut buf = [0u8; 16384];
    let mut total = 0usize;
    for _ in 0..20 {
        match stream.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                let s = String::from_utf8_lossy(&buf[..total]);
                if let Some(header_end) = s.find("\r\n\r\n") {
                    let content_length = s[..header_end]
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if total >= header_end + 4 + content_length {
                        break;
                    }
                }
                if total == buf.len() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let req = String::from_utf8_lossy(&buf[..total]);
    assert!(
        req.to_ascii_lowercase().contains("authorization: bearer"),
        "missing auth header"
    );
    let resp = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        if (200..300).contains(&status) {
            "OK"
        } else {
            "ERR"
        },
        body.len(),
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

fn spawn_mock(status: u16, body: String) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let ready = Arc::new(Barrier::new(2));
    let ready2 = ready.clone();
    let handle = thread::spawn(move || {
        ready2.wait();
        let (stream, _) = listener.accept().expect("accept");
        handle_one(stream, status, &body);
    });
    ready.wait();
    // tiny settle
    thread::sleep(Duration::from_millis(5));
    (format!("http://{addr}/v1"), handle)
}

#[test]
fn openai_compat_http_roundtrip() {
    let body = json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "mock-ok"
            }
        }],
        "usage": {
            "prompt_tokens": 11,
            "completion_tokens": 7,
            "total_tokens": 18,
            "prompt_tokens_details": { "cached_tokens": 3 },
            "completion_tokens_details": { "reasoning_tokens": 2 }
        }
    })
    .to_string();
    let (base, handle) = spawn_mock(200, body);
    let mut model = OpenAiCompatModel {
        config: OpenAiCompatConfig {
            base_url: base,
            api_key: "test-key".into(),
            model: "mock-model".into(),
            organization: None,
            timeout_secs: 5,
        },
        completions_url_override: None,
        last_usage: None,
    };
    let resp = model
        .complete(CompletionRequest {
            messages: vec![Message {
                role: Role::User,
                content: "hi".into(),
                tool_call_id: None,
                name: None,
            }],
            tools: vec![CapabilitySession::with_defaults()
                .resolve_loaded_tool("memory_recall")
                .unwrap()
                .clone()],
            ..Default::default()
        })
        .expect("complete");
    assert_eq!(resp.text.as_deref(), Some("mock-ok"));
    let usage = model.last_usage.expect("provider usage");
    assert_eq!(usage.input_tokens, Some(11));
    assert_eq!(usage.output_tokens, Some(7));
    assert_eq!(usage.total_tokens, Some(18));
    assert_eq!(usage.cached_input_tokens, Some(3));
    assert_eq!(usage.reasoning_tokens, Some(2));
    handle.join().unwrap();
}

#[test]
fn openai_compat_http_error_surface() {
    let body = r#"{"error":{"message":"nope"}}"#.to_string();
    let (base, handle) = spawn_mock(401, body);
    let mut model = OpenAiCompatModel {
        config: OpenAiCompatConfig {
            base_url: base,
            api_key: "bad".into(),
            model: "x".into(),
            organization: None,
            timeout_secs: 5,
        },
        completions_url_override: None,
        last_usage: None,
    };
    let err = model
        .complete(CompletionRequest {
            messages: vec![Message {
                role: Role::User,
                content: "hi".into(),
                tool_call_id: None,
                name: None,
            }],
            tools: vec![],
            ..Default::default()
        })
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("401")
            || msg.contains("nope")
            || msg.contains("HTTP")
            || msg.contains("error"),
        "{msg}"
    );
    handle.join().unwrap();
}
