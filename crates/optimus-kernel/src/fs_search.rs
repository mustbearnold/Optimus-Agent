//! Searching the workspace: by content, and by name.
//!
//! Both are backed by ripgrep's own crates rather than by shelling out to `rg`.
//! Three reasons, in order of how much they matter:
//!
//! 1. **Policy.** A shelled-out search is a process spawn, so it carries
//!    `ToolPolicy::Process` and trips SmartDeny — every lookup becomes an
//!    approval prompt. Linked in, a search is what it actually is: a workspace
//!    read, the same class as `read_file`.
//! 2. **Availability.** `rg` may not be installed. A tool that works on the
//!    author's machine and not the user's is worse than no tool.
//! 3. **Confinement.** These run against [`FsRoots`], the same allowlist every
//!    other read obeys — not inside the command sandbox with its own separate
//!    and (as it turned out) differently-broken view of the world.
//!
//! Secret basenames are skipped for the same reason `read_text` refuses them.
//! A search that printed matching lines out of `.env` would be a way to read a
//! file the dedicated read tool declines to open.

use std::path::Path;

use globset::{Glob, GlobMatcher};
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::SearcherBuilder;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::fs_sandbox::{is_denied_name, FsRoots};

/// Never return more than this, whatever the caller asks for.
///
/// A search is answered into a model's context window, and an unbounded match
/// list is a way to spend the whole window on one call.
const MAX_RESULTS_CEILING: usize = 500;
const DEFAULT_MAX_RESULTS: usize = 100;

/// The largest file worth searching, matching `read_file`'s own cap.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Workspace-relative, forward-slashed, as every other tool reports paths.
    pub path: String,
    pub line: u64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct SearchRequest<'a> {
    pub pattern: &'a str,
    /// Subtree to search; defaults to the whole workspace.
    pub path: Option<&'a str>,
    /// Restrict to files whose relative path matches this glob.
    pub glob: Option<&'a str>,
    pub case_sensitive: bool,
    pub max_results: Option<usize>,
    /// Secret basenames remain denied unless the active developer grant opts in.
    pub allow_secrets: bool,
}

/// Search file contents for a regular expression.
pub fn search_content(
    roots: &FsRoots,
    request: &SearchRequest<'_>,
) -> Result<Vec<SearchHit>, String> {
    let limit = clamp_limit(request.max_results);
    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(!request.case_sensitive)
        .line_terminator(Some(b'\n'))
        .build(request.pattern)
        .map_err(|error| format!("bad pattern: {error}"))?;
    let glob = compile_glob(request.glob)?;
    let mut searcher = SearcherBuilder::new().line_number(true).build();

    let mut hits = Vec::new();
    for entry in walk(roots, request.path, request.allow_secrets)? {
        if hits.len() >= limit {
            break;
        }
        let relative = roots_relative(roots, &entry);
        if !glob.as_ref().is_none_or(|g| g.is_match(&relative)) {
            continue;
        }
        // Binary and oversized files are skipped rather than reported: a
        // matching byte offset inside a compiled artifact is not an answer to
        // anything a person asked.
        if std::fs::metadata(&entry).map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
            continue;
        }
        let remaining = limit - hits.len();
        let mut found = Vec::new();
        let sink = UTF8(|line, text| {
            found.push(SearchHit {
                path: relative.clone(),
                line,
                text: text.trim_end().to_string(),
            });
            Ok(found.len() < remaining)
        });
        // A file that cannot be read is not a failed search. Permissions and
        // races are normal in a live tree; the other matches still stand.
        let _ = searcher.search_path(&matcher, &entry, sink);
        hits.append(&mut found);
    }
    Ok(hits)
}

/// Find files whose workspace-relative path matches a glob.
pub fn find_files_with_secret_policy(
    roots: &FsRoots,
    glob: &str,
    path: Option<&str>,
    max_results: Option<usize>,
    allow_secrets: bool,
) -> Result<Vec<String>, String> {
    let limit = clamp_limit(max_results);
    let matcher = compile_glob(Some(glob))?.ok_or("a glob is required")?;
    let mut found: Vec<String> = walk(roots, path, allow_secrets)?
        .into_iter()
        .map(|entry| roots_relative(roots, &entry))
        .filter(|relative| matcher.is_match(relative))
        .take(limit)
        .collect();
    // Deterministic output: the walk is parallel-capable and a tool whose
    // result order shifts between identical calls is not replayable.
    found.sort();
    Ok(found)
}

fn clamp_limit(requested: Option<usize>) -> usize {
    requested
        .unwrap_or(DEFAULT_MAX_RESULTS)
        .clamp(1, MAX_RESULTS_CEILING)
}

fn compile_glob(pattern: Option<&str>) -> Result<Option<GlobMatcher>, String> {
    pattern
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            Glob::new(value)
                .map(|glob| glob.compile_matcher())
                .map_err(|error| format!("bad glob: {error}"))
        })
        .transpose()
}

