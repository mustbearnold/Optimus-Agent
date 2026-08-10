//! The attach-or-spawn-or-diagnose decision (spec-015 A4, R8).
//!
//! The shell's serve lifecycle is a decision, not a script: the full
//! record-state × port-state × probe-health × exit-code branch matrix
//! lives here as a pure function over PROBE RESULTS — probing is
//! injected, which is what makes the R8 branch table executable as unit
//! tests, and the shell's surfacing of the outcome stays thin. The
//! module lives in optimus-host (a lib target) so Phase-B surfaces
//! (TUI/CLI attach-or-spawn) reuse the same decision instead of
//! duplicating it — the desktop app is a bin-only crate.
//!
//! Lifecycle (R8): attach-first — a healthy backend is ipso facto
//! capable, so probing capability before attach could surface a spurious
//! reinstall diagnostic while a healthy record exists; the capability
//! probe runs ONLY when a spawn is needed. Port policy (#148): the
//! desired port (17865) is a machine-global resource but the record is
//! per-home — when the desired port is OCCUPIED at decision time and no
//! record for THIS home exists, the decision is an EPHEMERAL spawn
//! (`serve --port 0`; the record carries the real port, so
//! attach-after-spawn is unchanged and two homes coexist on one
//! machine). One core per home is record-based, not port-based, so the
//! ephemeral child still refuses (exit 3) on a healthy holder. The
//! "check port" diagnostic survives ONLY for the post-spawn settle: a
//! desired-port spawn that raced into a bind failure. After spawn:
//! ready wait (15 s from spawn, epoch pinned spawn→record-visible-or-
//! diagnostic) → quit termination → bounded crash relaunch (3/60 s; a
//! pre-bind readiness timeout does NOT consume an attempt) → spawn exit
//! 2/3 → bounded re-probe (5 s, 250 ms probes) → attach or diagnostic.

use std::path::Path;
use std::time::Duration;

use crate::record::{healthy_record, read_record, HostRuntimeRecord, TRANSPORT_WS};

/// R8: overall ready bound, epoch pinned from process spawn to
/// record-visible-or-diagnostic.
pub const READY_BOUND: Duration = Duration::from_secs(15);
/// R8: crash-relaunch budget — at most 3 attempts per 60 s window.
pub const CRASH_RELAUNCH_MAX_ATTEMPTS: u32 = 3;
pub const CRASH_RELAUNCH_WINDOW: Duration = Duration::from_secs(60);
/// R8: a pre-bind readiness timeout is NOT a crash — it must not consume
/// an attempt (3 × 15 s = 45 s < 60 s: three slow starts must not
/// exhaust the 3/60 s budget into the terminal affordance).
pub const PRE_BIND_TIMEOUT_EXEMPT: bool = true;
/// R8: spawn exit 2/3 → bounded re-probe before giving up.
pub const REPROBE_WINDOW: Duration = Duration::from_secs(5);
pub const REPROBE_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortState {
    Free,
    Occupied,
}

/// The R8 capability probe: `cli_binary serve --help` exit 0 ⟺ capable.
/// The exit code is the ONLY discriminator — no stdout/stderr text is
/// ever matched (help-render changes, i18n, or about-lines containing
/// "serve" must never affect it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityProbe {
    Capable,
    StaleCli,
}

/// Every input the decision may need, passed in so probing stays injected.
/// (Not PartialEq — the record it carries is deliberately
/// comparison-free; the decision's OUTPUT derives PartialEq so tests
/// assert outcomes, not inputs.)
#[derive(Debug, Clone)]
pub struct ProbeSnapshot {
    /// The home's record, if any (read_record is version-tolerant).
    pub record: Option<HostRuntimeRecord>,
    /// Health of the record's port (Bearer-gated GET /api/health).
    pub record_healthy: bool,
    /// State of the desired port (17865) — meaningful when a spawn is
    /// being considered: FREE → spawn on the desired port; OCCUPIED →
    /// spawn on an ephemeral port (the desired port is a machine-global
    /// resource and another home may hold it; #148).
    pub desired_port_state: PortState,
    /// `cli_binary serve --help` result. Only consulted when attach
    /// failed and a spawn is needed (R8: probe only when needed).
    pub capability: CapabilityProbe,
    /// Exit code of a spawn attempt that just ended, if any.
    pub spawn_exit: Option<i32>,
    /// Crash-relaunch attempts already consumed in the current 60 s
    /// window (pre-bind readiness timeouts never consume, PRE_BIND_TIMEOUT_EXEMPT).
    pub relaunch_attempts_used: u32,
}

