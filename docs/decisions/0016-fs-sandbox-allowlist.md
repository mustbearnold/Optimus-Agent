# ADR-0016-B: FS sandbox allowlist for desktop Files pane

> **Alias:** ADR-0016-B (file historically numbered `0016-fs-sandbox-allowlist.md`).
> Distinct from ADR-0016-A (canonical tool/pack contract). See decisions index.

- **Status:** Accepted; `FsRoots` read path Confirmed (implementation complete beyond original “in progress” wording)
- **Date:** historical Phase P1; status honesty 2026-07-25 (P16)

## Context
Optimus Desktop needs a Hermes/Codex-class Files rail. Arbitrary filesystem access is unacceptable on a personal-agent host.

## Decision
All desktop FS IPC (`fs_list`, `fs_read`, later write/rename/delete) goes through `optimus_kernel::fs_sandbox::FsRoots`:

1. Explicit root list (default: `OPTIMUS_HOME`; projects add roots later).
2. Canonicalize-and-prefix check; reject `..` and symlink escape.
3. Secret basename denylist (`.env`, `auth.json`, `*.pem`, etc.) unless `allow_secrets` grant.
4. Read size caps (default ≤ 1–2 MiB).

## Consequences
- UI never receives paths outside roots.
- SmartDeny can gate writes separately.
- Tests in `fs_sandbox` are security-critical regressions.

## Alternatives rejected
- Full user-profile access by default
- iframe `file://` browsing
