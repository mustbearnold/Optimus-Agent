# Phase 16 — gateway HTTP + campaign IPC

Date: 2026-07-19  
Priority: function > Hermes; UI polish last.  
Tests: deterministic (unique home, free port, health poll not blind sleep, max_requests exit).

## Delivered

### HTTP webhook gateway (127.0.0.1 only)
```bash
set OPTIMUS_GATEWAY_TOKEN=<32-or-more-random-characters>
optimus gateway serve --port 8788
# optional: --max-requests N  (exits after N requests — test harness)
```

| Method | Path | Behavior |
|---|---|---|
| GET | `/health` | liveness |
| POST | `/inbound` | durable enqueue |
| POST | `/drain` | Kernel turn → outbox |
| POST | `/drain_all` | drain full inbox |
| GET | `/inbox` `/outbox` | inspect queues |

Channel adapters (Telegram later) POST the same JSON shape.

### Campaign desktop IPC
`campaign_list` · `campaign_create` · `campaign_run` · `campaign_status`

Current behaviour requires bearer authorization, validates any supplied browser
origin/CSRF header, bounds request bodies/rates/drain batches, and redacts public
errors and health paths. See ADR-0020.

## Evidence
```text
cargo test -p optimus-cli --test gateway_http  # ok
npx playwright test                            # 11 passed (campaign IPC)
install/relaunch                               # pid on Programs\OptimusAgent
```
