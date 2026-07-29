//! What a repository says about how work in it must be done (program P41).
//!
//! A run should not reconstruct repository facts inside a prompt. "Is `main`
//! protected?", "what does the gate run?", "which instruction files govern this
//! file?" have answers, and the answers live in git, in the forge, and in the
//! tree. This resolves them once into a [`RepositoryPolicyProfile`].
//!
//! Three honesty rules shape the whole module, and every one of them exists
//! because the comfortable answer is the dangerous one:
//!
//! 1. **Absent is not satisfied.** A branch with no protection ruleset resolves
//!    to [`BranchProtection::Unprotected`] — a recorded fact — never to
//!    "requirements met because there are none".
//! 2. **Unknown is not absent.** If the forge could not be asked, that is
//!    [`BranchProtection::Unknown`]. Collapsing it into `Unprotected` would turn
//!    an expired token into a green light.
//! 3. **A repository cannot weaken its own floor.** Declared configuration may
//!    add sensitive paths and name verification commands; it may not shrink the
//!    built-in sensitive set, and it may not resolve verification to nothing.
//!    Otherwise the first thing a bad patch does is edit the file that decides
//!    whether patches get checked.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::command::{CommandError, CommandRunner};

/// Long enough for a cold `gh` call, short enough not to hang a phase.
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);

/// Instruction files a run must read, in the order they are discovered.
const INSTRUCTION_FILE_NAMES: &[&str] = &["AGENTS.md", "CLAUDE.md"];

/// Places a forge keeps a pull-request template, most conventional first.
const PR_TEMPLATE_PATHS: &[&str] = &[
    ".github/pull_request_template.md",
    ".github/PULL_REQUEST_TEMPLATE.md",
    ".github/PULL_REQUEST_TEMPLATE/pull_request_template.md",
    "docs/pull_request_template.md",
    "PULL_REQUEST_TEMPLATE.md",
];

/// Paths whose change always deserves elevated review, whatever the repository
/// says (program P46 §1).
///
/// This is a floor, not a default: declared configuration is unioned with it,
/// never substituted for it. A patch that could disable its own review is the
/// one patch review exists for.
const SENSITIVE_FLOOR: &[&str] = &[
    ".github/**",
    "scripts/verify.sh",
    "scripts/check-*.py",
    "justfile",
    "Justfile",
    "**/AGENTS.md",
    "**/CLAUDE.md",
    ".optimus/**",
    "**/*.pem",
    "**/*.key",
    "**/id_rsa*",
    "**/.env",
    "**/.env.*",
];

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("command: {0}")]
    Command(#[from] CommandError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}

type Result<T> = std::result::Result<T, RepositoryError>;

/// Whether a branch is guarded, and — when it is not — whether anyone checked.
///
/// The three states are deliberately not two. `Unprotected` means the forge
/// answered and there is no ruleset. `Unknown` means the forge did not answer,
/// which is a different fact with a different consequence: a run may publish
/// against an unprotected branch, but it must not publish while pretending an
/// unanswered question was answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchProtection {
    Protected {
        required_checks: Vec<String>,
        required_reviews: u32,
        /// Whether the branch must be up to date with base before merging.
        strict: bool,
    },
    Unprotected,
    Unknown {
        reason: String,
    },
}

impl BranchProtection {
    /// The checks the forge will enforce. Empty for every non-`Protected`
    /// state — including `Unknown`, where "no checks listed" must not read as
    /// "no checks required".
    #[must_use]
    pub fn required_checks(&self) -> &[String] {
        match self {
            Self::Protected {
                required_checks, ..
            } => required_checks,
            _ => &[],
        }
    }

    /// Whether the answer is known at all. A caller that needs certainty —
    /// anything about to push or merge — checks this before the contents.
    #[must_use]
    pub fn is_determined(&self) -> bool {
        !matches!(self, Self::Unknown { .. })
    }

    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Protected { .. } => "protected",
            Self::Unprotected => "unprotected",
            Self::Unknown { .. } => "unknown",
        }
    }
}

/// The commands that check work, at two depths.
///
/// `focused` is what a run may afford to repeat; `full` is what it must pass
/// before publishing. Both are `Option` because "this repository did not tell
/// us" is a real answer and inventing a plausible `cargo test` is not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerificationCommands {
    pub focused: Option<Vec<String>>,
    pub full: Option<Vec<String>>,
}

impl VerificationCommands {
    /// Whether a run can prove anything at all with this.
    #[must_use]
    pub fn can_verify(&self) -> bool {
        self.full.is_some() || self.focused.is_some()
    }
}

