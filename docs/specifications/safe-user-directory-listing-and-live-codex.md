---
title: Safe user directory listing and live Codex regression
status: current
issue: https://github.com/mustbearnold/Optimus-Agent/issues/5
owners:
  - optimus-packs
  - optimus-kernel
  - optimus-desktop
---

# Safe user directory listing and live Codex regression

## Evidence

**Confirmed current behaviour:** On 2026-07-21 a native installed Desktop turn using Codex OAuth, `gpt-5.6-sol`, and medium thinking received `list all files on my onedrive desktop on windows`. The first model step activated the Desktop pack. The second step emitted a recursive PowerShell `terminal` call because no implemented structured directory-listing tool was advertised. Runtime job `8bbcab35-d710-496e-b42c-1a03d19869cd` entered `awaiting_approval`, while execution manifest `0e5ec175-87ce-47f2-ae1a-eb20be2660f8` and the Desktop message were marked failed after 9.6 seconds. The Active Tasks card retained the text `running`.

## Required behaviour

### Typed capability

The Desktop pack MUST expose an implemented `list_directory` tool after activation.

The input contract MUST accept only:

- `location`: `workspace` or `onedrive_desktop`;
- `recursive`: optional boolean;
- `cursor`: optional non-negative integer offset;
- `limit`: optional positive integer, clamped to a host ceiling.

The tool MUST NOT accept an arbitrary absolute root. OneDrive Desktop discovery MUST derive candidates from the process's `OneDrive`, `OneDriveConsumer`, and `OneDriveCommercial` environment values, append `Desktop`, canonicalize existing directories, deduplicate canonical roots, and fail closed when no candidate exists.

### Listing semantics

Entries MUST remain under their canonical root, MUST NOT follow directory symlinks during recursion, and MUST be sorted deterministically by relative path. Each page MUST report the selected location, canonical roots, entries, the effective cursor and limit, whether results were truncated, and a next cursor when more entries remain. Secret file contents MUST never be read; this capability returns metadata only.

### Approval presentation

A tool stopped by SmartDeny MUST NOT remain visually `running`. Desktop MUST render the affected task as `approval required`, render an approval-required outcome rather than a generic failed pill, and direct the user to the existing Capabilities approval surface. The runtime approval gate MUST remain intact.

### Live-provider evaluation

A dedicated ignored-by-default integration evaluation MUST use the real Codex OAuth provider with `gpt-5.6-sol` and medium thinking. When explicitly enabled by a developer, it MUST run the original OneDrive Desktop prompt through a fresh kernel home and assert:

- `list_directory` was invoked;
- `terminal` was not invoked;
- the turn succeeded with non-empty assistant synthesis;
- the execution produced one terminal outcome.

Mocked provider tests remain required for deterministic CI. The live evaluation is an additional release/development gate, not a replacement for deterministic tests.

## Verification

- Tool descriptor, policy, schema, and pack activation tests.
- Filesystem discovery, containment, symlink, deterministic recursion, and pagination tests.
- Kernel scripted-model tool dispatch and terminal-outcome tests.
- Desktop Playwright approval-required and task-finalization tests.
- Explicit real Codex OAuth GPT-5.6 Sol medium evaluation.
- Full workspace format, Clippy, tests, and rustdoc.
- Engineering Memory generation, strict validation, and currentness.
- Native installed-app CUA verification of the successful live turn.
