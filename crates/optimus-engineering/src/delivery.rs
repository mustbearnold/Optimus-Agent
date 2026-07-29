//! Publishing a run: the push, the draft pull request, and the sentence a
//! human approved (program P44, E44.1/E44.2; ADR-0058).
//!
//! Everything before `READY_TO_PUBLISH` happens inside a worktree the run
//! owns. Everything here happens somewhere a mistake cannot be deleted — so
//! every rule is a type or a refusal rather than an instruction:
//!
//! 1. **The approval is a sentence, and the record holds it.** A
//!    [`PublishPlan`] renders its consequence once; a yes is recorded as
//!    `HumanApproval` evidence whose summary *is* that sentence. Publishing
//!    refuses unless the record already holds those exact words. The sentence
//!    embeds the commit, so a worktree that moved after approval produces a
//!    different sentence and the old approval covers nothing.
//! 2. **The push publishes the approved commit, not the branch tip.** The
//!    refspec is `<sha>:refs/heads/<branch>`. A commit that landed after
//!    approval stays local instead of riding out on a branch-name push.
//! 3. **Deleting, renaming and forcing are unconstructible.** The refspec is
//!    built, never accepted; names that would smuggle a second meaning are
//!    refused at construction with the reason named; there is no force field
//!    and no delete function. Renaming or deleting a remote PR head closes
//!    the PR, so the operation does not exist here.
//! 4. **An effect is confirmed by reading it back.** `git push` exiting zero
//!    is transport, not truth: `git ls-remote` must report the approved
//!    commit at the branch, and `gh pr view` must report that head on the
//!    created PR. Only the confirmed pair corroborates
//!    ([`EvidenceItem::observed_confirmed`]).
//! 5. **The PR number is parsed, never chosen.** GitHub assigns it; output
//!    without one is a refusal, not a guess.
//! 6. **The body has no prose parameter.** [`crate::pr_body::body_from_evidence`]
//!    renders the run record, each claim citing the evidence row that backs
//!    it, and the publisher calls it itself — there is no argument through
//!    which prose could arrive.

use std::path::PathBuf;
use std::time::Duration;

use crate::command::{CommandError, CommandOutcome, CommandRunner};
use crate::controller::{describe, store_output};
use crate::phase::{DevPhase, EvidenceKind};
use crate::pr_body::{body_from_evidence, short, title_from_origin};
use crate::publish_plan::PublishPlan;
use crate::run::{DevTaskRun, EvidenceItem, RunError};

use EvidenceKind as E;

/// A push moves objects; give a cold connection room without letting the
/// phase hang on it.
const PUSH_TIMEOUT: Duration = Duration::from_secs(300);
/// One forge API call.
const FORGE_TIMEOUT: Duration = Duration::from_secs(120);
/// A local git query, or a single-ref `ls-remote`.
pub(crate) const QUERY_TIMEOUT: Duration = Duration::from_secs(30);

/// What `ls-remote` found when the branch was not there at all. A named
/// absence, so a mismatch error never renders an empty string as a SHA.
const ABSENT: &str = "(absent)";

