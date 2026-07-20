# ADR-0009: Durable kernel chat sessions

## Status

Accepted — 2026-07-18

## Context

Kernel turns held `messages` only in RAM. Hermes differentiates itself with cross-session continuity; Optimus already has durable jobs/memory/skills — chat transcripts must match that durability bar.

## Decision

1. **Session store** at `{home}/sessions.db` (SQLite WAL)
   - `sessions(id, title, created_at, updated_at, packs_json, messages_json)`
2. **API**
   - `Kernel::open_session(home, config, session_id: Option)` — create or resume
   - `Kernel::session_id()` / `save_session()` after each successful turn
   - `list_sessions(home)` / `get_session_meta`
3. **Pack restore**
   - Persist loaded pack ids; on resume call `CapabilitySession::restore_loaded` (bypass activate budget for exact prior state; budget still applies to new activations)
4. **System prompt**
   - Still rebuilt from restored packs on each turn (cache-stable identity text can wait)
5. **CLI**
   - `--session <uuid>` on `chat` / `chat-offline`
   - `optimus sessions list`

## Non-goals

- Context compression / summarization (later)
- Multi-user gateway routing
- FTS session search (can add like Hermes session_search later)

## Consequences

- Process death no longer wipes chat
- Session files are local-only under OPTIMUS home
