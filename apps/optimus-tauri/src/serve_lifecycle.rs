//! The shell's serve lifecycle (spec-015 A4/R8) — the THIN surfacing of
//! the attach-or-spawn-or-diagnose decision.
//!
//! `spawn_decision` (optimus-host) owns the branch matrix and the
//! 3/60 s + 15 s budget arithmetic; this module only performs the
//! lifecycle around it: attach-first → spawn (capability probe ONLY when
//! a spawn is needed) → ready wait (15 s from spawn, epoch pinned
//! spawn→record-visible-or-diagnostic) → quit termination → bounded
//! crash relaunch (3/60 s; pre-bind readiness timeouts never consume an
//! attempt) with the single recovery affordance. The broker command
//! answers the renderer's WS attach from the live record.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use optimus_host::spawn_decision::{
    decide, re_probe, CapabilityProbe, CrashBudget, Decision, Diagnostic, PortState, ProbeSnapshot,
    Reprobed, READY_BOUND,
};
use optimus_host::{process_secret, read_record, record_is_healthy, EXIT_REFUSED, TRANSPORT_WS};

/// The shell's surfacing of the lifecycle outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServeOutcome {
    /// Attached to a healthy ws holder (attach-first).
    Attached { port: u16, token: String },
    /// Spawned `optimus serve` and reached readiness within READY_BOUND.
    Spawned { port: u16, token: String },
    /// A named diagnostic (terminal): "a host is already serving this
    /// home in HTTP mode" (healthy http holder), "serve failed to start:
    /// check port 17865", "serve failed to start", or the reinstall
    /// diagnostic (stale CLI). The single recovery affordance.
    Diagnostic(String),
}

impl ServeOutcome {
    pub fn as_attach(&self) -> Option<(u16, String)> {
        match self {
            ServeOutcome::Attached { port, token } | ServeOutcome::Spawned { port, token } => {
                Some((*port, token.clone()))
            }
            ServeOutcome::Diagnostic(_) => None,
        }
    }
}

