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

/// R8 latency shaping: cap reasoning effort at `low` for every step after the
/// first of a fresh turn. The caller keeps the user's choice for the first
/// step (fresh turn only — an approval-resumed turn re-enters with recorded
/// calls and is capped from its first resumed step).
///
/// Ordering: `off` < `minimal` < `low` < everything above. `off` is never
/// upgraded; `auto`/`None` becomes `low`; unknown levels are treated as
/// above `low` and capped, so a provider-specific high-effort label cannot
/// slip through.
pub fn cap_effort_for_later_steps(effort: Option<String>) -> Option<String> {
    let Some(level) = effort else {
        return Some("low".into());
    };
    let above_low = !matches!(level.as_str(), "off" | "minimal" | "low");
    if above_low {
        Some("low".into())
    } else {
        Some(level)
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
    use super::{apply_fast_mode, cap_effort_for_later_steps, normalize_thinking_level};

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

    #[test]
    fn later_steps_cap_effort_at_low_but_never_upgrade_off() {
        // R8: every step after the first of a fresh turn caps at `low`.
        assert_eq!(
            cap_effort_for_later_steps(Some("high".into())),
            Some("low".into())
        );
        assert_eq!(
            cap_effort_for_later_steps(Some("medium".into())),
            Some("low".into())
        );
        assert_eq!(
            cap_effort_for_later_steps(Some("xhigh".into())),
            Some("low".into())
        );
        assert_eq!(
            cap_effort_for_later_steps(Some("max".into())),
            Some("low".into())
        );
        assert_eq!(
            cap_effort_for_later_steps(Some("low".into())),
            Some("low".into())
        );
        // `minimal` and `off` are at or below the cap and never upgraded.
        assert_eq!(
            cap_effort_for_later_steps(Some("minimal".into())),
            Some("minimal".into())
        );
        assert_eq!(
            cap_effort_for_later_steps(Some("off".into())),
            Some("off".into())
        );
        // `auto`/`None` resolves to the cap.
        assert_eq!(cap_effort_for_later_steps(None), Some("low".into()));
    }

    #[test]
    fn fast_mode_applies_before_the_step_cap() {
        // Fast mode caps `high` at `medium`; the step cap then lands on `low`.
        assert_eq!(
            cap_effort_for_later_steps(apply_fast_mode(Some("high".into()), true)),
            Some("low".into())
        );
        // Off survives both.
        assert_eq!(
            cap_effort_for_later_steps(apply_fast_mode(Some("off".into()), true)),
            Some("off".into())
        );
    }
}