/// Why a delivery step was refused or could not be believed. Every variant is
/// a fact about the plan, the record, or the forge — not an opinion.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    #[error("branch name {branch:?} is refused: {reason}")]
    ForbiddenBranch {
        branch: String,
        reason: &'static str,
    },
    #[error("remote name {remote:?} is refused: {reason}")]
    ForbiddenRemote {
        remote: String,
        reason: &'static str,
    },
    #[error(
        "{given:?} is not a full lowercase commit SHA — an abbreviation could \
         resolve differently on the remote, and the approved sentence must \
         name exactly one commit"
    )]
    MalformedHeadSha { given: String },
    #[error("the run has no branch recorded, so there is nothing to publish")]
    RunHasNoBranch,
    #[error(
        "the repository profile could not name a default branch, and a pull \
         request against a guessed base is a diff against the wrong history"
    )]
    NoBaseBranch,
    #[error("{phase:?} does not carry publish authority; only Published does")]
    CannotPublishFrom { phase: DevPhase },
    #[error("no recorded human approval says: {consequence}")]
    Unapproved { consequence: String },
    #[error("the remote refused the push: {detail}")]
    PushRefused { detail: String },
    #[error(
        "the push client failed after the approved commit reached the remote \
         ({detail}); the branch is live — recover from the record, do not \
         blindly retry"
    )]
    PushLandedClientFailed { detail: String },
    #[error(
        "the push failed and the remote could not be asked what it holds: \
         {detail}; the outcome is unknown, not refused"
    )]
    PushOutcomeUnknown { detail: String },
    #[error(
        "the push exited zero but the remote reports {found} where \
         {expected} was published; the receipt is not believed"
    )]
    PushUnconfirmed { expected: String, found: String },
    #[error("repository {repository:?} is refused: {reason}")]
    ForbiddenRepository {
        repository: String,
        reason: &'static str,
    },
    #[error(
        "remote {remote:?} has no forge repository in its URL, so a pull \
         request cannot be addressed to it by name"
    )]
    NoForgeRepository { remote: String },
    #[error(
        "the remote branch is at {found}, not the approved {expected}; \
         push the approved commit before opening a pull request"
    )]
    RemoteHeadMismatch { expected: String, found: String },
    #[error("git could not answer: {detail}")]
    GitUnanswered { detail: String },
    #[error("the forge refused to create the pull request: {detail}")]
    PrRefused { detail: String },
    #[error(
        "gh printed no pull-request number; a number is GitHub's to assign \
         and will not be invented (output began: {output:?})"
    )]
    NumberUnparsed { output: String },
    #[error("the forge's answer could not be read: {detail}")]
    ForgeAnswerUnreadable { detail: String },
    #[error(
        "the created pull request reports {field} = {found:?} where \
         {expected:?} was planned"
    )]
    PrUnconfirmed {
        field: &'static str,
        expected: String,
        found: String,
    },
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error(transparent)]
    Run(#[from] RunError),
    #[error("storing evidence output: {0}")]
    Io(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, DeliveryError>;

/// Proof that the branch reached the remote at the approved commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushReceipt {
    pub remote: String,
    pub branch: String,
    pub head_sha: String,
}

/// Proof that a draft pull request exists for the pushed head. The number is
/// GitHub's: nothing in this crate constructs one except the parsers that
/// read it out of what the forge answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftPrReceipt {
    pub number: u64,
    pub url: String,
    pub head_sha: String,
}

/// Executes a publication and records what actually happened.
///
/// Mirrors [`crate::controller::RunDriver`]: borrows the run so the caller
/// keeps responsibility for persisting it, runs real commands through an
/// injected runner, and stores every captured output where the record's
/// digests point.
pub struct Publisher<'a, R: CommandRunner> {
    run: &'a mut DevTaskRun,
    runner: R,
    worktree: PathBuf,
    evidence_dir: PathBuf,
}

