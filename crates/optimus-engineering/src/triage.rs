//! The `TRIAGE` output contract (program P41, E41.4 and E41.5).
//!
//! `TRIAGE` already had to produce acceptance criteria and a reproduction
//! before it could exit. What it did not have was any requirement about what
//! those items *say*. `stated_by(navigator, AcceptanceCriteria, "it should
//! work")` satisfied the phase table exactly as well as a real contract does,
//! and the run then entered `IMPLEMENT` with nothing to satisfy — which is how
//! a plausible patch for an unstated problem gets written.
//!
//! Two things this module refuses to conflate.
//!
//! **A failed check is a triage to retry, not an issue to close.** Rejecting an
//! issue and failing a triage have the same symptom — the run does not proceed
//! — and completely different remedies. One closes somebody's bug report; the
//! other spends another attempt. So a [`TriageVerdict`] can only ever say the
//! *triage* is wrong. Closing an issue takes an explicit
//! [`TriageResult::Refused`], and a refusal is held to the same evidentiary
//! standard as a contract: "too vague" backed by a quote from the issue is a
//! claim, and "too vague" backed by nothing is a shrug.
//!
//! **Admissible is not good.** [`TriageVerdict::Admissible`] means nothing here
//! is demonstrably wrong. Deterministic code cannot tell whether acceptance
//! criteria are the *right* ones; it can tell that a quote is really in the
//! issue, that a named component really exists, and that a change touching
//! `.github/**` is not filed as low risk. Those are the claims this module
//! checks, and the ones it cannot check it does not pretend to.
//!
//! Every heuristic here may only move a result *toward* refusing the triage,
//! never toward admitting it — the same asymmetry
//! [ADR-0054](../../../docs/decisions/0054-a-selector-may-only-over-select.md)
//! gives the impact selector, for the same reason.

use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::phase::EvidenceKind;
use crate::repository::RepositoryPolicyProfile;
use crate::roles::RoleIdentity;
use crate::run::{EvidenceDraft, EvidenceItem};

/// Below this, a "quote" is a fragment that would match almost any issue, so
/// grounding it proves nothing. Measured after whitespace normalisation.
const MIN_QUOTE_CHARS: usize = 12;

/// How much of the repository's guard a change is expected to disturb.
///
/// Not a severity and not a priority: a risk class decides how much review a
/// patch needs, which is why a triage may not set it below what its own
/// component list implies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    /// Ordinary code, covered by the existing gates.
    Low,
    /// Touches a path the repository protects — CI, the gate scripts, the task
    /// runner, instruction files, keys.
    Sensitive,
    /// Changes a boundary: a crate's public surface, a decision recorded in an
    /// ADR, or the shape of the phase table itself.
    Architectural,
}

/// One acceptance criterion and the thing that will decide it.
///
/// `checked_by` is required because a criterion nobody can check is a wish. It
/// is free text — a command, a test path, an observable — since at triage time
/// the test may not exist yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceCriterion {
    pub statement: String,
    pub checked_by: String,
}

/// The triage's own estimate of how big the change is.
///
/// A self-report, and treated as one: a model has every incentive to
/// under-estimate here, so the size checks in [`check`] lean on the *grounded*
/// component list as well.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChangeScope {
    pub expected_files: usize,
}

/// Everything `TRIAGE` owes before a run may enter `INVESTIGATE` (E41.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriageOutput {
    /// The issue restated as a concrete problem.
    pub problem: String,
    /// Quotes from the issue. Checked against the issue text, so a triage
    /// cannot support its reading with something the reporter never wrote.
    pub evidence: Vec<String>,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    /// Repository-relative paths. Checked for existence.
    pub owning_components: Vec<PathBuf>,
    /// Repository-relative paths. May be empty — a new feature has no tests
    /// yet, and saying so is honest.
    pub relevant_tests: Vec<PathBuf>,
    pub risk: RiskClass,
    pub change_scope: ChangeScope,
    /// When to stop and ask, rather than keep trying. Must say something the
    /// acceptance criteria do not.
    pub stop_condition: String,
}