/// cli_binary discovery (R8): `cli_binary` from install-meta.json, then
/// PATH. install-meta.json lives at $OPTIMUS_INSTALL_ROOT or
/// <data-home>/optimus-agent/install-meta.json and carries
/// `cli_binary`/`tauri_binary`/`desktop_binary` as its only binary-path
/// fields.
fn discover_cli() -> Option<PathBuf> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".local/share"))
        })
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let root = std::env::var_os("OPTIMUS_INSTALL_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_home.join("optimus-agent"));
    let meta_path = root.join("install-meta.json");
    if let Ok(raw) = std::fs::read_to_string(&meta_path) {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(binary) = meta.get("cli_binary").and_then(|v| v.as_str()) {
                let candidate = PathBuf::from(binary);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    // PATH fallback: the installed `optimus` (or a dev build on PATH).
    for dir in std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default()
    {
        let candidate = dir.join("optimus");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// The R8 capability probe: `cli_binary serve --help` exit 0 ⟺ capable.
/// The exit code is the ONLY discriminator — no stdout/stderr text is
/// ever parsed.
fn capability_probe(cli: &PathBuf) -> CapabilityProbe {
    let status = Command::new(cli)
        .arg("serve")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(status) if status.success() => CapabilityProbe::Capable,
        _ => CapabilityProbe::StaleCli,
    }
}

/// Is the desired port occupied? (A bind check, not a health probe.)
fn port_state(port: u16) -> PortState {
    match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            drop(listener);
            PortState::Free
        }
        Err(_) => PortState::Occupied,
    }
}

struct ServeLifecycleInner {
    home: PathBuf,
    child: Mutex<Option<Child>>,
    outcome: Mutex<ServeOutcome>,
    budget: Mutex<CrashBudget>,
    /// The shell-minted process secret (ADR-0084): delivered to the child
    /// via env; the shell presents it on its own shell-kind connection.
    secret: String,
    port: u16,
}

/// The lifecycle handle shared with the tauri commands.
#[derive(Clone)]
pub struct ServeLifecycle(Arc<ServeLifecycleInner>);

impl ServeLifecycle {
    /// Attach-first lifecycle: decide from probe results, spawn only when
    /// attach fails, wait READY_BOUND for readiness, then keep a watcher
    /// that relaunches within the 3/60 s crash budget.
    pub fn start(home: PathBuf, port: u16) -> Self {
        let inner = Arc::new(ServeLifecycleInner {
            home: home.clone(),
            child: Mutex::new(None),
            outcome: Mutex::new(ServeOutcome::Diagnostic("serve lifecycle starting".into())),
            budget: Mutex::new(CrashBudget::new()),
            secret: process_secret().unwrap_or_default(),
            port,
        });
        let lifecycle = ServeLifecycle(inner);
        lifecycle.ensure_serve();
        lifecycle.watch();
        lifecycle
    }

    fn ensure_serve(&self) {
        let inner = &self.0;
        // Attach-first: read the record + health-check it. A healthy
        // backend is ipso facto capable — the capability probe runs ONLY
        // when a spawn is needed.
        let record = read_record(&inner.home);
        let record_healthy = record.as_ref().is_some_and(record_is_healthy);
        let snapshot = ProbeSnapshot {
            record,
            record_healthy,
            desired_port_state: PortState::Free,
            capability: CapabilityProbe::Capable,
            spawn_exit: None,
            relaunch_attempts_used: 0,
        };
        match decide(&snapshot) {
            Decision::Attach { port, token } => {
                *inner.outcome.lock().unwrap() = ServeOutcome::Attached { port, token };
            }
            Decision::Diagnose(diagnostic) => {
                *inner.outcome.lock().unwrap() = ServeOutcome::Diagnostic(named(diagnostic));
            }
            Decision::Spawn => self.spawn_and_wait(),
        }
    }

    fn spawn_and_wait(&self) {
        let inner = &self.0;
        let cli = match discover_cli() {
            Some(cli) => cli,
            None => {
                *inner.outcome.lock().unwrap() = ServeOutcome::Diagnostic(
                    "serve failed to start: the Optimus CLI is not installed".into(),
                );
                return;
            }
        };
        // The R8 capability probe runs only here — a spawn is needed.
        if capability_probe(&cli) == CapabilityProbe::StaleCli {
            *inner.outcome.lock().unwrap() = ServeOutcome::Diagnostic(
                "the installed Optimus CLI does not support `optimus serve` — reinstall".into(),
            );
            return;
        }

        // Spawn with the process secret delivered via env (ADR-0084).
        // The dial ticket is serve's own: it mints one and writes it to
        // the record (the attach contract, R7).
        let child = Command::new(&cli)
            .arg("serve")
            .arg("--home")
            .arg(&inner.home)
            .arg("--port")
            .arg(inner.port.to_string())
            .env(optimus_host::PROCESS_SECRET_ENV, &inner.secret)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                *inner.outcome.lock().unwrap() =
                    ServeOutcome::Diagnostic(format!("serve failed to start: {error}"));
                return;
            }
        };
        let child_id = child.id();

        // Ready wait (R8): epoch pinned from process spawn to
        // record-visible-or-diagnostic. The record appears only after
        // bind; the health check gates the token.
        let epoch = Instant::now();
        while epoch.elapsed() < READY_BOUND {
            // A crashed child before readiness is a crash: consume the
            // budget and relaunch if still within it.
            if let Ok(Some(status)) = child.try_wait() {
                let code = status.code();
                self.on_child_exit(code);
                return;
            }
            if let Some(record) = read_record(&inner.home) {
                if record.transport_label() == TRANSPORT_WS && record_is_healthy(&record) {
                    // Ready: the watcher owns the child from here.
                    *inner.child.lock().unwrap() = Some(child);
                    *inner.outcome.lock().unwrap() = ServeOutcome::Spawned {
                        port: record.port,
                        token: record.token,
                    };
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child_id;

        // Pre-bind readiness timeout: bounded, and it must NOT consume a
        // crash-relaunch attempt (a slow-but-healthy start is not a
        // crash; 3 × 15 s = 45 s < 60 s). The child may still be
        // starting; the recovery affordance re-runs the lifecycle.
        eprintln!(
            "[optimus-tauri] serve did not become ready within {}s",
            READY_BOUND.as_secs()
        );
        *inner.outcome.lock().unwrap() = ServeOutcome::Diagnostic(
            "serve failed to start (no record within the readiness bound)".into(),
        );
    }

    /// A spawned child exited. Exit 2/3: bounded re-probe (5 s, 250 ms)
    /// → attach to a race winner, else the honest diagnostic. Any other
    /// exit before readiness: crash — consume the budget, relaunch within
    /// 3/60 s.
    fn on_child_exit(&self, code: Option<i32>) {
        let inner = &self.0;
        *inner.child.lock().unwrap() = None;
        match code {
            Some(code) if code == optimus_host::EXIT_BIND_OR_SECURITY || code == EXIT_REFUSED => {
                // Race recovery: the winner writes the record only after
                // bind. Re-probe for REPROBE_WINDOW, then settle.
                let reprobed = re_probe(
                    &inner.home,
                    read_record,
                    || port_state(inner.port),
                    record_is_healthy,
                );
                match reprobed {
                    Reprobed::Attach { port, token } => {
                        *inner.outcome.lock().unwrap() = ServeOutcome::Attached { port, token };
                    }
                    Reprobed::Settled(PortState::Occupied) => {
                        *inner.outcome.lock().unwrap() = ServeOutcome::Diagnostic(format!(
                            "serve failed to start: check port {}",
                            inner.port
                        ));
                    }
                    Reprobed::Settled(PortState::Free) => {
                        *inner.outcome.lock().unwrap() =
                            ServeOutcome::Diagnostic("serve failed to start".into());
                    }
                }
            }
            _ => {
                // Crash before readiness: bounded relaunch.
                let mut budget = inner.budget.lock().unwrap();
                if budget.can_relaunch(Instant::now()) {
                    budget.record_crash(Instant::now());
                    drop(budget);
                    eprintln!(
                        "[optimus-tauri] serve crashed; relaunching within the 3/60 s budget"
                    );
                    self.spawn_and_wait();
                } else {
                    *inner.outcome.lock().unwrap() = ServeOutcome::Diagnostic(
                        "serve keeps failing to start — check the Optimus install".into(),
                    );
                }
            }
        }
    }

    /// Watch the child; a mid-session crash relaunches within the budget.
    fn watch(&self) {
        let inner = Arc::clone(&self.0);
        let lifecycle = self.clone();
        std::thread::spawn(move || {
            loop {
                let child = inner.child.lock().unwrap().take();
                let Some(mut child) = child else {
                    // Terminal diagnostic or attached: nothing to watch.
                    if matches!(*inner.outcome.lock().unwrap(), ServeOutcome::Diagnostic(_)) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(250));
                    continue;
                };
                let _ = child.wait();
                // Re-decide attach-first: a fresh serve may now hold the
                // home; only when attach fails do we relaunch our own.
                let record = read_record(&inner.home);
                let record_healthy = record.as_ref().is_some_and(record_is_healthy);
                let decision = decide(&ProbeSnapshot {
                    record,
                    record_healthy,
                    desired_port_state: port_state(inner.port),
                    capability: CapabilityProbe::Capable,
                    spawn_exit: None,
                    relaunch_attempts_used: 0,
                });
                match decision {
                    Decision::Attach { port, token } => {
                        *inner.outcome.lock().unwrap() = ServeOutcome::Attached { port, token };
                    }
                    Decision::Diagnose(diagnostic) => {
                        *inner.outcome.lock().unwrap() =
                            ServeOutcome::Diagnostic(named(diagnostic));
                        return;
                    }
                    Decision::Spawn => {
                        let mut budget = inner.budget.lock().unwrap();
                        if budget.can_relaunch(Instant::now()) {
                            budget.record_crash(Instant::now());
                            drop(budget);
                            lifecycle.spawn_and_wait();
                        } else {
                            *inner.outcome.lock().unwrap() = ServeOutcome::Diagnostic(
                                "serve keeps failing to start — check the Optimus install".into(),
                            );
                            return;
                        }
                    }
                }
            }
        });
    }

    /// The broker answer (A3): the healthy v2/ws record — the renderer's
    /// WS attach ticket — or None (confirmed absence → terminal
    /// affordance).
    pub fn broker_ticket(&self) -> Option<(u16, String)> {
        let outcome = self.0.outcome.lock().unwrap();
        outcome.as_attach()
    }

    /// Quit termination: kill the spawned serve on shell exit.
    pub fn terminate(&self) {
        if let Some(mut child) = self.0.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn named(diagnostic: Diagnostic) -> String {
    match diagnostic {
        Diagnostic::HttpHolder { message, .. } => message,
        Diagnostic::PortOccupiedNoRecord { port } => {
            format!("serve failed to start: check port {port}")
        }
        Diagnostic::SpawnFailedGeneric => "serve failed to start".into(),
        Diagnostic::StaleCli => {
            "the installed Optimus CLI does not support `optimus serve` — reinstall".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_ticket_answers_nothing_before_the_lifecycle_runs() {
        // The lifecycle is start()ed in the shell; a fresh handle has no
        // outcome yet — the broker must answer None, never invent a
        // ticket.
        let home =
            std::env::temp_dir().join(format!("tauri-lifecycle-none-{}", std::process::id()));
        std::fs::create_dir_all(&home).ok();
        let lifecycle = ServeLifecycle::start(home.clone(), 0);
        // 0 is an invalid serve port; attach-first finds no record and
        // the spawn cannot run — the outcome is a diagnostic.
        let outcome = lifecycle.0.outcome.lock().unwrap().clone();
        assert!(matches!(outcome, ServeOutcome::Diagnostic(_)));
        lifecycle.terminate();
        let _ = std::fs::remove_dir_all(&home);
    }
}
