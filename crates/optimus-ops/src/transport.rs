//! spec-017 R1: the transport-adapter contract and the generic adapter loop.
//!
//! One implementation path for every messaging transport. An adapter is a
//! `TransportAdapter` over a transport-native client (Bot API, Gateway
//! websocket, Socket Mode, IMAP/SMTP, …); everything after the transport —
//! allowlist refusal, durable enqueue, claim→turn→settle, owed-send delivery,
//! ordered event rows — is this module and is shared by every transport.
//!
//! The durable spine this loop drives lives in [`crate::gateway`] and
//! [`crate::gateway::outbound_ledger`] and is deliberately NOT re-implemented
//! here: local SQLite stays the delivery authority (ADR-0021), a routing
//! address stays `transport:external-id` (ADR-0071), and an outbound send
//! stays a durable obligation (ADR-0070).
//!
//! The Telegram adapter (program P28) keeps its own legacy `poll_once` path
//! byte-for-byte compatible; this module is the path new adapters (Discord,
//! Slack, Email) and the gateway supervisor are built on.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::gateway::outbound_ledger::{claim_outbound, settle_outbound, OutboundSettlement};
use crate::gateway::{
    claim_one, complete_claim, enqueue, fail_claim, release_claim, GatewayClaim, GatewayPaths,
};
use uuid::Uuid;

/// Stable transport identifiers. The string form IS the gateway `channel`
/// column value and the routing-address prefix (ADR-0071), so it is a wire
/// contract: never rename a variant's string without migrating rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportId {
    Telegram,
    Discord,
    Slack,
    Email,
    WhatsApp,
    Signal,
}

impl TransportId {
    pub const ALL: [TransportId; 6] = [
        TransportId::Telegram,
        TransportId::Discord,
        TransportId::Slack,
        TransportId::Email,
        TransportId::WhatsApp,
        TransportId::Signal,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TransportId::Telegram => "telegram",
            TransportId::Discord => "discord",
            TransportId::Slack => "slack",
            TransportId::Email => "email",
            TransportId::WhatsApp => "whatsapp",
            TransportId::Signal => "signal",
        }
    }

    pub fn parse(value: &str) -> Option<TransportId> {
        TransportId::ALL
            .iter()
            .copied()
            .find(|t| t.as_str() == value)
    }
}

/// A transport-native inbound unit, before canonicalization and authorization.
///
/// `from` is the routing-address payload (chat id, channel id, sender email);
/// the canonical session id is derived as `transport:from` by the loop.
/// Attachments are artifact-store paths already materialized by the adapter,
/// never remote URLs fetched lazily by the turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawInbound {
    pub from: String,
    pub text: String,
    pub attachments: Vec<String>,
}

/// Outcome of one outbound send. Exactly one of the three is returned per
/// attempt (terminal-outcome law): the adapter never returns "maybe" and never
/// returns nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    /// Adapter confirmed the platform accepted the message (local receipt only;
    /// external exactly-once is never claimed).
    Confirmed { provider_message_id: String },
    /// Network/timeout left delivery unknown — operator must recover.
    Ambiguous { detail: String },
    /// Definite failure (bad target, auth, refusal).
    Failed { detail: String },
}

/// Maps a send outcome onto the durable obligation settlement (ADR-0070).
pub fn settlement_for_outcome(outcome: SendOutcome) -> OutboundSettlement {
    match outcome {
        SendOutcome::Confirmed {
            provider_message_id,
        } => OutboundSettlement::Delivered {
            provider_message_id,
        },
        SendOutcome::Ambiguous { detail } => OutboundSettlement::Ambiguous { detail },
        SendOutcome::Failed { detail } => OutboundSettlement::Failed { detail },
    }
}

/// spec-017 R1: one contract for every live transport.
///
/// Implementations are single-threaded clients; the supervisor gives each
/// adapter its own thread, so the trait needs no interior mutability.
pub trait TransportAdapter: Send {
    /// The stable transport identity; `transport().as_str()` is the channel.
    fn transport(&self) -> TransportId;

    /// Whether this adapter is configured and enabled for `home` (fail-closed:
    /// absent or disabled config means false).
    fn is_enabled(&self, home: &Path) -> bool;

    /// Pull every inbound update the transport has waiting (long-poll, socket
    /// frame batch, or poll cadence). An empty vec means "nothing new".
    fn poll_inbound(&mut self, home: &Path) -> Result<Vec<RawInbound>, String>;