/// Configuration a repository checked in about itself.
///
/// Deliberately inert: it carries no authority, only recommendations and
/// names. ADR-0044 Decision 5 — a checked-in file may declare stack and
/// commands; it may not grant credentials, outside-project access, or
/// unrestricted execution. Nothing here can express those, which is the point.
/// A field that cannot be written cannot be abused.
#[derive(Debug, Clone, Default)]
pub struct DeclaredPolicy {
    pub focused_verification: Option<Vec<String>>,
    pub full_verification: Option<Vec<String>>,
    /// Added to the built-in floor. Never subtracted from it.
    pub extra_sensitive_paths: Vec<String>,
}

/// Everything about a repository a run needs before it starts.
#[derive(Debug, Clone)]
pub struct RepositoryPolicyProfile {
    /// `None` when it could not be determined. Not guessed to `main`: a wrong
    /// base branch silently produces a diff against the wrong history.
    pub default_branch: Option<String>,
    pub protection: BranchProtection,
    pub pr_template: Option<PathBuf>,
    /// Root-first, so the nearest file is last and wins on conflict.
    pub instruction_files: Vec<PathBuf>,
    pub sensitive_paths: Vec<String>,
    pub verification: VerificationCommands,
}

impl RepositoryPolicyProfile {
    /// Fields resolution could not determine.
    ///
    /// A run reads this before deciding it knows enough to proceed, rather than
    /// discovering an empty field three phases later and treating it as a
    /// permissive answer.
    #[must_use]
    pub fn unresolved(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.default_branch.is_none() {
            missing.push("default_branch");
        }
        if !self.protection.is_determined() {
            missing.push("protection");
        }
        if !self.verification.can_verify() {
            missing.push("verification");
        }
        if self.instruction_files.is_empty() {
            missing.push("instruction_files");
        }
        missing
    }

    /// Whether `path` is one the repository treats as sensitive.
    ///
    /// Matching is glob-ish on purpose — `**` spans directories, `*` does not
    /// span a separator — because the patterns are written by humans in
    /// configuration, not compiled from code.
    #[must_use]
    pub fn is_sensitive(&self, path: &str) -> bool {
        let normalized = path.replace('\\', "/");
        let normalized = normalized.trim_start_matches("./");
        self.sensitive_paths
            .iter()
            .any(|pattern| glob_match(pattern, normalized))
    }
}

/// Resolve a repository's policy profile.
///
/// `runner` supplies git and forge access, so this stays free of the control
/// plane: the resolver asks, it does not reach.
pub fn resolve_profile<R: CommandRunner>(
    runner: &R,
    repo_root: &Path,
    declared: &DeclaredPolicy,
) -> Result<RepositoryPolicyProfile> {
    if !repo_root.is_dir() {
        return Err(RepositoryError::Invalid(format!(
            "repository root does not exist: {}",
            repo_root.display()
        )));
    }
    let default_branch = resolve_default_branch(runner, repo_root);
    let protection = match default_branch.as_deref() {
        Some(branch) => resolve_protection(runner, repo_root, branch),
        None => BranchProtection::Unknown {
            reason: "default branch unknown, so no branch could be queried".into(),
        },
    };
    Ok(RepositoryPolicyProfile {
        default_branch,
        protection,
        pr_template: find_pr_template(repo_root),
        instruction_files: instruction_files_for(repo_root, Path::new("")),
        sensitive_paths: sensitive_paths(declared),
        verification: resolve_verification(runner, repo_root, declared),
    })
}

/// Instruction files governing `relative_path`, root first.
///
/// Root first because the nearest file is the most specific, and a reader that
/// applies them in order ends on the one that should win. A file outside the
/// repository is not consulted — an instruction set a run can be steered to by
/// a path is not an instruction set.
#[must_use]
pub fn instruction_files_for(repo_root: &Path, relative_path: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut directory = PathBuf::new();
    let push_from = |directory: &Path, found: &mut Vec<PathBuf>| {
        for name in INSTRUCTION_FILE_NAMES {
            let candidate = directory.join(name);
            if repo_root.join(&candidate).is_file() {
                found.push(candidate);
            }
        }
    };
    push_from(&directory, &mut found);
    for component in relative_path.components() {
        let std::path::Component::Normal(part) = component else {
            // `..` would walk out of the repository; nothing above the root
            // governs work inside it.
            continue;
        };
        directory = directory.join(part);
        if !repo_root.join(&directory).is_dir() {
            break;
        }
        push_from(&directory, &mut found);
    }
    found
}

/// The union of the built-in floor and whatever the repository added.
fn sensitive_paths(declared: &DeclaredPolicy) -> Vec<String> {
    let mut set: BTreeSet<String> = SENSITIVE_FLOOR.iter().map(|s| (*s).to_string()).collect();
    set.extend(
        declared
            .extra_sensitive_paths
            .iter()
            .map(|s| s.replace('\\', "/")),
    );
    set.into_iter().collect()
}

