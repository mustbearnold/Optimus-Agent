# Optimus Agent

Optimus Agent is a durable local AI-agent runtime with a native desktop shell, evidence-backed memory, policy-controlled tools, sessions, campaigns, cron, and multi-agent work.

Ubuntu is the primary desktop target. Windows remains supported through platform-specific shell and installer paths.

## Instruction authority

- Building or changing Optimus: [`AGENTS.md`](AGENTS.md) is the development
  authority for humans and coding agents.
- Running the installed product: [`OPTIMUS_AGENTS.md`](OPTIMUS_AGENTS.md) is the
  constitution embedded into Optimus chat sessions.
- Development requests are not product requirements. “Work autonomously on
  Optimus” tells a coding agent how to develop; it does not silently change how
  the Optimus product handles tools, permissions, models, or approvals.

## Status

The daily-use path includes:

- **Default desktop:** Electron + React workbench over a Rust host (`optimus-desktop --host-only`)
- **Legacy rollback:** Wry/Tao native shell (WebKitGTK on Ubuntu / WebView2 on Windows)
- Durable Rust runtime and SQLite state
- Local command capture with approval controls
- Owned process-tree cancellation and timeouts
- Streaming chat, projects, sessions, campaigns, and cron
- User-scoped Ubuntu install, application launcher, CLI links, and uninstaller

See [sota-scorecard.md](docs/architecture/sota-scorecard.md) for the broader capability scorecard and
[desktop-shell-and-ipc-matrix.md](docs/contracts/desktop-shell-and-ipc-matrix.md) for shell/IPC authority.

Optimus uses independent product SemVer plus a fail-closed Hermes parity version. It is currently Optimus `0.1.0`, tracking Hermes `0.19.0`, with parity unverified. Run `optimus version` or see [optimus-versioning.md](docs/architecture/optimus-versioning.md). Installers reject a numerical Hermes version match unless every feature and comparative performance gate passes.

## Ubuntu prerequisites

The native desktop is tested on Ubuntu 26.04 x86_64.

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libgtk-3-dev \
  libwebkit2gtk-4.1-dev \
  libxdo-dev \
  desktop-file-utils
```

Install a current stable Rust toolchain if `cargo` is not already available:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Playwright UI tests also require Node.js, npm, and the pinned Chromium payload:

```bash
cd apps/optimus-desktop
npm ci
npx playwright install chromium
cd ../..
```

## Build and run on Ubuntu

```bash
cargo build -p optimus-desktop -p optimus-cli
cargo run -p optimus-desktop
```

Run the browser-compatible test server:

```bash
cargo run -p optimus-desktop -- --http 8787
```

Then open `http://127.0.0.1:8787` or run Playwright from `apps/optimus-desktop`.

## Install for the current Ubuntu user

No root access is used for application installation.

```bash
bash scripts/rebuild-install-relaunch.sh          # release build, install, relaunch
bash scripts/rebuild-install-relaunch.sh --dev    # debug build, install, relaunch
```

The default install creates:

```text
~/.local/share/optimus-agent/
  .optimus-agent-install
  bin/optimus-desktop
  bin/optimus
  bin/optimus-cli
  VERSION.txt
  install-meta.json
  uninstall.sh
  README-INSTALL.txt

~/.local/share/applications/optimus-agent.desktop
~/.local/share/icons/hicolor/scalable/apps/optimus-agent.svg
~/.local/bin/optimus
~/.local/bin/optimus-cli
```

Launch **Optimus Agent** from the Ubuntu application menu or run:

```bash
~/.local/share/optimus-agent/bin/optimus-desktop
```

The installer refuses a foreign non-empty install root, existing non-Optimus
CLI links, and symlinked desktop-entry/icon destinations. Uninstall requires
the Optimus ownership marker and never removes application state.

Uninstall:

```bash
~/.local/share/optimus-agent/uninstall.sh
```

