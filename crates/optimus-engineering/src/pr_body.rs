//! The pull-request body as a rendering of the run record (program P44,
//! E44.3's core; ADR-0058 rule 6).
//!
//! A PR body is prose, and prose is where a model asserts things no command
//! observed. So the body has no prose parameter anywhere: [`body_from_evidence`]
//! is the only producer, it reads the run record and nothing else, and every
//! claim line cites the evidence row that backs it. "Unsupported claims
//! rejected" is not a checker here — it is the absence of anything to write an
//! unsupported claim with.
//!
//! Filling the repository's own PR template shape is the rest of E44.3 and
//! deliberately not attempted yet: a template's sections are a request for
//! prose, and the honest content is what the record holds.

use crate::phase::EvidenceKind;
use crate::run::{DevTaskRun, TaskOrigin};

use EvidenceKind as E;

/// The PR title, from the task origin and nowhere else.
#[must_use]
pub fn title_from_origin(origin: &TaskOrigin) -> String {
    match origin {
        TaskOrigin::Issue { title, .. } => title.trim().to_string(),
        TaskOrigin::Request { text } => {
            let line = text.lines().next().unwrap_or("").trim();
            line.chars().take(72).collect()
        }
    }
}

/// Render the run record as a pull-request body.
///
/// Every claim line is derived from a corroborating evidence item and cites
/// its sequence number, so a reviewer can hold any sentence against the row
/// that backs it. Items that did not corroborate — a red run, an unconfirmed
/// receipt — never render as claims; the record keeps them, the body does not
/// repeat them as achievements. Nor does a *superseded* green: only evidence
/// from each phase's final attempt renders, because a run that repaired and
/// re-verified must not present the pre-repair attempt's passing runs as
/// claims about the tree that ships.
#[must_use]
pub fn body_from_evidence(run: &DevTaskRun) -> String {
    const SECTIONS: &[(&str, &[EvidenceKind])] = &[
        (
            "Summary",
            &[E::ProblemStatement, E::Reproduction, E::AcceptanceCriteria],
        ),
        ("Change", &[E::Diff, E::RegressionTest]),
        (
            "Verification",
            &[
                E::BaselineVerification,
                E::FocusedTestRun,
                E::DifferentialProof,
                E::FullVerification,
            ],
        ),
        ("Review", &[E::ReviewFindings]),
    ];

    let mut body = String::from(
        "<!-- Rendered from the run record; every claim cites the evidence row that backs it. -->\n",
    );
    for (heading, kinds) in SECTIONS {
        let mut lines: Vec<String> = Vec::new();
        if *heading == "Summary" {
            lines.push(match &run.origin {
                TaskOrigin::Issue { number, title } => {
                    format!("- Issue #{number}: {}", escape_markdown(title.trim()))
                }
                TaskOrigin::Request { text } => {
                    format!(
                        "- Requested directly: {}",
                        escape_markdown(text.lines().next().unwrap_or("").trim())
                    )
                }
            });
        }
        let final_attempt = |item: &crate::run::EvidenceItem| {
            run.phase_attempts.get(&item.phase).copied().unwrap_or(1) == item.attempt
        };
        for item in run
            .evidence
            .iter()
            .filter(|item| item.corroborates && final_attempt(item))
        {
            if !kinds.contains(&item.kind) {
                continue;
            }
            lines.push(match (&item.command, &item.sha) {
                (Some(command), Some(sha)) => format!(
                    "- `{command}` — exit {} at {} (evidence {})",
                    item.exit_status.unwrap_or_default(),
                    short(sha),
                    item.seq
                ),
                _ => format!(
                    "- {} ({}, evidence {})",
                    escape_markdown(item.summary.lines().next().unwrap_or("(no summary)")),
                    item.author.role.as_str(),
                    item.seq
                ),
            });
        }
        if !lines.is_empty() {
            body.push_str(&format!("\n## {heading}\n\n"));
            for line in lines {
                body.push_str(&line);
                body.push('\n');
            }
        }
    }
    body
}

/// Backslash-escape the markdown that matters in untrusted text — an issue
/// title is any GitHub user's words, and an unterminated `<!--` in one would
/// swallow every line after it, evidence citations included, in the rendered
/// body while the raw text still carried them. CommonMark honours a backslash
/// before any ASCII punctuation, so the text stays readable and stops being
/// markup.
fn escape_markdown(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            let escape = matches!(
                c,
                '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '<' | '>' | '#' | '@' | '|'
            );
            escape.then_some('\\').into_iter().chain(std::iter::once(c))
        })
        .collect()
}

