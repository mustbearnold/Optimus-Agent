---
knowledge_type: process
status: current
owns:
  - docs/contributing/artifact-naming.md
watches:
  - AGENTS.md
  - docs/contributing/github-conventions.md
  - docs/plans/s-plus-plus-plus-program.md
  - docs/architecture/architecture-marks.md
  - docs/decisions/README.md
  - .github/pull_request_template.md
covers:
  - docs/contributing/artifact-naming.md
depends_on:
  - docs/contributing/github-conventions.md
  - docs/plans/s-plus-plus-plus-program.md
  - docs/architecture/architecture-marks.md
validated_by: []
last_verified_commit: null
---

# Artifact naming planes (mandatory)

This is the **canonical identity model** for Optimus engineering artifacts.
Humans and **coding agents** must use these planes without conflating them.

GitHub delivery mechanics (commits, branches, labels, PR titles) live in
[github-conventions.md](./github-conventions.md). This document defines **what
each identifier means** and **which plane it belongs to**.

## Core law

> **Identifiers from different planes are never interchangeable.**
>
> `P12` ≠ `PR #12` ≠ `ADR-0012` ≠ grade `S+++` ≠ runtime `agent@version`.

Coding agents **must** refuse to invent cross-plane renames, “align” a program
phase to a PR number, or treat a GitHub issue number as an ADR id.

## The six planes

| Plane | Token shape | Authority | Example |
|---|---|---|---|
| **1. Decision** | `ADR-NNNN` (zero-padded) | `docs/decisions/NNNN-*.md` | `ADR-0034` |
| **2. Program** | `P##` (S+++ program phase) | `docs/plans/s-plus-plus-plus-program.md` | `P12` Security envelope |
| **3. Plan / microtask** | Plan-local id (`S1`, `C3`, `M7`…) | Owning plan doc under `docs/plans/` | P12 microtask `S3` |
| **4. Delivery** | `PR #N` + local branch `pr/N-slug` | GitHub + `scripts/github_pr_branch.py` | `PR #21`, branch `pr/21-p12-command-fs-envelope` |
| **5. Grade / mark** | Mark name + grade | `docs/architecture/architecture-marks.md` | Security **A-** → **S+++** |
| **6. Runtime product** | `id@version` / crate / pack id | Source contracts, SemVer, EM | `workspace_writer@1`, crate `optimus-runtime` |

### Plane 1 — Decision (ADR)

- File: `docs/decisions/NNNN-short-kebab.md`
- Title: `# ADR-NNNN: <Decision title> (P##)` when a program phase owns it
- Numbers are **monotonic and permanent**. Never renumber, reuse, or “fix”
  history by rewriting an old ADR to hide prior reasoning (see ADR index).
- New ADRs use the modern frontmatter + sections template (Context, Decision,
  Consequences, Alternatives, Risks, Reconsideration).
- Link the program phase in prose (`P11`) and in the title parenthetical; do
  **not** force ADR number == program phase.

### Plane 2 — Program (`P##`)

- S+++ architecture climb only: **P10–P19** in
  [s-plus-plus-plus-program.md](../plans/s-plus-plus-plus-program.md)
  (P0–P5 = trust spine; P6–P9 reserved).
- Sequence is **grade-ordered**, not GitHub-ordered.
- One program phase may span **multiple PRs**.
- One PR may touch **at most one primary program phase** (plus hold-suite
  regressions). Do not merge “P12+P13” in one delivery unless explicitly
  approved as a hold/fix exception.
- In commit/PR titles, program phase is optional scope **text**, not a
  substitute for the delivery number:

```text
🏗️ architecture: S+++ P12 command capability envelope
♻️ refactor(kernel): peel agent contracts for P11
```

### Plane 3 — Plan / microtask

- Microtasks live inside plan docs (`M*`, `C*`, `S*`, exit gates).
- Issues may cite microtasks: `architecture: P12 S3 path preflight for RunCommand`.
- Microtask ids are **plan-local**. Do not mint global “ticket ids” that collide
  with ADR or PR numbers.

### Plane 4 — Delivery (GitHub)

| Stage | Local branch | Remote PR head | Identity |
|---|---|---|---|
| Before PR | `wip/<short-kebab>` | (none / same) | no PR yet |
| PR open | **`pr/<N>-<short-kebab>`** | stays `wip/<short-kebab>` | **PR #N** is truth |
| Merged | delete local | may auto-delete remote | PR remains history |

Rules (coding-agent hard gates):