See [desktop-install-relaunch.md](docs/architecture/desktop-install-relaunch.md) for profiles, environment overrides, verification, and Windows commands.

## Test

All gates run through one command:

```bash
just verify
```

Narrower tiers for the inner loop:

```bash
just gates    # static gates + fmt        (~1s)
just check    # + compile and clippy      (~35s)
just test     # + Rust tests              (~60s)
just ui       # Vitest, Electron, Playwright
```

[`scripts/verify.sh`](scripts/verify.sh) is the single source of truth. The
justfile, the `pre-push` hook, humans, and coding agents all call it, so there
is no second command list to drift. It runs every gate to completion and reports
the full picture rather than stopping at the first failure.

Enable the local gate once per clone (push is gated, not commit):

```bash
just setup-hooks
```

The Hermes parity gate is fail-closed by design and is deliberately **not** part
of `just verify`. Check it with `just parity`.

Clippy is advisory until the existing warnings are cleared; `just lint` shows
them, and `OPTIMUS_CLIPPY_STRICT=1` makes it a hard gate.

The runtime cancellation suite includes a Linux descendant-process regression. It proves that cancellation removes the full owned Unix process group and prevents delayed child effects.

## Workspace

Emoji provides a compact visual label for each workspace area.

```text
💻 apps/optimus-cli          jobs, skills, packs, chat, sessions, auth, vertical
🖥️ apps/optimus-desktop      Rust host (--host-only) + legacy Wry shell
🖥️ apps/optimus-electron     Default Electron shell (React workbench)
🎨 apps/optimus-ui           React + Vite SPA (default workbench UI)
🧠 crates/optimus-kernel     turns, providers, sessions (re-exports peels)
🤖 crates/optimus-agent      specialist contracts, registry, invocations
🔀 crates/optimus-workflow   workflow defs, DAG run store, built-in verticals
📦 crates/optimus-artifacts  content-addressed handoff store
🛰️ crates/optimus-ops        gateway + cron store
📊 crates/optimus-eval       offline eval / replay
💾 crates/optimus-store      job ledger and events
💾 crates/optimus-graph      job and node domain
🔁 crates/optimus-runtime    effectors, process ownership, capture, policy
🧩 crates/optimus-memory     MetaMemory
🎯 crates/optimus-skills     Skills 2.0
📚 crates/optimus-packs      progressive capability packs
🌐 crates/optimus-browser    CDP browser backend
```

## Developing Optimus

Start with [`AGENTS.md`](AGENTS.md). Development uses assigned isolated
worktrees and the repository-managed checkpoint/land workflow; coding agents do
not use raw history-changing Git, pull requests, issues, or `gh`.

Artifact identity planes (`P##` ≠ task id ≠ `ADR-NNNN` ≠ grade) are defined in
[artifact-naming.md](docs/contributing/artifact-naming.md).

### Default desktop (Electron + React)

```bash
cargo build -p optimus-desktop
npm --prefix apps/optimus-ui install && npm --prefix apps/optimus-ui run build
npm --prefix apps/optimus-electron install
npm --prefix apps/optimus-electron run dev   # React workbench + Rust host
```

Legacy HTML-in-Electron rollback: `npm --prefix apps/optimus-electron run dev:legacy-html`  
Legacy Wry native shell: `cargo run -p optimus-desktop` (no Electron).

See `apps/optimus-electron/README.md`, ADR-0028, and the IPC matrix contract.

## Windows

Windows keeps the PowerShell installer and the WebView2 desktop backend.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\rebuild-install-relaunch.ps1
```

The Bash installer automatically delegates to that script when run under Git Bash, MSYS, or Cygwin.

## Documentation

- [Documentation home](docs/README.md) — start here for every current answer
- [Current status](docs/current/status.md)
- [Current roadmap](docs/current/roadmap.md)
- [System overview](docs/architecture/system-overview.md)
- [Decision index](docs/decisions/README.md)

## North star

**Verified durable operator runtime with a measured learning loop.**
