//! Codex OAuth + Responses HTTP mock (no internet).

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use optimus_kernel::{
    from_codex_responses_response, to_codex_responses_request, verify_user_only, CodexAuthStore,
    CodexOAuthConfig, CodexOAuthModel, CodexTokens, CompletionRequest, CredentialProtector,
    KernelError, Message, ModelProvider, Role,
};
use optimus_packs::CapabilitySession;
use serde_json::json;
use tempfile::tempdir;

#[cfg(windows)]
const TEST_PROTECTION_LABEL: &str = "dpapi-current-user";
#[cfg(not(windows))]
const TEST_PROTECTION_LABEL: &str = "test-xor-v1";

#[cfg(not(windows))]
#[derive(Debug)]
struct TestCredentialProtector;

#[cfg(not(windows))]
impl CredentialProtector for TestCredentialProtector {
    fn label(&self) -> &'static str {
        TEST_PROTECTION_LABEL
    }

    fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>, KernelError> {
        Ok(plaintext.iter().map(|byte| byte ^ 0xa5).collect())
    }

    fn unprotect(&self, ciphertext: &[u8]) -> Result<Vec<u8>, KernelError> {
        self.protect(ciphertext)
    }
}

#[cfg(windows)]
fn open_test_store(home: &std::path::Path) -> CodexAuthStore {
    CodexAuthStore::open(home).unwrap()
}

#[cfg(not(windows))]
fn open_test_store(home: &std::path::Path) -> CodexAuthStore {
    CodexAuthStore::open_with_protector(home, Arc::new(TestCredentialProtector)).unwrap()
}

#[cfg(windows)]
fn open_test_model(config: CodexOAuthConfig) -> CodexOAuthModel {
    CodexOAuthModel::new(config).unwrap()
}

#[cfg(not(windows))]
fn open_test_model(config: CodexOAuthConfig) -> CodexOAuthModel {
    let store =
        CodexAuthStore::open_with_protector(&config.home, Arc::new(TestCredentialProtector))
            .unwrap();
    CodexOAuthModel {
        config,
        responses_url_override: None,
        store,
        last_usage: None,
    }
}

fn read_http_request(stream: &mut TcpStream) -> std::io::Result<()> {
    let mut request = Vec::new();
    let mut expected_len = None;

    loop {
        let mut chunk = [0u8; 4096];
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(());
        }
        request.extend_from_slice(&chunk[..read]);

        if expected_len.is_none() {
            if let Some(header_end) = request
                .windows(4)
                .position(|window| window == b"\x0d\x0a\x0d\x0a")
            {
                let body_start = header_end + 4;
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_len = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                expected_len = Some(body_start + content_len);
            }
        }

        if expected_len.is_some_and(|len| request.len() >= len) {
            return Ok(());
        }
    }
}

fn spawn_mock(status: u16, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let body = body.to_string();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = read_http_request(&mut stream);
            let resp = format!(
                "HTTP/1.1 {status} OK\x0d\x0aContent-Type: application/json\x0d\x0aContent-Length: {}\x0d\x0aConnection: close\x0d\x0a\x0d\x0a{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Write);
        }
    });
    thread::sleep(Duration::from_millis(20));
    format!("http://{addr}/responses")
}

fn mock_streaming_model(body: &str) -> (tempfile::TempDir, CodexOAuthModel) {
    let dir = tempdir().unwrap();
    let store = open_test_store(dir.path());
    store
        .save_tokens(
            CodexTokens {
                access_token: "test-access".into(),
                refresh_token: "test-refresh".into(),
                account_id: None,
                id_token: None,
            },
            "http://127.0.0.1:9",
            "test",
        )
        .unwrap();
    let mut model = open_test_model(CodexOAuthConfig {
        home: dir.path().to_path_buf(),
        model: "gpt-5.4".into(),
        timeout_secs: 5,
    });
    model.responses_url_override = Some(spawn_mock(200, body));
    (dir, model)
}