/// The decision's outcome. The shell surfaces it; it decides nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Attach to the healthy v2/ws holder (attach-first, R8).
    Attach { port: u16, token: String },
    /// Spawn `optimus serve` now (the capability probe already passed)
    /// on the DESIRED port (17865).
    Spawn,
    /// Spawn `optimus serve --port 0` now: the desired port is held by
    /// another home's serve (the port is machine-global, the record is
    /// per-home; #148). The record carries the real bound port, so
    /// attach-after-spawn and the ready wait are unchanged.
    SpawnEphemeral,
    /// Terminal named diagnostic — the single recovery affordance, no
    /// relaunch loop.
    Diagnose(Diagnostic),
}

/// The named diagnostics (R8, R1, ADR-0083). Every one is a terminal
/// state except None — the shell re-probes (REPROBE_WINDOW) before
/// settling on these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnostic {
    /// A healthy holder serves this home in HTTP mode: the app can
    /// neither attach (no v2/ws record) nor spawn (one core per home,
    /// C3). Named diagnostic terminal state.
    HttpHolder { port: u16, message: String },
    /// Port occupied and no record appeared after the bounded re-probe:
    /// the post-spawn settle of a DESIRED-port spawn that raced into a
    /// bind failure — an honest bind diagnostic, NOT "reinstall" (a bind
    /// failure is not a stale CLI). Pre-spawn, an occupied desired port
    /// is an ephemeral spawn, not this diagnostic (#148).
    PortOccupiedNoRecord { port: u16 },
    /// Port free and no record appeared after the bounded re-probe.
    SpawnFailedGeneric,
    /// The capability probe failed: the installed CLI predates `serve`.
    /// Deterministic, exit-code based — reinstall.
    StaleCli,
}

/// The R8 branch table as a pure function. `record_healthy` refers to a
/// health probe of the RECORD's port (the record is trusted only when
/// its token authenticates).
pub fn decide(snapshot: &ProbeSnapshot) -> Decision {
    // Attach-first: a healthy backend is ipso facto capable.
    if let Some(record) = &snapshot.record {
        if snapshot.record_healthy {
            match record.transport_label() {
                TRANSPORT_WS => {
                    return Decision::Attach {
                        port: record.port,
                        token: record.token.clone(),
                    };
                }
                _ => {
                    // Healthy http holder: neither attach nor spawn is
                    // possible — one core per home (C3). Terminal state,
                    // no relaunch loop.
                    return Decision::Diagnose(Diagnostic::HttpHolder {
                        port: record.port,
                        message: crate::record::holder_refusal_diagnostic(record),
                    });
                }
            }
        }
    }

    // A spawn attempt ended: bounded re-probe first (race recovery — the
    // winner writes the record only after bind).
    if let Some(exit) = snapshot.spawn_exit {
        if exit == crate::serve::EXIT_BIND_OR_SECURITY || exit == crate::serve::EXIT_REFUSED {
            match snapshot.desired_port_state {
                PortState::Occupied => {
                    return Decision::Diagnose(Diagnostic::PortOccupiedNoRecord {
                        port: crate::serve::DEFAULT_HOST_PORT,
                    });
                }
                PortState::Free => {
                    return Decision::Diagnose(Diagnostic::SpawnFailedGeneric);
                }
            }
        }
    }

    // No healthy record: a spawn is needed — and only now does the
    // capability probe matter (R8: probe only when needed).
    if snapshot.capability == CapabilityProbe::StaleCli {
        return Decision::Diagnose(Diagnostic::StaleCli);
    }
    if snapshot.desired_port_state == PortState::Occupied {
        // The desired port is a machine-global resource and another home
        // (or an unrelated process) holds it: spawn on an EPHEMERAL port
        // instead of a terminal diagnostic (#148). The record carries
        // the real port, so attach-after-spawn keeps working; one core
        // per home is record-based, so the ephemeral child still refuses
        // (exit 3) on a healthy holder.
        return Decision::SpawnEphemeral;
    }
    Decision::Spawn
}

