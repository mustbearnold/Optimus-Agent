//! Canonical bounded trace/span identities and append-only local trace evidence.

use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{KernelError, Result};

fn invalid(reason: impl Into<String>) -> KernelError {
    KernelError::Model(format!("invalid trace evidence: {}", reason.into()))
}

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn parse(value: &str) -> Result<Self> {
                Ok(Self(Uuid::parse_str(value)?))
            }

            pub fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

uuid_id!(TraceId);
uuid_id!(SpanId);

impl From<Uuid> for TraceId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
}

impl TraceContext {
    pub fn new(trace_id: TraceId, span_id: SpanId, parent_span_id: Option<SpanId>) -> Self {
        Self {
            trace_id,
            span_id,
            parent_span_id,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Ambiguous,
}

impl SpanStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Ambiguous => "ambiguous",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "ambiguous" => Ok(Self::Ambiguous),
            _ => Err(invalid("unknown span status")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TraceEventKind {
    Started,
    Evidence,
    Linked,
    Terminal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceSpan {
    pub context: TraceContext,
    pub subsystem: String,
    pub subject: String,
    pub status: SpanStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceEvent {
    pub context: TraceContext,
    pub sequence: u64,
    pub kind: TraceEventKind,
    pub subject: String,
    pub evidence_sha256: String,
    pub observed_unix: u64,
}

pub struct TraceStore {
    conn: Connection,
}

impl TraceStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS trace_spans(
               span_id TEXT PRIMARY KEY,trace_id TEXT NOT NULL,parent_span_id TEXT,
               subsystem TEXT NOT NULL,subject TEXT NOT NULL,
               status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','cancelled','ambiguous')),
               FOREIGN KEY(parent_span_id) REFERENCES trace_spans(span_id)
             );
             CREATE TABLE IF NOT EXISTS trace_events(
               span_id TEXT NOT NULL REFERENCES trace_spans(span_id),
               sequence INTEGER NOT NULL,kind TEXT NOT NULL,subject TEXT NOT NULL,
               evidence_sha256 TEXT NOT NULL,observed_unix INTEGER NOT NULL,
               PRIMARY KEY(span_id,sequence)
             );",
        )?;
        Ok(Self { conn })
    }

    pub fn begin_root(&self, subsystem: &str, subject: &str) -> Result<TraceContext> {
        let context = TraceContext::new(TraceId::new(), SpanId::new(), None);
        self.register_span(context, subsystem, subject)?;
        Ok(context)
    }

    pub fn begin_child(
        &self,
        parent: TraceContext,
        subsystem: &str,
        subject: &str,
    ) -> Result<TraceContext> {
        let context = TraceContext::new(parent.trace_id, SpanId::new(), Some(parent.span_id));
        self.register_span(context, subsystem, subject)?;
        Ok(context)
    }

    pub fn register_span(
        &self,
        context: TraceContext,
        subsystem: &str,
        subject: &str,
    ) -> Result<()> {
        validate_label(subsystem, "subsystem")?;
        validate_label(subject, "subject")?;
        if let Some(parent) = context.parent_span_id {
            if parent == context.span_id {
                return Err(invalid("span cannot parent itself"));
            }
            let parent_trace = self
                .conn
                .query_row(
                    "SELECT trace_id FROM trace_spans WHERE span_id=?1",
                    params![parent.to_string()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| invalid("parent span does not exist"))?;
            if parent_trace != context.trace_id.to_string() {
                return Err(invalid("parent belongs to another trace"));
            }
        }
        self.conn.execute(
            "INSERT INTO trace_spans(span_id,trace_id,parent_span_id,subsystem,subject,status)
             VALUES(?1,?2,?3,?4,?5,'running')",
            params![
                context.span_id.to_string(),
                context.trace_id.to_string(),
                context.parent_span_id.map(|value| value.to_string()),
                subsystem,
                subject
            ],
        )?;
        Ok(())
    }

    pub fn append_event(
        &self,
        context: TraceContext,
        kind: TraceEventKind,
        subject: &str,
        evidence_sha256: String,
    ) -> Result<TraceEvent> {
        if kind == TraceEventKind::Terminal {
            return Err(invalid("terminal events are owned by span settlement"));
        }
        self.append(context, kind, subject, evidence_sha256)
    }

    pub fn settle(&self, context: TraceContext, status: SpanStatus) -> Result<()> {
        if status == SpanStatus::Running {
            return Err(invalid("span settlement requires terminal status"));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let changed = transaction.execute(
            "UPDATE trace_spans SET status=?1 WHERE span_id=?2 AND trace_id=?3 AND status='running'",
            params![
                status.as_str(),
                context.span_id.to_string(),
                context.trace_id.to_string()
            ],
        )?;
        if changed != 1 {
            return Err(invalid("span is missing, mismatched, or already terminal"));
        }
        let sequence: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(sequence),0)+1 FROM trace_events WHERE span_id=?1",
            params![context.span_id.to_string()],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO trace_events(span_id,sequence,kind,subject,evidence_sha256,observed_unix)
             VALUES(?1,?2,'terminal',?3,?4,?5)",
            params![
                context.span_id.to_string(),
                sequence,
                status.as_str(),
                format!("{:064x}", 0),
                now_unix() as i64
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn append(
        &self,
        context: TraceContext,
        kind: TraceEventKind,
        subject: &str,
        evidence_sha256: String,
    ) -> Result<TraceEvent> {
        validate_label(subject, "event subject")?;
        validate_sha256(&evidence_sha256)?;
        let span = self.span(context)?;
        if span.status != SpanStatus::Running {
            return Err(invalid("cannot append to terminal span"));
        }
        let sequence: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(sequence),0)+1 FROM trace_events WHERE span_id=?1",
            params![context.span_id.to_string()],
            |row| row.get(0),
        )?;
        let kind_text = serde_json::to_string(&kind)?;
        let kind_text = kind_text.trim_matches('"');
        let event = TraceEvent {
            context,
            sequence: sequence as u64,
            kind,
            subject: subject.into(),
            evidence_sha256,
            observed_unix: now_unix(),
        };
        self.conn.execute(
            "INSERT INTO trace_events(span_id,sequence,kind,subject,evidence_sha256,observed_unix)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                context.span_id.to_string(),
                sequence,
                kind_text,
                event.subject,
                event.evidence_sha256,
                event.observed_unix as i64
            ],
        )?;
        Ok(event)
    }

    pub fn span(&self, context: TraceContext) -> Result<TraceSpan> {
        self.conn
            .query_row(
                "SELECT trace_id,parent_span_id,subsystem,subject,status FROM trace_spans
                 WHERE span_id=?1",
                params![context.span_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| invalid("span does not exist"))
            .and_then(|(trace_id, parent, subsystem, subject, status)| {
                let persisted = TraceContext::new(
                    TraceId::parse(&trace_id)?,
                    context.span_id,
                    parent.as_deref().map(SpanId::parse).transpose()?,
                );
                if persisted != context {
                    return Err(invalid("trace context does not match persisted span"));
                }
                Ok(TraceSpan {
                    context,
                    subsystem,
                    subject,
                    status: SpanStatus::parse(&status)?,
                })
            })
    }

    pub fn events(&self, context: TraceContext) -> Result<Vec<TraceEvent>> {
        self.span(context)?;
        let mut statement = self.conn.prepare(
            "SELECT sequence,kind,subject,evidence_sha256,observed_unix FROM trace_events
             WHERE span_id=?1 ORDER BY sequence",
        )?;
        let rows = statement.query_map(params![context.span_id.to_string()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (sequence, kind, subject, evidence_sha256, observed_unix) = row?;
            Ok(TraceEvent {
                context,
                sequence: sequence as u64,
                kind: serde_json::from_str(&format!("\"{kind}\""))?,
                subject,
                evidence_sha256,
                observed_unix: observed_unix as u64,
            })
        })
        .collect()
    }
}

fn validate_label(value: &str, field: &str) -> Result<()> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(invalid(format!("{field} is invalid")));
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<()> {
    if !optimus_crypto::is_sha256_hex(value) {
        return Err(invalid("evidence hash must be SHA-256 hex"));
    }
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