#[test]
fn codex_store_roundtrip_and_status() {
    let dir = tempdir().unwrap();
    let store = open_test_store(dir.path());
    let st = store.status().unwrap();
    assert!(!st.present);
    store
        .save_tokens(
            CodexTokens {
                access_token: "private-access-sentinel".into(),
                refresh_token: "private-refresh-sentinel".into(),
                account_id: Some("aid".into()),
                id_token: None,
            },
            "https://chatgpt.com/backend-api/codex",
            "test",
        )
        .unwrap();
    let st2 = store.status().unwrap();
    assert!(st2.present);
    assert!(st2.has_refresh);
    assert_eq!(st2.account_id.as_deref(), Some("aid"));
    let stored = std::fs::read(store.path()).unwrap();
    let stored_text = String::from_utf8_lossy(&stored);
    assert!(!stored_text.contains("private-access-sentinel"));
    assert!(!stored_text.contains("private-refresh-sentinel"));
    assert!(!stored_text.contains("refresh_token"));
    assert!(stored_text.contains(TEST_PROTECTION_LABEL));
    verify_user_only(store.path()).unwrap();
}

#[test]
fn legacy_plaintext_migrates_once_and_corruption_fails_without_rewrite() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("auth.json");
    std::fs::write(
        &path,
        r#"{"version":1,"providers":{"openai-codex":{"tokens":{"access_token":"legacy-access","refresh_token":"legacy-refresh","account_id":"legacy-account","id_token":null},"last_refresh":null,"auth_mode":"legacy","base_url":"https://chatgpt.com/backend-api/codex"}}}"#,
    )
    .unwrap();
    let store = open_test_store(dir.path());

    let status = store.status().unwrap();

    assert!(status.present);
    assert_eq!(status.account_id.as_deref(), Some("legacy-account"));
    let migrated = std::fs::read(&path).unwrap();
    let migrated_text = String::from_utf8_lossy(&migrated);
    assert!(migrated_text.contains(TEST_PROTECTION_LABEL));
    assert!(!migrated_text.contains("legacy-access"));
    verify_user_only(&path).unwrap();

    std::fs::write(
        &path,
        format!(
            "{{\"version\":2,\"protection\":\"{TEST_PROTECTION_LABEL}\",\"ciphertext_hex\":\"zz\"}}"
        ),
    )
    .unwrap();
    let corrupt = std::fs::read(&path).unwrap();
    assert!(store.status().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), corrupt);
}

