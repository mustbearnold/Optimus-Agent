//! Shared IPC wire contracts.
//!
//! The transport-internal envelope (`IpcEnvelope`/`IpcReply`) is the host
//! API; the surface-protocol wire layer adapts it to JSON-RPC 2.0
//! (spec-015 R4) without changing method semantics.

use serde::{Deserialize, Serialize};

/// The one protocol artifact version (spec-015 R10, law 12): framing,
/// method vocabulary, event vocabulary, and payload shapes version together.
/// Exchanged in the `hello` handshake; the surface-contract gate compares
/// this const against the committed schema's declared version.
pub const PROTOCOL_VERSION: u64 = 1;

/// The wire event vocabulary (`StreamEvent` on the renderer,
/// `contracts.ts:410-418`) plus nothing — `host.ready`/`host.error` are
/// wire-level notifications, declared in the schema, not stream events.
/// The surface-contract gate compares this const against the schema's
/// event set.
pub const STREAM_EVENT_VOCABULARY: &[&str] = &[
    "delta",
    "thinking",
    "status",
    "tool",
    "timing",
    "done",
    "cancelled",
    "error",
];

/// Registry methods that are NOT wire methods: shell/main-only channels
/// (`window_*`, `pick_folder`, `open_path`/`open_url`) — documented
/// non-wire channels (spec-015 R2). `project_root_stage_native` is NOT
/// here: it is the shell-gated bucket (see [`SHELL_GATED_METHODS`]).
pub const NON_WIRE_CHANNELS: &[&str] = &[
    "window_minimize",
    "window_maximize",
    "window_close",
    "window_drag",
    "window_outer_position",
    "window_set_outer_position",
    "pick_folder",
    "open_path",
    "open_url",
];

/// The superseded blocking chat family (spec-015 R2): blocking and
/// non-cancellable (`chat.rs:34-51`); NOT wire-reachable, NOT
/// renderer-callable. The registry keeps them for in-process users
/// (CLI/TUI/optimus-ops).
pub const SUPERSEDED_CHAT_FAMILY: &[&str] = &["chat", "chat_offline", "chat_approval_resolve"];

/// The streaming trio promoted to first-class wire methods (R2).
pub const STREAMING_TRIO: &[&str] = &["chat_start", "chat_cancel", "chat_approval_resolve_start"];

/// The explicit protocol-method set (R2/R12): the handshake method, the
/// event notification, and the server-origin notifications. Named so the
/// gate formula never flags them as extras.
pub const PROTOCOL_METHODS: &[&str] = &["hello", "event", "host.ready", "host.error"];

/// Server-origin-only methods: a client request for any of these is
/// rejected `-32601` (R6 — a client must not inject events or spoof
/// readiness).
pub const SERVER_ORIGIN_METHODS: &[&str] = &["event", "host.ready", "host.error"];

/// The shell-gated staging method (R2/R7/R12): wire-reachable from
/// `client_kind:"shell"` connections presenting the process secret ONLY;
/// serve injects the secret into the call params server-side.
pub const SHELL_GATED_METHODS: &[&str] = &["project_root_stage_native"];

/// Dispatch classes (spec-015 R3): control-plane operations execute on the
/// connection's own read/event loop; chat turns and registry/effect methods
/// share the bounded worker pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireClass {
    /// `hello`, `chat_cancel`, stream-registry operations — connection loop.
    Control,
    /// `chat_start` — worker-dispatched, streamed events.
    ChatStart,
    /// `chat_approval_resolve_start` — worker-dispatched, streamed events.
    ResolveStart,
    /// `project_root_stage_native` on a shell-kind connection (secret
    /// injected server-side) — worker-dispatched.
    ShellGated,
    /// Every other registry wire method — worker-dispatched.
    Registry,
    /// Not wire-reachable on this connection: `-32601` (unknown method or
    /// kind violation).
    Rejected,
}

/// The wire method table — the actual dispatch (pinned by the
/// accepted-method-table test in serve_protocol.rs).
pub fn wire_method_class(method: &str, kind: crate::handshake::ClientKind) -> WireClass {
    use crate::handshake::ClientKind;
    if SERVER_ORIGIN_METHODS.contains(&method) {
        return WireClass::Rejected;
    }
    if SUPERSEDED_CHAT_FAMILY.contains(&method) || NON_WIRE_CHANNELS.contains(&method) {
        return WireClass::Rejected;
    }
    match method {
        "hello" | "chat_cancel" => WireClass::Control,
        "chat_start" => WireClass::ChatStart,
        "chat_approval_resolve_start" => WireClass::ResolveStart,
        _ if SHELL_GATED_METHODS.contains(&method) => {
            if kind == ClientKind::Shell {
                WireClass::ShellGated
            } else {
                // Kind violation: the method is not in this connection's
                // allowed set (R4's `-32601` rule).
                WireClass::Rejected
            }
        }
        _ => WireClass::Registry,
    }
}

#[derive(Debug, Deserialize)]
pub struct IpcEnvelope {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct IpcReply {
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