/// Why a triage will not produce a contract for this issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalReason {
    /// Nothing in the issue is concrete enough to write criteria against.
    TooVague,
    /// More than one task. Requires a proposed split (E41.5).
    TooLarge,
    /// Real and clear, but not this repository's to fix.
    OutOfScope,
}

impl RefusalReason {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TooVague => "too_vague",
            Self::TooLarge => "too_large",
            Self::OutOfScope => "out_of_scope",
        }
    }
}

/// What a triage produced: a contract, or a grounded refusal.
///
/// Refusal is a first-class result rather than something inferred from an
/// empty contract. A triage that returns nothing has failed; a triage that
/// says "this issue cannot be worked, here is the quote that shows it" has
/// done its job, and only the second one closes anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriageResult {
    Contract(TriageOutput),
    Refused {
        reason: RefusalReason,
        detail: String,
        /// Quotes from the issue supporting the refusal. Checked, like a
        /// contract's.
        evidence: Vec<String>,
        /// For `TooLarge`: the tasks this issue should become. At least two,
        /// because "split it" without a split is not a split.
        #[serde(default)]
        split_on: Vec<String>,
    },
}

impl TriageResult {
    /// Whether acting on this closes or splits the issue, as opposed to
    /// advancing the run.
    #[must_use]
    pub fn is_refusal(&self) -> bool {
        matches!(self, Self::Refused { .. })
    }
}

/// Something the triage said that the issue or the repository does not
/// support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ungrounded {
    /// A quote that is not in the issue.
    QuoteNotInIssue { quote: String },
    /// A quote too short to have located anything.
    QuoteTooShort { quote: String },
    /// A named path that does not exist.
    PathNotFound { field: &'static str, path: PathBuf },
    /// A component the repository protects, filed below `Sensitive`.
    RiskUnderstated { path: PathBuf, declared: RiskClass },
    /// A stop condition that only repeats an acceptance criterion.
    StopConditionRestatesCriterion,
    /// Demonstrably more than one task (E41.5): the contract should have been
    /// a `TooLarge` refusal with a proposed split.
    Oversized {
        measure: &'static str,
        found: usize,
        limit: usize,
    },
}

impl Ungrounded {
    #[must_use]
    pub fn explain(&self) -> String {
        match self {
            Self::QuoteNotInIssue { quote } => {
                format!("evidence quote is not in the issue text: {quote:?}")
            }
            Self::QuoteTooShort { quote } => format!(
                "evidence quote is under {MIN_QUOTE_CHARS} characters, so matching it proves \
                 nothing: {quote:?}"
            ),
            Self::PathNotFound { field, path } => {
                format!("{field} names {}, which does not exist", path.display())
            }
            Self::RiskUnderstated { path, declared } => format!(
                "{} is a protected path, so the risk class cannot be {declared:?}",
                path.display()
            ),
            Self::StopConditionRestatesCriterion => {
                "the stop condition repeats an acceptance criterion, so it says nothing about \
                 when to stop"
                    .to_string()
            }
            Self::Oversized {
                measure,
                found,
                limit,
            } => format!(
                "{measure} is {found}, over the limit of {limit} — this is more than one task, \
                 and belongs in a too_large refusal with a proposed split"
            ),
        }
    }
}

/// The outcome of checking a triage result.
///
/// Every variant except [`Admissible`](TriageVerdict::Admissible) blames the
/// *triage*. None of them blames the issue: closing an issue takes an explicit
/// [`TriageResult::Refused`] that passed these same checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriageVerdict {
    /// Nothing here is demonstrably wrong. Not a judgement that it is right.
    Admissible,
    /// Fields missing or blank — the triage did not finish. The issue is
    /// untouched and the phase has an attempt left.
    Incomplete { missing: Vec<&'static str> },
    /// The triage said things the issue or the repository does not support.
    Ungrounded(Vec<Ungrounded>),
}

impl TriageVerdict {
    #[must_use]
    pub fn is_admissible(&self) -> bool {
        matches!(self, Self::Admissible)
    }

