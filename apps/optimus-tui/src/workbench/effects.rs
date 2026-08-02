//! What a tool call is allowed to have done, read from the canonical contract.
//!
//! Grouping hides individual calls behind one header, so the rule for what may
//! be hidden cannot be a list of tool names kept in this surface's head — that
//! list drifts the moment a pack adds a tool, and it drifts silently in the
//! direction of hiding an effect. The pack catalog already declares a
//! [`ToolPolicy`] per tool and `optimus-packs` is the canonical tool contract,
//! so the classification is read from there and anything the catalog does not
//! know is treated as unfoldable.

use std::collections::HashMap;
use std::sync::OnceLock;

use optimus_packs::{builtin_catalog, ToolPolicy};

/// Declared policy of every built-in tool, keyed by the id lifecycle events
/// carry. Built once: the catalog is a pure function of the binary.
fn policies() -> &'static HashMap<String, ToolPolicy> {
    static POLICIES: OnceLock<HashMap<String, ToolPolicy>> = OnceLock::new();
    POLICIES.get_or_init(|| {
        builtin_catalog()
            .values()
            .flat_map(|pack| pack.tools.iter())
            .map(|tool| (tool.id.as_str().to_string(), tool.policy))
            .collect()
    })
}

/// Whether repeated calls to this tool may be folded away by default.
///
/// True only for the policies that read and do not change anything: a
/// workspace read, a memory or skill lookup, a web search. Everything else
/// stays one visible row per call, and an id the catalog does not carry — an
/// extension tool, a future pack — is unknown rather than assumed harmless.
///
/// `Browser` is deliberately excluded even though a snapshot only observes:
/// navigating and clicking share that one policy, so the contract cannot tell
/// the observation from the effect. Browser work gets its own block kind with
/// its own evidence in a later phase rather than being folded away here.
pub fn is_observation(tool_id: &str) -> bool {
    matches!(
        policies().get(tool_id),
        Some(
            ToolPolicy::WorkspaceRead
                | ToolPolicy::MemoryRead
                | ToolPolicy::SkillRead
                | ToolPolicy::NetworkRead
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_lookups_may_be_folded_away() {
        for tool in [
            "read_file",
            "list_dir",
            "find_files",
            "search_content",
            "web_search",
            "memory_recall",
            "skill_resolve",
        ] {
            assert!(is_observation(tool), "{tool} only observes");
        }
    }

    #[test]
    fn nothing_that_changes_anything_is_ever_folded_away() {
        for tool in [
            "write_file",
            "patch_file",
            "delete_path",
            "rename_path",
            "mkdir",
            "terminal",
            "activate_pack",
        ] {
            assert!(
                !is_observation(tool),
                "{tool} has an effect and must stay one visible row"
            );
        }
    }

    #[test]
    fn browser_work_is_not_folded_because_the_policy_cannot_tell_look_from_touch() {
        for tool in ["browser_navigate", "browser_snapshot", "browser_click"] {
            assert!(!is_observation(tool), "{tool}");
        }
    }

    #[test]
    fn a_tool_the_catalog_does_not_know_is_unknown_not_harmless() {
        assert!(!is_observation("some_extension_tool"));
        assert!(!is_observation(""));
    }

    /// The classification is derived, so it cannot silently disagree with the
    /// contract it claims to read.
    #[test]
    fn every_classified_tool_is_one_the_catalog_actually_declares() {
        let catalog = policies();
        assert!(
            catalog.len() >= 10,
            "the built-in catalog should be non-trivial: {}",
            catalog.len()
        );
        for (id, policy) in catalog {
            assert_eq!(
                is_observation(id),
                matches!(
                    policy,
                    ToolPolicy::WorkspaceRead
                        | ToolPolicy::MemoryRead
                        | ToolPolicy::SkillRead
                        | ToolPolicy::NetworkRead
                ),
                "{id} classified against its own declared policy {policy:?}"
            );
        }
    }
}