impl<'a, R: CommandRunner> Publisher<'a, R> {
    pub fn new(
        run: &'a mut DevTaskRun,
        runner: R,
        worktree: impl Into<PathBuf>,
        evidence_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            run,
            runner,
            worktree: worktree.into(),
            evidence_dir: evidence_dir.into(),
        }
    }

    #[must_use]
    pub fn run(&self) -> &DevTaskRun {
        self.run
    }

    /// Push the approved commit, confirm the remote agrees, and record the
    /// receipt.
    ///
    /// The remote is read back **whatever the push client said**: exit zero
    /// is transport, not truth — and non-zero is not proof of no effect
    /// either. A push that lands while its client dies to a timeout must not
    /// be recorded as "refused", and an operation row is written on every
    /// exit path, so an outward effect is never absent from the record.
    ///
    /// # Errors
    /// Refused before anything runs when the phase lacks publish authority or
    /// the record holds no approval saying exactly what this push would do.
    /// After running: a refused, landed-but-unreceipted, unknown-outcome or
    /// unconfirmed push is recorded (as non-corroborating evidence) and
    /// returned as the error it is.
    pub fn push(&mut self, plan: &PublishPlan) -> Result<PushReceipt> {
        self.ensure_authorized(plan)?;
        let outcome = self
            .runner
            .run(&self.worktree, "git", &plan.push_args(), PUSH_TIMEOUT)?;
        let read_back = self.remote_head(plan);
        if !outcome.succeeded() {
            let detail = first_line(&outcome.stderr);
            let (note, error) = match &read_back {
                Ok(found) if *found == plan.head_sha() => (
                    format!(
                        "push client failed but the remote reports {} at refs/heads/{} — \
                         the branch is live without a receipt",
                        short(found),
                        plan.branch()
                    ),
                    DeliveryError::PushLandedClientFailed { detail },
                ),
                Ok(found) => (
                    format!("push refused — remote holds {found}"),
                    DeliveryError::PushRefused { detail },
                ),
                Err(_) => (
                    "push failed and the remote could not be asked what it holds".to_string(),
                    DeliveryError::PushOutcomeUnknown { detail },
                ),
            };
            self.record_operation(E::PushReceipt, &outcome, plan, false, &note)?;
            return Err(error);
        }
        let found = match read_back {
            Ok(found) => found,
            Err(err) => {
                self.record_operation(
                    E::PushReceipt,
                    &outcome,
                    plan,
                    false,
                    "push exited zero; the read-back failed, so the receipt is not believed",
                )?;
                return Err(err);
            }
        };
        let confirmed = found == plan.head_sha();
        let note = if confirmed {
            format!(
                "remote confirms {} at refs/heads/{}",
                short(&found),
                plan.branch()
            )
        } else {
            format!("remote reports {found}, not the pushed commit")
        };
        self.record_operation(E::PushReceipt, &outcome, plan, confirmed, &note)?;
        if !confirmed {
            return Err(DeliveryError::PushUnconfirmed {
                expected: plan.head_sha().to_string(),
                found,
            });
        }
        Ok(PushReceipt {
            remote: plan.remote().to_string(),
            branch: plan.branch().to_string(),
            head_sha: plan.head_sha().to_string(),
        })
    }

    /// Create the draft pull request for the pushed head and record the
    /// receipt, with the number GitHub assigned.
    ///
    /// The remote branch is re-read first: a PR is only opened for a branch
    /// standing at the approved commit. The title comes from the task origin
    /// and the body from [`body_from_evidence`] — there is no argument for
    /// either, so there is nothing a model can assert here.
    ///
    /// # Errors
    /// Everything [`Publisher::push`] refuses, plus: a remote without a forge
    /// repository in its URL, a branch not at the approved commit, forge
    /// output without a PR number, and a created PR whose confirmed head,
    /// draft state or base is not the planned one.
    pub fn create_draft_pr(&mut self, plan: &PublishPlan) -> Result<DraftPrReceipt> {
        self.ensure_authorized(plan)?;
        let repository = plan.repository().map(str::to_string).ok_or_else(|| {
            DeliveryError::NoForgeRepository {
                remote: plan.remote().to_string(),
            }
        })?;
        let found = self.remote_head(plan)?;
        if found != plan.head_sha() {
            return Err(DeliveryError::RemoteHeadMismatch {
                expected: plan.head_sha().to_string(),
                found,
            });
        }

        let title = title_from_origin(&self.run.origin);
        let body = body_from_evidence(self.run);
        let create_args: Vec<String> = [
            "pr",
            "create",
            "--repo",
            repository.as_str(),
            "--draft",
            "--head",
            plan.branch(),
            "--base",
            plan.base_branch(),
            "--title",
            title.as_str(),
            "--body",
            body.as_str(),
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        let created = self
            .runner
            .run(&self.worktree, "gh", &create_args, FORGE_TIMEOUT)?;
        if !created.succeeded() {
            self.record_operation(
                E::DraftPullRequest,
                &created,
                plan,
                false,
                "creation refused",
            )?;
            return Err(DeliveryError::PrRefused {
                detail: first_line(&created.stderr),
            });
        }
        let Some(number) = parse_pr_number(&String::from_utf8_lossy(&created.stdout), &repository)
        else {
            self.record_operation(
                E::DraftPullRequest,
                &created,
                plan,
                false,
                "no number printed",
            )?;
            return Err(DeliveryError::NumberUnparsed {
                output: first_line(&created.stdout),
            });
        };

        let view = match self.view_pr(&repository, number) {
            Ok(view) => view,
            Err(err) => {
                // The PR exists on the forge — it was created and numbered —
                // even though its confirmation failed. That fact must be in
                // the record, or a retry would open a second PR for work that
                // already has one.
                let note = format!(
                    "PR #{number} created but its confirmation failed; \
                     a pull request exists on the forge without a believed receipt"
                );
                self.record_operation(E::DraftPullRequest, &created, plan, false, &note)?;
                return Err(err);
            }
        };
        if view.number != number {
            let note = format!(
                "asked for PR #{number}, the forge described #{}",
                view.number
            );
            self.record_operation(E::DraftPullRequest, &created, plan, false, &note)?;
            return Err(DeliveryError::PrUnconfirmed {
                field: "number",
                expected: number.to_string(),
                found: view.number.to_string(),
            });
        }
        let mismatch = [
            ("headRefOid", plan.head_sha(), view.head_sha.as_str()),
            (
                "isDraft",
                "true",
                if view.is_draft { "true" } else { "false" },
            ),
            ("baseRefName", plan.base_branch(), view.base.as_str()),
        ]
        .into_iter()
        .find(|(_, expected, found)| expected != found);
        if let Some((field, expected, found)) = mismatch {
            let note = format!("PR #{number} reports {field} = {found}, not {expected}");
            self.record_operation(E::DraftPullRequest, &created, plan, false, &note)?;
            return Err(DeliveryError::PrUnconfirmed {
                field,
                expected: expected.to_string(),
                found: found.to_string(),
            });
        }

        let note = format!(
            "draft PR #{number} at {}, head {}",
            view.url,
            short(&view.head_sha)
        );
        self.record_operation(E::DraftPullRequest, &created, plan, true, &note)?;
        Ok(DraftPrReceipt {
            number,
            url: view.url,
            head_sha: plan.head_sha().to_string(),
        })
    }

    /// The two gates every outward write passes: the phase must carry publish
    /// authority, and the record must hold a human approval saying exactly
    /// what this plan would do — recorded **at the `ReadyToPublish` gate, in
    /// its current attempt**. Approval of different words — an earlier head,
    /// another branch, a paraphrase — covers nothing; and neither does a row
    /// with the right words planted in an earlier phase, because the row that
    /// authorizes must be the row the human answered when the gate asked.
    fn ensure_authorized(&self, plan: &PublishPlan) -> Result<()> {
        if !self.run.phase.contract().authority.can_publish() {
            return Err(DeliveryError::CannotPublishFrom {
                phase: self.run.phase,
            });
        }
        let consequence = plan.consequence();
        let gate_attempt = self
            .run
            .phase_attempts
            .get(&DevPhase::ReadyToPublish)
            .copied()
            .unwrap_or(0);
        let approved = self.run.evidence.iter().any(|item| {
            item.kind == E::HumanApproval
                && item.corroborates
                && item.phase == DevPhase::ReadyToPublish
                && item.attempt == gate_attempt
                && item.summary == consequence
        });
        if approved {
            Ok(())
        } else {
            Err(DeliveryError::Unapproved { consequence })
        }
    }

    /// What the remote holds at the plan's branch, read rather than assumed.
    fn remote_head(&self, plan: &PublishPlan) -> Result<String> {
        let ref_name = format!("refs/heads/{}", plan.branch());
        let args: Vec<String> = ["ls-remote", plan.remote(), ref_name.as_str()]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let outcome = self
            .runner
            .run(&self.worktree, "git", &args, QUERY_TIMEOUT)?;
        if !outcome.succeeded() {
            return Err(DeliveryError::GitUnanswered {
                detail: first_line(&outcome.stderr),
            });
        }
        let stdout = String::from_utf8_lossy(&outcome.stdout);
        Ok(stdout
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let sha = fields.next()?;
                (fields.next()? == ref_name).then(|| sha.to_string())
            })
            .next()
            .unwrap_or_else(|| ABSENT.to_string()))
    }

    fn view_pr(&self, repository: &str, number: u64) -> Result<PrView> {
        let number_text = number.to_string();
        let args: Vec<String> = [
            "pr",
            "view",
            number_text.as_str(),
            "--repo",
            repository,
            "--json",
            "number,headRefOid,isDraft,baseRefName,url",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        let outcome = self
            .runner
            .run(&self.worktree, "gh", &args, FORGE_TIMEOUT)?;
        if !outcome.succeeded() {
            return Err(DeliveryError::ForgeAnswerUnreadable {
                detail: first_line(&outcome.stderr),
            });
        }
        parse_pr_view(&String::from_utf8_lossy(&outcome.stdout))
    }

    /// One evidence row per outward operation, corroborating only when the
    /// command exited zero *and* its effect was read back — the receipt's log
    /// is stored where the record's digest points, like any phase step's.
    fn record_operation(
        &mut self,
        kind: EvidenceKind,
        outcome: &CommandOutcome,
        plan: &PublishPlan,
        confirmed: bool,
        note: &str,
    ) -> Result<()> {
        let output = outcome.combined_output();
        store_output(&self.evidence_dir, &output)?;
        let draft = EvidenceItem::observed_confirmed(
            kind,
            outcome.command_line(),
            outcome.status_code(),
            plan.head_sha().to_string(),
            &output,
            confirmed,
        )
        .with_summary(format!("{note} — {}", describe(outcome)));
        self.run.record(draft)?;
        Ok(())
    }
}

