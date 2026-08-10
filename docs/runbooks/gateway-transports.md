---
doc_id: runbook-gateway-transports
doc_type: reference
plane: current
status: planned
authority: supporting
summary: Operator runbook for gateway transports — Telegram, Discord, Slack, Email live today; WhatsApp and Signal per ADR-0091.
reviewed_on: 2026-08-11
review_by: 2026-12-11
---

# Gateway transports runbook

Planned. Contents (next delivery): per-transport configuration files under
`{home}/gateway/*.json`, env-var credentials, allowlist shapes, the
`optimus gateway run` supervisor lifecycle, and the signal-cli system
dependency + WhatsApp webhook operator checklist (ADR-0091).

Live transports as of 2026-08-11: Telegram (long-poll), Discord (gateway
websocket), Slack (Socket Mode), Email (IMAP/SMTP) — see spec-017.
