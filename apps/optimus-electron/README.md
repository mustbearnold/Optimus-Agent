# optimus-electron

Repository-level default Electron shell for the Optimus React workbench.
Durable work, sessions, policy, approvals, files, artifacts, and terminal
outcomes remain owned by `optimus-desktop --host-only`.

## Prerequisites

```bash
cargo build -p optimus-desktop
bun install --frozen-lockfile
```

## Development

```bash
# Default: React Vite renderer plus the Rust host
bun run --cwd apps/optimus-electron dev

# Explicit rollback surface
bun run --cwd apps/optimus-electron dev:legacy-html
```

Production-like repository proof first builds relative assets and then launches
Electron without a Vite URL:

```bash
bun run --cwd apps/optimus-ui build
bun run --cwd apps/optimus-electron start
```

This is not an installed-app or packaging command.

## Environment

| Variable | Meaning |
|---|---|
| `OPTIMUS_HOST_PORT` | Rust host port; default `17865` |
| `OPTIMUS_HTTP_TOKEN` | Optional main-process host credential; the host mints one if unset |
| `OPTIMUS_HOME` | Optimus data home |
| `OPTIMUS_ELECTRON_USER_DATA` | Optional explicit Electron profile path; used for isolated compiled-shell evidence |
| `OPTIMUS_ELECTRON_UI` | `react` by default; `legacy` is the rollback switch |
| `OPTIMUS_UI_DEV_URL` | Explicit React development URL |
| `OPTIMUS_HOST_EXTERNAL=1` | Use an already-running Rust host |

## Architecture

```text
React renderer
  `-- context-isolated preload
        |-- bounded invoke/chat --> Electron main --> bearer + CSRF --> Rust host
        `-- Browser chrome/rect --> Electron main --> WebContentsView
```

Production assets are served from `apps/optimus-ui/dist` through
`optimus-app://ui/`. Electron main retains the bearer token; React receives no
credential. Main allowlists method names, bounds request sizes and the active
stream count, routes events by stream/session identity, and owns the stream
`AbortController`.

The native preview allows HTTPS and loopback HTTP only. It has no Node preload,
uses context isolation and sandboxing, and denies permissions, downloads, and
new windows. It is the user-facing preview, not the Rust agent Browser effector.

## Verification

```bash
bun run --cwd apps/optimus-ui test
bun run --cwd apps/optimus-ui build
bun run --cwd apps/optimus-electron check
xvfb-run -a bun run --cwd apps/optimus-electron test:e2e
```

The Playwright project includes deterministic React browser contracts and a
compiled Electron-shell smoke using an isolated `local/tmp/**` Optimus home and
offline provider. Evidence from that suite must be labelled “compiled Electron
shell,” not “installed application.”
