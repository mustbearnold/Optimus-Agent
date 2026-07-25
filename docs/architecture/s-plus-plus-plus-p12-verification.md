# S+++ P12 verification — command capability envelope

Date: 2026-07-25  
Planes: program **P12** · decision **ADR-0035** · delivery **PR #22**

## Exit evidence

| Microtask | Evidence |
|---|---|
| S1 ADR | `docs/decisions/0035-command-capability-envelope.md` |
| S2 Linux confined bwrap | `crates/optimus-runtime/src/command_envelope.rs`; tests `command_envelope.rs` (write-outside fails) |
| S3 Windows residual / fail-closed | `command_envelope_supported`; ConfinedNoNetwork refuses non-Linux |
| S4 Isolation → envelope | `WorkIsolationMode::command_fs_envelope`; Kernel loads settings |
| S5 Nested breakout hold | existing cancellation nested systemd / setsid tests still green |
| S6 Shared egress | `crates/optimus-kernel/src/network_policy.rs`; browser + web_search |
| S7 Multi-agent re-grade | `architecture-marks.md` Multi-agent **S+++** |
| S8 Security S+++ | marks + security map + system-overview |

## Commands

```bash
cargo test -p optimus-runtime --test command_envelope
cargo test -p optimus-runtime
cargo test -p optimus-kernel --lib
cargo test -p optimus-kernel --test specialist_vertical
```

## Grade moves

| Mark | Before | After |
|---|---|---|
| Security boundary design | A- | **S+++** |
| Multi-agent readiness | S (interim) | **S+++** |
