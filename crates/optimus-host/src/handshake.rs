//! `hello` handshake validation and credential classes for the surface
//! protocol (spec-015 R5/R7, ADR-0084).
//!
//! The FIRST client frame on either carrier MUST be a `hello` request
//! `{"jsonrpc":"2.0","id":1,"method":"hello","params":{"protocol_version":1,
//! "client_kind":"renderer|tui|cli|shell","ticket":"…"}}` — ticket required
//! on WebSocket, empty/omitted on stdio for the renderer/tui/cli kinds (pipe
//! ownership is the stdio credential).
//!
//! Credential classes (complete, pinned by serve_protocol.rs): the record
//! token authenticates renderer/tui/cli; the staging PROCESS SECRET
//! authenticates shell. A shell-kind claim presenting the record token is
//! rejected; a renderer/tui/cli-kind claim presenting the process secret is
//! rejected; a shell-kind hello without the secret is rejected on BOTH
//! carriers (pipe ownership is NOT a shell credential).
//!
//! Origin allowlist (defense-in-depth, not authorization — the credential is
//! the authorization): packaged Tauri v2 webview origins plus loopback
//! origins, IPv4 and IPv6; missing-Origin and `Origin: null` are accepted
//! with a valid credential. The wry-era `optimus://localhost` origin is
//! retired and MUST NOT be re-admitted (ADR-0084).

use serde_json::Value;

use crate::contract::PROTOCOL_VERSION;

/// Bounds (tunable constants, spec-015 R7): max concurrent WS connections,
/// max concurrent streams per connection, and the hello deadline.
pub const MAX_CONNECTIONS: usize = 8;
pub const MAX_STREAMS: usize = 16;
/// Default hello deadline. Tunable via `OPTIMUS_SERVE_HELLO_TIMEOUT_MS`
/// (dev affordance in the `OPTIMUS_OFFLINE_LATENCY_MS` pattern — a silent
/// connection must not hold an 8-slot bound indefinitely).
pub const HELLO_TIMEOUT_DEFAULT_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    Renderer,
    Tui,
    Cli,
    Shell,
}

