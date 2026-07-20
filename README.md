# Optimus Agent

Rebuild of the personal-agent category to exceed Hermes Agent on reliability, measured learning, evidence-native memory, cost, security, durable multi-agent work, and Windows-first quality.

## Status

**Daily-use + operator path (phase 12):** browser HTTP effector, durable cron, streaming desktop, unsigned install/relaunch.

Scorecard: [sota-scorecard.md](docs/architecture/sota-scorecard.md) — architecture wins locked; surface breadth still catching Hermes.

## Docs

- [Architecture](docs/architecture/optimus-exceeds-hermes.md)
- ADRs 0000–0013 under `docs/decisions/`
- Phase verifications 0–11 under `docs/architecture/`
- [Desktop UI mock](docs/design/optimus-desktop-ui.html)

## Workspace

```text
apps/optimus-cli          jobs · skills · packs · chat · sessions · auth
apps/optimus-desktop      WebView2 shell + UI + Kernel IPC
crates/optimus-kernel     turn · Codex SSE · sessions · compression · tools
crates/optimus-store      job ledger + events
crates/optimus-graph      job/node domain
crates/optimus-runtime    effectors · capture · policy · skill bridge
crates/optimus-memory     MetaMemory
crates/optimus-skills     Skills 2.0
crates/optimus-packs      progressive capability packs
```

## Quick commands

```bash
cargo test --workspace -- --test-threads=1
cargo run -p optimus-cli -- auth codex import-hermes
cargo run -p optimus-cli -- chat --provider codex "hello"
cargo run -p optimus-desktop
cargo run -p optimus-desktop -- --http 8787   # Playwright / browser testing
# then: cd apps/optimus-desktop && npx playwright test

# After every desktop rebuild — unsigned local install + relaunch:
bash scripts/rebuild-install-relaunch.sh          # release
bash scripts/rebuild-install-relaunch.sh --dev    # faster
```

Install lands in `%LOCALAPPDATA%\Programs\OptimusAgent\` with Start Menu + Desktop shortcuts. See `docs/architecture/desktop-install-relaunch.md`.

## Dev (Windows)

```bash
export TEMP='C:/Users/mustb/AppData/Local/Temp'
export TMP='C:/Users/mustb/AppData/Local/Temp'
export CARGO_TARGET_DIR='E:/Projects/Optimus Agent/local/tmp/cargo-target'

cargo test --workspace
cargo run -p optimus-cli -- --help
```

## North star

**Verified durable operator runtime with a measured learning loop.**
