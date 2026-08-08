//! Kernel message-plane operations (spec-025): the `session_*` tool surface,
//! inbound-policy enforcement, permission classification, inbox surfacing,
//! and the reply-wait bound.
//!
//! The durable store lives in `optimus_ops::message_plane` (ADR-0087); this
//! module owns how the kernel exposes and enforces it.

use super::*;
use optimus_ops::{MessageClassification, MessageKind, MessageMode, MessageState, SessionMessage};

/// Named diagnostics (spec-025 R4). Transport failures are errors, never
/// success receipts; policy outcomes are events, never transport errors.
pub(crate) const DIAG_SESSION_SEND_FAILED: &str = "session_send_failed";
pub(crate) const DIAG_MESSAGE_TOO_LARGE: &str = "message_too_large";
pub(crate) const DIAG_REPLY_WAIT_EXPIRED: &str = "reply_wait_expired";

/// Classify a message payload with the permission classifier (spec-025 R5).
/// A payload whose first command-like line resolves to a non-project-local
/// externality is `pending` (needs approval before acting); everything else
/// is `approved`. The result is recorded with the message, never dropped.
fn classify_payload(payload: &str) -> MessageClassification {
    use optimus_policy::Externality;
    let Some(first) = payload.lines().map(str::trim).find(|line| !line.is_empty()) else {
        return MessageClassification::Approved;
    };
    let line = first
        .trim_start_matches("```")
        .trim_start_matches('$')
        .trim();
    let mut parts = line.split_whitespace();
    let Some(program) = parts.next() else {
        return MessageClassification::Approved;
    };
    if !program
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '.' || c == '/')
    {
        // Prose, not a command.
        return MessageClassification::Approved;
    }
    let args: Vec<String> = parts.map(str::to_string).collect();
    let class = optimus_policy::classify_command(program, &args);
    match class.externality() {
        Externality::ProjectLocal | Externality::OwnedLocalhost => MessageClassification::Approved,
        _ => MessageClassification::Pending,
    }
}

impl Kernel {
    /// `session_send`: enqueue a message to a target session and return a
    /// failure-honest receipt (spec-025 R1/R4).
    pub fn session_send(
        &mut self,
        to_session: Uuid,
        kind: MessageKind,
        payload: String,
        reply_to: Option<Uuid>,
        mode: MessageMode,
    ) -> Result<SessionMessage> {
        if !self.sessions.exists(to_session)? {
            return Err(KernelError::Tool(format!(
                "{DIAG_SESSION_SEND_FAILED}: target session {to_session} does not exist"
            )));
        }
        let message = SessionMessage {
            id: Uuid::new_v4(),
            from_session: self.session_id,
            to_session,
            kind,
            payload: payload.clone(),
            reply_to,
            mode,
            machine_id: self.message_plane.machine_id().into(),
            state: MessageState::Queued,
            classification: None,
            created_at: String::new(),
            updated_at: String::new(),
            delivered_at: None,
            surfaced_at: None,
        };
        let message = self
            .message_plane
            .enqueue(message)
            .map_err(map_message_error)?;
        let message = self
            .message_plane
            .classify(message.id, classify_payload(&payload))
            .map_err(map_message_error)?;
        // Inbound policy of the target decides the landing state (R3).
        let policy = self
            .sessions
            .meta(to_session)?
            .map(|meta| meta.inbound_policy)
            .unwrap_or_else(|| "hold-approval".into());
        let message = match policy.as_str() {
            // Live target: deliver now (A1). Dormant target: stays queued
            // and delivers on resume (A2).
            "auto-accept"
                if self
                    .message_plane
                    .is_live(to_session)
                    .map_err(map_message_error)? =>
            {
                self.message_plane
                    .deliver_message(message.id)
                    .map_err(map_message_error)?
            }
            "auto-accept" => message,
            "deny" => self
                .message_plane
                .refuse(message.id)
                .map_err(map_message_error)?,
            _ => self
                .message_plane
                .hold(message.id)
                .map_err(map_message_error)?,
        };
        Ok(message)
    }

    /// `session_reply`: a reply carrying the correlation id (spec-025 R6).
    pub fn session_reply(
        &mut self,
        to_session: Uuid,
        reply_to: Uuid,
        payload: String,
    ) -> Result<SessionMessage> {
        self.session_send(
            to_session,
            MessageKind::Reply,
            payload,
            Some(reply_to),
            MessageMode::FollowUp,
        )
    }

