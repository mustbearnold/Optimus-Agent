# Phase 14 — gateway + cron serve + trajectory eval

Date: 2026-07-19  
Priority: function > Hermes; UI polish last.

## Delivered

### 1. Local operator gateway (durable)
Path: `$OPTIMUS_HOME/gateway/{inbox,outbox,processed,failed}/`

| CLI | Behavior |
|---|---|
| `gateway send` | enqueue inbound JSON (simulates webhook/channel) |
| `gateway inbox` / `outbox` | list durable messages |
| `gateway drain` / `drain-all` | Kernel turn → outbox, archive inbox |

Adapter seam for Telegram/Discord later — same JSON shape.

### 2. Cron operator daemon
`cron serve --interval N [--with-gateway] [--max-loops K]`

Ticks due cron + optional full gateway drain each loop.

### 3. Deterministic trajectory eval
`eval run` — offline built-in suite (echo, memory recall, pack activate).  
Isolated case homes; no network; must be reproducible.

`eval report --binding PATH --measurements PATH [--thresholds PATH]` — exact
ten-case candidate report from bounded explicit JSON evidence. Inputs are
preflighted before run state; failing thresholds still print the report and return
a non-zero exit. `eval run` remains the compatibility trajectory command.

## Evidence

```text
kernel lib tests: 17 passed (gateway + eval unit)
gateway send → drain → outbox ok
eval run → passed=3 failed=0
cron serve --max-loops 1 --with-gateway → cron + gateway lines
playwright 10/10
install/relaunch pid on Programs\OptimusAgent
```

## Commands

```bash
optimus gateway send "hi" --channel local
optimus gateway drain
optimus eval run
python scripts/engineering_memory.py binding > ../optimus-binding.json
optimus eval report --binding ../optimus-binding.json --measurements measurements.json --thresholds thresholds.json
optimus cron serve --interval 5 --with-gateway
```
