# S+++ P14 verification — observability export

Date: 2026-07-25  
Planes: program **P14** · decision **ADR-0037** · delivery (open PR assigns)

## Exit evidence

| Microtask | Evidence |
|---|---|
| O1 ADR export format | `docs/decisions/0037-local-causal-export.md` (local JSON, not OTLP) |
| O2 Export CLI + redaction | `write_causal_export`, `optimus trace export`; tests redaction |
| O3 Obs gate | `scripts/check-observability-gate.py` export surface + causal_trace |
| O4 Denial + cancel terminals | `security_denial_codes_*`, `cancelled_turn_is_reconstructible_*` |
| O5 Observability **S+++** | architecture-marks + system-overview + obs map |

## Commands

```bash
cargo test -p optimus-kernel --test causal_trace
python3 scripts/check-observability-gate.py
```

## Grade moves

| Mark | Before | After |
|---|---|---|
| Observability / eval | A- | **S+++** |
