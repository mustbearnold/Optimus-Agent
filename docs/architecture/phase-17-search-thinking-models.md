# Phase 17 — thinking levels, Codex catalog, web_search

Date: 2026-07-19

## User-facing
- Thinking: `off | low | medium | high | xhigh | max | ultra` → Codex `reasoning.effort`
- Fast mode → `service_tier: priority` when on
- Models: gpt-5.4 / mini / pro, gpt-5.5 / pro, gpt-5.3-codex (+spark), gpt-5.2, gpt-5.1-codex(-max)
- **web_search** core tool (DuckDuckGo HTML + Wikipedia fallback) — news queries work

## Evidence
```text
cargo test -p optimus-kernel  # incl. web_search_live 2/2
npx playwright test           # thinking levels + model options
```