#[test]
fn codex_responses_http_roundtrip() {
    let dir = tempdir().unwrap();
    let store = open_test_store(dir.path());
    store
        .save_tokens(
            CodexTokens {
                access_token: "test-access".into(),
                refresh_token: "test-refresh".into(),
                account_id: None,
                id_token: None,
            },
            "http://127.0.0.1:9", // unused when override set
            "test",
        )
        .unwrap();
    let url = spawn_mock(
        200,
        r#"{"output":[{"type":"message","content":[{"type":"output_text","text":"codex-hi"}]}],"usage":{"input_tokens":13,"output_tokens":5,"total_tokens":18,"input_tokens_details":{"cached_tokens":4},"output_tokens_details":{"reasoning_tokens":1}}}"#,
    );
    let mut model = open_test_model(CodexOAuthConfig {
        home: dir.path().to_path_buf(),
        model: "gpt-5.4".into(),
        timeout_secs: 5,
    });
    model.responses_url_override = Some(url);
    let resp = model
        .complete(CompletionRequest {
            messages: vec![Message {
                role: Role::User,
                content: "hi".into(),
                tool_call_id: None,
                name: None,
            }],
            tools: vec![CapabilitySession::with_defaults()
                .resolve_loaded_tool("read_file")
                .unwrap()
                .clone()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(resp.text.as_deref(), Some("codex-hi"));
    let usage = model.last_usage.expect("provider usage");
    assert_eq!(usage.input_tokens, Some(13));
    assert_eq!(usage.output_tokens, Some(5));
    assert_eq!(usage.total_tokens, Some(18));
    assert_eq!(usage.cached_input_tokens, Some(4));
    assert_eq!(usage.reasoning_tokens, Some(1));
}

#[test]
fn codex_streaming_records_completed_usage() {
    let (_dir, mut model) = mock_streaming_model(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"streamed\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"streamed\"}]}],\"usage\":{\"input_tokens\":9,\"output_tokens\":3,\"total_tokens\":12}}}\n\n",
    );
    let response = model
        .complete_streaming(
            CompletionRequest {
                messages: vec![Message {
                    role: Role::User,
                    content: "hi".into(),
                    tool_call_id: None,
                    name: None,
                }],
                tools: vec![],
                ..Default::default()
            },
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(response.text.as_deref(), Some("streamed"));
    assert_eq!(model.last_usage.unwrap().total_tokens, Some(12));
}

#[test]
fn production_streaming_rejects_malformed_completed_tool_call() {
    let dir = tempdir().unwrap();
    let store = open_test_store(dir.path());
    store
        .save_tokens(
            CodexTokens {
                access_token: "test-access".into(),
                refresh_token: "test-refresh".into(),
                account_id: None,
                id_token: None,
            },
            "http://127.0.0.1:9",
            "test",
        )
        .unwrap();
    let url = spawn_mock(
        200,
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"type\":\"function_call\",\"call_id\":\"completed-bad\",\"name\":\"memory_recall\"}]}}\n\n",
    );
    let mut model = open_test_model(CodexOAuthConfig {
        home: dir.path().to_path_buf(),
        model: "gpt-5.4".into(),
        timeout_secs: 5,
    });
    model.responses_url_override = Some(url);
    let error = model
        .complete_streaming(
            CompletionRequest {
                messages: vec![Message {
                    role: Role::User,
                    content: "hi".into(),
                    tool_call_id: None,
                    name: None,
                }],
                tools: vec![],
                ..Default::default()
            },
            &mut |_| {},
        )
        .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("memory_recall"));
    assert!(message.contains("missing string arguments"));
}

#[test]
fn production_streaming_validates_completed_output_after_item_call() {
    let dir = tempdir().unwrap();
    let store = open_test_store(dir.path());
    store
        .save_tokens(
            CodexTokens {
                access_token: "test-access".into(),
                refresh_token: "test-refresh".into(),
                account_id: None,
                id_token: None,
            },
            "http://127.0.0.1:9",
            "test",
        )
        .unwrap();
    let url = spawn_mock(
        200,
        "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"call_id\":\"valid-item\",\"name\":\"memory_recall\",\"arguments\":\"{}\"}}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"type\":\"function_call\",\"call_id\":\"completed-bad\",\"name\":\"web_search\"}]}}\n\n",
    );
    let mut model = open_test_model(CodexOAuthConfig {
        home: dir.path().to_path_buf(),
        model: "gpt-5.4".into(),
        timeout_secs: 5,
    });
    model.responses_url_override = Some(url);
    let error = model
        .complete_streaming(
            CompletionRequest {
                messages: vec![Message {
                    role: Role::User,
                    content: "hi".into(),
                    tool_call_id: None,
                    name: None,
                }],
                tools: vec![],
                ..Default::default()
            },
            &mut |_| {},
        )
        .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("web_search"));
    assert!(message.contains("missing string arguments"));
}

#[test]
fn production_streaming_rejects_no_space_malformed_and_empty_sse() {
    for (body, expected) in [
        (
            "data:{not-json}\n\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"safe text\"}\n\n",
            "SSE event has invalid JSON",
        ),
        ("data: [DONE]\n\n", "empty text and no tool_calls"),
    ] {
        let (_dir, mut model) = mock_streaming_model(body);
        let error = model
            .complete_streaming(
                CompletionRequest {
                    messages: vec![Message {
                        role: Role::User,
                        content: "hi".into(),
                        tool_call_id: None,
                        name: None,
                    }],
                    tools: vec![],
                    ..Default::default()
                },
                &mut |_| {},
            )
            .unwrap_err();
        assert!(error.to_string().contains(expected));
    }
}

#[test]
fn import_helpers_parse_shapes() {
    let hermes = json!({
        "providers": {"openai-codex": {"tokens": {"access_token":"h","refresh_token":"hr"}}}
    });
    let t = optimus_kernel::extract_codex_tokens_from_hermes(&hermes).unwrap();
    assert_eq!(t.access_token, "h");
    let cli = json!({"tokens": {"access_token":"c","refresh_token":"cr","account_id":"x"}});
    let t2 = optimus_kernel::extract_codex_tokens_from_codex_cli(&cli).unwrap();
    assert_eq!(t2.account_id.as_deref(), Some("x"));
}

#[test]
fn responses_mapper_unit() {
    let body = to_codex_responses_request(
        &CompletionRequest {
            messages: vec![Message {
                role: Role::User,
                content: "x".into(),
                tool_call_id: None,
                name: None,
            }],
            tools: vec![],
            ..Default::default()
        },
        "gpt-5.4",
    );
    assert_eq!(body["model"], "gpt-5.4");
    let parsed = from_codex_responses_response(&json!({
        "output": [{"type":"message","content":[{"type":"output_text","text":"ok"}]}]
    }))
    .unwrap();
    assert_eq!(parsed.text.as_deref(), Some("ok"));
}
