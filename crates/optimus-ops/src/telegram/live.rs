//! The live Bot API transport behind [`super::TelegramTransport`].
//!
//! This is a client, not a server: every call is outbound to
//! `api.telegram.org`, and the parent module's promise that the adapter never
//! opens a listen port survives intact. Long polling is what buys that — the
//! process holds a request open rather than being called back.
//!
//! Three things here are contracts rather than implementation details:
//!
//! - **A refusal and a silence are different answers.** Telegram declining a
//!   request it understood (4xx) means nothing was sent, so the obligation goes
//!   back to the pending pool for a bounded retry. Telegram failing partway
//!   (5xx), or the connection dying, means this process does not know, and
//!   ADR-0070 says an unknown outcome is the operator's to close — never a
//!   retry, because a retry of a send that landed is a duplicate message to a
//!   real person.
//! - **The token is a path segment.** It sits in every request URL, and `ureq`
//!   quotes the URL in the errors it returns. Every string leaving this module
//!   is redacted before it becomes a `TelegramError`, a log line, or a `detail`
//!   column an operator will paste into a bug report.
//! - **A long reply is several messages.** The Bot API caps one `sendMessage` at
//!   4096 UTF-16 code units, so a reply past that is split rather than
//!   truncated — the chat gets the whole answer. Splitting changes what a
//!   failure means, and the classification below says how.

use std::time::Duration;

use serde_json::{json, Value};

use super::{Result, SendOutcome, TelegramError, TelegramTransport, TelegramUpdate};

/// Public Bot API root. Every method hangs off `<base>/bot<token>/<method>`.
const DEFAULT_API_BASE: &str = "https://api.telegram.org";

/// What replaces the credential in anything a human or a log will read.
const REDACTED: &str = "<redacted>";

/// The Bot API's hard cap on one `sendMessage`, counted in UTF-16 code units.
const MAX_MESSAGE_UNITS: usize = 4096;

/// Updates to ask for per poll. Small enough that one slow turn does not sit on
/// a large batch, large enough that a burst does not need many round trips.
const UPDATE_BATCH: u64 = 25;

/// Long-poll hold bounds. Zero is short polling, which the API documentation
/// calls out as hammering Telegram's servers; past a minute the request starts
/// outliving connection reapers between here and there.
const MIN_POLL_HOLD_SECS: u64 = 30;
const MAX_POLL_HOLD_SECS: u64 = 60;

/// Slack the local read gets over the hold Telegram was asked for, so a poll
/// that returns exactly on time is not mistaken for a dead connection.
const POLL_MARGIN_SECS: u64 = 15;

/// How long one send may run before this process stops claiming to know its
/// outcome. Well inside `SEND_LEASE_SECS`, so the answer arrives before the
/// stale-lease sweep would have called the obligation unknown anyway.
const SEND_TIMEOUT_SECS: u64 = 30;

/// The only update kind this adapter can turn into a turn. Asking for just this
/// keeps edits, joins, and channel posts out of the queue instead of enqueuing
/// work for a reader that does not exist.
const ALLOWED_UPDATES: [&str; 1] = ["message"];

/// Telegram's Bot API over HTTPS.
///
/// Every field is safe to print: the credential is never one of them, only the
/// name of the variable it lives in.
#[derive(Debug)]
pub struct LiveTelegramTransport {
    /// Name of the environment variable holding the token — never the token.
    /// Reading it per call means a rotated credential takes effect on the next
    /// request rather than the next restart.
    token_env: String,
    api_base: String,
    poll_hold_secs: u64,
    poll_agent: ureq::Agent,
    send_agent: ureq::Agent,
}

impl LiveTelegramTransport {
    /// Build a transport against the real Bot API.
    pub fn new(token_env: &str, poll_hold_secs: u64) -> Result<Self> {
        Self::with_api_base(token_env, poll_hold_secs, DEFAULT_API_BASE)
    }

    /// Build a transport against `api_base` instead of Telegram.
    ///
    /// This exists so the request this module actually constructs can be
    /// asserted against a real socket. A mock of the transport would only prove
    /// the tests agree with themselves about the shape of a Bot API call.
    pub fn with_api_base(token_env: &str, poll_hold_secs: u64, api_base: &str) -> Result<Self> {
        // Fail here rather than at delivery time. A missing token discovered
        // mid-send is an obligation nobody can settle; discovered at startup it
        // is an operator seeing an error on the command they just ran.
        token(token_env)?;
        let poll_hold_secs = poll_hold_secs.clamp(MIN_POLL_HOLD_SECS, MAX_POLL_HOLD_SECS);
        Ok(Self {
            token_env: token_env.to_string(),
            api_base: api_base.trim_end_matches('/').to_string(),
            poll_hold_secs,
            poll_agent: agent_with_timeout(poll_hold_secs + POLL_MARGIN_SECS),
            send_agent: agent_with_timeout(SEND_TIMEOUT_SECS),
        })
    }

