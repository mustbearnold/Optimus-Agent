//! spec-017 R7/R8: `optimus gateway run` — the multi-adapter supervisor.
//!
//! One worker thread per configured transport, all riding the shared
//! claim→turn→settle cycle ([`optimus_kernel::adapter_cycle`]). The supervisor
//! itself never touches a transport: it builds adapters, watches status, and
//! restarts dead workers with backoff. State is visible to any process through
//! `{home}/gateway/supervisor.json` (see `optimus gateway status`).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use optimus_kernel::{
    list_outbox_receipts, read_supervisor_snapshot, spawn_adapter_worker, spawn_snapshot_writer,
    AdapterState, AdapterStatus, InboundMessage, SupervisorState, TelegramAdapter,
};

/// How often the supervisor persists its snapshot (cross-process status face).
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(5);

/// Build every adapter configured for `home`. Each module owns its config
/// shape; a transport with no config file contributes nothing (its status is
/// simply absent from the snapshot until configured).
fn build_adapters(home: &std::path::Path) -> Vec<Box<dyn optimus_kernel::TransportAdapter>> {
    let mut adapters: Vec<Box<dyn optimus_kernel::TransportAdapter>> = Vec::new();
    if let Ok(Some(adapter)) = TelegramAdapter::open(home) {
        adapters.push(adapter);
    }
    // Discord, Slack, Email, Signal and WhatsApp adapters plug in here as
    // their transports land (spec-017 A-criteria, ADR-0091).
    adapters
}

/// The turn every adapter runs: the same host route the desktop app and the
/// standalone telegram loop use (ADR-0071). A high-risk effect pauses for the
/// operator here exactly as it does locally.
fn turn(home: PathBuf) -> impl FnMut(&InboundMessage) -> Result<(String, Option<String>), String> {
    move |message| optimus_host::gateway_turn(&home, message)
}

/// `optimus gateway run` — supervise every configured adapter forever.
/// Returns when every spawned worker has exited (e.g. all adapters disabled),
/// so a disabled home doubles as a smoke check.
pub fn run(home: PathBuf) -> Result<(), String> {
    let adapters = build_adapters(&home);
    if adapters.is_empty() {
        eprintln!("[gateway] no adapters configured — nothing to supervise");
        eprintln!(
            "[gateway] configure {}/gateway/*.json (see spec-017)",
            home.display()
        );
        return Ok(());
    }

    let registry = Arc::new(Mutex::new(SupervisorState::default()));
    for adapter in &adapters {
        let mut state = registry.lock().map_err(|e| e.to_string())?;
        state.adapters.push(AdapterStatus {
            transport: adapter.transport().as_str().to_string(),
            state: AdapterState::Stopped,
            last_error: Some("starting".into()),
            started_unix: None,
            uptime_secs: 0,
        });
    }

    let mut handles = Vec::new();
    for adapter in adapters {
        let home = home.clone();
        let registry = registry.clone();
        handles.push(spawn_adapter_worker(
            home.clone(),
            adapter,
            turn(home),
            registry,
        ));
    }
    let writer = spawn_snapshot_writer(home.clone(), registry.clone(), SNAPSHOT_INTERVAL);
    // Workers run until SIGINT terminates the process; the snapshot writer
    // keeps the status surface fresh meanwhile.
    for handle in handles {
        let _ = handle.join();
    }
    drop(writer);
    Ok(())
}

/// `optimus gateway status` — per-adapter state from the persisted snapshot.
pub fn status(home: &std::path::Path) {
    let state = read_supervisor_snapshot(home);
    if state.adapters.is_empty() {
        println!(
            "[gateway] no supervisor snapshot for {} (is `optimus gateway run` active?)",
            home.display()
        );
        return;
    }
    for adapter in &state.adapters {
        match (&adapter.state, &adapter.last_error) {
            (AdapterState::Running, _) => {
                println!(
                    "[gateway] {} running ({}s)",
                    adapter.transport, adapter.uptime_secs
                );
            }
            (AdapterState::Stopped, Some(detail)) => {
                println!("[gateway] {} stopped: {detail}", adapter.transport);
            }
            (AdapterState::Stopped, None) => {
                println!("[gateway] {} stopped", adapter.transport);
            }
            (AdapterState::Failed, Some(detail)) => {
                println!("[gateway] {} failed: {detail}", adapter.transport);
            }
            (AdapterState::Failed, None) => {
                println!("[gateway] {} failed", adapter.transport);
            }
        }
    }
    // Recent outbox receipt flags for operator visibility.
    for row in list_outbox_receipts(home, 5).unwrap_or_default() {
        let receipt = row
            .delivered_unix
            .map(|t| format!("delivered={t}"))
            .unwrap_or_else(|| {
                if row.ambiguous_send {
                    "AMBIGUOUS".into()
                } else {
                    "no-receipt".into()
                }
            });
        println!("  {}  {}  {}", row.message_id, row.outbound.status, receipt);
    }
}
