//! What one publication will do, settled before anyone is asked to approve
//! it (program P44; ADR-0058 rules 1–3).
//!
//! A [`PublishPlan`] is the noun the approval sentence describes. Construction
//! is the gate: every part — remote, branch, base, commit, repository — is
//! validated with the reason named, so a plan that exists is a plan whose
//! refspec means exactly what its consequence sentence says. Deleting,
//! renaming and forcing are not filtered out here; they are unconstructible,
//! because the refspec is built from these validated parts and nothing else
//! is accepted anywhere.
//!
//! [`crate::delivery`] executes a plan; this module only states one.

use std::path::Path;

use crate::command::CommandRunner;
use crate::delivery::{first_line, DeliveryError, QUERY_TIMEOUT};
use crate::phase::EvidenceKind;
use crate::repository::RepositoryPolicyProfile;
use crate::run::{DevTaskRun, EvidenceItem};

use EvidenceKind as E;

type Result<T> = std::result::Result<T, DeliveryError>;

/// Everything one publication will do, resolved before anyone is asked to
/// approve it. Construction is the gate: a plan that exists is a plan whose
/// refspec means exactly what its consequence sentence says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishPlan {
    remote: String,
    /// `host/owner/name`, when the remote URL names a forge repository —
    /// host-qualified so the approval sentence names *where*, not just what
    /// the path looks like. `None` for a local or unparseable remote —
    /// pushable, but no PR can be addressed.
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
        if let Some(repository) = &repository {
            validate_repository(repository)?;
        }
        if branch == base_branch {
            return Err(DeliveryError::ForbiddenBranch {
                branch,
                reason: "it is the base branch — publishing onto the base bypasses review, \
                         and a pull request cannot target itself",
            });
        }
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
    /// Config-driven amplification is pinned off explicitly: `push.followTags`
    /// could ride tags out on this push and `push.recurseSubmodules` could
    /// push repositories this plan never named, and both are writable by any
    /// earlier phase that holds project write authority.
    #[must_use]
    pub fn push_args(&self) -> Vec<String> {
        vec![
            "push".to_string(),
            "--no-follow-tags".to_string(),
            "--recurse-submodules=no".to_string(),
            self.remote.clone(),
            format!("{}:refs/heads/{}", self.head_sha, self.branch),
        ]
    }

    #[must_use]
    pub fn remote(&self) -> &str {
        &self.remote
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
    // The *push* URL: `git push` honours `remote.<name>.pushurl`, and a plan
    // resolved from the fetch URL would confirm one host while writing to
    // another.
    let url = git_stdout(runner, worktree, &["remote", "get-url", "--push", remote])?;
    PublishPlan::new(
        remote,
        parse_forge_repository(&url),
        branch,
        head_sha,
        base_branch,
    )
}

/// `host/owner/name` out of a remote URL, for the forge shapes people
/// actually configure: `git@host:owner/name.git`, `https://host/owner/name`,
/// `ssh://git@host/owner/name`. A local path yields `None` — pushable, but
/// not addressable by name.
///
/// The host is kept on purpose. `owner/name` alone reads identically whether
/// the URL points at github.com or at a host that merely mimics its path
/// shape, so the approval sentence would too; carrying the host makes a
/// rewritten or look-alike remote visible in the words the human reads, and
/// `gh --repo` accepts the `HOST/OWNER/NAME` form, pinning the API call to
/// the same host the push goes to.
#[must_use]
pub fn parse_forge_repository(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let (host, path) = if let Some((_, rest)) = url.split_once("://") {
        rest.split_once('/')?
    } else if let Some((host, path)) = url.split_once(':') {
        // scp-like, unless the "host" is really a path segment.
        if host.contains('/') {
            return None;
        }
        (host, path)
    } else {
        return None;
    };
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let mut segments = path.split('/');
    let owner = segments.next()?;
    let name = segments.next()?;
    let name = name.strip_suffix(".git").unwrap_or(name);
    let repository = format!("{host}/{owner}/{name}");
    (segments.next().is_none() && validate_repository(&repository).is_ok()).then_some(repository)
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
    } else if branch.chars().any(|c| !c.is_ascii_graphic()) {
        Some(
            "a byte that does not render as it compares could make the approved \
             sentence read differently from the words it binds",
        )
    } else if branch == "HEAD" || branch == "@" {
        Some("git reads this as the current branch, not a branch name")
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
    } else if remote.starts_with('.') || remote.starts_with('~') {
        Some(
            "a path, not a configured remote name — `git push .` writes the main \
             checkout's own ref store, the containment ADR-0052 exists to prevent",
        )
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

/// `owner/name` or `host/owner/name`. The one field that renders into the
/// approval sentence *and* rides into `gh --repo`, so it is held to the
/// charset forges actually allow — anything else could smuggle words into
/// the sentence a human reads or arguments into the forge call.
fn validate_repository(repository: &str) -> Result<()> {
    let segments: Vec<&str> = repository.split('/').collect();
    let segment_ok = |s: &str| {
        !s.is_empty()
            && !s.starts_with('-')
            && !s.starts_with('.')
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    };
    let reason = if !(2..=3).contains(&segments.len()) {
        Some("a repository is owner/name, optionally host-qualified — nothing else")
    } else if !segments.iter().all(|s| segment_ok(s)) {
        Some(
            "a segment outside [A-Za-z0-9._-], or starting with '-' or '.', could \
             smuggle words into the approval sentence or arguments into the forge call",
        )
    } else {
        None
    };
    match reason {
        Some(reason) => Err(DeliveryError::ForbiddenRepository {
            repository: repository.to_string(),
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
            ("main", "the base branch"),
            ("HEAD", "the current branch, not a name"),
            ("@", "the current branch, not a name"),
            ("wip/fix\u{202E}evil", "a byte that renders differently"),
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
    fn a_remote_must_be_a_configured_name_not_a_url_or_a_path() {
        for remote in [
            "",
            "-origin",
            "https://github.com/o/r",
            "a b",
            ".",
            "..",
            "~",
        ] {
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
    fn a_repository_that_could_smuggle_words_or_arguments_is_refused() {
        for repository in [
            "",
            "-x/y",
            "--repo=evil/x",
            "a\nb/c",
            "owner",
            "host/owner/name/extra",
            "owner/name, and also delete main",
        ] {
            assert!(
                matches!(
                    PublishPlan::new("origin", Some(repository.into()), "wip/x", SHA, "main"),
                    Err(DeliveryError::ForbiddenRepository { .. })
                ),
                "{repository:?}"
            );
        }
        for repository in ["owner/name", "github.com/mustbearnold/Optimus-Agent"] {
            assert!(
                PublishPlan::new("origin", Some(repository.into()), "wip/x", SHA, "main").is_ok(),
                "{repository:?}"
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
                "--no-follow-tags".to_string(),
                "--recurse-submodules=no".to_string(),
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
                Some("github.com/mustbearnold/Optimus-Agent"),
                "{url}"
            );
        }
        // The host is part of the answer: a look-alike path on another host
        // parses to visibly different words, so the approval sentence differs.
        assert_eq!(
            parse_forge_repository("https://evil.example.com/mustbearnold/Optimus-Agent")
                .as_deref(),
            Some("evil.example.com/mustbearnold/Optimus-Agent"),
        );
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
}
