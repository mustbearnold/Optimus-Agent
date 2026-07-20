# Phase 17b — Codex OAuth 400 fix + GPT-5.6 Sol/Terra/Luna + search

Date: 2026-07-19

## Root cause of UI 400
ChatGPT Codex OAuth rejects:
- top-level `reasoning_effort` → `Unsupported parameter`
- bare `service_tier: priority` (removed; Fast caps effort instead)
- invalid model ids (`gpt-5.4-pro`, `gpt-5.5-pro`, bare `gpt-5.6`, `sol`…)
- effort `ultra` (not supported; maps → `max`)

Supported efforts (live): `none|minimal|low|medium|high|xhigh|max`

## OAuth-valid models (live-probed)
- `gpt-5.6-sol` · `gpt-5.6-terra` (default) · `gpt-5.6-luna`
- `gpt-5.5` · `gpt-5.4` · `gpt-5.4-mini` · `gpt-5.3-codex-spark`

## Search
Multi-backend `web_search`: Google News RSS → DDG → Wikipedia.  
Live: “news today in nz” returned real NZ Herald/Stuff/RNZ headlines via Codex + tool calls.

## CLI
```bash
optimus chat --provider codex --model gpt-5.6-terra --thinking medium "…"
optimus chat --provider codex --model gpt-5.6-sol --thinking high "…"
```