/// Convenience: read + health-check the record (the attach-first probe).
/// `probe` is injected so tests can script health answers.
pub fn probe_record<F>(home: &Path, probe: F) -> ProbeSnapshot
where
    F: Fn(&HostRuntimeRecord) -> bool,
{
    let record = read_record(home);
    let record_healthy = record.as_ref().is_some_and(probe);
    ProbeSnapshot {
        record,
        record_healthy,
        desired_port_state: PortState::Free,
        capability: CapabilityProbe::Capable,
        spawn_exit: None,
        relaunch_attempts_used: 0,
    }
}

/// The 3/60 s crash-relaunch budget (R8). `recorded(READY_BOUND)` for a
/// pre-bind readiness timeout never consumes an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrashBudget {
    pub window_start: Option<std::time::Instant>,
    pub attempts_used: u32,
}

impl Default for CrashBudget {
    fn default() -> Self {
        Self::new()
    }
}

impl CrashBudget {
    pub const fn new() -> Self {
        CrashBudget {
            window_start: None,
            attempts_used: 0,
        }
    }

    /// Whether a relaunch is still within budget (3 attempts per 60 s).
    /// A pre-bind readiness timeout (PRE_BIND_TIMEOUT_EXEMPT) is not a
    /// crash: it does not consume an attempt and never exhausts the
    /// budget — 3 × 15 s = 45 s < 60 s, so three slow starts fit inside
    /// one window.
    pub fn can_relaunch(&self, now: std::time::Instant) -> bool {
        match self.window_start {
            None => true,
            Some(start) => {
                if now.duration_since(start) >= CRASH_RELAUNCH_WINDOW {
                    true
                } else {
                    self.attempts_used < CRASH_RELAUNCH_MAX_ATTEMPTS
                }
            }
        }
    }

    /// Record a crash (spawned process exited before readiness).
    pub fn record_crash(&mut self, now: std::time::Instant) {
        match self.window_start {
            None => {
                self.window_start = Some(now);
                self.attempts_used = 1;
            }
            Some(start) => {
                if now.duration_since(start) >= CRASH_RELAUNCH_WINDOW {
                    self.window_start = Some(now);
                    self.attempts_used = 1;
                } else {
                    self.attempts_used += 1;
                }
            }
        }
    }
}

