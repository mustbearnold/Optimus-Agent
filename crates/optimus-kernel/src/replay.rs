//! Immutable content-addressed replay fixtures and a zero-effect offline executor.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{ExecutionManifest, ExecutionStatus, KernelError, Result, TraceId};

pub const REPLAY_BUNDLE_VERSION: u16 = 1;
pub const REPLAY_REPORT_VERSION: u16 = 1;
pub const MAX_REPLAY_FIXTURES: usize = 64;
pub const MAX_REPLAY_FIXTURE_BYTES: usize = 1_048_576;
pub const MAX_REPLAY_BUNDLE_BYTES: usize = 4_194_304;

fn invalid(reason: impl Into<String>) -> KernelError {
    KernelError::Model(format!("invalid replay evidence: {}", reason.into()))
}

fn validate_sha256(value: &str, field: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(format!("{field} must be a SHA-256 hex digest")));
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct ReplayBundleId(Uuid);

impl ReplayBundleId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    fn parse(value: &str) -> Result<Self> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

impl Default for ReplayBundleId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ReplayBundleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct FixtureId(String);

impl FixtureId {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into().to_ascii_lowercase();
        validate_sha256(&value, "fixture id")?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum FixtureKind {
    ModelResponse,
    ToolOutcome,
    StageMetadata,
    TerminalEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayFixture {
    pub id: FixtureId,
    pub stage: u32,
    pub kind: FixtureKind,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

impl ReplayFixture {
    pub fn new(
        stage: u32,
        kind: FixtureKind,
        media_type: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<Self> {
        let fixture = Self {
            id: FixtureId::parse(digest(&bytes))?,
            stage,
            kind,
            media_type: media_type.into(),
            bytes,
        };
        fixture.validate()?;
        Ok(fixture)
    }

    fn validate(&self) -> Result<()> {
        if self.stage == 0 {
            return Err(invalid("fixture stage must be nonzero"));
        }
        if self.media_type.is_empty()
            || self.media_type.len() > 255
            || self.media_type.chars().any(char::is_control)
        {
            return Err(invalid("fixture media type is invalid"));
        }
        if self.bytes.is_empty() || self.bytes.len() > MAX_REPLAY_FIXTURE_BYTES {
            return Err(invalid("fixture byte length is outside policy"));
        }
        if digest(&self.bytes) != self.id.as_str() {
            return Err(invalid("fixture content hash mismatch"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayStage {
    pub sequence: u32,
    pub kind: FixtureKind,
    pub input_sha256: String,
    pub fixture_id: FixtureId,
}

impl ReplayStage {
    pub fn fixture(
        sequence: u32,
        kind: FixtureKind,
        input_sha256: String,
        fixture_id: FixtureId,
    ) -> Self {
        Self {
            sequence,
            kind,
            input_sha256,
            fixture_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayBundle {
    pub id: ReplayBundleId,
    pub version: u16,
    pub source_manifest_id: Uuid,
    pub trace_id: String,
    pub contract_sha256: String,
    pub tool_catalog_sha256: String,
    pub policy_sha256: String,
    pub expected_terminal_sha256: String,
    pub stages: Vec<ReplayStage>,
    pub fixtures: Vec<ReplayFixture>,
}

impl ReplayBundle {
    pub fn validate(&self) -> Result<()> {
        if self.version != REPLAY_BUNDLE_VERSION {
            return Err(invalid("unsupported replay bundle version"));
        }
        TraceId::parse(&self.trace_id).map_err(|_| invalid("trace identity is invalid"))?;
        for (value, field) in [
            (&self.contract_sha256, "contract hash"),
            (&self.tool_catalog_sha256, "tool catalog hash"),
            (&self.policy_sha256, "policy hash"),
            (&self.expected_terminal_sha256, "terminal hash"),
        ] {
            validate_sha256(value, field)?;
        }
        if self.fixtures.is_empty() || self.fixtures.len() > MAX_REPLAY_FIXTURES {
            return Err(invalid("fixture count is outside policy"));
        }
        if self.stages.is_empty() || self.stages.len() > MAX_REPLAY_FIXTURES {
            return Err(invalid("stage count is outside policy"));
        }
        let total = self.fixtures.iter().try_fold(0usize, |total, fixture| {
            fixture.validate()?;
            total
                .checked_add(fixture.bytes.len())
                .ok_or_else(|| invalid("fixture byte total overflow"))
        })?;
        if total > MAX_REPLAY_BUNDLE_BYTES {
            return Err(invalid("bundle exceeds byte policy"));
        }
        let fixture_ids = self
            .fixtures
            .iter()
            .map(|fixture| fixture.id.clone())
            .collect::<BTreeSet<_>>();
        if fixture_ids.len() != self.fixtures.len() {
            return Err(invalid("duplicate fixture identity"));
        }
        let mut referenced = BTreeSet::new();
        for (index, stage) in self.stages.iter().enumerate() {
            if stage.sequence as usize != index + 1 {
                return Err(invalid("stages must be contiguous and ordered"));
            }
            validate_sha256(&stage.input_sha256, "stage input hash")?;
            let fixture = self
                .fixtures
                .iter()
                .find(|fixture| fixture.id == stage.fixture_id)
                .ok_or_else(|| invalid("stage fixture is missing"))?;
            if fixture.stage != stage.sequence || fixture.kind != stage.kind {
                return Err(invalid("stage fixture metadata mismatch"));
            }
            if !referenced.insert(stage.fixture_id.clone()) {
                return Err(invalid("fixture is referenced more than once"));
            }
        }
        if referenced != fixture_ids {
            return Err(invalid("bundle contains unreferenced fixtures"));
        }
        let terminal = self
            .fixtures
            .iter()
            .find(|fixture| fixture.id.as_str() == self.expected_terminal_sha256)
            .ok_or_else(|| invalid("expected terminal fixture is missing"))?;
        if terminal.kind != FixtureKind::TerminalEvidence {
            return Err(invalid("expected terminal fixture has wrong kind"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayPlan {
    pub bundle_id: ReplayBundleId,
    pub source_manifest_id: Uuid,
    pub trace_id: String,
    pub stages: Vec<ReplayStage>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayExecutionStatus {
    Succeeded,
    Failed,
    Cancelled,
    Ambiguous,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayExecutionReport {
    pub id: Uuid,
    pub version: u16,
    pub bundle_id: ReplayBundleId,
    pub source_manifest_id: Uuid,
    pub trace_id: String,
    pub status: ReplayExecutionStatus,
    pub completed_stages: usize,
    pub blockers: Vec<String>,
    pub terminal_sha256: String,
    pub report_sha256: String,
}

pub struct ReplayStore {
    conn: Connection,
}

impl ReplayStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS replay_bundles(
               id TEXT PRIMARY KEY,source_manifest_id TEXT NOT NULL UNIQUE,
               bundle_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS replay_fixtures(
               bundle_id TEXT NOT NULL REFERENCES replay_bundles(id) ON DELETE RESTRICT,
               fixture_id TEXT NOT NULL,stage INTEGER NOT NULL,kind TEXT NOT NULL,
               byte_length INTEGER NOT NULL,bytes BLOB NOT NULL,
               PRIMARY KEY(bundle_id,fixture_id)
             );
             CREATE TABLE IF NOT EXISTS replay_reports(
               id TEXT PRIMARY KEY,bundle_id TEXT NOT NULL UNIQUE REFERENCES replay_bundles(id),
               report_json TEXT NOT NULL
             );",
        )?;
        Ok(Self { conn })
    }

    pub fn insert_bundle(&self, source: &ExecutionManifest, bundle: &ReplayBundle) -> Result<()> {
        bundle.validate()?;
        if source.status == ExecutionStatus::Running {
            return Err(invalid("source manifest must be terminal"));
        }
        if bundle.source_manifest_id != source.id
            || bundle.tool_catalog_sha256 != source.tool_catalog_sha256
            || bundle.policy_sha256 != source.policy_sha256
        {
            return Err(invalid("bundle does not match source manifest"));
        }
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO replay_bundles(id,source_manifest_id,bundle_json) VALUES(?1,?2,?3)",
            params![
                bundle.id.to_string(),
                source.id.to_string(),
                serde_json::to_string(bundle)?
            ],
        )?;
        for fixture in &bundle.fixtures {
            transaction.execute(
                "INSERT INTO replay_fixtures(bundle_id,fixture_id,stage,kind,byte_length,bytes)
                 VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    bundle.id.to_string(),
                    fixture.id.as_str(),
                    fixture.stage as i64,
                    serde_json::to_string(&fixture.kind)?,
                    fixture.bytes.len() as i64,
                    fixture.bytes
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn bundle(&self, id: ReplayBundleId) -> Result<ReplayBundle> {
        let text = self
            .conn
            .query_row(
                "SELECT bundle_json FROM replay_bundles WHERE id=?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| invalid("replay bundle does not exist"))?;
        let mut bundle: ReplayBundle = serde_json::from_str(&text)?;
        let mut statement = self.conn.prepare(
            "SELECT fixture_id,byte_length,bytes FROM replay_fixtures
             WHERE bundle_id=?1 ORDER BY stage,kind,fixture_id",
        )?;
        let rows = statement.query_map(params![id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        let stored = rows
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(id, length, bytes)| (id, (length, bytes)))
            .collect::<BTreeMap<_, _>>();
        if stored.len() != bundle.fixtures.len() {
            return Err(invalid("persisted fixture set is incomplete"));
        }
        for fixture in &mut bundle.fixtures {
            let (length, bytes) = stored
                .get(fixture.id.as_str())
                .ok_or_else(|| invalid("persisted fixture identity is missing"))?;
            if *length < 0 || *length as usize != bytes.len() {
                return Err(invalid("persisted fixture byte length mismatch"));
            }
            fixture.bytes.clone_from(bytes);
        }
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn plan(
        &self,
        source: &ExecutionManifest,
        bundle_id: ReplayBundleId,
        trace_id: &str,
    ) -> Result<ReplayPlan> {
        let bundle = self.bundle(bundle_id)?;
        if source.status == ExecutionStatus::Running
            || source.id != bundle.source_manifest_id
            || source.tool_catalog_sha256 != bundle.tool_catalog_sha256
            || source.policy_sha256 != bundle.policy_sha256
            || trace_id != bundle.trace_id
        {
            return Err(invalid(
                "source, policy, catalog, or trace drift blocks replay",
            ));
        }
        Ok(ReplayPlan {
            bundle_id,
            source_manifest_id: source.id,
            trace_id: trace_id.into(),
            stages: bundle.stages,
        })
    }

    pub fn execute(&self, plan: &ReplayPlan) -> Result<ReplayExecutionReport> {
        let inputs = plan
            .stages
            .iter()
            .map(|stage| (stage.sequence, stage.input_sha256.clone()))
            .collect::<BTreeMap<_, _>>();
        self.execute_with_input_hashes(plan, &inputs)
    }

    pub fn execute_with_input_hashes(
        &self,
        plan: &ReplayPlan,
        input_hashes: &BTreeMap<u32, String>,
    ) -> Result<ReplayExecutionReport> {
        let bundle = self.bundle(plan.bundle_id)?;
        if plan.source_manifest_id != bundle.source_manifest_id
            || plan.trace_id != bundle.trace_id
            || plan.stages != bundle.stages
        {
            return Err(invalid("replay plan does not match immutable bundle"));
        }
        let mut completed_stages = 0usize;
        let mut blockers = Vec::new();
        for stage in &plan.stages {
            match input_hashes.get(&stage.sequence) {
                Some(actual) if actual == &stage.input_sha256 => {}
                Some(_) => {
                    blockers.push(format!("stage_{}_input_hash_mismatch", stage.sequence));
                    break;
                }
                None => {
                    blockers.push(format!("stage_{}_input_hash_missing", stage.sequence));
                    break;
                }
            }
            let fixture = bundle
                .fixtures
                .iter()
                .find(|fixture| fixture.id == stage.fixture_id)
                .ok_or_else(|| invalid("planned fixture is missing"))?;
            fixture.validate()?;
            completed_stages += 1;
        }
        let id = Uuid::new_v4();
        let mut report = ReplayExecutionReport {
            id,
            version: REPLAY_REPORT_VERSION,
            bundle_id: plan.bundle_id,
            source_manifest_id: plan.source_manifest_id,
            trace_id: plan.trace_id.clone(),
            status: if blockers.is_empty() {
                ReplayExecutionStatus::Succeeded
            } else {
                ReplayExecutionStatus::Failed
            },
            completed_stages,
            blockers,
            terminal_sha256: bundle.expected_terminal_sha256,
            report_sha256: String::new(),
        };
        report.report_sha256 = digest(&serde_json::to_vec(&report)?);
        self.conn.execute(
            "INSERT INTO replay_reports(id,bundle_id,report_json) VALUES(?1,?2,?3)",
            params![
                report.id.to_string(),
                report.bundle_id.to_string(),
                serde_json::to_string(&report)?
            ],
        )?;
        Ok(report)
    }

    pub fn report(&self, id: Uuid) -> Result<ReplayExecutionReport> {
        let text = self
            .conn
            .query_row(
                "SELECT report_json FROM replay_reports WHERE id=?1",
                params![id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| invalid("replay report does not exist"))?;
        let report: ReplayExecutionReport = serde_json::from_str(&text)?;
        if report.version != REPLAY_REPORT_VERSION {
            return Err(invalid("unsupported replay report version"));
        }
        let mut unhashed = report.clone();
        unhashed.report_sha256.clear();
        if digest(&serde_json::to_vec(&unhashed)?) != report.report_sha256 {
            return Err(invalid("replay report hash mismatch"));
        }
        Ok(report)
    }
}

#[allow(dead_code)]
fn _parse_bundle_id(value: &str) -> Result<ReplayBundleId> {
    ReplayBundleId::parse(value)
}