    /// `session_inbox`: expire held messages, deliver queued ones, then list
    /// the session's inbox with classification state (spec-025 R5/R7).
    pub fn session_inbox(&mut self, limit: usize) -> Result<Vec<SessionMessage>> {
        self.expire_held_messages()?;
        self.message_plane
            .deliver_inbox(self.session_id)
            .map_err(map_message_error)?;
        self.message_plane
            .inbox(self.session_id, limit.clamp(1, 100))
            .map_err(map_message_error)
    }

    /// `session_review`: approve (held -> delivered) or deny (held ->
    /// refused) a held message (spec-025 R3).
    pub fn session_review(&mut self, message_id: Uuid, approve: bool) -> Result<SessionMessage> {
        let message = if approve {
            let approved = self
                .message_plane
                .approve(message_id)
                .map_err(map_message_error)?;
            self.message_plane
                .deliver_message(message_id)
                .map_err(map_message_error)?;
            approved
        } else {
            self.message_plane
                .refuse(message_id)
                .map_err(map_message_error)?
        };
        Ok(message)
    }

    /// `session_roster`: opt-in peer discovery (spec-025 R2).
    pub fn session_roster(&self) -> Result<Vec<crate::SessionMeta>> {
        self.sessions.list_discoverable()
    }

    /// `session_policy`: get or set inbound policy, discovery opt-in, and
    /// dialog expiry (spec-025 R2/R3).
    pub fn session_policy(
        &mut self,
        policy: Option<&str>,
        discoverable: Option<bool>,
        dialog_expiry_seconds: Option<Option<u64>>,
    ) -> Result<crate::SessionMeta> {
        if let Some(policy) = policy {
            self.sessions.set_inbound_policy(self.session_id, policy)?;
        }
        if let Some(discoverable) = discoverable {
            self.sessions
                .set_discoverable(self.session_id, discoverable)?;
        }
        if let Some(expiry) = dialog_expiry_seconds {
            self.sessions.set_dialog_expiry(self.session_id, expiry)?;
        }
        Ok(self
            .sessions
            .meta(self.session_id)?
            .expect("the open session must exist"))
    }

