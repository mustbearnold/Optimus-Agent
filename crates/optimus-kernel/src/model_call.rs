//! Shaping a model call: reasoning effort, fast mode, pack names.
//!
//! Split out of `lib.rs` under the module-size law.

use super::*;

/// Normalize UI thinking levels for ChatGPT Codex OAuth and DeepSeek V4.
/// `auto` deliberately becomes None: each provider chooses its documented
/// default instead of receiving a made-up effort string. `off` remains an
/// explicit sentinel so DeepSeek can disable thinking rather than silently
/// falling back to its enabled default.
pub fn normalize_thinking_level(level: Option<&str>) -> Option<String> {
    let raw = level?.trim().to_ascii_lowercase();
    match raw.as_str() {
        "" | "auto" => None,
        "off" | "none" | "false" | "0" => Some("off".into()),
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

#[cfg(test)]
mod tests {
    use super::{apply_fast_mode, normalize_thinking_level};

    #[test]
    fn auto_omits_provider_specific_effort() {
        assert_eq!(normalize_thinking_level(Some("auto")), None);
        assert_eq!(normalize_thinking_level(Some(" AUTO ")), None);
        assert_eq!(apply_fast_mode(None, false), None);
        assert_eq!(normalize_thinking_level(Some("off")), Some("off".into()));
    }

    #[test]
    fn legacy_and_maximum_labels_remain_compatible() {
        assert_eq!(
            normalize_thinking_level(Some("minimal")),
            Some("minimal".into())
        );
        assert_eq!(normalize_thinking_level(Some("ultra")), Some("max".into()));
        assert_eq!(
            normalize_thinking_level(Some("x-high")),
            Some("xhigh".into())
        );
    }
}
