//! Shaping a model call: reasoning effort, fast mode, pack names.
//!
//! Split out of `lib.rs` under the module-size law.

use super::*;

/// Normalize UI thinking levels for ChatGPT Codex OAuth.
/// Supported backend values: none, minimal, low, medium, high, xhigh, max.
pub fn normalize_thinking_level(level: Option<&str>) -> Option<String> {
    let raw = level?.trim().to_ascii_lowercase();
    match raw.as_str() {
        "" | "off" | "none" | "false" | "0" => None,
        "minimal" | "min" => Some("minimal".into()),
        "low" | "medium" | "high" | "xhigh" | "max" => Some(raw),
        "x-high" | "extra" | "extra_high" => Some("xhigh".into()),
        // Codex OAuth has no "ultra"; map to max (highest supported).
        "ultra" | "maximum" => Some("max".into()),
        other => Some(other.to_string()),
    }
}

/// Apply Fast mode: prefer lower latency by capping effort.
pub fn apply_fast_mode(effort: Option<String>, fast: bool) -> Option<String> {
    if !fast {
        return effort;
    }
    match effort.as_deref() {
        None => Some("low".into()),
        Some("high") | Some("xhigh") | Some("max") => Some("medium".into()),
        other => other.map(|s| s.to_string()),
    }
}

pub(crate) fn pack_names(packs: &CapabilitySession) -> Vec<String> {
    packs
        .loaded_packs()
        .into_iter()
        .map(|p| p.as_str().to_string())
        .collect()
}