    /// How long each poll asks Telegram to hold the connection open.
    pub fn poll_hold_secs(&self) -> u64 {
        self.poll_hold_secs
    }

    fn call(&self, agent: &ureq::Agent, method: &str, body: Value) -> ApiOutcome {
        let token = match token(&self.token_env) {
            Ok(token) => token,
            // A credential that vanished between construction and now cannot
            // have sent anything, so this is definite rather than unknown.
            Err(error) => {
                return ApiOutcome::Refused {
                    detail: error.to_string(),
                }
            }
        };
        let url = format!("{}/bot{}/{}", self.api_base, token, method);
        match agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(body)
        {
            Ok(response) => match response.into_string() {
                Ok(text) => read_envelope(&text),
                // The call was made and answered; only the answer was lost.
                Err(error) => ApiOutcome::Unknown {
                    detail: redact(&error.to_string(), &token),
                },
            },
            Err(ureq::Error::Status(code, response)) => {
                let body = response.into_string().unwrap_or_default();
                let detail = redact(
                    &describe(&body).unwrap_or_else(|| format!("HTTP {code}")),
                    &token,
                );
                if (500..600).contains(&code) {
                    ApiOutcome::Unknown { detail }
                } else {
                    ApiOutcome::Refused { detail }
                }
            }
            Err(error) => ApiOutcome::Unknown {
                detail: redact(&error.to_string(), &token),
            },
        }
    }
}

impl TelegramTransport for LiveTelegramTransport {
    fn get_updates(&mut self, offset: u64) -> Result<Vec<TelegramUpdate>> {
        let body = json!({
            "offset": offset,
            "limit": UPDATE_BATCH,
            "timeout": self.poll_hold_secs,
            "allowed_updates": ALLOWED_UPDATES,
        });
        // Polling owns no ledger entry, so refused and unknown collapse to the
        // same thing here: this cycle learned nothing, acknowledged nothing, and
        // the next one asks again from the same offset. Nothing is lost by
        // failing loudly, and the caller decides whether to keep going.
        let result = match self.call(&self.poll_agent, "getUpdates", body) {
            ApiOutcome::Answered(result) => result,
            ApiOutcome::Refused { detail } | ApiOutcome::Unknown { detail } => {
                return Err(TelegramError::Msg(format!("getUpdates: {detail}")))
            }
        };
        Ok(result
            .as_array()
            .map(|updates| updates.iter().filter_map(read_update).collect())
            .unwrap_or_default())
    }

    fn send_message(&mut self, chat_id: &str, text: &str) -> Result<SendOutcome> {
        let parts = split_for_send(text);
        if parts.is_empty() {
            // Definite, and it stays definite however many times it is retried.
            // Better a bounded retry that dead-letters than a Bot API rejection
            // an operator has to decode.
            return Ok(SendOutcome::Failed {
                detail: "refusing to send an empty message".into(),
            });
        }
        let total = parts.len();
        let mut ids = Vec::with_capacity(total);
        for (index, part) in parts.iter().enumerate() {
            let body = json!({ "chat_id": chat_id, "text": part });
            match self.call(&self.send_agent, "sendMessage", body) {
                ApiOutcome::Answered(result) => ids.push(message_id(&result)),
                // Before the first part lands, the whole reply is still safe to
                // retry. After it lands it is not: retrying would repeat what
                // the chat already has, so a mid-reply failure is exactly the
                // unknown an operator exists to close.
                ApiOutcome::Refused { detail } if index == 0 => {
                    return Ok(SendOutcome::Failed { detail })
                }
                ApiOutcome::Unknown { detail } if index == 0 => {
                    return Ok(SendOutcome::Ambiguous { detail })
                }
                ApiOutcome::Refused { detail } | ApiOutcome::Unknown { detail } => {
                    return Ok(SendOutcome::Ambiguous {
                        detail: format!(
                            "part {} of {total} failed after {index} sent: {detail}",
                            index + 1
                        ),
                    })
                }
            }
        }
        Ok(SendOutcome::Confirmed {
            provider_message_id: ids.join(","),
        })
    }
}

/// What one Bot API call produced, before any caller reads meaning into it.
enum ApiOutcome {
    /// The platform answered and said yes. Carries the `result` field.
    Answered(Value),
    /// The platform answered and said no. Nothing was done.
    Refused { detail: String },
    /// No answer this process can trust. Whether anything was done is unknown.
    Unknown { detail: String },
}

fn agent_with_timeout(secs: u64) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(secs))
        .build()
}