    /// Inbound authorization (spec-017 R6): may `from` drive a turn? Refused
    /// messages are never enqueued and are recorded as
    /// `transport_refused_unauthorized` event rows.
    fn is_allowed(&self, from: &str) -> bool;

    /// Deliver one outbound message to a routing target (`from` of the inbound
    /// that owes it). Exactly one terminal outcome or an adapter-level error
    /// (which the loop settles as Ambiguous — the platform may have it).
    fn send(&mut self, target: &str, body: &str) -> Result<SendOutcome, String>;

    /// Best-effort health probe; a transport that has no cheap probe reports ok.
    fn health(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Construct the adapter for a transport from its config, or None when the
/// transport is not configured for this home. Every adapter module (telegram,
/// discord, slack, email, …) exposes this exact convention so the supervisor
/// builds the whole registry without knowing transports.
pub type AdapterBuilder = fn(home: &Path) -> Result<Option<Box<dyn TransportAdapter>>, String>;

/// Ordered, durable event rows for transport observability (spec-017 R10).
///
/// `seq` is a monotonic AUTOINCREMENT so rows are queryable in the order the
/// adapter recorded them, per transport and per kind.
pub const EVENT_INBOUND_RECEIVED: &str = "inbound_received";
pub const EVENT_INBOUND_CLAIMED: &str = "inbound_claimed";
pub const EVENT_TURN_STARTED: &str = "turn_started";
pub const EVENT_TURN_COMPLETED: &str = "turn_completed";
pub const EVENT_SEND_ATTEMPTED: &str = "send_attempted";
pub const EVENT_SEND_OUTCOME: &str = "send_outcome";
pub const EVENT_REFUSED: &str = "inbound_refused";

/// The named diagnostic for an unauthorized inbound (spec-017 R6).
pub const REFUSED_DIAGNOSTIC: &str = "transport_refused_unauthorized";

fn open_events(connection: &rusqlite::Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS gateway_events(
           seq INTEGER PRIMARY KEY AUTOINCREMENT,
           unix INTEGER NOT NULL,
           transport TEXT NOT NULL,
           kind TEXT NOT NULL,
           detail TEXT NOT NULL
         );",
    )
}

fn open_gateway_db(paths: &GatewayPaths) -> Result<rusqlite::Connection, String> {
    let connection = rusqlite::Connection::open(&paths.database).map_err(|e| e.to_string())?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")
        .map_err(|e| e.to_string())?;
    Ok(connection)
}

/// Record one ordered, durable transport event row (R10).
pub fn record_transport_event(
    home: impl AsRef<Path>,
    transport: &str,
    kind: &str,
    detail: &str,
) -> Result<(), String> {
    let paths = GatewayPaths::open(home).map_err(|e| e.to_string())?;
    let connection = open_gateway_db(&paths)?;
    open_events(&connection).map_err(|e| e.to_string())?;
    connection
        .execute(
            "INSERT INTO gateway_events(unix, transport, kind, detail) VALUES(?1,?2,?3,?4)",
            params![now_unix() as i64, transport, kind, detail],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Query transport event rows, newest first (R10).
pub fn list_transport_events(
    home: impl AsRef<Path>,
    transport: Option<&str>,
    limit: usize,
) -> Result<Vec<TransportEvent>, String> {
    let paths = GatewayPaths::open(home).map_err(|e| e.to_string())?;
    let connection = open_gateway_db(&paths)?;
    open_events(&connection).map_err(|e| e.to_string())?;
    let mut statement = match transport {
        Some(_) => connection
            .prepare(
                "SELECT seq,unix,transport,kind,detail FROM gateway_events
                 WHERE transport=?1 ORDER BY seq DESC LIMIT ?2",
            )
            .map_err(|e| e.to_string())?,
        None => connection
            .prepare(
                "SELECT seq,unix,transport,kind,detail FROM gateway_events
                 ORDER BY seq DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?,
    };
    let rows: Vec<TransportEvent> = if let Some(channel) = transport {
        statement
            .query_map(params![channel, limit as i64], row_to_event)
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<_>>()
            .map_err(|e| e.to_string())?
    } else {
        statement
            .query_map(params![limit as i64], row_to_event)
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<_>>()
            .map_err(|e| e.to_string())?
    };
    Ok(rows)
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<TransportEvent> {
    Ok(TransportEvent {
        seq: row.get(0)?,
        unix: row.get::<_, i64>(1)? as u64,
        transport: row.get(2)?,
        kind: row.get(3)?,
        detail: row.get(4)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportEvent {
    pub seq: i64,
    pub unix: u64,
    pub transport: String,
    pub kind: String,
    pub detail: String,
}

/// Per-cycle accounting, transport-agnostic.
#[derive(Debug, Default, Clone)]
pub struct AdapterCycleResult {
    pub enqueued: Vec<String>,
    pub drained: Vec<String>,
    pub refused: Vec<String>,
    pub receipts: Vec<String>,
    pub ambiguous: Vec<String>,
    pub failed_sends: Vec<String>,
}

impl AdapterCycleResult {
    pub fn is_idle(&self) -> bool {
        self.enqueued.is_empty() && self.drained.is_empty() && self.refused.is_empty()
    }
}

/// How long one outbound send may hold its obligation before the stale sweep
/// calls it unknown. Matches the Telegram adapter's bound.
const SEND_LEASE_SECS: u64 = 120;

/// Claim lease for one inbound turn: 15 minutes is long enough for a real
/// provider turn with tool calls, short enough that a dead worker is
/// recoverable by the next cycle.
const TURN_LEASE_SECS: u64 = 900;

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// spec-017 R1/R6/R9/R10: one canonical claim→turn→settle cycle, shared by
/// every transport.
///
/// poll → authorize (refuse + event) → enqueue (event) → claim (event) →
/// turn (events) → commit/fail → deliver every owed send for this transport.
/// Only messages this cycle enqueued are claimed — a foreign channel's FIFO
/// head is released and stops the sweep, exactly like the legacy Telegram
/// cycle, so one slow transport never starves another.
pub fn adapter_cycle<A, F>(
    home: &Path,
    adapter: &mut A,
    mut turn: F,
) -> Result<AdapterCycleResult, String>
where
    A: TransportAdapter,
    F: FnMut(&crate::gateway::InboundMessage) -> Result<(String, Option<String>), String>,
{
    let transport = adapter.transport().as_str();
    let mut result = AdapterCycleResult::default();

    let raw = adapter.poll_inbound(home)?;
    for inbound in raw {
        if inbound.text.trim().is_empty() && inbound.attachments.is_empty() {
            continue;
        }
        if !adapter.is_allowed(&inbound.from) {
            let detail = format!("{}:{}", REFUSED_DIAGNOSTIC, inbound.from);
            record_transport_event(home, transport, EVENT_REFUSED, &detail)?;
            result.refused.push(inbound.from);
            continue;
        }
        let session = Some(format!("{}:{}", transport, inbound.from));
        record_transport_event(home, transport, EVENT_INBOUND_RECEIVED, &inbound.from)?;
        let message = enqueue(home, transport, &inbound.text, "auto", session.as_deref())
            .map_err(|e| e.to_string())?;
        result.enqueued.push(message.id);
    }

    // Claim in FIFO order (received_unix, then id), not enqueue order:
    // same-second batch entries share received_unix, so the head is not
    // necessarily the first entry we pushed. A head we did not enqueue this
    // cycle belongs to another transport — release it and stop; everything
    // after it is foreign too.
    let mut owed = result
        .enqueued
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    while !owed.is_empty() {
        let Some(claim) = claim_one(home, Uuid::new_v4(), now_unix(), TURN_LEASE_SECS)
            .map_err(|e| e.to_string())?
        else {
            break;
        };
        if !owed.remove(&claim.message().id) {
            release_claim(home, &claim, now_unix()).map_err(|e| e.to_string())?;
            break;
        }
        run_claim(home, adapter, &claim, &mut turn, &mut result)?;
    }

    deliver_owed_sends(home, adapter, &mut result)?;
    Ok(result)
}

/// Run one claimed message's turn and record its terminal outcome.
fn run_claim<A, F>(
    home: &Path,
    adapter: &mut A,
    claim: &GatewayClaim,
    turn: &mut F,
    result: &mut AdapterCycleResult,
) -> Result<(), String>
where
    A: TransportAdapter,
    F: FnMut(&crate::gateway::InboundMessage) -> Result<(String, Option<String>), String>,
{
    let transport = adapter.transport().as_str();
    record_transport_event(home, transport, EVENT_INBOUND_CLAIMED, &claim.message().id)?;
    record_transport_event(home, transport, EVENT_TURN_STARTED, &claim.message().id)?;
    let outcome = turn(claim.message());
    let drained = match outcome {
        Ok(success) => {
            record_transport_event(
                home,
                transport,
                EVENT_TURN_COMPLETED,
                &format!("{}:ok", claim.message().id),
            )?;
            complete_claim(home, claim, Ok(success), now_unix()).map_err(|e| e.to_string())?
        }
        Err(error) => {
            record_transport_event(
                home,
                transport,
                EVENT_TURN_COMPLETED,
                &format!("{}:error:{error}", claim.message().id),
            )?;
            fail_claim(home, claim, &error, now_unix()).map_err(|e| e.to_string())?
        }
    };
    result.drained.push(drained.id);
    Ok(())
}

/// Send every reply the outbound ledger owes this transport, one attempt each,
/// stopping at the first send that is not confirmed (the refused/dark transport
/// will not answer differently a millisecond later; the failed obligation is
/// back in the pending pool for the next cycle's retry).
fn deliver_owed_sends<A>(
    home: &Path,
    adapter: &mut A,
    result: &mut AdapterCycleResult,
) -> Result<(), String>
where
    A: TransportAdapter,
{
    let transport = adapter.transport().as_str();
    while let Some(claim) = claim_outbound(
        home,
        Some(transport),
        Uuid::new_v4(),
        now_unix(),
        SEND_LEASE_SECS,
    )
    .map_err(|e| e.to_string())?
    {
        let owed = claim.obligation().clone();
        let Some(target) = owed.target.strip_prefix(&format!("{transport}:")) else {
            let detail = format!("unroutable {transport} target {}", owed.target);
            settle_outbound(
                home,
                &claim,
                OutboundSettlement::Failed { detail },
                now_unix(),
            )
            .map_err(|e| e.to_string())?;
            result.failed_sends.push(owed.message_id);
            break;
        };
        record_transport_event(home, transport, EVENT_SEND_ATTEMPTED, &owed.message_id)?;
        let outcome = match adapter.send(target, &owed.body) {
            Ok(outcome) => outcome,
            Err(error) => {
                let detail = format!("transport error: {error}");
                settle_outbound(
                    home,
                    &claim,
                    OutboundSettlement::Ambiguous { detail },
                    now_unix(),
                )
                .map_err(|e| e.to_string())?;
                result.ambiguous.push(owed.message_id);
                return Err(error);
            }
        };
        let confirmed = matches!(outcome, SendOutcome::Confirmed { .. });
        let outcome_kind = match &outcome {
            SendOutcome::Confirmed { .. } => "delivered",
            SendOutcome::Ambiguous { .. } => "ambiguous",
            SendOutcome::Failed { .. } => "failed",
        };
        settle_outbound(home, &claim, settlement_for_outcome(outcome), now_unix())
            .map_err(|e| e.to_string())?;
        record_transport_event(
            home,
            transport,
            EVENT_SEND_OUTCOME,
            &format!("{}:{outcome_kind}", owed.message_id),
        )?;
        if confirmed {
            result.receipts.push(owed.message_id);
        } else {
            if outcome_kind == "ambiguous" {
                result.ambiguous.push(owed.message_id);
            } else {
                result.failed_sends.push(owed.message_id);
            }
            break;
        }
    }
    Ok(())
}

/// Delegating impl so the supervisor can hold adapters as boxed objects and
/// still drive the generic cycle and worker directly (spec-017 R7).
impl TransportAdapter for Box<dyn TransportAdapter> {
    fn transport(&self) -> TransportId {
        (**self).transport()
    }
    fn is_enabled(&self, home: &Path) -> bool {
        (**self).is_enabled(home)
    }
    fn poll_inbound(&mut self, home: &Path) -> Result<Vec<RawInbound>, String> {
        (**self).poll_inbound(home)
    }
    fn is_allowed(&self, from: &str) -> bool {
        (**self).is_allowed(from)
    }
    fn send(&mut self, target: &str, body: &str) -> Result<SendOutcome, String> {
        (**self).send(target, body)
    }
    fn health(&self) -> Result<(), String> {
        (**self).health()
    }
}

/// Supervisor-visible per-adapter state (spec-017 R7).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterStatus {
    pub transport: String,
    pub state: AdapterState,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub started_unix: Option<u64>,
    #[serde(default)]
    pub uptime_secs: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AdapterState {
    Running,
    Stopped,
    Failed,
}

/// Snapshot of every supervised adapter, persisted under
/// `{home}/gateway/supervisor.json` so the status surface can report across
/// processes without owning the workers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SupervisorState {
    pub adapters: Vec<AdapterStatus>,
}

pub fn snapshot_path(home: &Path) -> PathBuf {
    home.join("gateway").join("supervisor.json")
}

/// Atomically persist the supervisor snapshot (temp + rename).
pub fn write_supervisor_snapshot(home: &Path, state: &SupervisorState) -> Result<(), String> {
    let path = snapshot_path(home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())
}

/// Read the persisted snapshot; empty state when no supervisor has run.
pub fn read_supervisor_snapshot(home: &Path) -> SupervisorState {
    std::fs::read(snapshot_path(home))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

/// Backoff bounds for supervisor restarts (spec-017 R7).
const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Consecutive failed cycles before the supervisor stops pretending the
/// adapter is healthy; it keeps retrying with backoff either way.
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// Run one adapter forever: poll → cycle → backoff, with panic isolation and
/// per-adapter status kept in the shared registry (spec-017 R7).
///
/// Returns immediately after spawning the worker. The worker exits cleanly
/// (state Stopped) when the adapter is not enabled for `home`.
pub fn spawn_adapter_worker<A, F>(
    home: PathBuf,
    mut adapter: A,
    mut turn: F,
    registry: Arc<Mutex<SupervisorState>>,
) -> std::thread::JoinHandle<()>
where
    A: TransportAdapter + 'static,
    F: FnMut(&crate::gateway::InboundMessage) -> Result<(String, Option<String>), String>
        + Send
        + 'static,
{
    std::thread::spawn(move || {
        let transport = adapter.transport().as_str().to_string();
        let started = now_unix();
        let mut failures: u32 = 0;
        if !adapter.is_enabled(&home) {
            set_status(
                &registry,
                transport,
                AdapterState::Stopped,
                Some("not configured".into()),
                Some(started),
                0,
            );
            return;
        }
        set_status(
            &registry,
            transport.clone(),
            AdapterState::Running,
            None,
            Some(started),
            0,
        );
        loop {
            let cycle = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                adapter_cycle(&home, &mut adapter, &mut turn)
            }));
            match cycle {
                Ok(Ok(result)) => {
                    failures = 0;
                    set_status(
                        &registry,
                        transport.clone(),
                        AdapterState::Running,
                        None,
                        Some(started),
                        now_unix().saturating_sub(started),
                    );
                    if result.is_idle() {
                        std::thread::sleep(Duration::from_secs(1));
                    }
                }
                Ok(Err(error)) => {
                    failures = failures.saturating_add(1);
                    let detail = if failures >= MAX_CONSECUTIVE_FAILURES {
                        format!("{error} (persistent; retrying with backoff)")
                    } else {
                        error
                    };
                    set_status(
                        &registry,
                        transport.clone(),
                        AdapterState::Failed,
                        Some(detail),
                        Some(started),
                        now_unix().saturating_sub(started),
                    );
                    std::thread::sleep(backoff(failures));
                }
                Err(_) => {
                    set_status(
                        &registry,
                        transport.clone(),
                        AdapterState::Failed,
                        Some("adapter panicked; restarting".into()),
                        Some(started),
                        now_unix().saturating_sub(started),
                    );
                    std::thread::sleep(MAX_BACKOFF);
                }
            }
        }
    })
}

fn backoff(failures: u32) -> Duration {
    let secs = MIN_BACKOFF
        .as_secs()
        .saturating_mul(1u64 << failures.min(5));
    Duration::from_secs(secs.min(MAX_BACKOFF.as_secs()))
}

fn set_status(
    registry: &Arc<Mutex<SupervisorState>>,
    transport: String,
    state: AdapterState,
    last_error: Option<String>,
    started_unix: Option<u64>,
    uptime_secs: u64,
) {
    if let Ok(mut guard) = registry.lock() {
        let entry = guard.adapters.iter_mut().find(|a| a.transport == transport);
        match entry {
            Some(entry) => {
                entry.state = state;
                entry.last_error = last_error;
                entry.started_unix = started_unix;
                entry.uptime_secs = uptime_secs;
            }
            None => guard.adapters.push(AdapterStatus {
                transport,
                state,
                last_error,
                started_unix,
                uptime_secs,
            }),
        }
    }
}

/// Snapshot writer loop: persist the registry every few seconds so the status
/// surface is readable across processes even while workers churn.
pub fn spawn_snapshot_writer(
    home: PathBuf,
    registry: Arc<Mutex<SupervisorState>>,
    interval: Duration,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || loop {
        let state = registry.lock().map(|g| g.clone()).unwrap_or_default();
        let _ = write_supervisor_snapshot(&home, &state);
        std::thread::sleep(interval);
    })
}

/// Run one adapter cycle once (test/diagnostic entry, no supervision).
pub fn cycle_once<A, F>(home: &Path, adapter: &mut A, turn: F) -> Result<AdapterCycleResult, String>
where
    A: TransportAdapter,
    F: FnMut(&crate::gateway::InboundMessage) -> Result<(String, Option<String>), String>,
{
    adapter_cycle(home, adapter, turn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::InboundMessage;
    use crate::telegram::TelegramConfig;
    use tempfile::tempdir;

    /// A scriptable fake adapter: queue of raws, allowlist, recorded sends.
    struct FakeAdapter {
        transport: TransportId,
        pending: Vec<RawInbound>,
        allowlist: Vec<String>,
        pub sent: Vec<(String, String)>,
        next_send: Option<SendOutcome>,
    }

    impl FakeAdapter {
        fn new(transport: TransportId) -> Self {
            Self {
                transport,
                pending: Vec::new(),
                allowlist: vec!["42".into()],
                sent: Vec::new(),
                next_send: None,
            }
        }
        fn push(&mut self, from: &str, text: &str) {
            self.pending.push(RawInbound {
                from: from.into(),
                text: text.into(),
                attachments: Vec::new(),
            });
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
            Ok(std::mem::take(&mut self.pending))
        }
        fn is_allowed(&self, from: &str) -> bool {
            self.allowlist.iter().any(|a| a == from)
        }
        fn send(&mut self, target: &str, body: &str) -> Result<SendOutcome, String> {
            self.sent.push((target.into(), body.into()));
            Ok(self.next_send.take().unwrap_or(SendOutcome::Confirmed {
                provider_message_id: "m1".into(),
            }))
        }
    }

    #[test]
    fn full_cycle_claims_turns_and_delivers() {
        let dir = tempdir().unwrap();
        let mut adapter = FakeAdapter::new(TransportId::Discord);
        adapter.push("42", "hello discord");

        let result = cycle_once(dir.path(), &mut adapter, |inbound: &InboundMessage| {
            assert_eq!(inbound.channel, "discord");
            Ok((
                format!("reply:{}", inbound.text),
                inbound.session_id.clone(),
            ))
        })
        .unwrap();

        assert_eq!(result.enqueued.len(), 1);
        assert_eq!(result.drained.len(), 1);
        assert_eq!(result.receipts.len(), 1);
        assert!(result.refused.is_empty());
        assert_eq!(adapter.sent.len(), 1);
        assert_eq!(
            adapter.sent[0],
            ("42".to_string(), "reply:hello discord".to_string())
        );
    }

    #[test]
    fn unauthorized_inbound_is_refused_before_any_turn() {
        let dir = tempdir().unwrap();
        let mut adapter = FakeAdapter::new(TransportId::Slack);
        adapter.push("stranger", "drive me");

        let result = cycle_once(dir.path(), &mut adapter, |_| {
            panic!("unauthorized message must not reach a turn")
        })
        .unwrap();

        assert!(result.enqueued.is_empty());
        assert_eq!(result.refused, vec!["stranger".to_string()]);
        assert!(adapter.sent.is_empty());
        let events = list_transport_events(dir.path(), Some("slack"), 10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EVENT_REFUSED);
        assert!(events[0].detail.contains(REFUSED_DIAGNOSTIC));
        // The gateway itself saw nothing.
        assert!(crate::gateway::list_inbox(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn refused_events_are_ordered_and_queryable_by_transport() {
        let dir = tempdir().unwrap();
        record_transport_event(dir.path(), "discord", EVENT_REFUSED, "a").unwrap();
        record_transport_event(dir.path(), "discord", EVENT_REFUSED, "b").unwrap();
        record_transport_event(dir.path(), "telegram", EVENT_REFUSED, "c").unwrap();

        let discord = list_transport_events(dir.path(), Some("discord"), 10).unwrap();
        assert_eq!(discord.len(), 2);
        assert_eq!(discord[0].detail, "b");
        assert_eq!(discord[1].detail, "a");
        assert!(discord[0].seq > discord[1].seq);
        let telegram = list_transport_events(dir.path(), Some("telegram"), 10).unwrap();
        assert_eq!(telegram.len(), 1);
    }

    #[test]
    fn failed_send_settles_failed_not_ambiguous() {
        let dir = tempdir().unwrap();
        let mut adapter = FakeAdapter::new(TransportId::Discord);
        adapter.push("42", "hi");
        adapter.next_send = Some(SendOutcome::Failed {
            detail: "channel_deleted".into(),
        });

        let result = cycle_once(dir.path(), &mut adapter, |inbound| {
            Ok(("reply".into(), inbound.session_id.clone()))
        })
        .unwrap();

        assert_eq!(result.failed_sends.len(), 1);
        assert!(result.ambiguous.is_empty());
        assert!(
            crate::gateway::outbound_receipts::list_ambiguous_sends(dir.path(), 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn ambiguous_send_leaves_operator_recovery() {
        let dir = tempdir().unwrap();
        let mut adapter = FakeAdapter::new(TransportId::Slack);
        adapter.push("42", "hi");
        adapter.next_send = Some(SendOutcome::Ambiguous {
            detail: "timeout".into(),
        });

        let result = cycle_once(dir.path(), &mut adapter, |inbound| {
            Ok(("reply".into(), inbound.session_id.clone()))
        })
        .unwrap();

        assert_eq!(result.ambiguous.len(), 1);
        assert_eq!(
            crate::gateway::outbound_receipts::list_ambiguous_sends(dir.path(), 10)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn supervisor_snapshot_roundtrip() {
        let dir = tempdir().unwrap();
        let state = SupervisorState {
            adapters: vec![AdapterStatus {
                transport: "telegram".into(),
                state: AdapterState::Running,
                last_error: None,
                started_unix: Some(1),
                uptime_secs: 5,
            }],
        };
        write_supervisor_snapshot(dir.path(), &state).unwrap();
        let read = read_supervisor_snapshot(dir.path());
        assert_eq!(read.adapters.len(), 1);
        assert_eq!(read.adapters[0].state, AdapterState::Running);
        assert_eq!(read.adapters[0].transport, "telegram");
    }

    #[test]
    fn telegram_config_roundtrip_is_compatible() {
        // The contract must not disturb the telegram config wire shape.
        let config = TelegramConfig {
            enabled: true,
            bot_token_env: "OPTIMUS_TELEGRAM_BOT_TOKEN".into(),
            allowed_chat_ids: vec!["1".into()],
        };
        let raw = serde_json::to_string(&config).unwrap();
        let parsed: TelegramConfig = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.allowed_chat_ids, vec!["1".to_string()]);
    }

    #[test]
    fn worker_exits_stopped_when_disabled() {
        struct Disabled;
        impl TransportAdapter for Disabled {
            fn transport(&self) -> TransportId {
                TransportId::Email
            }
            fn is_enabled(&self, _home: &Path) -> bool {
                false
            }
            fn poll_inbound(&mut self, _home: &Path) -> Result<Vec<RawInbound>, String> {
                Ok(Vec::new())
            }
            fn is_allowed(&self, _from: &str) -> bool {
                false
            }
            fn send(&mut self, _target: &str, _body: &str) -> Result<SendOutcome, String> {
                Err("never".into())
            }
        }
        let dir = tempdir().unwrap();
        let registry: Arc<Mutex<SupervisorState>> =
            Arc::new(Mutex::new(SupervisorState::default()));
        let handle = spawn_adapter_worker(
            dir.path().to_path_buf(),
            Disabled,
            |_| Ok((String::new(), None)),
            Arc::clone(&registry),
        );
        handle.join().unwrap();
        let state = registry.lock().unwrap().clone();
        assert_eq!(state.adapters.len(), 1);
        assert_eq!(state.adapters[0].state, AdapterState::Stopped);
    }

    #[test]
    fn idle_cycle_returns_clean() {
        let dir = tempdir().unwrap();
        let mut adapter = FakeAdapter::new(TransportId::Telegram);
        let result = cycle_once(dir.path(), &mut adapter, |_| Err("must not run".into())).unwrap();
        assert!(result.is_idle());
    }
}
