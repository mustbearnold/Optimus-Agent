# Optimus Agent

Optimus Agent is a durable local AI-agent runtime with a native Tauri desktop
shell, React workbench, evidence-backed memory, policy-controlled tools,
sessions, campaigns, cron, and multi-agent work. Ubuntu is the primary desktop
target; Windows keeps the PowerShell installer and WebView2 backend.

## Instruction authority

- Building or changing Optimus: [`AGENTS.md`](AGENTS.md) is the development
  authority for humans and coding agents.
- Running the installed product: [`OPTIMUS_AGENTS.md`](OPTIMUS_AGENTS.md) is the
  constitution embedded into Optimus chat sessions.
- Development requests are not product requirements. "Work autonomously on
  Optimus" tells a coding agent how to develop; it does not silently change how
  the Optimus product handles tools, permissions, models, or approvals.

## Quickstart

```bash
# Ubuntu prerequisites (native desktop)
sudo apt-get install -y build-essential pkg-config libgtk-3-dev \
  libwebkit2gtk-4.1-dev libxdo-dev desktop-file-utils

# Build and run the default desktop (Tauri + React over the Rust host)
cargo build -p optimus-desktop -p optimus-cli
bun install --frozen-lockfile
bun run --cwd apps/optimus-ui build
bun run --cwd apps/optimus-tauri dev

# Install for the current user (no root)
bash scripts/rebuild-install-relaunch.sh          # release build, install, relaunch

# All gates, one command
just verify
```

The default install creates `~/.local/share/optimus-agent/` with the host,
Tauri shell, CLI links, desktop entry, and uninstaller. Launch **Optimus
Agent** from the application menu. Full profiles, environment overrides,
verification, Windows commands, and uninstall: [`docs/runbooks/install-relaunch.md`](docs/runbooks/install-relaunch.md).

## Capabilities and law

- **Specs (living truth):** [`specs/BACKLOG.md`](specs/BACKLOG.md) and one
  directory per capability under `specs/NNN-<slug>/` — requirements and
  acceptance criteria; `plan.md`/`tasks.md` exist only while work is active.
- **Architecture:** [`docs/architecture.md`](docs/architecture.md) — system
  shape, surfaces, and measured claims.
- **Decisions:** [`docs/decisions/`](docs/decisions/README.md) — ADRs.
- **Runbooks:** [`docs/runbooks/`](docs/runbooks/) — install/relaunch,
  engineering-memory lenses, project hygiene, agent domain, architecture marks.
- **Development law:** [`AGENTS.md`](AGENTS.md) — main-only development, the
  instruction-plane firewall, naming planes, and the gate spine
  ([`scripts/verify.sh`](scripts/verify.sh) is the single source of truth).
- **Product constitution:** [`OPTIMUS_AGENTS.md`](OPTIMUS_AGENTS.md).

## North star

**Verified durable operator runtime with a measured learning loop.**
