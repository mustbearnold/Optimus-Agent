# Issue tracker: GitHub

Issues and PRDs for this repo live as GitHub issues on
`mustbearnold/Optimus-Agent`. Use the `gh` CLI for all operations.

## Conventions

- **Create an issue**: use a repository issue form or
  `gh issue create --title "..." --body-file <path>`. Every issue body must
  contain `Goal`, `Context`, `Constraints`, and testable `Done when` sections.
- **Read an issue**: `gh issue view <number> --comments`, filtering comments by `jq` and also fetching labels.
- **List issues**: `gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'` with appropriate `--label` and `--state` filters.
- **Comment on an issue**: `gh issue comment <number> --body "..."`
- **Apply / remove labels**: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **Close**: `gh issue close <number> --comment "..."`

Infer the repo from `git remote -v` — `gh` does this automatically when run inside a clone.

## Required issue contract

- One issue describes one independently valuable outcome. Do not bundle
  unrelated outcomes or split below a complete, reviewable change.
- One Codex task owns that issue, with one `wip/<slug>` branch, one dedicated
  `local/worktrees/<slug>` checkout, and one PR.
- Assign the issue and move it to `🚧 status:in-progress` before implementation.
- Activate `caveman-optimus`. Use Plan mode first for architecture, ambiguity,
  or risk; otherwise record a task plan before writing.
- Public issues, comments, PRs, commits, and logs contain no credentials or
  private information. Redact before publication; push protection is a backup.

## Repo-specific constraints that override the defaults

These come from `AGENTS.md` and `docs/contributing/github-conventions.md`, and
they bind any skill that opens a PR or names a branch:

- **A change/build/fix/delivery request activates the full repository delivery
  loop.** Codex may commit, push, open/update the draft PR, request review, and
  enable gated auto-merge for the named issue. Read-only requests do not.
  Install, deploy, release, and live-model actions still require explicit scope.
- **Branch naming is two-plane.** The local branch is `pr/<N>-<slug>`; the
  remote head stays `wip/<slug>`. Renaming or deleting the remote head closes
  the open PR. See `docs/contributing/artifact-naming.md`.
- **The issue worktree is the only writer.** Start it from fresh `origin/main`;
  never implement in the main checkout or another issue's worktree.
- **`python3 scripts/github_pr_branch.py check` must exit 0** before a merge.
- **Commits are emoji-first Conventional Commits.** Open the PR as draft,
  request `@codex review`, resolve findings/conversations, make it ready, then
  run `gh pr merge --auto --merge` after all local gates pass. Required CI and
  branch protection decide when GitHub merges.
- **Monitor to a terminal outcome.** Confirm `MERGED`, issue closure, and
  worktree/local-branch cleanup. Otherwise report an evidenced blocker; never
  leave an open PR unattended or bypass a red gate.

## Pull requests as a triage surface

**PRs as a request surface: no.** _(Set to `yes` if this repo treats external PRs as feature requests; `/triage` reads this flag.)_

When set to `yes`, PRs run through the same labels and states as issues, using the `gh pr` equivalents:

- **Read a PR**: `gh pr view <number> --comments` and `gh pr diff <number>` for the diff.
- **List external PRs for triage**: `gh pr list --state open --json number,title,body,labels,author,authorAssociation,comments` then keep only `authorAssociation` of `CONTRIBUTOR`, `FIRST_TIME_CONTRIBUTOR`, or `NONE` (drop `OWNER`/`MEMBER`/`COLLABORATOR`).
- **Comment / label / close**: `gh pr comment`, `gh pr edit --add-label`/`--remove-label`, `gh pr close`.

GitHub shares one number space across issues and PRs, so a bare `#42` may be either — resolve with `gh pr view 42` and fall back to `gh issue view 42`.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --comments`.

## Wayfinding operations

Used by `/wayfinder`. The **map** is a single issue with **child** issues as tickets.

- **Map**: a single issue labelled `🗺️ wayfinder:map`, holding the Notes / Decisions-so-far / Fog body. `gh issue create --label "🗺️ wayfinder:map"` (label names carry their emoji prefix — `gh --label` needs the full name).
- **Child ticket**: an issue linked to the map as a GitHub sub-issue (`gh api` on the sub-issues endpoint). Where sub-issues aren't enabled, add the child to a task list in the map body and put `Part of #<map>` at the top of the child body. Labels: one of `🔍 wayfinder:research` / `🧪 wayfinder:prototype` / `🔥 wayfinder:grilling` / `🔧 wayfinder:task` (full names, emoji included). Once claimed, the ticket is assigned to the driving dev.
- **Blocking**: GitHub's **native issue dependencies** — the canonical, UI-visible representation. Add an edge with `gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`, where `<blocker-db-id>` is the blocker's numeric **database id** (`gh api repos/<owner>/<repo>/issues/<n> --jq .id`, _not_ the `#number` or `node_id`). GitHub reports `issue_dependencies_summary.blocked_by` (open blockers only — the live gate). Where dependencies aren't available, fall back to a `Blocked by: #<n>, #<n>` line at the top of the child body. A ticket is unblocked when every blocker is closed.
- **Frontier query**: list the map's open children (`gh issue list --state open`, scoped to the map's sub-issues / task list), drop any with an open blocker (`issue_dependencies_summary.blocked_by > 0`, or an open issue in the `Blocked by` line) or an assignee; first in map order wins.
- **Claim**: `gh issue edit <n> --add-assignee @me` — the session's first write.
- **Resolve**: `gh issue comment <n> --body "<answer>"`, then `gh issue close <n>`, then append a context pointer (gist + link) to the map's Decisions-so-far.