    #[must_use]
    pub fn explain(&self) -> String {
        match self {
            Self::Admissible => "nothing in this triage is demonstrably wrong".to_string(),
            Self::Incomplete { missing } => {
                format!("the triage did not finish; missing: {}", missing.join(", "))
            }
            Self::Ungrounded(problems) => {
                let lines: Vec<String> = problems.iter().map(Ungrounded::explain).collect();
                format!("the triage is not grounded:\n  - {}", lines.join("\n  - "))
            }
        }
    }
}

/// Ceilings above which an issue is more than one task (E41.5).
///
/// A repository may lower these and may not raise them, the same way it may add
/// to the sensitive floor and not subtract from it. [`TriageLimits::tighten`]
/// is the only constructor besides the ceiling, and it takes the minimum of
/// each field, so weakening is not expressible rather than merely discouraged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriageLimits {
    pub max_components: usize,
    pub max_criteria: usize,
    pub max_files: usize,
}

impl TriageLimits {
    /// The ceiling. Three owning components is already a change that crosses
    /// boundaries; six acceptance criteria is usually several deliverables
    /// wearing one issue number.
    pub const CEILING: Self = Self {
        max_components: 3,
        max_criteria: 6,
        max_files: 20,
    };

    /// Element-wise minimum with the ceiling. There is no way to build a
    /// looser set.
    #[must_use]
    pub fn tighten(components: usize, criteria: usize, files: usize) -> Self {
        Self {
            max_components: components.min(Self::CEILING.max_components),
            max_criteria: criteria.min(Self::CEILING.max_criteria),
            max_files: files.min(Self::CEILING.max_files),
        }
    }
}

impl Default for TriageLimits {
    fn default() -> Self {
        Self::CEILING
    }
}

/// What the checks are performed against.
pub struct TriageContext<'a> {
    pub repo_root: &'a Path,
    /// The issue body as the forge returned it. Quotes are checked against
    /// this and nothing else.
    pub issue_body: &'a str,
    pub profile: &'a RepositoryPolicyProfile,
    pub limits: TriageLimits,
}

/// Check a triage result against its contract.
///
/// Presence first, then grounding, then size. Presence problems are reported
/// on their own: a contract missing its component list has nothing to ground,
/// and reporting twenty grounding failures caused by one missing field wastes
/// the retry it is meant to inform.
#[must_use]
pub fn check(result: &TriageResult, ctx: &TriageContext) -> TriageVerdict {
    match result {
        TriageResult::Contract(output) => check_contract(output, ctx),
        TriageResult::Refused {
            reason,
            detail,
            evidence,
            split_on,
        } => check_refusal(*reason, detail, evidence, split_on, ctx),
    }
}