1. **PR number is assigned by GitHub**, never chosen to match `P##`.
2. After open: local branch **must** be `pr/<N>-…` via
   `python3 scripts/github_pr_branch.py open|adopt`.
3. **Never rename/delete the remote head** of an open PR (closes the PR).
4. Slug may include the program phase for humans (`p12-command-fs-envelope`)
   but the **leading number is always the PR number**.
5. Commits and PR titles: **emoji-first Conventional Commits**
   ([github-conventions.md](./github-conventions.md)).
6. Labels: emoji + `namespace:value`; minimum on PRs:
   one `type:` + ≥1 `area:` + one `size:`.

### Plane 5 — Grade / mark

- Grades (`S+++`, `A-`, …) measure **architecture quality dimensions**.
- A grade moves only when source + tests + docs meet exit criteria
  ([architecture-marks.md](../architecture/architecture-marks.md)).
- Never claim a mark is S+++ because “P12 is done” without updating marks and
  gates. Planned work is never graded Confirmed.

### Plane 6 — Runtime product identity

- Agents, workflows, packs, tools, and crate public APIs use **product ids and
  versions** (`agent_id@version`, `ToolDesc`, SemVer crates).
- These are **not** program phases and **not** ADRs.
- Do not name a Rust type `P12Runner` or an agent `adr_0034_agent`.

## Worked example (do this every time)

| Layer | Correct value | Wrong value |
|---|---|---|
| Program | **P12** Security boundary → S+++ | calling it “phase 21” |
| ADR (if needed) | **ADR-0035** (next free number) | ADR-0012 or ADR-P12 |
| Delivery | **PR #21** (whatever GitHub assigns) | forcing branch `pr/12-…` because P12 |
| Local branch | `pr/21-p12-command-fs-envelope` | `p12`, `pr/12-…`, `feature/security` |
| Remote head | `wip/p12-command-fs-envelope` | renamed to `pr/21-…` (closes PR) |
| Grade target | Security **S+++** after exit gate | “S+++ because PR merged” |
| Runtime | capability envelope APIs / effect ids | `P12Envelope` as public product name |

Cross-reference phrase for PR bodies:

```markdown
**Planes:** program `P12` · delivery `PR #21` · decision `ADR-00xx` (if any) ·
mark Security A-→S+++
```

## Coding-agent enforcement checklist

Before naming a file, branch, commit, ADR, issue, or PR, every coding agent
**must**:

1. Identify which **plane** the identifier belongs to.
2. Use the **token shape** for that plane only.
3. Never set Delivery number equal to Program phase “for neatness”.
4. Never renumber ADRs or invent ADR numbers without scanning
   `docs/decisions/`.
5. Put program phase in **title text** / body / ADR parenthetical; put PR
   number only in **delivery** (`PR #N`, `pr/N-…`).
6. Follow [github-conventions.md](./github-conventions.md) for emoji commits,
   labels, and branch lifecycle.
7. On architecture work: update plan microtasks, marks, ADR (if boundary), and
   Engineering Memory per `AGENTS.md` workflow — same change set as code when
   the phase exit gate requires it.
8. If unsure of the next ADR number or program phase: **read the index/plan**;
   do not guess.

## Where each plane is written

| Plane | Primary docs |
|---|---|
| Decision | `docs/decisions/`, [README index](../decisions/README.md) |
| Program | `docs/plans/s-plus-plus-plus-program.md` (+ trust spine for P0–P5) |
| Plan / microtask | `docs/plans/**` |
| Delivery | [github-conventions.md](./github-conventions.md), PR template, `github_pr_branch.py` |
| Grade | `docs/architecture/architecture-marks.md` |
| Runtime product | crate APIs, `optimus-packs::ToolDesc`, version gates, EM |

## Anti-patterns (refuse these)

- “Open PR #12 for P12 so numbers match”
- Branch `pr/12-…` when the open PR is `#21`
- ADR titled only “P12” with no decision statement
- Renaming remote `wip/…` to `pr/N-…` (closes the PR)
- Commit subject without leading type emoji
- Claiming mark S+++ without marks file + exit tests
- Using program phase as a runtime agent or workflow id
- Free-text labels or branches (`tmp`, `fix2`, `Johns-PR`)

## Related

- Developer laws: [`AGENTS.md`](../../AGENTS.md) (Naming planes section)
- GitHub process: [github-conventions.md](./github-conventions.md)
- Product runtime constitution (separate): [`OPTIMUS_AGENTS.md`](../../OPTIMUS_AGENTS.md)
  — does **not** govern repo artifact naming
