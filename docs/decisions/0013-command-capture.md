# ADR-0013: Command stdout/stderr capture

## Status

Accepted — 2026-07-18

## Context

Phase 10 terminal tool ran durable `RunCommand` jobs but only returned job status. Agents need stdout/stderr to act on command results (Hermes-class terminal tools surface output).

## Decision

1. **`CommandCapture`** on runtime: `stdout`, `stderr`, `exit_code`, `truncated_stdout` / `truncated_stderr`, `timed_out`
2. **Bounded pipes**: max 32 KiB per stream; excess discarded with truncate flags
3. **Timeout**: existing kill-on-timeout; capture partial output when available
4. **Durability**: append `command_output` event on the node with capture JSON (no secrets redaction yet — local workspace only)
5. **Kernel `terminal` tool**: returns capture fields in tool JSON after `run_all`
6. **Non-zero exit**: still node failed; capture attached to failure event payload when possible

## Non-goals

- Streaming live output to UI
- PTY / interactive shells
- Global secret scrubbing of capture