fn check_contract(output: &TriageOutput, ctx: &TriageContext) -> TriageVerdict {
    let mut missing = Vec::new();
    if output.problem.trim().is_empty() {
        missing.push("problem");
    }
    if output.evidence.is_empty() {
        missing.push("evidence");
    }
    if output.acceptance_criteria.is_empty() {
        missing.push("acceptance_criteria");
    }
    if output
        .acceptance_criteria
        .iter()
        .any(|c| c.statement.trim().is_empty())
    {
        missing.push("acceptance_criteria[].statement");
    }
    if output
        .acceptance_criteria
        .iter()
        .any(|c| c.checked_by.trim().is_empty())
    {
        missing.push("acceptance_criteria[].checked_by");
    }
    if output.owning_components.is_empty() {
        missing.push("owning_components");
    }
    if output.stop_condition.trim().is_empty() {
        missing.push("stop_condition");
    }
    if !missing.is_empty() {
        return TriageVerdict::Incomplete { missing };
    }

    let mut problems = Vec::new();
    ground_quotes(&output.evidence, ctx.issue_body, &mut problems);

    for path in &output.owning_components {
        ground_path("owning_components", path, ctx.repo_root, &mut problems);
    }
    for path in &output.relevant_tests {
        ground_path("relevant_tests", path, ctx.repo_root, &mut problems);
    }

    if output.risk < RiskClass::Sensitive {
        if let Some(matcher) = sensitive_matcher(ctx.profile) {
            for path in &output.owning_components {
                if matcher.is_match(path) {
                    problems.push(Ungrounded::RiskUnderstated {
                        path: path.clone(),
                        declared: output.risk,
                    });
                }
            }
        }
    }

    let stop = normalise(&output.stop_condition);
    if output
        .acceptance_criteria
        .iter()
        .any(|c| normalise(&c.statement) == stop)
    {
        problems.push(Ungrounded::StopConditionRestatesCriterion);
    }

    // Size last: a contract that is over the line should have been a refusal,
    // and saying so is more useful once the rest of it has been checked.
    let grounded_components = output
        .owning_components
        .iter()
        .filter(|path| ctx.repo_root.join(path).exists())
        .count();
    if grounded_components > ctx.limits.max_components {
        problems.push(Ungrounded::Oversized {
            measure: "owning components",
            found: grounded_components,
            limit: ctx.limits.max_components,
        });
    }
    if output.acceptance_criteria.len() > ctx.limits.max_criteria {
        problems.push(Ungrounded::Oversized {
            measure: "acceptance criteria",
            found: output.acceptance_criteria.len(),
            limit: ctx.limits.max_criteria,
        });
    }
    if output.change_scope.expected_files > ctx.limits.max_files {
        problems.push(Ungrounded::Oversized {
            measure: "expected files",
            found: output.change_scope.expected_files,
            limit: ctx.limits.max_files,
        });
    }

    if problems.is_empty() {
        TriageVerdict::Admissible
    } else {
        TriageVerdict::Ungrounded(problems)
    }
}

fn check_refusal(
    reason: RefusalReason,
    detail: &str,
    evidence: &[String],
    split_on: &[String],
    ctx: &TriageContext,
) -> TriageVerdict {
    let mut missing = Vec::new();
    if detail.trim().is_empty() {
        missing.push("detail");
    }
    if evidence.is_empty() {
        missing.push("evidence");
    }
    if reason == RefusalReason::TooLarge && split_on.len() < 2 {
        missing.push("split_on");
    }
    if split_on.iter().any(|part| part.trim().is_empty()) {
        missing.push("split_on[]");
    }
    if !missing.is_empty() {
        return TriageVerdict::Incomplete { missing };
    }

    let mut problems = Vec::new();
    ground_quotes(evidence, ctx.issue_body, &mut problems);
    if problems.is_empty() {
        TriageVerdict::Admissible
    } else {
        TriageVerdict::Ungrounded(problems)
    }
}

/// Every quote must be findable in the issue, and long enough that finding it
/// meant something.
fn ground_quotes(quotes: &[String], issue_body: &str, problems: &mut Vec<Ungrounded>) {
    let haystack = normalise(issue_body);
    for quote in quotes {
        let needle = normalise(quote);
        if needle.chars().count() < MIN_QUOTE_CHARS {
            problems.push(Ungrounded::QuoteTooShort {
                quote: quote.clone(),
            });
        } else if !haystack.contains(&needle) {
            problems.push(Ungrounded::QuoteNotInIssue {
                quote: quote.clone(),
            });
        }
    }
}

fn ground_path(field: &'static str, path: &Path, repo_root: &Path, problems: &mut Vec<Ungrounded>) {
    if !repo_root.join(path).exists() {
        problems.push(Ungrounded::PathNotFound {
            field,
            path: path.to_path_buf(),
        });
    }
}

/// Whitespace-collapsed and lowercased, so a quote survives being re-wrapped
/// or re-cased on the way through a model.
fn normalise(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn sensitive_matcher(profile: &RepositoryPolicyProfile) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    let mut any = false;
    for pattern in &profile.sensitive_paths {
        if let Ok(glob) = Glob::new(pattern) {
            builder.add(glob);
            any = true;
        }
    }
    if !any {
        return None;
    }
    builder.build().ok()
}