impl ClientKind {
    pub fn parse(value: &str) -> Option<ClientKind> {
        match value {
            "renderer" => Some(ClientKind::Renderer),
            "tui" => Some(ClientKind::Tui),
            "cli" => Some(ClientKind::Cli),
            "shell" => Some(ClientKind::Shell),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ClientKind::Renderer => "renderer",
            ClientKind::Tui => "tui",
            ClientKind::Cli => "cli",
            ClientKind::Shell => "shell",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carrier {
    Stdio,
    Ws,
}

/// The outcome of a failed `hello`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelloError {
    /// Invalid request: reply `-32600` with `id:null`; the connection stays
    /// open (unknown `client_kind`, second hello, method-before-hello).
    InvalidRequest(String),
    /// Credential rejected: reply `-32000`, close `4001`.
    TicketRejected,
    /// Unsupported protocol version: reply `-32001`, close `4002`.
    UnsupportedVersion(u64),
}

/// Validated `hello` params.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloParams {
    pub client_kind: ClientKind,
}

/// Validate the `hello` params against the credential classes.
///
/// `record_token` is the serve process's dial ticket (env-delivered or
/// manual-start mint; written to the record). `process_secret` is the
/// env-delivered staging secret, `None` on a manual serve — which MUST
/// reject all shell-kind connections (the staging relay is unavailable
/// outside the spawn path).
pub fn validate_hello(
    params: &Value,
    carrier: Carrier,
    record_token: &str,
    process_secret: Option<&str>,
) -> Result<HelloParams, HelloError> {
    let version = params
        .get("protocol_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| HelloError::InvalidRequest("protocol_version required".into()))?;
    if version != PROTOCOL_VERSION {
        return Err(HelloError::UnsupportedVersion(version));
    }
    let kind_value = params
        .get("client_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| HelloError::InvalidRequest("client_kind required".into()))?;
    let kind = ClientKind::parse(kind_value)
        .ok_or_else(|| HelloError::InvalidRequest(format!("unknown client_kind: {kind_value}")))?;
    let presented = params.get("ticket").and_then(Value::as_str);

    match kind {
        ClientKind::Shell => {
            // Pipe ownership is NOT a shell credential: the secret must be
            // presented on BOTH carriers.
            let secret = process_secret.ok_or(HelloError::TicketRejected)?;
            match presented {
                Some(token) if constant_eq(token, secret) => Ok(HelloParams { client_kind: kind }),
                Some(token) if constant_eq(token, record_token) => {
                    // A shell-kind claim presenting the record token is a
                    // class violation, rejected like any other wrong
                    // credential.
                    Err(HelloError::TicketRejected)
                }
                _ => Err(HelloError::TicketRejected),
            }
        }
        ClientKind::Renderer | ClientKind::Tui | ClientKind::Cli => {
            match presented {
                // Stdio renderer/tui/cli may omit the ticket: pipe ownership
                // is the stdio credential (R5).
                None if carrier == Carrier::Stdio => Ok(HelloParams { client_kind: kind }),
                Some(token) if constant_eq(token, record_token) => {
                    Ok(HelloParams { client_kind: kind })
                }
                // A renderer/tui/cli-kind claim presenting the process
                // secret is a class violation (R5, ADR-0084).
                Some(token) if constant_eq(token, process_secret.unwrap_or("")) => {
                    Err(HelloError::TicketRejected)
                }
                _ => Err(HelloError::TicketRejected),
            }
        }
    }
}

/// Constant-time comparison (the `os.rs:105-118` pattern).
fn constant_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.as_bytes()
        .iter()
        .zip(right.as_bytes())
        .fold(0u8, |difference, (l, r)| difference | (l ^ r))
        == 0
}

/// The WebSocket Origin allowlist (spec-015 R7, ADR-0084): packaged Tauri
/// v2 webview origins ∪ loopback origins (IPv4 + IPv6, any port). Missing
/// Origin (raw non-browser clients) and `Origin: null` (custom-scheme
/// webviews, sandboxed iframes) are accepted — the credential is the
/// authorization; this check only blocks non-loopback pages, which cannot
/// present a loopback Origin.
pub fn origin_allowed(origin: Option<&str>) -> bool {
    match origin {
        None => true,
        Some("null") => true,
        Some(origin) => {
            origin == "tauri://localhost"
                || origin == "http://tauri.localhost"
                || origin.starts_with("http://127.0.0.1:")
                || origin.starts_with("http://localhost:")
                || origin.starts_with("http://[::1]:")
        }
    }
}

/// The hello deadline for this launch: `OPTIMUS_SERVE_HELLO_TIMEOUT_MS`
/// when set (dev affordance), else the pinned default.
pub fn hello_timeout_ms() -> u64 {
    std::env::var("OPTIMUS_SERVE_HELLO_TIMEOUT_MS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(HELLO_TIMEOUT_DEFAULT_MS)
}

/// [`hello_timeout_ms`] as a [`std::time::Duration`] (the socket read
/// timeout pre-hello).
pub fn hello_timeout_duration() -> std::time::Duration {
    std::time::Duration::from_millis(hello_timeout_ms())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hello(kind: &str, ticket: Option<&str>, version: u64) -> Value {
        let mut params = json!({
            "protocol_version": version,
            "client_kind": kind,
        });
        if let Some(ticket) = ticket {
            params["ticket"] = json!(ticket);
        }
        params
    }

    const RECORD_TOKEN: &str = "record-token-0123456789abcdef";
    const SECRET: &str = "process-secret-0123456789abcdef";

    #[test]
    fn renderer_with_record_token_is_accepted_on_both_carriers() {
        for carrier in [Carrier::Stdio, Carrier::Ws] {
            let result = validate_hello(
                &hello("renderer", Some(RECORD_TOKEN), 1),
                carrier,
                RECORD_TOKEN,
                Some(SECRET),
            );
            assert!(result.is_ok(), "carrier {carrier:?}: {result:?}");
        }
    }

    #[test]
    fn stdio_ticket_omission_is_pipe_ownership() {
        let result = validate_hello(&hello("tui", None, 1), Carrier::Stdio, RECORD_TOKEN, None);
        assert_eq!(result.unwrap().client_kind, ClientKind::Tui);
    }

    #[test]
    fn ws_without_ticket_is_rejected() {
        let result = validate_hello(&hello("cli", None, 1), Carrier::Ws, RECORD_TOKEN, None);
        assert_eq!(result, Err(HelloError::TicketRejected));
    }

    #[test]
    fn wrong_ticket_is_rejected() {
        let result = validate_hello(
            &hello("renderer", Some("nope-nope-nope-nope"), 1),
            Carrier::Ws,
            RECORD_TOKEN,
            None,
        );
        assert_eq!(result, Err(HelloError::TicketRejected));
    }

    #[test]
    fn shell_kind_requires_the_process_secret_on_both_carriers() {
        // Correct secret: accepted on BOTH carriers.
        for carrier in [Carrier::Stdio, Carrier::Ws] {
            let ok = validate_hello(
                &hello("shell", Some(SECRET), 1),
                carrier,
                RECORD_TOKEN,
                Some(SECRET),
            );
            assert!(ok.is_ok(), "carrier {carrier:?}: {ok:?}");
        }
        // Record token as a shell credential: class violation on both.
        for carrier in [Carrier::Stdio, Carrier::Ws] {
            let bad = validate_hello(
                &hello("shell", Some(RECORD_TOKEN), 1),
                carrier,
                RECORD_TOKEN,
                Some(SECRET),
            );
            assert_eq!(bad, Err(HelloError::TicketRejected));
        }
        // No secret at all: rejected even over stdio (pipe ownership is not
        // a shell credential).
        let none = validate_hello(
            &hello("shell", None, 1),
            Carrier::Stdio,
            RECORD_TOKEN,
            Some(SECRET),
        );
        assert_eq!(none, Err(HelloError::TicketRejected));
    }

    #[test]
    fn renderer_kind_presenting_the_process_secret_is_rejected() {
        let result = validate_hello(
            &hello("renderer", Some(SECRET), 1),
            Carrier::Ws,
            RECORD_TOKEN,
            Some(SECRET),
        );
        assert_eq!(result, Err(HelloError::TicketRejected));
    }

    #[test]
    fn manual_serve_rejects_all_shell_kinds() {
        let result = validate_hello(
            &hello("shell", Some(SECRET), 1),
            Carrier::Ws,
            RECORD_TOKEN,
            None,
        );
        assert_eq!(result, Err(HelloError::TicketRejected));
    }

    #[test]
    fn unknown_kind_is_an_invalid_request() {
        let result = validate_hello(&hello("wat", None, 1), Carrier::Ws, RECORD_TOKEN, None);
        assert!(matches!(result, Err(HelloError::InvalidRequest(_))));
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let result = validate_hello(
            &hello("renderer", Some(RECORD_TOKEN), 2),
            Carrier::Ws,
            RECORD_TOKEN,
            None,
        );
        assert_eq!(result, Err(HelloError::UnsupportedVersion(2)));
    }

    #[test]
    fn origin_allowlist_matches_the_pinned_set() {
        assert!(origin_allowed(None));
        assert!(origin_allowed(Some("null")));
        assert!(origin_allowed(Some("tauri://localhost")));
        assert!(origin_allowed(Some("http://tauri.localhost")));
        assert!(origin_allowed(Some("http://127.0.0.1:5173")));
        assert!(origin_allowed(Some("http://localhost:17865")));
        assert!(origin_allowed(Some("http://[::1]:5173")));
        assert!(
            !origin_allowed(Some("optimus://localhost")),
            "wry origin retired"
        );
        assert!(!origin_allowed(Some("https://evil.example")));
        assert!(!origin_allowed(Some("http://10.0.0.1:17865")));
        assert!(!origin_allowed(Some("http://127.0.0.2:17865")));
    }

    #[test]
    fn constant_eq_rejects_length_and_byte_differences() {
        assert!(constant_eq("abc", "abc"));
        assert!(!constant_eq("abc", "abcd"));
        assert!(!constant_eq("abc", "abd"));
    }
}
