# ATTIC

Quarantine for files of unclear value during the SDD migration. Nothing here
is deleted; git history and this index preserve every item. The human decides
fates; emptying the attic is a human decision (protocol invariant 5).

| File | Original path | Why atticked | Suggested fate |
|---|---|---|---|
| `claude-settings-local.json` | `.claude/settings.local.json` | Untracked agent-config cruft (Claude Code local settings, 105 B); the Claude adapter was removed from the repo (2026-08-04) and the file is inert | Delete |