impl TriageOutput {
    /// The evidence `TRIAGE` owes, built from a contract that passed [`check`].
    ///
    /// Taking the verdict as an argument rather than re-deriving it is
    /// deliberate: a caller has to have checked, and the type says so.
    ///
    /// # Errors
    /// The verdict, unchanged, when it was not [`TriageVerdict::Admissible`].
    pub fn evidence_drafts(
        &self,
        author: RoleIdentity,
        verdict: &TriageVerdict,
    ) -> Result<Vec<EvidenceDraft>, TriageVerdict> {
        if !verdict.is_admissible() {
            return Err(verdict.clone());
        }
        let criteria = self
            .acceptance_criteria
            .iter()
            .map(|c| format!("{} (checked by: {})", c.statement, c.checked_by))
            .collect::<Vec<_>>()
            .join("; ");
        Ok(vec![
            EvidenceItem::stated_by(
                author.clone(),
                EvidenceKind::AcceptanceCriteria,
                format!("{} — stop when: {}", criteria, self.stop_condition),
            ),
            EvidenceItem::stated_by(
                author,
                EvidenceKind::Reproduction,
                format!(
                    "{} — from the issue: {}",
                    self.problem,
                    self.evidence.join(" / ")
                ),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{BranchProtection, VerificationCommands};
    use crate::roles::Role;

    const ISSUE: &str = "The evidence panel scrolls to the top whenever a new run \
        starts, which loses the row I was reading. Reproduced on Linux with two \
        concurrent runs.";

    fn profile() -> RepositoryPolicyProfile {
        RepositoryPolicyProfile {
            default_branch: Some("main".into()),
            protection: BranchProtection::Unprotected,
            pr_template: None,
            instruction_files: Vec::new(),
            sensitive_paths: vec![".github/**".into(), "justfile".into()],
            verification: VerificationCommands::default(),
        }
    }

    fn contract() -> TriageOutput {
        TriageOutput {
            problem: "the evidence panel resets its scroll position on a new run".into(),
            evidence: vec!["scrolls to the top whenever a new run starts".into()],
            acceptance_criteria: vec![AcceptanceCriterion {
                statement: "a new run does not move the evidence panel's scroll position".into(),
                checked_by: "apps/optimus-ui vitest".into(),
            }],
            owning_components: vec![PathBuf::from("crates")],
            relevant_tests: vec![PathBuf::from("scripts")],
            risk: RiskClass::Low,
            change_scope: ChangeScope { expected_files: 2 },
            stop_condition: "two attempts without the panel holding position".into(),
        }
    }

    fn context<'a>(root: &'a Path, profile: &'a RepositoryPolicyProfile) -> TriageContext<'a> {
        TriageContext {
            repo_root: root,
            issue_body: ISSUE,
            profile,
            limits: TriageLimits::CEILING,
        }
    }

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .to_path_buf()
    }

    #[test]
    fn a_complete_grounded_contract_is_admissible() {
        let p = profile();
        let verdict = check(&TriageResult::Contract(contract()), &context(&root(), &p));
        assert_eq!(verdict, TriageVerdict::Admissible, "{}", verdict.explain());
    }

    #[test]
    fn a_criterion_nobody_can_check_is_incomplete() {
        let mut output = contract();
        output.acceptance_criteria[0].checked_by = "  ".into();
        let p = profile();
        let verdict = check(&TriageResult::Contract(output), &context(&root(), &p));
        assert!(
            matches!(&verdict, TriageVerdict::Incomplete { missing }
                if missing.contains(&"acceptance_criteria[].checked_by")),
            "{verdict:?}"
        );
    }

    #[test]
    fn evidence_the_reporter_never_wrote_is_ungrounded() {
        let mut output = contract();
        output.evidence = vec!["the panel crashes the whole application".into()];
        let p = profile();
        let verdict = check(&TriageResult::Contract(output), &context(&root(), &p));
        assert!(
            matches!(&verdict, TriageVerdict::Ungrounded(problems)
                if problems.iter().any(|x| matches!(x, Ungrounded::QuoteNotInIssue { .. }))),
            "{verdict:?}"
        );
    }

    #[test]
    fn a_quote_short_enough_to_match_anything_grounds_nothing() {
        let mut output = contract();
        output.evidence = vec!["the".into()];
        let p = profile();
        let verdict = check(&TriageResult::Contract(output), &context(&root(), &p));
        assert!(
            matches!(&verdict, TriageVerdict::Ungrounded(problems)
                if problems.iter().any(|x| matches!(x, Ungrounded::QuoteTooShort { .. }))),
            "{verdict:?}"
        );
    }

    #[test]
    fn a_rewrapped_quote_still_grounds() {
        // Models re-wrap and re-case. The quote is the reporter's words, not
        // their whitespace.
        let mut output = contract();
        output.evidence = vec!["Scrolls   To The Top\n  Whenever A New Run Starts".into()];
        let p = profile();
        let verdict = check(&TriageResult::Contract(output), &context(&root(), &p));
        assert_eq!(verdict, TriageVerdict::Admissible, "{}", verdict.explain());
    }

    #[test]
    fn a_component_that_does_not_exist_is_ungrounded() {
        let mut output = contract();
        output.owning_components = vec![PathBuf::from("crates/optimus-evidence-panel/src/lib.rs")];
        let p = profile();
        let verdict = check(&TriageResult::Contract(output), &context(&root(), &p));
        assert!(
            matches!(&verdict, TriageVerdict::Ungrounded(problems)
                if problems.iter().any(|x| matches!(
                    x, Ungrounded::PathNotFound { field: "owning_components", .. }))),
            "{verdict:?}"
        );
    }

    #[test]
    fn a_protected_component_cannot_be_filed_as_low_risk() {
        let mut output = contract();
        output.owning_components = vec![PathBuf::from("justfile")];
        let p = profile();
        let verdict = check(&TriageResult::Contract(output), &context(&root(), &p));
        assert!(
            matches!(&verdict, TriageVerdict::Ungrounded(problems)
                if problems.iter().any(|x| matches!(x, Ungrounded::RiskUnderstated { .. }))),
            "{verdict:?}"
        );
    }

    #[test]
    fn the_same_protected_component_at_sensitive_is_fine() {
        let mut output = contract();
        output.owning_components = vec![PathBuf::from("justfile")];
        output.risk = RiskClass::Sensitive;
        let p = profile();
        let verdict = check(&TriageResult::Contract(output), &context(&root(), &p));
        assert_eq!(verdict, TriageVerdict::Admissible, "{}", verdict.explain());
    }

    #[test]
    fn a_stop_condition_that_repeats_a_criterion_is_not_a_stop_condition() {
        let mut output = contract();
        output.stop_condition = output.acceptance_criteria[0].statement.to_uppercase();
        let p = profile();
        let verdict = check(&TriageResult::Contract(output), &context(&root(), &p));
        assert!(
            matches!(&verdict, TriageVerdict::Ungrounded(problems)
                if problems.contains(&Ungrounded::StopConditionRestatesCriterion)),
            "{verdict:?}"
        );
    }

    #[test]
    fn a_contract_that_is_really_a_programme_is_sent_back_to_be_split() {
        let mut output = contract();
        output.owning_components = vec![
            PathBuf::from("crates"),
            PathBuf::from("apps"),
            PathBuf::from("docs"),
            PathBuf::from("scripts"),
        ];
        let p = profile();
        let verdict = check(&TriageResult::Contract(output), &context(&root(), &p));
        assert!(
            matches!(&verdict, TriageVerdict::Ungrounded(problems)
                if problems.iter().any(|x| matches!(
                    x, Ungrounded::Oversized { measure: "owning components", .. }))),
            "{verdict:?}"
        );
    }

    #[test]
    fn limits_can_be_tightened_and_cannot_be_loosened() {
        let tighter = TriageLimits::tighten(1, 1, 1);
        assert_eq!(tighter.max_components, 1);
        let attempted = TriageLimits::tighten(99, 99, 99);
        assert_eq!(attempted, TriageLimits::CEILING);
    }

    #[test]
    fn a_refusal_without_a_quote_is_a_shrug() {
        let p = profile();
        let verdict = check(
            &TriageResult::Refused {
                reason: RefusalReason::TooVague,
                detail: "not enough information".into(),
                evidence: Vec::new(),
                split_on: Vec::new(),
            },
            &context(&root(), &p),
        );
        assert!(
            matches!(&verdict, TriageVerdict::Incomplete { missing } if missing.contains(&"evidence")),
            "{verdict:?}"
        );
    }

    #[test]
    fn a_refusal_cannot_invent_what_the_issue_said() {
        let p = profile();
        let verdict = check(
            &TriageResult::Refused {
                reason: RefusalReason::TooVague,
                detail: "no reproduction offered".into(),
                evidence: vec!["it does not work sometimes, no further detail".into()],
                split_on: Vec::new(),
            },
            &context(&root(), &p),
        );
        assert!(
            matches!(&verdict, TriageVerdict::Ungrounded(problems)
                if problems.iter().any(|x| matches!(x, Ungrounded::QuoteNotInIssue { .. }))),
            "{verdict:?}"
        );
    }

    #[test]
    fn too_large_without_a_split_is_not_a_split() {
        let p = profile();
        let verdict = check(
            &TriageResult::Refused {
                reason: RefusalReason::TooLarge,
                detail: "this is four features".into(),
                evidence: vec!["scrolls to the top whenever a new run starts".into()],
                split_on: vec!["only one part".into()],
            },
            &context(&root(), &p),
        );
        assert!(
            matches!(&verdict, TriageVerdict::Incomplete { missing } if missing.contains(&"split_on")),
            "{verdict:?}"
        );
    }

    #[test]
    fn a_grounded_refusal_is_admissible() {
        let p = profile();
        let verdict = check(
            &TriageResult::Refused {
                reason: RefusalReason::TooLarge,
                detail: "the panel fix and the concurrency work are separate".into(),
                evidence: vec!["Reproduced on Linux with two concurrent runs".into()],
                split_on: vec![
                    "hold the scroll position".into(),
                    "two concurrent runs".into(),
                ],
            },
            &context(&root(), &p),
        );
        assert_eq!(verdict, TriageVerdict::Admissible, "{}", verdict.explain());
    }

    #[test]
    fn no_verdict_ever_blames_the_issue() {
        // The property the module exists for: a check can say the triage is
        // wrong and can never, by itself, close somebody's bug report.
        let p = profile();
        let mut broken = contract();
        broken.evidence = vec!["invented, and long enough to be checked".into()];
        broken.owning_components = vec![PathBuf::from("no/such/place")];
        let verdicts = [
            check(&TriageResult::Contract(broken), &context(&root(), &p)),
            check(
                &TriageResult::Contract(TriageOutput {
                    problem: String::new(),
                    ..contract()
                }),
                &context(&root(), &p),
            ),
        ];
        for verdict in verdicts {
            assert!(
                matches!(
                    verdict,
                    TriageVerdict::Incomplete { .. } | TriageVerdict::Ungrounded(_)
                ),
                "a verdict may only blame the triage: {verdict:?}"
            );
        }
    }

    #[test]
    fn evidence_drafts_are_refused_unless_the_contract_was_admitted() {
        let output = contract();
        let author = RoleIdentity::new(Role::Navigator, "session-nav");
        let refused = output
            .evidence_drafts(
                author.clone(),
                &TriageVerdict::Incomplete {
                    missing: vec!["problem"],
                },
            )
            .expect_err("an unadmitted contract produces no evidence");
        assert!(matches!(refused, TriageVerdict::Incomplete { .. }));

        let drafts = output
            .evidence_drafts(author, &TriageVerdict::Admissible)
            .expect("an admitted contract produces its evidence");
        assert_eq!(drafts.len(), 2);
    }
}
