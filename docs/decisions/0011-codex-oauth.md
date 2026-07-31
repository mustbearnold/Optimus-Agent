---
doc_id: decisions-0011-codex-oauth
doc_type: decision
plane: decision
status: current
authority: record
summary: Decision record for ADR-0011: OpenAI Codex OAuth provider, including its context, consequences, and current documentary status.
reviewed_on: 2026-07-31
review_by: 2026-10-31
---

# ADR-0011: OpenAI Codex OAuth provider

## Status

Accepted — 2026-07-18

## Context

Hermes supports ChatGPT Codex OAuth (`openai-codex`) with tokens in `~/.hermes/auth.json`, refresh against `auth.openai.com`, and inference on `chatgpt.com/backend-api/codex` via the **Responses** API. Optimus users on this machine already have Codex credentials; Optimus must consume them without hijacking Hermes’ store, and offer its own login/import path.

## Decision

1. **Token store** at `{OPTIMUS_HOME}/auth.json` (Optimus-owned; never writes Hermes/Codex CLI files).
2. **Import sources** (read-only):
   - Hermes: `%LOCALAPPDATA%/hermes/auth.json` → `providers.openai-codex.tokens` or pool
   - Codex CLI: `~/.codex/auth.json` → `tokens.access_token` / `refresh_token`
3. **Refresh**: `POST https://auth.openai.com/oauth/token`  
   `grant_type=refresh_token&client_id=app_EMoamEEZ73f0CkXaXp7hrann&refresh_token=…`
4. **Device login** (interactive CLI): OpenAI deviceauth usercode/token + auth code exchange (same endpoints as Hermes).
5. **Inference**: `CodexOAuthModel` implements `ModelProvider` via  
   `POST {base}/responses` with Chat→Responses mapping (tools as function tools).  
   Default base: `https://chatgpt.com/backend-api/codex`  
   Default model: `gpt-5.4` (override `OPTIMUS_CODEX_MODEL`).
6. **Headers**: `Authorization: Bearer <access>` (+ `ChatGPT-Account-Id` when `account_id` present).
7. **CLI**:
   - `optimus auth codex status|import|login|logout`
   - `optimus chat --provider codex "…"`

## Non-goals

- Mutating Hermes/Codex CLI auth files
- Credential pool rotation (single Optimus session token)
- Full Hermes Responses parity (encrypted reasoning replay, etc.)

## Security

- Never log tokens
- Import is explicit user action
- Refresh token single-use risk documented (prefer Optimus-owned session via `login`)
