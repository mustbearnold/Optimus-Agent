//! Provenance-bound provider telemetry with deterministic integer aggregates.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{KernelError, ModelId, ProviderId, Result, TraceId};

pub const MAX_TELEMETRY_SAMPLES: usize = 1_000;
pub const MAX_TELEMETRY_LATENCY_MILLIS: u64 = 86_400_000;

fn invalid(reason: impl Into<String>) -> KernelError {
    KernelError::Model(format!("invalid route telemetry: {}", reason.into()))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteTelemetryOutcome {
    Succeeded,
    Failed,
    Cancelled,
    Ambiguous,
}

impl RouteTelemetryOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteTelemetryObservation {
    pub attempt_id: Uuid,
    pub route_id: Uuid,
    pub trace_id: Option<TraceId>,
    pub provider: ProviderId,
    pub model: ModelId,
    pub outcome: RouteTelemetryOutcome,
    pub latency_millis: u64,
    pub cost_microunits: u64,
    pub observed_unix: u64,
}

impl RouteTelemetryObservation {
    fn validate(&self) -> Result<()> {
        if self.latency_millis == 0 || self.latency_millis > MAX_TELEMETRY_LATENCY_MILLIS {
            return Err(invalid("latency is outside policy"));
        }
        if self.observed_unix == 0 || self.observed_unix > now_unix().saturating_add(300) {
            return Err(invalid("observation time is outside policy"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteTelemetryAggregate {
    pub provider: ProviderId,
    pub model: ModelId,
    pub since_unix: u64,
    pub samples: usize,
    pub successes: usize,
    pub success_basis_points: u16,
    pub median_latency_millis: u64,
    pub p95_latency_millis: u64,
    pub total_cost_microunits: u64,
    pub mean_cost_microunits: u64,
}

pub fn record_route_telemetry(
    home: impl AsRef<Path>,
    observation: &RouteTelemetryObservation,
) -> Result<()> {
    observation.validate()?;
    let connection = open(home)?;
    let route = connection
        .query_row(
            "SELECT selected_provider,selected_model,trace_id FROM route_decisions WHERE id=?1",
            params![observation.route_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| invalid("route decision does not exist"))?;
    let trace = observation.trace_id.map(|value| value.to_string());
    if route.0 != observation.provider.as_str()
        || route.1 != observation.model.as_str()
        || route.2 != trace
    {
        return Err(invalid(
            "observation does not match route provider, model, or trace",
        ));
    }
    connection.execute(
        "INSERT INTO route_telemetry(
           attempt_id,route_id,trace_id,provider,model,outcome,latency_millis,cost_microunits,
           observed_unix
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            observation.attempt_id.to_string(),
            observation.route_id.to_string(),
            trace,
            observation.provider.as_str(),
            observation.model.as_str(),
            observation.outcome.as_str(),
            observation.latency_millis as i64,
            i64::try_from(observation.cost_microunits)
                .map_err(|_| invalid("cost exceeds SQLite integer range"))?,
            observation.observed_unix as i64
        ],
    )?;
    Ok(())
}

pub fn route_telemetry_aggregate(
    home: impl AsRef<Path>,
    provider: ProviderId,
    model: &str,
    since_unix: u64,
    limit: usize,
) -> Result<Option<RouteTelemetryAggregate>> {
    let model = ModelId::parse(model.to_string())?;
    if limit == 0 || limit > MAX_TELEMETRY_SAMPLES {
        return Err(invalid("aggregate sample limit is outside policy"));
    }
    let path = home.as_ref().join("routing.db");
    if !path.exists() {
        return Ok(None);
    }
    let connection = open(home)?;
    let mut statement = connection.prepare(
        "SELECT outcome,latency_millis,cost_microunits FROM route_telemetry
         WHERE provider=?1 AND model=?2 AND observed_unix>=?3
         ORDER BY observed_unix DESC,attempt_id DESC LIMIT ?4",
    )?;
    let rows = statement
        .query_map(
            params![
                provider.as_str(),
                model.as_str(),
                since_unix as i64,
                limit as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if rows.is_empty() {
        return Ok(None);
    }
    let samples = rows.len();
    let successes = rows.iter().filter(|row| row.0 == "succeeded").count();
    let mut latencies = rows.iter().map(|row| row.1 as u64).collect::<Vec<_>>();
    latencies.sort_unstable();
    let median_latency_millis = if samples % 2 == 0 {
        latencies[samples / 2 - 1]
            .checked_add(latencies[samples / 2])
            .ok_or_else(|| invalid("median latency overflow"))?
            / 2
    } else {
        latencies[samples / 2]
    };
    let p95_index = (95usize
        .checked_mul(samples)
        .ok_or_else(|| invalid("p95 sample overflow"))?
        .div_ceil(100))
    .saturating_sub(1);
    let total_cost_microunits = rows.iter().try_fold(0u64, |total, row| {
        let cost = u64::try_from(row.2).map_err(|_| invalid("negative cost in telemetry"))?;
        total
            .checked_add(cost)
            .ok_or_else(|| invalid("telemetry cost overflow"))
    })?;
    Ok(Some(RouteTelemetryAggregate {
        provider,
        model,
        since_unix,
        samples,
        successes,
        success_basis_points: u16::try_from(successes * 10_000 / samples)
            .expect("basis points are bounded"),
        median_latency_millis,
        p95_latency_millis: latencies[p95_index],
        total_cost_microunits,
        mean_cost_microunits: total_cost_microunits / samples as u64,
    }))
}

fn open(home: impl AsRef<Path>) -> Result<Connection> {
    std::fs::create_dir_all(home.as_ref())?;
    let connection = Connection::open(home.as_ref().join("routing.db"))?;
    let route_table: Option<String> = connection
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='route_decisions'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if route_table.is_none() {
        return Err(invalid("routing ledger must exist before telemetry"));
    }
    connection.execute_batch(
        "PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS route_telemetry(
           attempt_id TEXT PRIMARY KEY,
           route_id TEXT NOT NULL REFERENCES route_decisions(id) ON DELETE RESTRICT,
           trace_id TEXT,provider TEXT NOT NULL,model TEXT NOT NULL,
           outcome TEXT NOT NULL CHECK(outcome IN ('succeeded','failed','cancelled','ambiguous')),
           latency_millis INTEGER NOT NULL CHECK(latency_millis>0),
           cost_microunits INTEGER NOT NULL CHECK(cost_microunits>=0),
           observed_unix INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS route_telemetry_lookup
           ON route_telemetry(provider,model,observed_unix);",
    )?;
    Ok(connection)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
