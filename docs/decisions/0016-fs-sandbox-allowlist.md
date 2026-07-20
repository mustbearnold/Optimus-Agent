# ADR-0016: FS sandbox allowlist for desktop Files pane

## Status
Accepted (implementation in progress Phase P1)

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
