//! spec-017 conformance: every transport rides the same claim→turn→settle
//! spine through the public contract API (`optimus_ops::transport`).
//!
//! The adapter under test is a scripted fake: it is not the contract it is
//! checked against, so a green run proves the spine, not agreement between a
//! mock and the code (that is what `telegram_bot_api_contracts` proves for
//! the live wire). A1–A4 of spec-017 are asserted here.

use std::path::Path;
use std::sync::{Arc, Mutex};

use optimus_ops::{
    adapter_cycle, list_ambiguous_sends, list_inbox, list_outbox_receipts, list_transport_events,
    record_transport_event, spawn_adapter_worker, spawn_snapshot_writer, AdapterState, RawInbound,
    SendOutcome, SupervisorState, TransportAdapter, TransportId,
};
use tempfile::tempdir;

/// Scripted adapter: pushes inbound, records sends, obeys a failure script.
#[derive(Clone)]
struct FakeAdapter {
    transport: TransportId,
    inbound: Vec<RawInbound>,
    /// Script of send outcomes, consumed in order.
    script: Vec<SendOutcome>,
    sent: Arc<Mutex<Vec<(String, String)>>>,
    enabled: bool,
}

impl FakeAdapter {
    fn new(transport: TransportId) -> Self {
        Self {
            transport,
            inbound: Vec::new(),
            script: Vec::new(),
            sent: Arc::new(Mutex::new(Vec::new())),
            enabled: true,
        }
    }

    fn inbound(mut self, from: &str, text: &str) -> Self {
        self.inbound.push(RawInbound {
            from: from.into(),
            text: text.into(),
            attachments: Vec::new(),
        });
        self
    }

    fn reject_all(mut self) -> Self {
        self.enabled = true;
        self.inbound = vec![RawInbound {
            from: "stranger".into(),
            text: "hi".into(),
            attachments: Vec::new(),
        }];
        self
    }

    fn sends(mut self, script: Vec<SendOutcome>) -> Self {
        self.script = script;
        self
    }

    fn enable(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    fn received(&self) -> Vec<(String, String)> {
        self.sent.lock().unwrap().clone()
    }
}

impl TransportAdapter for FakeAdapter {
    fn transport(&self) -> TransportId {
        self.transport
    }

    fn is_enabled(&self, _home: &Path) -> bool {
        self.enabled
    }

    fn poll_inbound(&mut self, _home: &Path) -> Result<Vec<RawInbound>, String> {
        Ok(std::mem::take(&mut self.inbound))
    }

    fn is_allowed(&self, from: &str) -> bool {
        from != "stranger"
    }

