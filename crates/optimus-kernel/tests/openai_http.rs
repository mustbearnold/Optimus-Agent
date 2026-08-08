//! Live HTTP round-trip against a local mock OpenAI server (no internet).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use optimus_kernel::{
    CompletionRequest, DeepseekModel, Message, ModelProvider, OpenAiCompatConfig,
    OpenAiCompatModel, Role,
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

/// Mock SSE server: reads the request, then writes `frames` as real
/// `data:` lines with `inter_frame_ms` delay between them, then `[DONE]`.
/// Close-delimited (no Content-Length) so the client reads incrementally.
fn spawn_stream_mock(
    frames: Vec<String>,
    inter_frame_ms: u64,
    assert_stream_body: bool,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let ready = Arc::new(Barrier::new(2));
    let ready2 = ready.clone();
    let handle = thread::spawn(move || {
        ready2.wait();
        let (mut stream, _) = listener.accept().expect("accept");
        stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
        // Consume the request (headers + body), mirroring handle_one.
        let mut buf = [0u8; 16384];
        let mut total = 0usize;
        stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
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
            req.to_ascii_lowercase().contains("authorization:"),
            "missing auth header"
        );
        if assert_stream_body {
            assert!(
                req.contains("\"stream\":true"),
                "request body must set stream:true, got: {}",
                req
            );
        }
        let head =
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(head.as_bytes());
        for frame in &frames {
            let _ = stream.write_all(format!("data: {frame}\n\n").as_bytes());
            let _ = stream.flush();
            thread::sleep(Duration::from_millis(inter_frame_ms));
        }
        let _ = stream.write_all(b"data: [DONE]\n\n");
        let _ = stream.flush();
        let _ = stream.shutdown(std::net::Shutdown::Both);
    });
    ready.wait();
    thread::sleep(Duration::from_millis(5));
    (format!("http://{addr}/v1"), handle)
}

fn deepseek_model(base: String) -> DeepseekModel {
    DeepseekModel {
        config: OpenAiCompatConfig {
            base_url: base,
            api_key: "test-key".into(),
            model: "deepseek-v4-flash".into(),
            organization: None,
            timeout_secs: 5,
        },
        completions_url_override: None,
        last_usage: None,
    }
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
                reasoning_content: None,
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
                reasoning_content: None,
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

#[test]
fn deepseek_http_roundtrip_preserves_reasoning_content() {
    let body = json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "deepseek-ok",
                "reasoning_content": "private-step"
            }
        }]
    })
    .to_string();
    let (base, handle) = spawn_mock(200, body);
    let mut model = DeepseekModel {
        config: OpenAiCompatConfig {
            base_url: base,
            api_key: "test-key".into(),
            model: "deepseek-v4-flash".into(),
            organization: None,
            timeout_secs: 5,
        },
        completions_url_override: None,
        last_usage: None,
    };
    assert_eq!(
        model.identity(),
        ("deepseek".to_string(), "deepseek-v4-flash".to_string())
    );
    let response = model
        .complete(CompletionRequest {
            reasoning_effort: Some("xhigh".into()),
            ..Default::default()
        })
        .expect("complete");
    assert_eq!(response.text.as_deref(), Some("deepseek-ok"));
    assert_eq!(response.reasoning_content.as_deref(), Some("private-step"));
    handle.join().unwrap();
}

#[test]
fn deepseek_stream_delivers_incremental_deltas_and_thinking() {
    let frames = vec![
        json!({"choices":[{"delta":{"reasoning_content":"think step 1"}}]}).to_string(),
        json!({"choices":[{"delta":{"reasoning_content":" think step 2"}}]}).to_string(),
        json!({"choices":[{"delta":{"content":"Hel"}}]}).to_string(),
        json!({"choices":[{"delta":{"content":"lo "}}]}).to_string(),
        json!({"choices":[{"delta":{"content":"world"}}]}).to_string(),
    ];
    // 30ms between frames: if the client buffered the whole response, the
    // elapsed time would still pass, but the SINK would receive one delta.
    // We assert at least two deltas AND that the first arrived before the
    // stream finished (proves incremental delivery, not one-shot).
    let (base, handle) = spawn_stream_mock(frames, 30, true);
    let mut model = deepseek_model(base);
    let mut deltas: Vec<String> = Vec::new();
    let mut thinking: Vec<String> = Vec::new();
    let mut first_delta_at = std::time::Instant::now();
    let mut first_delta_seen = false;
    let mut sink_events = 0usize;
    let response = model
        .complete_streaming_cancellable(
            CompletionRequest {
                reasoning_effort: Some("low".into()),
                ..Default::default()
            },
            &mut |event| {
                sink_events += 1;
                match event {
                    optimus_kernel::StreamEvent::TextDelta(t) => {
                        deltas.push(t);
                        if !first_delta_seen {
                            first_delta_seen = true;
                            first_delta_at = std::time::Instant::now();
                        }
                    }
                    optimus_kernel::StreamEvent::ThinkingDelta(t) => thinking.push(t),
                    _ => {}
                }
            },
            &optimus_kernel::CancellationToken::new(),
        )
        .expect("stream");
    // The mock writes frames with 30ms sleeps; if deltas arrived only after
    // the whole body was buffered, the first delta would land after all
    // frames (>= 150ms). Incremental delivery means it lands well before.
    assert!(
        first_delta_at.elapsed().as_millis() < 120,
        "first delta should arrive incrementally, not after the full body: {:?}ms",
        first_delta_at.elapsed().as_millis()
    );
    assert_eq!(
        deltas,
        vec!["Hel".to_string(), "lo ".to_string(), "world".to_string()]
    );
    assert_eq!(
        thinking,
        vec!["think step 1".to_string(), " think step 2".to_string()]
    );
    assert_eq!(response.text.as_deref(), Some("Hello world"));
    assert_eq!(
        response.reasoning_content.as_deref(),
        Some("think step 1 think step 2")
    );
    assert!(
        sink_events >= 5,
        "expected >=5 sink events, got {sink_events}"
    );
    handle.join().unwrap();
}

