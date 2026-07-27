//! Bounded skills index for the system prompt.
//!
//! A model cannot call for a skill it does not know exists, so the *names* go
//! into the system prompt. The skill **body** is deliberately not included: it
//! still arrives through a tool call, which keeps the evidence trail and the
//! permission check on the path that authorizes use.
//!
//! The index is bounded on both axes — how many skills, and how much of each —
//! so a large registry cannot quietly consume the turn's context.

use optimus_skills::{SkillStatus, SkillView};

/// Most skills listed in the prompt.
pub(crate) const SKILL_INDEX_MAX: usize = 24;

/// Longest one-line summary kept per skill, in characters.
pub(crate) const SKILL_SUMMARY_CHARS: usize = 96;

/// Render the index block, or `None` when the registry has nothing to advertise.
pub(crate) fn skill_index_block(skills: &[SkillView]) -> String {
    let mut listed: Vec<&SkillView> = skills
        .iter()
        .filter(|skill| skill.status != SkillStatus::Deprecated)
        .collect();
    if listed.is_empty() {
        return String::new();
    }

    // Pinned first: pinning is human authority, so those belong in front of
    // whatever the promotion policy has merely proven.
    listed.sort_by(|a, b| {
        let rank = |s: &SkillView| match s.status {
            SkillStatus::Pinned => 0,
            _ => 1,
        };
        rank(a)
            .cmp(&rank(b))
            .then(
                b.success_rate
                    .partial_cmp(&a.success_rate)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.name.cmp(&b.name))
    });

    let total = listed.len();
    let shown = total.min(SKILL_INDEX_MAX);
    let mut out = String::from("\n\n## Skills index\n");
    for skill in listed.iter().take(shown) {
        out.push_str(&format!(
            "- {} (v{}, {}): {}\n",
            skill.name,
            skill.version,
            status_label(skill.status),
            summarize(&skill.body)
        ));
    }
    if total > shown {
        out.push_str(&format!("- … {} more not listed\n", total - shown));
    }
    out.push_str("Names only. Call skill_resolve for a body before following one.");
    out
}

fn status_label(status: SkillStatus) -> &'static str {
    match status {
        SkillStatus::Pinned => "pinned",
        SkillStatus::Proven => "proven",
        SkillStatus::Candidate => "candidate",
        SkillStatus::Deprecated => "deprecated",
    }
}

/// First non-empty line of the body, trimmed of heading markers and bounded.
fn summarize(body: &str) -> String {
    let line = body
        .lines()
        .map(|line| line.trim().trim_start_matches('#').trim())
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if line.chars().count() <= SKILL_SUMMARY_CHARS {
        return line.to_string();
    }
    let clipped: String = line.chars().take(SKILL_SUMMARY_CHARS).collect();
    format!("{}…", clipped.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn skill(name: &str, status: SkillStatus, body: &str, rate: f64) -> SkillView {
        SkillView {
            id: Uuid::new_v4(),
            name: name.into(),
            version: 1,
            status,
            body: body.into(),
            permissions: Vec::new(),
            uses: 0,
            successes: 0,
            failures: 0,
            total_tokens: 0,
            success_rate: rate,
        }
    }

    #[test]
    fn the_named_tool_is_one_optimus_packs_actually_advertises() {
        let skills = vec![skill("any", SkillStatus::Proven, "Does a thing", 1.0)];
        let block = skill_index_block(&skills);
        assert!(
            block.contains("skill_resolve"),
            "the prompt must name a real tool; skills_list does not exist"
        );
    }

    #[test]
    fn an_empty_registry_adds_nothing_to_the_prompt() {
        assert!(skill_index_block(&[]).is_empty());
    }

    #[test]
    fn deprecated_skills_are_not_advertised() {
        let skills = vec![skill("gone", SkillStatus::Deprecated, "Old way", 1.0)];
        assert!(skill_index_block(&skills).is_empty());
    }

    #[test]
    fn pinned_skills_are_listed_before_proven_ones() {
        let skills = vec![
            skill("beta", SkillStatus::Proven, "Proven approach", 1.0),
            skill("alpha", SkillStatus::Pinned, "Human-pinned approach", 0.1),
        ];
        let block = skill_index_block(&skills);
        let alpha = block.find("alpha").expect("alpha listed");
        let beta = block.find("beta").expect("beta listed");
        assert!(alpha < beta, "pinning is human authority and outranks rate");
    }

    #[test]
    fn the_body_stays_out_of_the_prompt() {
        let body = "Deploy the service\n\nStep 1: rotate the credential\nStep 2: restart";
        let skills = vec![skill("deploy", SkillStatus::Proven, body, 1.0)];
        let block = skill_index_block(&skills);
        assert!(block.contains("Deploy the service"));
        assert!(
            !block.contains("rotate the credential"),
            "only the first line is advertised; the body arrives by tool call"
        );
    }

    #[test]
    fn long_summaries_are_bounded() {
        let long = "x".repeat(400);
        let skills = vec![skill("verbose", SkillStatus::Proven, &long, 1.0)];
        let block = skill_index_block(&skills);
        assert!(block.contains('…'));
        assert!(block.len() < 400);
    }

    #[test]
    fn a_large_registry_is_capped_and_says_so() {
        let skills: Vec<SkillView> = (0..SKILL_INDEX_MAX + 5)
            .map(|i| {
                skill(
                    &format!("skill{i:02}"),
                    SkillStatus::Proven,
                    "Does a thing",
                    1.0,
                )
            })
            .collect();
        let block = skill_index_block(&skills);
        assert_eq!(block.matches("\n- skill").count(), SKILL_INDEX_MAX);
        assert!(block.contains("5 more not listed"));
    }
}