/// What `gh pr view` reported about the PR that was just created.
struct PrView {
    number: u64,
    head_sha: String,
    is_draft: bool,
    base: String,
    url: String,
}

/// The PR number out of what `gh pr create` printed — the URL of the created
/// PR, anchored to the repository the request was pinned to. `None` when no
/// `<repository>/pull/<digits>` appears; the caller refuses rather than
/// inventing one. Anchoring matters: unanchored, any `/pull/` in a warning
/// line or a cross-referenced URL could be read as the number.
#[must_use]
pub fn parse_pr_number(stdout: &str, repository: &str) -> Option<u64> {
    let anchor = format!("{repository}/pull/");
    stdout.lines().rev().find_map(|line| {
        let (_, rest) = line.split_once(anchor.as_str())?;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

fn parse_pr_view(stdout: &str) -> Result<PrView> {
    let unreadable = |detail: &str| DeliveryError::ForgeAnswerUnreadable {
        detail: detail.to_string(),
    };
    let value: serde_json::Value =
        serde_json::from_str(stdout).map_err(|e| unreadable(&format!("not JSON: {e}")))?;
    let field = |name: &str| {
        value
            .get(name)
            .ok_or_else(|| unreadable(&format!("no {name} field")))
    };
    Ok(PrView {
        number: field("number")?
            .as_u64()
            .ok_or_else(|| unreadable("number is not an integer"))?,
        head_sha: field("headRefOid")?
            .as_str()
            .ok_or_else(|| unreadable("headRefOid is not a string"))?
            .to_string(),
        is_draft: field("isDraft")?
            .as_bool()
            .ok_or_else(|| unreadable("isDraft is not a boolean"))?,
        base: field("baseRefName")?
            .as_str()
            .ok_or_else(|| unreadable("baseRefName is not a string"))?
            .to_string(),
        url: field("url")?
            .as_str()
            .ok_or_else(|| unreadable("url is not a string"))?
            .to_string(),
    })
}

pub(crate) fn first_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("(no output)")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pr_number_is_read_from_the_forge_url_or_not_at_all() {
        assert_eq!(
            parse_pr_number("https://github.com/o/r/pull/118\n", "github.com/o/r"),
            Some(118)
        );
        assert_eq!(
            parse_pr_number(
                "Warning: 2 uncommitted changes\nhttps://github.com/o/r/pull/9",
                "github.com/o/r"
            ),
            Some(9)
        );
        // Anchored to the pinned repository: a /pull/ URL under any other
        // repository is not this PR's number.
        assert_eq!(
            parse_pr_number("https://github.com/evil/x/pull/9999", "github.com/o/r"),
            None
        );
        for output in ["", "created", "https://github.com/o/r/pulls", "/pull/x"] {
            assert_eq!(
                parse_pr_number(output, "github.com/o/r"),
                None,
                "{output:?}"
            );
        }
    }
}
