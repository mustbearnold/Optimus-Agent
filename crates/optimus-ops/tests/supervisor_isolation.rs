//! spec-017 A3: two adapters under one supervisor — one dies, the other
//! keeps serving, the dead one restarts with backoff, no double-dispatch.
//!
//! The supervisor worker contract (`optimus_ops::transport::spawn_adapter_worker`)
//! is exercised with two scripted fakes on ONE home: adapter B panics every
//! cycle, adapter A keeps delivering. Exactly-once delivery is checked
//! through the outbound ledger receipts (ADR-0070): no claim is ever
//! double-dispatched, because claims are exclusive and the cycle settles
//! before releasing.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use optimus_ops::{
    adapter_cycle, list_outbox_receipts, spawn_adapter_worker, AdapterCycleResult, AdapterState,
    InboundMessage, RawInbound, SendOutcome, SupervisorState, TransportAdapter, TransportId,
};

#[derive(Clone)]
struct FakeAdapter {
    transport: TransportId,
    inbound: Vec<RawInbound>,
    sent: Arc<Mutex<Vec<String>>>,
    panic_every_cycle: bool,
}

impl FakeAdapter {
    fn new(transport: TransportId, panic_every_cycle: bool) -> Self {
        Self {
            transport,
            inbound: Vec::new(),
            sent: Arc::new(Mutex::new(Vec::new())),
            panic_every_cycle,
        }
    }
}

impl TransportAdapter for FakeAdapter {
    fn transport(&self) -> TransportId {
        self.transport
    }
    fn is_enabled(&self, _home: &Path) -> bool {
        true
    }
    fn poll_inbound(&mut self, _home: &Path) -> Result<Vec<RawInbound>, String> {
        if self.panic_every_cycle {
            panic!("injected adapter death (A3)");
        }
        Ok(std::mem::take(&mut self.inbound))
    }
    fn is_allowed(&self, _from: &str) -> bool {
        true
    }
    fn send(&mut self, target: &str, body: &str) -> Result<SendOutcome, String> {
        self.sent.lock().unwrap().push(format!("{target}:{body}"));
        Ok(SendOutcome::Confirmed {
            provider_message_id: "a3".into(),
        })
    }
    fn health(&self) -> Result<(), String> {
        Ok(())
    }
}

fn turn(message: &InboundMessage) -> Result<(String, Option<String>), String> {
    Ok((
        format!("reply:{}", message.text),
        message.session_id.clone(),
    ))
}

#[test]
fn supervisor_two_adapters_isolates_failure_and_never_double_dispatches() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_path_buf();

    // Adapter A (slack): one message queued, healthy.
    let mut a = FakeAdapter::new(TransportId::Slack, false);
    a.inbound.push(RawInbound {
        from: "C42".into(),
        text: "hello slack".into(),
        attachments: vec![],
    });
    // Adapter B (discord): dies on every cycle (injected panic).
    let b = FakeAdapter::new(TransportId::Discord, true);

    // A's cycle first: inbound → claim → turn → reply obligation → deliver.
    let result: AdapterCycleResult = adapter_cycle(&home, &mut a, turn).unwrap();
    assert_eq!(result.enqueued.len(), 1, "A enqueued its inbound");
    assert_eq!(result.drained.len(), 1, "A completed its turn");
    assert_eq!(result.receipts.len(), 1, "A's reply delivered exactly once");

    // Both adapters now live under the supervisor. B panics every cycle;
    // the worker must catch the panic, mark B failed, and keep retrying with
    // backoff while A keeps serving.
    let registry: Arc<Mutex<SupervisorState>> = Arc::new(Mutex::new(SupervisorState::default()));
    let _handle_a = spawn_adapter_worker(home.clone(), a.clone(), turn, registry.clone());
    let _handle_b = spawn_adapter_worker(home.clone(), b.clone(), turn, registry.clone());

    // Give B a few panic cycles and A a few healthy cycles.
    std::thread::sleep(Duration::from_millis(500));

    // B is Failed (panicked) — but its worker is still alive in backoff.
    let state = registry.lock().unwrap();
    let b_state = state
        .adapters
        .iter()
        .find(|s| s.transport == "discord")
        .expect("discord slot exists");
    assert_eq!(
        b_state.state,
        AdapterState::Failed,
        "B reports failed, not dead"
    );
    assert!(
        b_state.last_error.is_some(),
        "B's last error is surfaced on the status surface"
    );
    let a_state = state
        .adapters
        .iter()
        .find(|s| s.transport == "slack")
        .expect("slack slot exists");
    assert_eq!(a_state.state, AdapterState::Running, "A keeps serving");
    drop(state);

    // Exactly-once from the ledger's point of view: A's reply was delivered
    // once; nothing double-dispatched despite B's death.
    let receipts = list_outbox_receipts(&home, 10).unwrap();
    assert_eq!(receipts.len(), 1, "one receipt, never two");

    // Workers run forever; the test process exit cleans them up.
}