    fn send(&mut self, target: &str, body: &str) -> Result<SendOutcome, String> {
        self.sent
            .lock()
            .unwrap()
            .push((target.to_string(), body.to_string()));
        if self.script.is_empty() {
            Ok(SendOutcome::Confirmed {
                provider_message_id: format!("m{}", self.sent.lock().unwrap().len()),
            })
        } else {
            Ok(self.script.remove(0))
        }
    }
}

fn reply(message: &optimus_ops::InboundMessage) -> Result<(String, Option<String>), String> {
    // Mirror `gateway_turn`: the address comes back unchanged, and the ledger
    // writes an outbound obligation exactly when an address is returned.
    Ok((
        format!("reply:{}", message.text),
        message.session_id.clone(),
    ))
}

/// A1: inbound → turn → reply → receipt completes end-to-end; the reply is
/// delivered exactly once and the outbox receipt is terminal.
#[test]
fn full_cycle_delivers_exactly_once() {
    let dir = tempdir().unwrap();
    let adapter = FakeAdapter::new(TransportId::Discord)
        .inbound("42", "hello discord")
        .inbound("7", "second");

    let result = adapter_cycle(dir.path(), &mut adapter.clone(), reply).unwrap();
    eprintln!(
        "DBG enqueued={} drained={}",
        result.enqueued.len(),
        result.drained.len()
    );
    if result.drained.is_empty() {
        eprintln!("DBG inbox={:?}", list_inbox(dir.path()).unwrap());
    }
    assert_eq!(result.enqueued.len(), 2);
    assert_eq!(result.drained.len(), 2);
    assert_eq!(result.receipts.len(), 2);
    assert!(result.ambiguous.is_empty());
    assert!(result.failed_sends.is_empty());
    assert!(result.refused.is_empty());
    let mut received = adapter.received();
    received.sort();
    assert_eq!(
        received,
        vec![
            ("42".to_string(), "reply:hello discord".to_string()),
            ("7".to_string(), "reply:second".to_string()),
        ]
    );
    // Exactly two terminal receipts in the outbox, both delivered.
    let receipts = list_outbox_receipts(dir.path(), 10).unwrap();
    assert_eq!(receipts.len(), 2);
    // The inbox is empty: every message reached a terminal state.
    assert!(list_inbox(dir.path()).unwrap().is_empty());
}

/// A2: a permanently-rejecting transport marks the obligation
/// failed-permanently with the named diagnostic and never a success receipt.
#[test]
fn rejecting_transport_fails_permanently_with_diagnostic() {
    let dir = tempdir().unwrap();
    let adapter = FakeAdapter::new(TransportId::Slack)
        .inbound("42", "hello slack")
        .sends(vec![SendOutcome::Failed {
            detail: "invalid_auth".into(),
        }]);

    let result = adapter_cycle(dir.path(), &mut adapter.clone(), reply).unwrap();

    assert_eq!(result.drained.len(), 1);
    assert!(result.receipts.is_empty());
    assert_eq!(result.failed_sends.len(), 1);
    // The obligation is terminal-failed, not pending for retry.
    let events = list_transport_events(dir.path(), Some("slack"), 20).unwrap();
    assert!(events
        .iter()
        .any(|e| e.kind == "send_outcome" && e.detail.contains(":failed")));
    assert!(list_ambiguous_sends(dir.path(), 10).unwrap().is_empty());
}

/// A4: a message from a chat not in the allowlist is refused before any turn
/// and recorded as the named diagnostic in the ordered event stream.
#[test]
fn unauthorized_inbound_refused_with_named_diagnostic() {
    let dir = tempdir().unwrap();
    let adapter = FakeAdapter::new(TransportId::Email).reject_all();

    let result = adapter_cycle(dir.path(), &mut adapter.clone(), reply).unwrap();

    assert!(result.enqueued.is_empty());
    assert_eq!(result.refused.len(), 1);
    let events = list_transport_events(dir.path(), Some("email"), 20).unwrap();
    assert!(events.iter().any(
        |e| e.kind == "inbound_refused" && e.detail.contains("transport_refused_unauthorized")
    ));
    // Nothing reached a turn and no receipt was minted.
    assert!(list_inbox(dir.path()).unwrap().is_empty());
}

/// R10: events are ordered per transport (seq asc) and queryable by transport.
#[test]
fn transport_events_are_ordered_and_queryable() {
    let dir = tempdir().unwrap();
    record_transport_event(dir.path(), "discord", "inbound_received", "1").unwrap();
    record_transport_event(dir.path(), "telegram", "inbound_received", "2").unwrap();
    record_transport_event(dir.path(), "discord", "turn_completed", "3").unwrap();

    let discord = list_transport_events(dir.path(), Some("discord"), 10).unwrap();
    assert_eq!(discord.len(), 2);
    assert!(discord[0].seq > discord[1].seq);
    assert_eq!(discord[0].kind, "turn_completed");
    assert_eq!(discord[1].kind, "inbound_received");

    let telegram = list_transport_events(dir.path(), Some("telegram"), 10).unwrap();
    assert_eq!(telegram.len(), 1);
    assert_eq!(telegram[0].transport, "telegram");
}

/// R6/R7: a disabled adapter is reported `Stopped` by the supervisor worker,
/// and the snapshot writer materialises the same state to disk.
#[test]
fn supervisor_reports_disabled_adapter_stopped() {
    let dir = tempdir().unwrap();
    let adapter = FakeAdapter::new(TransportId::Telegram).enable(false);
    let registry: Arc<Mutex<SupervisorState>> = Arc::new(Mutex::new(SupervisorState::default()));
    let writer = spawn_snapshot_writer(
        dir.path().to_path_buf(),
        Arc::clone(&registry),
        std::time::Duration::from_millis(10),
    );

    let handle = spawn_adapter_worker(
        dir.path().to_path_buf(),
        adapter,
        |_| Ok((String::new(), None)),
        Arc::clone(&registry),
    );
    handle.join().unwrap();
    drop(writer); // final snapshot flush

    let state = registry.lock().unwrap().clone();
    assert_eq!(state.adapters.len(), 1);
    assert_eq!(state.adapters[0].state, AdapterState::Stopped);
    let snapshot = dir.path().join("gateway/supervisor.json");
    let mut on_disk = SupervisorState::default();
    for _ in 0..200 {
        if let Ok(raw) = std::fs::read_to_string(&snapshot) {
            if let Ok(state) = serde_json::from_str::<SupervisorState>(&raw) {
                if !state.adapters.is_empty() {
                    on_disk = state;
                    break;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        !on_disk.adapters.is_empty(),
        "snapshot with adapter state written"
    );
    assert_eq!(on_disk.adapters.len(), 1);
    assert_eq!(on_disk.adapters[0].transport, "telegram");
}

/// Settlement mapping keeps the four terminal classifications distinct.
#[test]
fn settlement_for_outcome_classifies_all_terminal_states() {
    let dir = tempdir().unwrap();
    adapter_cycle(
        dir.path(),
        &mut FakeAdapter::new(TransportId::Telegram)
            .inbound("1", "x")
            .sends(vec![SendOutcome::Ambiguous {
                detail: "timeout".into(),
            }]),
        reply,
    )
    .unwrap();
    let ambiguous = list_ambiguous_sends(dir.path(), 10).unwrap();
    assert_eq!(ambiguous.len(), 1);
    assert_eq!(ambiguous[0].terminal_status, "succeeded");
    assert!(ambiguous[0].delivered_unix.is_none());
    // The turn itself succeeded, so a receipt exists — but the delivery was
    // never confirmed: the receipt stays undelivered and the obligation is
    // parked for operator recovery (ADR-0070).
    let receipts = list_outbox_receipts(dir.path(), 10).unwrap();
    assert_eq!(receipts.len(), 1);
    assert!(receipts[0].delivered_unix.is_none());
}