fn resolve_default_branch<R: CommandRunner>(runner: &R, repo_root: &Path) -> Option<String> {
    // The remote's own idea of its default, when a remote is configured.
    if let Some(line) = ok_stdout(
        runner,
        repo_root,
        "git",
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    ) {
        if let Some(branch) = line.trim().strip_prefix("origin/") {
            if !branch.is_empty() {
                return Some(branch.to_string());
            }
        }
    }
    // A local repository with no remote still has a checked-out branch.
    ok_stdout(
        runner,
        repo_root,
        "git",
        &["symbolic-ref", "--short", "HEAD"],
    )
    .map(|line| line.trim().to_string())
    .filter(|branch| !branch.is_empty())
}

/// Ask the forge whether `branch` is protected.
///
/// A 404 is the answer "there is no ruleset" and resolves to `Unprotected`.
/// Every other failure — no `gh`, no network, no token, no permission — is
/// `Unknown`, because none of them are evidence that the branch is unguarded.
fn resolve_protection<R: CommandRunner>(
    runner: &R,
    repo_root: &Path,
    branch: &str,
) -> BranchProtection {
    let args = [
        "api".to_string(),
        format!("repos/{{owner}}/{{repo}}/branches/{branch}/protection"),
    ];
    let outcome = match runner.run(repo_root, "gh", &args, QUERY_TIMEOUT) {
        Ok(outcome) => outcome,
        Err(error) => {
            return BranchProtection::Unknown {
                reason: format!("could not run gh: {error}"),
            }
        }
    };
    if outcome.succeeded() {
        return parse_protection(&String::from_utf8_lossy(&outcome.stdout));
    }
    let stderr = String::from_utf8_lossy(&outcome.stderr);
    if stderr.contains("Branch not protected") || stderr.contains("HTTP 404") {
        return BranchProtection::Unprotected;
    }
    BranchProtection::Unknown {
        reason: first_line(&stderr).unwrap_or_else(|| "gh reported an error".into()),
    }
}

fn parse_protection(body: &str) -> BranchProtection {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return BranchProtection::Unknown {
            reason: "gh returned a body that is not JSON".into(),
        };
    };
    let required_checks = value
        .pointer("/required_status_checks/contexts")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let required_reviews = value
        .pointer("/required_pull_request_reviews/required_approving_review_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;
    let strict = value
        .pointer("/required_status_checks/strict")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    BranchProtection::Protected {
        required_checks,
        required_reviews,
        strict,
    }
}

/// Declared commands win; otherwise what the repository's task runner offers.
///
/// Detection never invents. If no recipe matches, the field stays `None` and
/// [`RepositoryPolicyProfile::unresolved`] says so, because a run that reports
/// "verified" from a command nobody declared has proven nothing.
fn resolve_verification<R: CommandRunner>(
    runner: &R,
    repo_root: &Path,
    declared: &DeclaredPolicy,
) -> VerificationCommands {
    let recipes = just_recipes(runner, repo_root);
    let detect = |candidates: &[&str]| -> Option<Vec<String>> {
        candidates
            .iter()
            .find(|name| recipes.contains(**name))
            .map(|name| vec!["just".to_string(), (*name).to_string()])
    };
    VerificationCommands {
        focused: declared
            .focused_verification
            .clone()
            .filter(|command| !command.is_empty())
            .or_else(|| detect(&["dev-check", "test-changed", "gates", "check"])),
        full: declared
            .full_verification
            .clone()
            .filter(|command| !command.is_empty())
            .or_else(|| detect(&["verify", "ci", "test"])),
    }
}

fn just_recipes<R: CommandRunner>(runner: &R, repo_root: &Path) -> BTreeSet<String> {
    let Some(stdout) = ok_stdout(runner, repo_root, "just", &["--summary"]) else {
        return BTreeSet::new();
    };
    stdout
        .split_whitespace()
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
}

fn find_pr_template(repo_root: &Path) -> Option<PathBuf> {
    PR_TEMPLATE_PATHS
        .iter()
        .map(PathBuf::from)
        .find(|candidate| repo_root.join(candidate).is_file())
}