/// Every searchable file under `path`, or under every root when absent.
///
/// Confinement is enforced by starting only at resolved roots and by refusing
/// to follow symlinks: a link pointing outside the workspace must not become a
/// way to read outside it.
fn walk(
    roots: &FsRoots,
    path: Option<&str>,
    allow_secrets: bool,
) -> Result<Vec<std::path::PathBuf>, String> {
    let starts = match path.filter(|value| !value.trim().is_empty()) {
        Some(path) => vec![roots
            .resolve_existing(path)
            .map_err(|error| format!("search {path}: {error}"))?],
        None => roots.roots().to_vec(),
    };

    let mut files = Vec::new();
    for start in starts {
        let mut builder = WalkBuilder::new(&start);
        builder.follow_links(false).hidden(true).git_ignore(true);
        for entry in builder.build().flatten() {
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let candidate = entry.into_path();
            if !allow_secrets && is_secret(&candidate) {
                continue;
            }
            files.push(candidate);
        }
    }
    Ok(files)
}

fn is_secret(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(is_denied_name)
}

fn roots_relative(roots: &FsRoots, path: &Path) -> String {
    for root in roots.roots() {
        if let Ok(relative) = path.strip_prefix(root) {
            return relative.to_string_lossy().replace('\\', "/");
        }
    }
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::{find_files_with_secret_policy, search_content, SearchRequest};
    use crate::fs_sandbox::FsRoots;
    use std::fs;
    use tempfile::TempDir;

    fn workspace() -> (TempDir, FsRoots) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {\n    needle();\n}\n").unwrap();
        fs::write(root.join("src/util.rs"), "pub fn helper() {}\n").unwrap();
        fs::write(root.join("README.md"), "no match here\n").unwrap();
        let roots = FsRoots::new(vec![root]).unwrap();
        (dir, roots)
    }

    fn search<'a>(pattern: &'a str) -> SearchRequest<'a> {
        SearchRequest {
            pattern,
            ..Default::default()
        }
    }

    #[test]
    fn a_match_reports_where_it_is_not_merely_that_it_exists() {
        let (_dir, roots) = workspace();
        let hits = search_content(&roots, &search("needle")).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "src/main.rs");
        assert_eq!(hits[0].line, 2, "a hit without a line number is a grep -l");
        assert_eq!(hits[0].text, "    needle();");
    }

    #[test]
    fn search_is_case_insensitive_unless_asked_otherwise() {
        let (_dir, roots) = workspace();
        assert_eq!(search_content(&roots, &search("NEEDLE")).unwrap().len(), 1);
        let exact = SearchRequest {
            pattern: "NEEDLE",
            case_sensitive: true,
            ..Default::default()
        };
        assert!(search_content(&roots, &exact).unwrap().is_empty());
    }

    #[test]
    fn a_glob_narrows_which_files_are_read() {
        let (_dir, roots) = workspace();
        let scoped = SearchRequest {
            pattern: "fn",
            glob: Some("**/*.md"),
            ..Default::default()
        };
        assert!(search_content(&roots, &scoped).unwrap().is_empty());
    }

    #[test]
    fn a_bad_pattern_is_an_error_not_an_empty_result() {
        // Empty results and "your regex is malformed" are different answers,
        // and a model shown the first will conclude the code is absent.
        let (_dir, roots) = workspace();
        assert!(search_content(&roots, &search("([unclosed")).is_err());
    }

    #[test]
    fn secrets_stay_unreadable_through_search() {
        // `read_file` refuses these by basename. A search that printed their
        // matching lines would route straight around that refusal.
        let (_dir, roots) = workspace();
        let root = &roots.roots()[0].clone();
        fs::write(root.join(".env"), "API_KEY=needle-secret\n").unwrap();
        let hits = search_content(&roots, &search("needle")).unwrap();
        assert!(
            hits.iter().all(|hit| !hit.path.contains(".env")),
            "search leaked a secret file: {hits:?}"
        );
    }

    #[test]
    fn results_are_capped_however_many_exist() {
        let (_dir, roots) = workspace();
        let root = roots.roots()[0].clone();
        fs::write(root.join("src/many.rs"), "needle\n".repeat(50)).unwrap();
        let capped = SearchRequest {
            pattern: "needle",
            max_results: Some(5),
            ..Default::default()
        };
        assert_eq!(search_content(&roots, &capped).unwrap().len(), 5);
    }

    #[test]
    fn an_absurd_limit_is_clamped_rather_than_honoured() {
        let (_dir, roots) = workspace();
        let greedy = SearchRequest {
            pattern: "needle",
            max_results: Some(usize::MAX),
            ..Default::default()
        };
        // Not an error — just bounded. The ceiling protects the context window.
        assert!(search_content(&roots, &greedy).unwrap().len() <= 500);
    }

    #[test]
    fn find_files_matches_on_the_relative_path() {
        let (_dir, roots) = workspace();
        let found = find_files_with_secret_policy(&roots, "src/*.rs", None, None, false).unwrap();
        assert_eq!(found, vec!["src/main.rs", "src/util.rs"]);
    }

    #[test]
    fn find_files_is_ordered_so_the_same_call_answers_the_same_way() {
        let (_dir, roots) = workspace();
        let first = find_files_with_secret_policy(&roots, "**/*.rs", None, None, false).unwrap();
        let second = find_files_with_secret_policy(&roots, "**/*.rs", None, None, false).unwrap();
        assert_eq!(first, second);
        assert!(first.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn searching_outside_the_workspace_is_refused() {
        let (_dir, roots) = workspace();
        let escape = SearchRequest {
            pattern: "root",
            path: Some("/etc"),
            ..Default::default()
        };
        assert!(
            search_content(&roots, &escape).is_err(),
            "the allowlist must bound a search as it bounds a read"
        );
    }
}