/// Twelve characters of a SHA for prose; enough to find, short enough to read.
pub(crate) fn short(sha: &str) -> &str {
    if sha.len() >= 12 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
        &sha[..12]
    } else {
        sha
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::DevPhase;
    use crate::roles::{Role, RoleIdentity};
    use crate::run::EvidenceItem;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn the_body_claims_exactly_what_corroborated_and_cites_it() {
        let mut run = DevTaskRun::new(
            "t-body",
            TaskOrigin::Issue {
                number: 86,
                title: "the divider drifts".into(),
            },
            "/repo",
        );
        run.record(EvidenceItem::stated_by(
            RoleIdentity::new(Role::Navigator, "nav-1"),
            E::ProblemStatement,
            "the divider drifts right on resize",
        ))
        .unwrap();
        // A command that passed, a command that failed: only one may become a
        // claim.
        run.phase = DevPhase::FullVerify;
        run.record(EvidenceItem::observed(
            E::FullVerification,
            "just verify",
            0,
            SHA,
            b"all green",
        ))
        .unwrap();
        run.record(EvidenceItem::observed(
            E::FullVerification,
            "just verify --broken",
            1,
            SHA,
            b"red",
        ))
        .unwrap();

        let body = body_from_evidence(&run);
        assert!(body.contains("- Issue #86: the divider drifts"));
        assert!(
            body.contains(&format!(
                "- `just verify` — exit 0 at {} (evidence 2)",
                short(SHA)
            )),
            "{body}"
        );
        assert!(
            !body.contains("--broken"),
            "a red run must not render as a claim: {body}"
        );
        assert!(
            body.contains("(navigator, evidence 1)"),
            "an asserted claim carries its role and row: {body}"
        );
        // Sections with nothing corroborated do not appear at all.
        assert!(!body.contains("## Review"), "{body}");
    }

    #[test]
    fn a_superseded_attempts_green_run_is_not_a_claim() {
        let mut run = DevTaskRun::new(
            "t-attempts",
            TaskOrigin::Request { text: "fix".into() },
            "/repo",
        );
        run.phase = DevPhase::FullVerify;
        run.phase_attempts.insert(DevPhase::FullVerify, 1);
        run.record(EvidenceItem::observed(
            E::FullVerification,
            "just verify --first-attempt",
            0,
            SHA,
            b"green, then superseded",
        ))
        .unwrap();
        // The phase is re-entered: repair happened, and the old green is about
        // a tree that no longer exists.
        run.phase_attempts.insert(DevPhase::FullVerify, 2);
        run.record(EvidenceItem::observed(
            E::FullVerification,
            "just verify --second-attempt",
            0,
            SHA,
            b"green at the tree that ships",
        ))
        .unwrap();

        let body = body_from_evidence(&run);
        assert!(body.contains("--second-attempt"), "{body}");
        assert!(
            !body.contains("--first-attempt"),
            "a superseded green must not render as a claim: {body}"
        );
    }

    #[test]
    fn untrusted_words_render_as_text_not_markup() {
        let run = DevTaskRun::new(
            "t-escape",
            TaskOrigin::Issue {
                number: 7,
                title: "crash <!-- @everyone see `rm -rf` [here](x)".into(),
            },
            "/repo",
        );
        let body = body_from_evidence(&run);
        // `\<` cannot open an HTML block, so an unterminated comment in a
        // title cannot swallow the evidence lines after it.
        assert!(
            body.contains(r"crash \<!--"),
            "the '<' must be escaped where the title enters the body: {body}"
        );
        assert!(
            !body.contains("crash <!--"),
            "the raw markup must not survive: {body}"
        );
        assert!(body.contains(r"\@everyone"), "{body}");
        assert!(body.contains(r"\`rm -rf\`"), "{body}");
        assert!(body.contains(r"\[here\](x)"), "{body}");
    }

    #[test]
    fn the_title_comes_from_the_origin_and_nowhere_else() {
        assert_eq!(
            title_from_origin(&TaskOrigin::Issue {
                number: 86,
                title: "  the divider drifts  ".into(),
            }),
            "the divider drifts"
        );
        let long = "x".repeat(100);
        assert_eq!(
            title_from_origin(&TaskOrigin::Request { text: long }).len(),
            72
        );
    }
}
