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

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::command::{CommandError, CommandOutcome, CommandRunner};
use crate::controller::{describe, store_output};
use crate::phase::{DevPhase, EvidenceKind};
use crate::pr_body::{body_from_evidence, short, title_from_origin};
use crate::repository::RepositoryPolicyProfile;
use crate::run::{DevTaskRun, EvidenceItem, RunError};

use EvidenceKind as E;

/// A push moves objects; give a cold connection room without letting the
/// phase hang on it.
const PUSH_TIMEOUT: Duration = Duration::from_secs(300);
/// One forge API call.
const FORGE_TIMEOUT: Duration = Duration::from_secs(120);
/// A local git query, or a single-ref `ls-remote`.
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);

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
        "the push exited zero but the remote reports {found} where \
         {expected} was published; the receipt is not believed"
    )]
    PushUnconfirmed { expected: String, found: String },
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

/// Everything one publication will do, resolved before anyone is asked to
/// approve it. Construction is the gate: a plan that exists is a plan whose
/// refspec means exactly what its consequence sentence says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishPlan {
    remote: String,
    /// `owner/name`, when the remote URL names a forge repository. `None` for
    /// a local or unparseable remote — pushable, but no PR can be addressed.
    repository: Option<String>,
    branch: String,
    head_sha: String,
    base_branch: String,
}

impl PublishPlan {
    /// Validate every part. Refusals name what the rejected input would have
    /// expressed, because "invalid" teaches nothing.
    pub fn new(
        remote: impl Into<String>,
        repository: Option<String>,
        branch: impl Into<String>,
        head_sha: impl Into<String>,
        base_branch: impl Into<String>,
    ) -> Result<Self> {
        let remote = remote.into();
        let branch = branch.into();
        let head_sha = head_sha.into();
        let base_branch = base_branch.into();
        validate_remote(&remote)?;
        validate_branch(&branch)?;
        validate_branch(&base_branch)?;
        validate_sha(&head_sha)?;
        Ok(Self {
            remote,
            repository,
            branch,
            head_sha,
            base_branch,
        })
    }

    /// The sentence a human approves — the consequence, never the mechanism.
    ///
    /// This exact string is what the kernel shows, what
    /// [`PublishPlan::approval_draft`] records, and what publication later
    /// requires to be on the record. It embeds the full commit SHA on
    /// purpose: approving one commit must not approve the commit after it,
    /// and equality on the sentence enforces that without a staleness check.
    #[must_use]
    pub fn consequence(&self) -> String {
        let repository = self
            .repository
            .clone()
            .unwrap_or_else(|| format!("remote {:?}", self.remote));
        format!(
            "Publish commit {} as branch {} on {}, then open a draft pull request against {}.",
            self.head_sha, self.branch, repository, self.base_branch
        )
    }

    /// The `HumanApproval` evidence a yes turns into.
    ///
    /// The summary *is* the consequence sentence — the record itself is the
    /// binding, human-readable next to the receipts. A UI that paraphrases
    /// instead of recording this draft has approved nothing, deliberately.
    #[must_use]
    pub fn approval_draft(&self) -> crate::run::EvidenceDraft {
        EvidenceItem::stated_by(
            crate::roles::RoleIdentity::controller(),
            E::HumanApproval,
            self.consequence(),
        )
    }

    /// The one remote write this crate can express: a fully-stated,
    /// fast-forward-only push of the approved commit to the named branch.
    ///
    /// No force, no delete, no rename — not filtered out, but absent: the
    /// vector is built from validated parts and nothing else is accepted.
    #[must_use]
    pub fn push_args(&self) -> Vec<String> {
        vec![
            "push".to_string(),
            self.remote.clone(),
            format!("{}:refs/heads/{}", self.head_sha, self.branch),
        ]
    }

    #[must_use]
    pub fn branch(&self) -> &str {
        &self.branch
    }

    #[must_use]
    pub fn head_sha(&self) -> &str {
        &self.head_sha
    }

    #[must_use]
    pub fn base_branch(&self) -> &str {
        &self.base_branch
    }

    #[must_use]
    pub fn repository(&self) -> Option<&str> {
        self.repository.as_deref()
    }
}