/// Stdout of a command that exited zero, or `None`. A failed query is not an
/// error here: every caller has a defined answer for "could not determine".
fn ok_stdout<R: CommandRunner>(
    runner: &R,
    cwd: &Path,
    program: &str,
    args: &[&str],
) -> Option<String> {
    let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    let outcome = runner.run(cwd, program, &args, QUERY_TIMEOUT).ok()?;
    outcome
        .succeeded()
        .then(|| String::from_utf8_lossy(&outcome.stdout).into_owned())
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// Match `path` against a configuration glob.
///
/// `**` spans separators, `*` and `?` do not. Enough for the patterns people
/// actually write in a sensitive-paths list, and small enough to read.
fn glob_match(pattern: &str, path: &str) -> bool {
    glob_here(pattern.as_bytes(), path.as_bytes())
}

fn glob_here(pattern: &[u8], path: &[u8]) -> bool {
    if pattern.is_empty() {
        return path.is_empty();
    }
    if pattern.starts_with(b"**") {
        let rest = strip_doublestar(pattern);
        // `**` may consume nothing, or any run of characters including `/`.
        for split in 0..=path.len() {
            if glob_here(rest, &path[split..]) {
                return true;
            }
        }
        return false;
    }
    match pattern[0] {
        b'*' => {
            for split in 0..=path.len() {
                if path[..split].contains(&b'/') {
                    break;
                }
                if glob_here(&pattern[1..], &path[split..]) {
                    return true;
                }
            }
            false
        }
        b'?' => !path.is_empty() && path[0] != b'/' && glob_here(&pattern[1..], &path[1..]),
        literal => !path.is_empty() && path[0] == literal && glob_here(&pattern[1..], &path[1..]),
    }
}

/// Skip a `**` and the separator that follows it, so `.github/**` matches
/// `.github/workflows/ci.yml` and also `.github/CODEOWNERS`.
fn strip_doublestar(pattern: &[u8]) -> &[u8] {
    let rest = &pattern[2..];
    match rest.first() {
        Some(b'/') => &rest[1..],
        _ => rest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_double_star_spans_directories_but_a_single_star_does_not() {
        assert!(glob_match(".github/**", ".github/workflows/ci.yml"));
        assert!(glob_match(".github/**", ".github/CODEOWNERS"));
        assert!(glob_match("**/AGENTS.md", "AGENTS.md"));
        assert!(glob_match("**/AGENTS.md", "crates/foo/AGENTS.md"));
        assert!(glob_match(
            "scripts/check-*.py",
            "scripts/check-crate-layers.py"
        ));
        assert!(!glob_match(
            "scripts/check-*.py",
            "scripts/nested/check-thing.py"
        ));
        assert!(!glob_match(".github/**", "docs/.github/x"));
    }

    #[test]
    fn the_sensitive_floor_cannot_be_shrunk_by_declaration() {
        // The escalation this exists to stop: a patch edits the repository's
        // own configuration so that the next patch is not reviewed.
        let declared = DeclaredPolicy {
            extra_sensitive_paths: vec!["src/secrets.rs".into()],
            ..DeclaredPolicy::default()
        };
        let paths = sensitive_paths(&declared);
        assert!(paths.iter().any(|p| p == "src/secrets.rs"));
        for floor in SENSITIVE_FLOOR {
            assert!(
                paths.iter().any(|p| p == floor),
                "declaration removed {floor} from the floor"
            );
        }
    }

    #[test]
    fn an_empty_declaration_does_not_erase_a_detected_command() {
        // `focused = []` in configuration must not resolve to "no checks".
        let declared = DeclaredPolicy {
            focused_verification: Some(vec![]),
            full_verification: Some(vec![]),
            ..DeclaredPolicy::default()
        };
        let commands = VerificationCommands {
            focused: declared
                .focused_verification
                .clone()
                .filter(|c| !c.is_empty()),
            full: declared.full_verification.clone().filter(|c| !c.is_empty()),
        };
        assert!(!commands.can_verify());
        assert_eq!(commands.focused, None, "an empty list is not a command");
    }

    #[test]
    fn unknown_protection_lists_no_checks_and_says_it_does_not_know() {
        let unknown = BranchProtection::Unknown {
            reason: "no token".into(),
        };
        assert!(unknown.required_checks().is_empty());
        assert!(!unknown.is_determined());
        // And the distinction that matters: absent is a determined answer.
        assert!(BranchProtection::Unprotected.is_determined());
        assert!(BranchProtection::Unprotected.required_checks().is_empty());
    }

    #[test]
    fn protection_is_parsed_from_what_the_forge_returns() {
        let body = serde_json::json!({
            "required_status_checks": { "strict": true, "contexts": ["verify", "gates"] },
            "required_pull_request_reviews": { "required_approving_review_count": 2 }
        })
        .to_string();
        match parse_protection(&body) {
            BranchProtection::Protected {
                required_checks,
                required_reviews,
                strict,
            } => {
                assert_eq!(required_checks, vec!["verify", "gates"]);
                assert_eq!(required_reviews, 2);
                assert!(strict);
            }
            other => panic!("expected protected, got {other:?}"),
        }
    }

    #[test]
    fn a_body_that_is_not_json_is_unknown_rather_than_unprotected() {
        assert!(!parse_protection("<html>gateway timeout</html>").is_determined());
    }
}