#[test]
fn deepseek_stream_assembles_fragmented_tool_calls() {
    let frames = vec![
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read_file","arguments":""}}]}}]}).to_string(),
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":"}}]}}]}).to_string(),
        json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"x.txt\"}"}}]}}]}).to_string(),
    ];
    let (base, handle) = spawn_stream_mock(frames, 5, true);
    let mut model = deepseek_model(base);
    let mut received: Vec<optimus_kernel::StreamEvent> = Vec::new();
    let response = model
        .complete_streaming(CompletionRequest::default(), &mut |event| {
            received.push(event)
        })
        .expect("stream");
    assert_eq!(response.text, None);
    assert_eq!(response.tool_calls.len(), 1);
    let call = &response.tool_calls[0];
    assert_eq!(call.id, "call_1");
    assert_eq!(call.name, "read_file");
    assert_eq!(call.arguments, serde_json::json!({"path": "x.txt"}));
    handle.join().unwrap();
}

#[test]
fn deepseek_stream_cancellation_stops_midstream() {
    // 6 frames x 40ms = 240ms total; cancel after the first delta arrives.
    let frames = vec![
        json!({"choices":[{"delta":{"content":"one"}}]}).to_string(),
        json!({"choices":[{"delta":{"content":"two"}}]}).to_string(),
        json!({"choices":[{"delta":{"content":"three"}}]}).to_string(),
        json!({"choices":[{"delta":{"content":"four"}}]}).to_string(),
        json!({"choices":[{"delta":{"content":"five"}}]}).to_string(),
        json!({"choices":[{"delta":{"content":"six"}}]}).to_string(),
    ];
    let (base, handle) = spawn_stream_mock(frames, 40, true);
    let mut model = deepseek_model(base);
    let token = optimus_kernel::CancellationToken::new();
    let mut first_delta_seen = false;
    let result = model.complete_streaming_cancellable(
        CompletionRequest::default(),
        &mut |event| {
            if matches!(event, optimus_kernel::StreamEvent::TextDelta(_)) && !first_delta_seen {
                first_delta_seen = true;
                token.cancel();
            }
        },
        &token,
    );
    assert!(
        result.is_err(),
        "cancellation mid-stream must surface an error"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("cancel"),
        "expected a cancellation error, got: {err}"
    );
    handle.join().unwrap();
}

#[test]
fn openai_compat_stream_roundtrip() {
    let frames = vec![
        json!({"choices":[{"delta":{"content":"compat-"}}]}).to_string(),
        json!({"choices":[{"delta":{"content":"stream"}}]}).to_string(),
        json!({"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":4,"completion_tokens":2,"total_tokens":6}}).to_string(),
    ];
    let (base, handle) = spawn_stream_mock(frames, 5, true);
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
    let mut events: Vec<optimus_kernel::StreamEvent> = Vec::new();
    let response = model
        .complete_streaming_cancellable(
            CompletionRequest::default(),
            &mut |event| events.push(event),
            &optimus_kernel::CancellationToken::new(),
        )
        .expect("stream");
    assert_eq!(response.text.as_deref(), Some("compat-stream"));
    let usage = model.last_usage.expect("usage from final chunk");
    assert_eq!(usage.input_tokens, Some(4));
    assert_eq!(usage.total_tokens, Some(6));
    // usage chunk carries no delta; the two content chunks each produce a delta.
    assert_eq!(
        events.len(),
        2,
        "expected exactly 2 TextDelta events, got {events:?}"
    );
    handle.join().unwrap();
}