/// Resolve a plan from what the run, the profile and git actually say.
/// Nothing is guessed: no branch on the run, no base in the profile, or no
/// readable head each refuse rather than default.
pub fn plan_publish<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    run: &DevTaskRun,
    profile: &RepositoryPolicyProfile,
    remote: &str,
) -> Result<PublishPlan> {
    let branch = run.branch.clone().ok_or(DeliveryError::RunHasNoBranch)?;
    let base_branch = profile
        .default_branch
        .clone()
        .ok_or(DeliveryError::NoBaseBranch)?;
    let head_sha = git_stdout(runner, worktree, &["rev-parse", "HEAD"])?;
    let url = git_stdout(runner, worktree, &["remote", "get-url", remote])?;
    PublishPlan::new(
        remote,
        parse_forge_repository(&url),
        branch,
        head_sha,
        base_branch,
    )
}

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
    /// # Errors
    /// Refused before anything runs when the phase lacks publish authority or
    /// the record holds no approval saying exactly what this push would do.
    /// After running: a refused push or an unconfirmed one is recorded (as
    /// non-corroborating evidence) and returned as the error it is.
    pub fn push(&mut self, plan: &PublishPlan) -> Result<PushReceipt> {
        self.ensure_authorized(plan)?;
        let outcome = self
            .runner
            .run(&self.worktree, "git", &plan.push_args(), PUSH_TIMEOUT)?;
        if !outcome.succeeded() {
            self.record_operation(E::PushReceipt, &outcome, plan, false, "push refused")?;
            return Err(DeliveryError::PushRefused {
                detail: first_line(&outcome.stderr),
            });
        }
        let found = self.remote_head(plan)?;
        let confirmed = found == plan.head_sha;
        let note = if confirmed {
            format!(
                "remote confirms {} at refs/heads/{}",
                short(&found),
                plan.branch
            )
        } else {
            format!("remote reports {found}, not the pushed commit")
        };
        self.record_operation(E::PushReceipt, &outcome, plan, confirmed, &note)?;
        if !confirmed {
            return Err(DeliveryError::PushUnconfirmed {
                expected: plan.head_sha.clone(),
                found,
            });
        }
        Ok(PushReceipt {
            remote: plan.remote.clone(),
            branch: plan.branch.clone(),
            head_sha: plan.head_sha.clone(),
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
        let repository =
            plan.repository
                .clone()
                .ok_or_else(|| DeliveryError::NoForgeRepository {
                    remote: plan.remote.clone(),
                })?;
        let found = self.remote_head(plan)?;
        if found != plan.head_sha {
            return Err(DeliveryError::RemoteHeadMismatch {
                expected: plan.head_sha.clone(),
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
        let Some(number) = parse_pr_number(&String::from_utf8_lossy(&created.stdout)) else {
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

        let view = self.view_pr(&repository, number)?;
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
            head_sha: plan.head_sha.clone(),
        })
    }

    /// The two gates every outward write passes: the phase must carry publish
    /// authority, and the record must hold a human approval saying exactly
    /// what this plan would do. Approval of different words — an earlier
    /// head, another branch, a paraphrase — covers nothing.
    fn ensure_authorized(&self, plan: &PublishPlan) -> Result<()> {
        if !self.run.phase.contract().authority.can_publish() {
            return Err(DeliveryError::CannotPublishFrom {
                phase: self.run.phase,
            });
        }
        let consequence = plan.consequence();
        let approved = self.run.evidence.iter().any(|item| {
            item.kind == E::HumanApproval && item.corroborates && item.summary == consequence
        });
        if approved {
            Ok(())
        } else {
            Err(DeliveryError::Unapproved { consequence })
        }
    }

    /// What the remote holds at the plan's branch, read rather than assumed.
    fn remote_head(&self, plan: &PublishPlan) -> Result<String> {
        let ref_name = format!("refs/heads/{}", plan.branch);
        let args: Vec<String> = ["ls-remote", plan.remote.as_str(), ref_name.as_str()]
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
            plan.head_sha.clone(),
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

/// `owner/name` out of a remote URL, for the forge shapes people actually
/// configure: `git@host:owner/name.git`, `https://host/owner/name`,
/// `ssh://git@host/owner/name`. A local path yields `None` — pushable, but
/// not addressable by name.
#[must_use]
pub fn parse_forge_repository(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches(".git").trim_end_matches('/');
    let path = if let Some((_, rest)) = url.split_once("://") {
        let (_, path) = rest.split_once('/')?;
        path
    } else if let Some((host, path)) = url.split_once(':') {
        // scp-like, unless the "host" is really a path segment.
        if host.contains('/') {
            return None;
        }
        path
    } else {
        return None;
    };
    let mut segments = path.split('/');
    let owner = segments.next()?;
    let name = segments.next()?;
    let well_formed = |s: &str| !s.is_empty() && !s.chars().any(char::is_whitespace);
    (segments.next().is_none() && well_formed(owner) && well_formed(name))
        .then(|| format!("{owner}/{name}"))
}

/// The PR number out of what `gh pr create` printed — the URL of the created
/// PR. `None` when no `/pull/<digits>` appears; the caller refuses rather
/// than inventing one.
#[must_use]
pub fn parse_pr_number(stdout: &str) -> Option<u64> {
    stdout.lines().rev().find_map(|line| {
        let (_, rest) = line.split_once("/pull/")?;
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

fn validate_branch(branch: &str) -> Result<()> {
    let reason = if branch.is_empty() {
        Some("an empty name would turn the refspec into a deletion")
    } else if branch.contains(':') {
        Some("a colon writes a refspec of its own, which could rename or delete the remote ref")
    } else if branch.starts_with('-') || branch.starts_with('+') {
        Some("a leading option or force marker would change what the push means")
    } else if branch.chars().any(char::is_whitespace) {
        Some("whitespace would split the name into arguments this plan never stated")
    } else if branch.contains('*') || branch.contains('?') || branch.contains('[') {
        Some("a wildcard could address refs this plan never named")
    } else if branch.starts_with("refs/") {
        Some("the plan qualifies the ref itself; a pre-qualified name would nest")
    } else if branch.starts_with('/')
        || branch.ends_with('/')
        || branch.contains("//")
        || branch.contains("..")
        || branch.contains("@{")
        || branch.ends_with(".lock")
        || branch.starts_with('.')
    {
        Some("git refuses this ref name, so the plan refuses it first")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(DeliveryError::ForbiddenBranch {
            branch: branch.to_string(),
            reason,
        }),
        None => Ok(()),
    }
}

fn validate_remote(remote: &str) -> Result<()> {
    let reason = if remote.is_empty() {
        Some("an empty remote names nothing")
    } else if remote.starts_with('-') {
        Some("a leading dash would be read as an option")
    } else if remote.chars().any(char::is_whitespace) {
        Some("whitespace would split into extra arguments")
    } else if remote.contains(':') || remote.contains('/') {
        Some("a URL or path bypasses the configured remote this plan was resolved against")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(DeliveryError::ForbiddenRemote {
            remote: remote.to_string(),
            reason,
        }),
        None => Ok(()),
    }
}

fn validate_sha(sha: &str) -> Result<()> {
    let full = sha.len() == 40
        && sha
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
    if full {
        Ok(())
    } else {
        Err(DeliveryError::MalformedHeadSha {
            given: sha.to_string(),
        })
    }
}

fn git_stdout<R: CommandRunner>(runner: &R, cwd: &Path, args: &[&str]) -> Result<String> {
    let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    let outcome = runner.run(cwd, "git", &args, QUERY_TIMEOUT)?;
    if !outcome.succeeded() {
        return Err(DeliveryError::GitUnanswered {
            detail: first_line(&outcome.stderr),
        });
    }
    Ok(String::from_utf8_lossy(&outcome.stdout).trim().to_string())
}

fn first_line(bytes: &[u8]) -> String {
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

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    fn plan() -> PublishPlan {
        PublishPlan::new(
            "origin",
            Some("mustbearnold/fixture".into()),
            "wip/fix-123",
            SHA,
            "main",
        )
        .expect("a well-formed plan")
    }

    #[test]
    fn a_name_that_would_delete_rename_or_force_is_refused_at_construction() {
        for (branch, hazard) in [
            ("", "deletion"),
            ("a:b", "a refspec of its own"),
            (":wip/x", "a refspec of its own"),
            ("-x", "an option"),
            ("+wip/x", "force"),
            ("wip x", "extra arguments"),
            ("wip/*", "a wildcard"),
            ("refs/heads/wip/x", "nesting"),
            ("wip/../main", "git refuses"),
        ] {
            let refused = PublishPlan::new("origin", None, branch, SHA, "main");
            assert!(
                matches!(refused, Err(DeliveryError::ForbiddenBranch { .. })),
                "{branch:?} should be refused ({hazard})"
            );
        }
        for branch in ["wip/fix-123", "pr/117-engineering-runs", "feature.a"] {
            assert!(
                PublishPlan::new("origin", None, branch, SHA, "main").is_ok(),
                "{branch:?}"
            );
        }
    }

    #[test]
    fn a_remote_must_be_a_configured_name_not_a_url() {
        for remote in ["", "-origin", "https://github.com/o/r", "a b"] {
            assert!(
                matches!(
                    PublishPlan::new(remote, None, "wip/x", SHA, "main"),
                    Err(DeliveryError::ForbiddenRemote { .. })
                ),
                "{remote:?}"
            );
        }
    }

    #[test]
    fn an_abbreviated_or_dressed_up_sha_is_refused() {
        let upper = SHA.to_uppercase();
        for sha in ["0123456", "", "HEAD", upper.as_str(), "main"] {
            assert!(
                matches!(
                    PublishPlan::new("origin", None, "wip/x", sha, "main"),
                    Err(DeliveryError::MalformedHeadSha { .. })
                ),
                "{sha:?}"
            );
        }
    }

    #[test]
    fn the_only_remote_write_is_a_fully_stated_fast_forward_push() {
        assert_eq!(
            plan().push_args(),
            vec![
                "push".to_string(),
                "origin".to_string(),
                format!("{SHA}:refs/heads/wip/fix-123"),
            ]
        );
    }

    #[test]
    fn the_consequence_names_the_commit_the_branch_the_repository_and_the_base() {
        let sentence = plan().consequence();
        assert_eq!(
            sentence,
            format!(
                "Publish commit {SHA} as branch wip/fix-123 on mustbearnold/fixture, \
                 then open a draft pull request against main."
            )
        );
        // A different commit is a different sentence — approval equality does
        // the staleness check.
        let moved = PublishPlan::new(
            "origin",
            Some("mustbearnold/fixture".into()),
            "wip/fix-123",
            SHA.replace('0', "f"),
            "main",
        )
        .unwrap();
        assert_ne!(moved.consequence(), sentence);
    }

    #[test]
    fn forge_repositories_parse_from_the_shapes_people_configure() {
        for url in [
            "git@github.com:mustbearnold/Optimus-Agent.git",
            "https://github.com/mustbearnold/Optimus-Agent",
            "https://github.com/mustbearnold/Optimus-Agent.git",
            "ssh://git@github.com/mustbearnold/Optimus-Agent",
        ] {
            assert_eq!(
                parse_forge_repository(url).as_deref(),
                Some("mustbearnold/Optimus-Agent"),
                "{url}"
            );
        }
        for url in [
            "/tmp/fixtures/remote.git",
            "../elsewhere/remote.git",
            "https://github.com/only-owner",
            "https://github.com/o/r/extra",
            "",
        ] {
            assert_eq!(parse_forge_repository(url), None, "{url}");
        }
    }

    #[test]
    fn the_pr_number_is_read_from_the_forge_url_or_not_at_all() {
        assert_eq!(
            parse_pr_number("https://github.com/o/r/pull/118\n"),
            Some(118)
        );
        assert_eq!(
            parse_pr_number("Warning: 2 uncommitted changes\nhttps://github.com/o/r/pull/9"),
            Some(9)
        );
        for output in ["", "created", "https://github.com/o/r/pulls", "/pull/x"] {
            assert_eq!(parse_pr_number(output), None, "{output:?}");
        }
    }
}