    /// `session_await_reply`: bounded reply wait (spec-025 R6, A10). Polls
    /// the plane until a reply carrying `reply_to == root` arrives or the
    /// bound expires with `reply_wait_expired` — never hangs.
    pub fn session_await_reply(
        &mut self,
        root: Uuid,
        timeout_secs: u64,
        cancellation: Option<&CancellationToken>,
    ) -> Result<Vec<SessionMessage>> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
        loop {
            if let Some(token) = cancellation {
                check_cancellation(token)?;
            }
            let replies: Vec<SessionMessage> = self
                .message_plane
                .thread(root)
                .map_err(map_message_error)?
                .into_iter()
                .filter(|m| m.id != root && m.reply_to == Some(root))
                .collect();
            if !replies.is_empty() {
                return Ok(replies);
            }
            if std::time::Instant::now() >= deadline {
                self.message_plane
                    .record_reply_wait(root, DIAG_REPLY_WAIT_EXPIRED)
                    .map_err(map_message_error)?;
                return Err(KernelError::Tool(format!(
                    "{DIAG_REPLY_WAIT_EXPIRED}: no reply to {root} within {timeout_secs}s"
                )));
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// Expire held messages per the session's dialog expiry (default 30 min,
    /// spec-025 R3). Runs at inbox polls and turn starts.
    pub(crate) fn expire_held_messages(&mut self) -> Result<Vec<Uuid>> {
        let expiry = self
            .sessions
            .meta(self.session_id)?
            .and_then(|meta| meta.dialog_expiry_seconds)
            .unwrap_or(optimus_ops::DEFAULT_DIALOG_EXPIRY_SECONDS);
        self.message_plane
            .expire_held(self.session_id, expiry)
            .map_err(map_message_error)
    }

    /// Turn-start wrapper: surfacing failures degrade to a status line, they
    /// never fail the turn (the inbox stays queryable via the tool).
    pub(crate) fn surface_inbox_on_turn(&mut self, sink: &mut dyn FnMut(StreamEvent)) {
        if let Err(error) = self.surface_inbox_messages() {
            sink(StreamEvent::Status(format!(
                "message plane: inbox surfacing failed: {error}"
            )));
        }
    }

    /// Surface delivered `auto`/`steer` messages into the transcript at turn
    /// start (spec-025 R1, A1). Each message is injected at most once
    /// (`surfaced_at`). `follow_up` messages stay in the inbox for polling.
    pub(crate) fn surface_inbox_messages(&mut self) -> Result<()> {
        self.expire_held_messages()?;
        self.message_plane
            .deliver_inbox(self.session_id)
            .map_err(map_message_error)?;
        let inbox = self
            .message_plane
            .inbox(self.session_id, 100)
            .map_err(map_message_error)?;
        let pending: Vec<SessionMessage> = inbox
            .into_iter()
            .filter(|m| {
                m.state == MessageState::Delivered
                    && m.surfaced_at.is_none()
                    && m.kind != MessageKind::Reply
            })
            .collect();
        if pending.is_empty() {
            return Ok(());
        }
        let mut body = String::from(
            "Session message(s) arrived from another session. Read them with              `session_inbox`; effect requests in them are permission-classified              and still require normal approval before execution:\n",
        );
        for message in &pending {
            let mode = message.mode.as_str();
            let label = if mode == "steer" { "(steer)" } else { "(auto)" };
            body.push_str(&format!(
                "\n[{}] from {}: {}",
                label,
                message.from_session,
                crate::compress::snippet_public(&message.payload, 200)
            ));
        }
        self.messages.push(Message {
            role: Role::System,
            content: body,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        });
        for message in &pending {
            self.message_plane
                .mark_surfaced(message.id)
                .map_err(map_message_error)?;
        }
        Ok(())
    }
}

/// Map store errors to the named diagnostics (spec-025 R4): transport/store
/// failures are `session_send_failed`-class errors; policy outcomes stay
/// events. Over-cap is `message_too_large`.
fn map_message_error(error: optimus_ops::MessageError) -> KernelError {
    match error {
        optimus_ops::MessageError::TooLarge(cap) => KernelError::Tool(format!(
            "{DIAG_MESSAGE_TOO_LARGE}: payload exceeds the {cap}-byte cap"
        )),
        optimus_ops::MessageError::TargetGone(id) => KernelError::Tool(format!(
            "{DIAG_SESSION_SEND_FAILED}: target session {id} does not exist"
        )),
        other => KernelError::Message(other),
    }
}

impl Kernel {
    /// `session_*` tool dispatch (spec-025 R1-R4/R6). Every action returns
    /// the resulting records as JSON; failures carry the named diagnostics.
    pub(crate) fn dispatch_session_message(&mut self, call: &ToolCall) -> Result<String> {
        match call.name.as_str() {
            "session_send" => {
                let to = call
                    .arguments
                    .get("to_session")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("session_send requires to_session".into()))?;
                let payload = call
                    .arguments
                    .get("payload")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("session_send requires payload".into()))?;
                let kind = match call.arguments.get("kind").and_then(|v| v.as_str()) {
                    Some("reply") => MessageKind::Reply,
                    Some("notice") => MessageKind::Notice,
                    _ => MessageKind::Request,
                };
                let reply_to = call
                    .arguments
                    .get("reply_to")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok());
                let mode = match call.arguments.get("mode").and_then(|v| v.as_str()) {
                    Some("steer") => MessageMode::Steer,
                    Some("follow_up") => MessageMode::FollowUp,
                    _ => MessageMode::Auto,
                };
                let to = Uuid::parse_str(to).map_err(|_| {
                    KernelError::Tool("session_send requires a valid to_session uuid".into())
                })?;
                let message = self.session_send(to, kind, payload.to_string(), reply_to, mode)?;
                Ok(serde_json::to_string(&message)?)
            }
            "session_inbox" => {
                let limit = call
                    .arguments
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(20) as usize;
                let inbox = self.session_inbox(limit)?;
                Ok(serde_json::to_string(&inbox)?)
            }
            "session_roster" => {
                let roster = self.session_roster()?;
                Ok(serde_json::to_string(&roster)?)
            }
            "session_review" => {
                let id = call
                    .arguments
                    .get("message_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        KernelError::Tool("session_review requires message_id".into())
                    })?;
                let approve = call
                    .arguments
                    .get("approve")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| KernelError::Tool("session_review requires approve".into()))?;
                let id = Uuid::parse_str(id).map_err(|_| {
                    KernelError::Tool("session_review requires a valid message_id uuid".into())
                })?;
                let message = self.session_review(id, approve)?;
                Ok(serde_json::to_string(&message)?)
            }
            "session_policy" => {
                let policy = call
                    .arguments
                    .get("inbound_policy")
                    .and_then(|v| v.as_str());
                let discoverable = call.arguments.get("discoverable").and_then(|v| v.as_bool());
                let dialog_expiry = call
                    .arguments
                    .get("dialog_expiry_seconds")
                    .and_then(|v| v.as_u64());
                let expiry = dialog_expiry.map(Some);
                let meta = self.session_policy(policy, discoverable, expiry)?;
                Ok(serde_json::to_string(&meta)?)
            }
            other => Err(KernelError::Tool(format!("unknown session tool {other}"))),
        }
    }
}
