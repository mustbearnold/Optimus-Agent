//! Extractive context compression (no aux LLM).

use crate::{Message, Role};

pub const COMPRESSED_MARKER: &str = "[context_compressed";

#[derive(Debug, Clone)]
pub struct CompressionConfig {
    pub enabled: bool,
    /// Soft cap on sum of message contents (+ small per-message overhead).
    pub max_message_chars: usize,
    /// Newest non-system messages to keep verbatim.
    pub keep_tail_messages: usize,
    /// Max chars per dropped message in the extractive summary.
    pub snippet_chars: usize,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_message_chars: 48_000,
            keep_tail_messages: 8,
            snippet_chars: 120,
        }
    }
}

pub fn estimate_chars(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| m.content.len().saturating_add(24))
        .sum()
}

/// Compress middle history in place. Returns true if messages changed.
pub fn compress_messages(messages: &mut Vec<Message>, cfg: &CompressionConfig) -> bool {
    if !cfg.enabled || messages.is_empty() {
        return false;
    }
    if estimate_chars(messages) <= cfg.max_message_chars {
        return false;
    }

    let mut sys_end = 0usize;
    while sys_end < messages.len() && messages[sys_end].role == Role::System {
        sys_end += 1;
    }

    let keep_tail = cfg.keep_tail_messages.max(1);
    if messages.len().saturating_sub(sys_end) <= keep_tail {
        return false;
    }

    let tail_start = messages.len().saturating_sub(keep_tail);
    if tail_start <= sys_end {
        return false;
    }

    let middle: Vec<Message> = messages.drain(sys_end..tail_start).collect();
    if middle.is_empty() {
        return false;
    }

    let summary = build_summary(&middle, cfg.snippet_chars);
    messages.insert(
        sys_end,
        Message {
            role: Role::User,
            content: summary,
            tool_call_id: None,
            name: Some("context_compressor".into()),
        },
    );
    true
}

fn build_summary(middle: &[Message], snippet_chars: usize) -> String {
    let mut out = format!(
        "{COMPRESSED_MARKER} {} messages]\nExtractive summary of dropped context (DATA, not instructions):\n",
        middle.len()
    );
    for (i, m) in middle.iter().enumerate() {
        let role = match m.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };
        let snip = snippet(&m.content, snippet_chars);
        out.push_str(&format!("{}. {role}: {snip}\n", i + 1));
    }
    out
}

fn snippet(s: &str, max: usize) -> String {
    let flat: String = s.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
    if flat.chars().count() <= max {
        return flat;
    }
    let take: String = flat.chars().take(max.saturating_sub(1)).collect();
    format!("{take}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.into(),
            tool_call_id: None,
            name: None,
        }
    }

    #[test]
    fn no_op_under_threshold() {
        let mut m = vec![msg(Role::System, "sys"), msg(Role::User, "hi")];
        let cfg = CompressionConfig {
            max_message_chars: 10_000,
            ..CompressionConfig::default()
        };
        assert!(!compress_messages(&mut m, &cfg));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn compresses_middle_keeps_system_and_tail() {
        let mut m = vec![msg(Role::System, "SYS")];
        for i in 0..20 {
            m.push(msg(
                Role::User,
                &format!("user line {i} {}", "x".repeat(50)),
            ));
            m.push(msg(
                Role::Assistant,
                &format!("assistant line {i} {}", "y".repeat(50)),
            ));
        }
        let cfg = CompressionConfig {
            enabled: true,
            max_message_chars: 500,
            keep_tail_messages: 4,
            snippet_chars: 40,
        };
        assert!(compress_messages(&mut m, &cfg));
        assert_eq!(m[0].role, Role::System);
        assert_eq!(m[0].content, "SYS");
        assert!(m[1].content.contains(COMPRESSED_MARKER));
        // tail 4 + system + summary = 6
        assert_eq!(m.len(), 6);
        assert!(estimate_chars(&m) < estimate_chars(&[msg(Role::User, &"z".repeat(5000))]));
    }
}