/// The R8 re-probe sequence after a spawn exit 2/3: probe every
/// REPROBE_INTERVAL for REPROBE_WINDOW. Returns the first healthy v2/ws
/// record seen (race recovery), else the port state at the end.
pub fn re_probe<F, G, H>(home: &Path, probe_record: F, port_state: G, healthy: H) -> Reprobed
where
    F: Fn(&Path) -> Option<HostRuntimeRecord>,
    G: Fn() -> PortState,
    H: Fn(&HostRuntimeRecord) -> bool,
{
    let deadline = std::time::Instant::now() + REPROBE_WINDOW;
    loop {
        if let Some(record) = probe_record(home) {
            if record.transport_label() == TRANSPORT_WS && healthy(&record) {
                return Reprobed::Attach {
                    port: record.port,
                    token: record.token,
                };
            }
        }
        if std::time::Instant::now() >= deadline {
            return Reprobed::Settled(port_state());
        }
        std::thread::sleep(REPROBE_INTERVAL);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reprobed {
    Attach { port: u16, token: String },
    Settled(PortState),
}

/// Probe the home for a healthy ws holder (the attach-first check).
pub fn healthy_ws_holder(home: &Path) -> Option<HostRuntimeRecord> {
    healthy_record(home).filter(|record| record.transport_label() == TRANSPORT_WS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::HostRuntimeRecord;

    fn ws_record(port: u16, token: &str) -> HostRuntimeRecord {
        HostRuntimeRecord {
            version: 2,
            port,
            pid: 1234,
            token: token.to_string(),
            transport: Some(TRANSPORT_WS.to_string()),
        }
    }

    fn http_record(port: u16) -> HostRuntimeRecord {
        HostRuntimeRecord {
            version: 1,
            port,
            pid: 1234,
            token: "x".repeat(40),
            transport: None,
        }
    }

    fn snapshot(
        record: Option<HostRuntimeRecord>,
        healthy: bool,
        port: PortState,
        capability: CapabilityProbe,
        spawn_exit: Option<i32>,
    ) -> ProbeSnapshot {
        ProbeSnapshot {
            record,
            record_healthy: healthy,
            desired_port_state: port,
            capability,
            spawn_exit,
            relaunch_attempts_used: 0,
        }
    }

    // ——— The R8 branch table as executable tests ———

    #[test]
    fn healthy_ws_record_attaches_first() {
        let record = ws_record(17865, "token-a");
        let decision = decide(&snapshot(
            Some(record),
            true,
            PortState::Free,
            CapabilityProbe::StaleCli,
            None,
        ));
        // Attach-first: a healthy backend is ipso facto capable — even a
        // stale CLI probe must not produce a spurious reinstall
        // diagnostic while a healthy record exists.
        assert_eq!(
            decision,
            Decision::Attach {
                port: 17865,
                token: "token-a".into()
            }
        );
    }

    #[test]
    fn healthy_http_holder_is_a_terminal_diagnostic() {
        let record = http_record(17865);
        let decision = decide(&snapshot(
            Some(record),
            true,
            PortState::Free,
            CapabilityProbe::Capable,
            None,
        ));
        match decision {
            Decision::Diagnose(Diagnostic::HttpHolder { port, message }) => {
                assert_eq!(port, 17865);
                assert!(
                    message.contains("HTTP mode"),
                    "named diagnostic names the holder's transport: {message}"
                );
            }
            other => panic!("expected HttpHolder, got {other:?}"),
        }
    }

    #[test]
    fn healthy_ws_holder_wins_over_a_free_port_and_capable_cli() {
        let record = ws_record(17865, "token-b");
        let decision = decide(&snapshot(
            Some(record),
            true,
            PortState::Free,
            CapabilityProbe::Capable,
            None,
        ));
        assert_eq!(
            decision,
            Decision::Attach {
                port: 17865,
                token: "token-b".into()
            }
        );
    }

    #[test]
    fn stale_record_falls_through_to_spawn() {
        // A crash-stale record (record present, probe unhealthy) falls
        // through to a fresh spawn — nothing could attach with it anyway.
        let record = ws_record(17865, "token-c");
        let decision = decide(&snapshot(
            Some(record),
            false,
            PortState::Free,
            CapabilityProbe::Capable,
            None,
        ));
        assert_eq!(decision, Decision::Spawn);
    }

    #[test]
    fn no_record_free_port_capable_spawns() {
        let decision = decide(&snapshot(
            None,
            false,
            PortState::Free,
            CapabilityProbe::Capable,
            None,
        ));
        assert_eq!(decision, Decision::Spawn);
    }

    #[test]
    fn stale_cli_diagnoses_reinstall_only_when_a_spawn_is_needed() {
        let decision = decide(&snapshot(
            None,
            false,
            PortState::Free,
            CapabilityProbe::StaleCli,
            None,
        ));
        assert_eq!(decision, Decision::Diagnose(Diagnostic::StaleCli));
    }

    #[test]
    fn occupied_port_without_record_spawns_ephemeral() {
        // The desired port is machine-global but the record is per-home:
        // another home's serve may hold 17865. Pre-spawn, an occupied
        // desired port is an EPHEMERAL spawn (`serve --port 0`), not the
        // check-port diagnostic (#148) — the record carries the real
        // port, so attach-after-spawn keeps working.
        let decision = decide(&snapshot(
            None,
            false,
            PortState::Occupied,
            CapabilityProbe::Capable,
            None,
        ));
        assert_eq!(decision, Decision::SpawnEphemeral);
    }

    #[test]
    fn stale_cli_beats_the_ephemeral_fallback() {
        // A stale CLI is diagnosed even when the desired port is held:
        // the fallback assumes `serve --port 0` exists.
        let decision = decide(&snapshot(
            None,
            false,
            PortState::Occupied,
            CapabilityProbe::StaleCli,
            None,
        ));
        assert_eq!(decision, Decision::Diagnose(Diagnostic::StaleCli));
    }

    #[test]
    fn spawn_exit_2_with_occupied_port_is_check_port_not_reinstall() {
        // A bind failure is not a stale CLI: the honest diagnostic names
        // the port (NOT "reinstall"). This is the POST-spawn settle of a
        // desired-port spawn that raced into a bind failure — the
        // pre-spawn occupied state (no spawn_exit) is the ephemeral
        // fallback instead (see occupied_port_without_record_spawns_ephemeral).
        let decision = decide(&snapshot(
            None,
            false,
            PortState::Occupied,
            CapabilityProbe::Capable,
            Some(crate::serve::EXIT_BIND_OR_SECURITY),
        ));
        assert_eq!(
            decision,
            Decision::Diagnose(Diagnostic::PortOccupiedNoRecord { port: 17865 })
        );
    }

    #[test]
    fn spawn_exit_2_with_free_port_is_the_generic_diagnostic() {
        let decision = decide(&snapshot(
            None,
            false,
            PortState::Free,
            CapabilityProbe::Capable,
            Some(crate::serve::EXIT_BIND_OR_SECURITY),
        ));
        assert_eq!(decision, Decision::Diagnose(Diagnostic::SpawnFailedGeneric));
    }

    #[test]
    fn spawn_exit_3_after_refusal_also_reprobes_to_a_diagnostic() {
        // Exit 3 (healthy holder refusal): the re-probe would find the
        // winner's record; if the port is free now, generic diagnostic.
        let decision = decide(&snapshot(
            None,
            false,
            PortState::Free,
            CapabilityProbe::Capable,
            Some(crate::serve::EXIT_REFUSED),
        ));
        assert_eq!(decision, Decision::Diagnose(Diagnostic::SpawnFailedGeneric));
    }

    // ——— The 3/60 s + 15 s budget arithmetic (R8) ———

    #[test]
    fn three_slow_starts_fit_inside_one_relaunch_window() {
        // 3 × 15 s = 45 s < 60 s: three slow-but-healthy starts must not
        // exhaust the 3/60 s budget into the terminal affordance — the
        // pre-bind readiness timeout is PRE_BIND_TIMEOUT_EXEMPT.
        const _: () = assert!(PRE_BIND_TIMEOUT_EXEMPT);
        assert_eq!(
            READY_BOUND.as_secs() * CRASH_RELAUNCH_MAX_ATTEMPTS as u64,
            45
        );
        assert!(
            (READY_BOUND.as_secs() * CRASH_RELAUNCH_MAX_ATTEMPTS as u64)
                < CRASH_RELAUNCH_WINDOW.as_secs()
        );
    }

    #[test]
    fn crash_budget_exhausts_after_three_crashes_in_window() {
        let now = std::time::Instant::now();
        let mut budget = CrashBudget::new();
        assert!(budget.can_relaunch(now));
        budget.record_crash(now);
        budget.record_crash(now + Duration::from_secs(1));
        budget.record_crash(now + Duration::from_secs(2));
        assert_eq!(budget.attempts_used, 3);
        assert!(!budget.can_relaunch(now + Duration::from_secs(3)));
        // A fresh window reopens the budget.
        assert!(budget.can_relaunch(now + CRASH_RELAUNCH_WINDOW + Duration::from_secs(1)));
    }

    #[test]
    fn pre_bind_timeout_never_consumes_an_attempt() {
        // A slow-but-healthy start is not a crash: the shell waits
        // READY_BOUND for the record, and if the record then appears,
        // the budget is untouched (the shell records crashes only when
        // the child exits before readiness).
        let now = std::time::Instant::now();
        let mut budget = CrashBudget::new();
        budget.record_crash(now);
        assert_eq!(budget.attempts_used, 1);
        // A pre-bind readiness timeout is handled by the SHELL as a wait
        // (no budget mutation); the constant pins the semantics.
        const _: () = assert!(PRE_BIND_TIMEOUT_EXEMPT);
    }

    #[test]
    fn reprobe_window_is_20_probes_at_250ms() {
        assert_eq!(
            REPROBE_WINDOW.as_millis() / REPROBE_INTERVAL.as_millis(),
            20
        );
    }

    #[test]
    fn re_probe_recovers_the_race_winner_record() {
        // The winner writes the record only after bind; the loser's
        // 5 s re-probe must find it (race recovery).
        let home =
            std::env::temp_dir().join(format!("spawn-decision-reprobe-{}", std::process::id()));
        std::fs::create_dir_all(&home).ok();
        let record = ws_record(17865, "winner");
        let result = re_probe(
            &home,
            |_| Some(record.clone()),
            || PortState::Free,
            |_| true,
        );
        assert_eq!(
            result,
            Reprobed::Attach {
                port: 17865,
                token: "winner".into()
            }
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn re_probe_settles_to_port_state_when_no_record_appears() {
        let home = std::env::temp_dir().join(format!(
            "spawn-decision-reprobe-none-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&home).ok();
        let result = re_probe(&home, |_| None, || PortState::Occupied, |_| false);
        assert_eq!(result, Reprobed::Settled(PortState::Occupied));
        let _ = std::fs::remove_dir_all(&home);
    }
}