/// The bot token, read from the environment variable named by `env_name`.
fn token(env_name: &str) -> Result<String> {
    let value = std::env::var(env_name).unwrap_or_default();
    let value = value.trim();
    if value.is_empty() {
        return Err(TelegramError::Msg(format!(
            "live telegram needs a bot token in ${env_name}"
        )));
    }
    Ok(value.to_string())
}

/// `text` with the bot credential removed, however it got in.
///
/// The literal replacement is the exact case. The `/bot…/` pass covers the rest:
/// a URL that reached the string in a form the literal no longer matches — a
/// truncation, a rotated token, an escape — still cannot carry a credential out
/// of this module.
fn redact(text: &str, token: &str) -> String {
    let text = text.replace(token, REDACTED);
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(start) = rest.find("/bot") {
        out.push_str(&rest[..start]);
        out.push_str("/bot");
        let after = &rest[start + "/bot".len()..];
        let end = after.find('/').unwrap_or(after.len());
        if end > 0 {
            out.push_str(REDACTED);
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// Read the `{ok, result, description}` envelope every Bot API method returns.
fn read_envelope(text: &str) -> ApiOutcome {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        // A 2xx this process cannot read is still a call Telegram accepted, so
        // the send may well have landed.
        return ApiOutcome::Unknown {
            detail: "unreadable Bot API response".into(),
        };
    };
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        return ApiOutcome::Answered(value.get("result").cloned().unwrap_or(Value::Null));
    }
    ApiOutcome::Refused {
        detail: describe(text).unwrap_or_else(|| "Bot API refused the call".into()),
    }
}

/// The `description` an error body carries, when it has one.
fn describe(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    let description = value.get("description")?.as_str()?.to_string();
    match value
        .pointer("/parameters/retry_after")
        .and_then(Value::as_i64)
    {
        Some(after) => Some(format!("{description} (retry_after {after}s)")),
        None => Some(description),
    }
}

/// The id Telegram assigned the message it just accepted.
fn message_id(result: &Value) -> String {
    result
        .get("message_id")
        .and_then(Value::as_i64)
        .map(|id| id.to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// One `Update` reduced to what a turn needs.
///
/// An update with nothing readable in it still comes back, with empty text: its
/// `update_id` is what advances the offset, and dropping it here would leave
/// Telegram redelivering the same unreadable update forever. The caller skips
/// blank text after advancing.
fn read_update(value: &Value) -> Option<TelegramUpdate> {
    let update_id = value.get("update_id").and_then(Value::as_u64)?;
    let message = value.get("message");
    Some(TelegramUpdate {
        update_id,
        chat_id: message
            .and_then(|message| message.pointer("/chat/id"))
            .map(render_chat_id)
            .unwrap_or_default(),
        text: message
            .and_then(|message| message.get("text"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        from_username: message
            .and_then(|message| message.pointer("/from/username"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// Chat ids are integers wider than they look and negative for groups, so they
/// are carried as the string Telegram will accept back rather than re-parsed.
fn render_chat_id(id: &Value) -> String {
    id.as_i64()
        .map(|id| id.to_string())
        .unwrap_or_else(|| id.as_str().unwrap_or_default().to_string())
}

/// Split `text` into parts Telegram will accept, preferring a paragraph break,
/// then a word break, and only then cutting mid-word.
///
/// The budget is UTF-16 code units because that is what the Bot API counts: an
/// emoji is two of them, so a reply comfortably under 4096 `char`s can still be
/// refused.
fn split_for_send(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut rest = text;
    while utf16_len(rest) > MAX_MESSAGE_UNITS {
        let hard = boundary_at(rest, MAX_MESSAGE_UNITS);
        let head = &rest[..hard];
        // Only take a natural break in the back half of the window. A break
        // near the start would spend a whole message on a few words and push
        // the rest of the reply into more parts than it needs.
        let cut = head
            .rfind('\n')
            .or_else(|| head.rfind(' '))
            .filter(|index| index * 2 > hard)
            .map(|index| index + 1)
            .unwrap_or(hard);
        parts.push(rest[..cut].to_string());
        rest = &rest[cut..];
    }
    if !rest.trim().is_empty() {
        parts.push(rest.to_string());
    }
    parts
}

/// The largest byte index at or before `units` UTF-16 code units that is also a
/// character boundary.
fn boundary_at(text: &str, units: usize) -> usize {
    let mut used = 0;
    for (index, ch) in text.char_indices() {
        let next = used + ch.len_utf16();
        if next > units {
            return index;
        }
        used = next;
    }
    text.len()
}

fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_reply_is_one_message() {
        assert_eq!(split_for_send("hello"), vec!["hello".to_string()]);
    }

    #[test]
    fn an_empty_reply_produces_nothing_to_send() {
        assert!(split_for_send("").is_empty());
        assert!(split_for_send("   \n ").is_empty());
    }

    #[test]
    fn a_long_reply_is_split_rather_than_truncated() {
        let body = "word ".repeat(2_000);
        let parts = split_for_send(&body);
        assert!(parts.len() > 1, "expected a split, got {}", parts.len());
        for part in &parts {
            assert!(utf16_len(part) <= MAX_MESSAGE_UNITS);
        }
        assert_eq!(parts.concat(), body, "splitting lost or reordered text");
    }

    #[test]
    fn splitting_prefers_a_paragraph_break() {
        let body = format!("{}\n{}", "a".repeat(4_000), "b".repeat(400));
        let parts = split_for_send(&body);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].ends_with('\n'));
        assert_eq!(parts[1], "b".repeat(400));
    }

    #[test]
    fn the_budget_counts_utf16_units_not_chars() {
        // Every emoji here is one `char` and two UTF-16 units, so a body that is
        // half the cap in `char`s is exactly at the cap in what Telegram counts.
        let body = "😀".repeat(MAX_MESSAGE_UNITS / 2 + 10);
        let parts = split_for_send(&body);
        assert!(parts.len() > 1);
        for part in &parts {
            assert!(utf16_len(part) <= MAX_MESSAGE_UNITS);
        }
        assert_eq!(parts.concat(), body);
    }

    #[test]
    fn text_with_no_break_at_all_still_splits() {
        let body = "x".repeat(MAX_MESSAGE_UNITS + 5);
        let parts = split_for_send(&body);
        assert_eq!(parts.len(), 2);
        assert_eq!(utf16_len(&parts[0]), MAX_MESSAGE_UNITS);
    }

    #[test]
    fn a_token_never_survives_redaction() {
        let token = "123456:AAH-secret-token";
        let raw = format!("https://api.telegram.org/bot{token}/sendMessage: timed out");
        let clean = redact(&raw, token);
        assert!(!clean.contains(token), "{clean}");
        assert!(clean.contains("/bot<redacted>/sendMessage"), "{clean}");
    }

    #[test]
    fn a_url_path_is_blanked_even_when_the_token_does_not_match() {
        let clean = redact("POST /bot999:OTHER/getUpdates failed", "123:MINE");
        assert!(!clean.contains("999:OTHER"), "{clean}");
    }

    #[test]
    fn redaction_is_idempotent_and_terminates() {
        let once = redact("/bot123:X/getUpdates", "123:X");
        assert_eq!(redact(&once, "123:X"), once);
    }

    #[test]
    fn a_refusal_carries_telegrams_own_description() {
        let ApiOutcome::Refused { detail } = read_envelope(
            r#"{"ok":false,"error_code":400,"description":"Bad Request: chat not found"}"#,
        ) else {
            panic!("ok:false must be a refusal");
        };
        assert_eq!(detail, "Bad Request: chat not found");
    }

    #[test]
    fn a_rate_limit_keeps_the_wait_telegram_asked_for() {
        let detail = describe(
            r#"{"ok":false,"description":"Too Many Requests","parameters":{"retry_after":30}}"#,
        )
        .expect("description");
        assert_eq!(detail, "Too Many Requests (retry_after 30s)");
    }

    #[test]
    fn an_unreadable_success_is_unknown_not_confirmed() {
        assert!(matches!(
            read_envelope("<html>502</html>"),
            ApiOutcome::Unknown { .. }
        ));
    }

    #[test]
    fn an_update_with_no_text_still_carries_its_id() {
        let update = read_update(&json!({
            "update_id": 41,
            "message": { "message_id": 7, "chat": { "id": -100200 } }
        }))
        .expect("an update with an id is readable");
        assert_eq!(update.update_id, 41);
        assert_eq!(update.chat_id, "-100200");
        assert!(update.text.is_empty());
    }

    #[test]
    fn a_text_update_reads_chat_and_sender() {
        let update = read_update(&json!({
            "update_id": 9,
            "message": {
                "message_id": 1,
                "chat": { "id": 42 },
                "from": { "username": "ada" },
                "text": "status?"
            }
        }))
        .expect("readable");
        assert_eq!(update.chat_id, "42");
        assert_eq!(update.text, "status?");
        assert_eq!(update.from_username.as_deref(), Some("ada"));
    }

    #[test]
    fn a_missing_token_is_refused_at_construction() {
        let error = LiveTelegramTransport::new("OPTIMUS_TELEGRAM_TOKEN_ABSENT_FOR_TEST", 30)
            .expect_err("no token means no transport");
        assert!(error.to_string().contains("bot token"), "{error}");
    }
}
