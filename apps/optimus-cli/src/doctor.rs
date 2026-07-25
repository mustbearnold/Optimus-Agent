//! Operator durability inventory (P18).
//!
//! Multi-DB homes do not share a transaction. Doctor is **read-only**: it never
//! creates or migrates databases. It reports presence, advertised schema/meta
//! versions, Work Graph quarantine, and the backup path set.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

/// Expected work-graph store schema after current migrations.
pub const WORK_GRAPH_SCHEMA_VERSION: &str = "7";
/// Campaign plane schema embedded in optimus.db.
pub const CAMPAIGN_SCHEMA_VERSION: &str = "4";
/// Memory meta schema written on open.
pub const MEMORY_SCHEMA_VERSION: &str = "2";

#[derive(Debug, Clone, Serialize)]
pub struct DbInventoryRow {
    pub id: String,
    pub relative_path: String,
    pub absolute_path: String,
    pub present: bool,
    pub schema_version: Option<String>,
    pub expected_schema: Option<String>,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuarantineRow {
    pub job_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorInventory {
    pub product_version: String,
    pub home: String,
    pub scope: String,
    pub databases: Vec<DbInventoryRow>,
    pub quarantined_jobs: Vec<QuarantineRow>,
    pub backup_paths: Vec<String>,
    pub issues: Vec<String>,
    /// Doctor never migrates; true when any inspected path failed closed.
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupList {
    pub home: String,
    pub scope: String,
    pub paths: Vec<BackupPath>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupPath {
    pub relative_path: String,
    pub absolute_path: String,
    pub present: bool,
    pub kind: String,
}

/// Fixed relative paths that constitute a process-local durability backup set.
pub fn backup_relative_paths() -> &'static [(&'static str, &'static str)] {
    &[
        ("optimus.db", "work_graph_and_campaigns"),
        ("optimus.db-wal", "sqlite_wal_if_present"),
        ("optimus.db-shm", "sqlite_shm_if_present"),
        ("sessions.db", "session_transcripts_and_effect_links"),
        ("sessions.db-wal", "sqlite_wal_if_present"),
        ("sessions.db-shm", "sqlite_shm_if_present"),
        ("memory.db", "metamemory_claims"),
        ("memory.db-wal", "sqlite_wal_if_present"),
        ("memory.db-shm", "sqlite_shm_if_present"),
        ("skills.db", "skills_registry"),
        ("skills.db-wal", "sqlite_wal_if_present"),
        ("skills.db-shm", "sqlite_shm_if_present"),
        ("execution.db", "execution_manifests"),
        ("execution.db-wal", "sqlite_wal_if_present"),
        ("execution.db-shm", "sqlite_shm_if_present"),
        ("cron.db", "cron_schedules_and_leases"),
        ("cron.db-wal", "sqlite_wal_if_present"),
        ("cron.db-shm", "sqlite_shm_if_present"),
        ("gateway/gateway.db", "gateway_delivery_authority"),
        ("gateway/gateway.db-wal", "sqlite_wal_if_present"),
        ("gateway/gateway.db-shm", "sqlite_shm_if_present"),
        ("gateway/inbox", "gateway_adapter_inbox_dir"),
        ("gateway/outbox", "gateway_adapter_outbox_dir"),
        ("gateway/processed", "gateway_adapter_processed_dir"),
        ("gateway/failed", "gateway_adapter_failed_dir"),
        ("workflow-runs.db", "workflow_run_ledger"),
        ("workflow-runs.db-wal", "sqlite_wal_if_present"),
        ("workflow-runs.db-shm", "sqlite_shm_if_present"),
        ("agent-invocations.db", "agent_invocation_ledger"),
        ("agent-invocations.db-wal", "sqlite_wal_if_present"),
        ("agent-invocations.db-shm", "sqlite_shm_if_present"),
        ("workflow-registry.db", "workflow_definition_registry"),
        ("workflow-registry.db-wal", "sqlite_wal_if_present"),
        ("workflow-registry.db-shm", "sqlite_shm_if_present"),
        ("agent-registry.db", "agent_descriptor_registry"),
        ("agent-registry.db-wal", "sqlite_wal_if_present"),
        ("agent-registry.db-shm", "sqlite_shm_if_present"),
        ("routing.db", "routing_telemetry"),
        ("routing.db-wal", "sqlite_wal_if_present"),
        ("routing.db-shm", "sqlite_shm_if_present"),
        ("project-authority.json", "project_root_authority"),
        ("settings.json", "product_settings"),
        // auth.json intentionally omitted from minimum set (secrets); whole-home
        // copy still preferred when operators need OAuth tokens restored.
        ("artifacts", "content_addressed_artifacts_dir"),
    ]
}

fn open_readonly(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())
}

fn meta_value(conn: &Connection, table: &str, key: &str) -> Result<Option<String>, String> {
    let has_table: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has_table {
        return Ok(None);
    }
    let value: Option<String> = conn
        .query_row(
            &format!("SELECT value FROM {table} WHERE key=?1"),
            [key],
            |row| row.get(0),
        )
        .optional_map_err(|e| e.to_string())?;
    Ok(value)
}

trait OptionalMapErr<T> {
    fn optional_map_err(self, f: impl FnOnce(rusqlite::Error) -> String) -> Result<T, String>;
}

impl<T> OptionalMapErr<Option<T>> for rusqlite::Result<T> {
    fn optional_map_err(self, f: impl FnOnce(rusqlite::Error) -> String) -> Result<Option<T>, String> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(f(e)),
        }
    }
}

fn inspect_meta_db(
    home: &Path,
    id: &str,
    rel: &str,
    meta_table: &str,
    expected: Option<&str>,
) -> DbInventoryRow {
    let absolute = home.join(rel);
    if !absolute.is_file() {
        return DbInventoryRow {
            id: id.into(),
            relative_path: rel.into(),
            absolute_path: absolute.display().to_string(),
            present: false,
            schema_version: None,
            expected_schema: expected.map(str::to_string),
            ok: true,
            detail: "absent (created on first use)".into(),
        };
    }
    match open_readonly(&absolute) {
        Ok(conn) => match meta_value(&conn, meta_table, "schema_version") {
            Ok(version) => {
                let ok = match (expected, version.as_deref()) {
                    (Some(exp), Some(got)) => exp == got,
                    (Some(_), None) => false,
                    _ => true,
                };
                let detail = match (expected, version.as_deref()) {
                    (Some(exp), Some(got)) if exp == got => format!("schema_version={got}"),
                    (Some(exp), Some(got)) => {
                        format!("schema skew: got {got}, expected {exp}")
                    }
                    (Some(exp), None) => {
                        format!("missing {meta_table}.schema_version (expected {exp})")
                    }
                    (_, Some(got)) => format!("schema_version={got}"),
                    _ => format!("present (no {meta_table}.schema_version)"),
                };
                DbInventoryRow {
                    id: id.into(),
                    relative_path: rel.into(),
                    absolute_path: absolute.display().to_string(),
                    present: true,
                    schema_version: version,
                    expected_schema: expected.map(str::to_string),
                    ok,
                    detail,
                }
            }
            Err(err) => DbInventoryRow {
                id: id.into(),
                relative_path: rel.into(),
                absolute_path: absolute.display().to_string(),
                present: true,
                schema_version: None,
                expected_schema: expected.map(str::to_string),
                ok: false,
                detail: format!("inspect failed: {err}"),
            },
        },
        Err(err) => DbInventoryRow {
            id: id.into(),
            relative_path: rel.into(),
            absolute_path: absolute.display().to_string(),
            present: true,
            schema_version: None,
            expected_schema: expected.map(str::to_string),
            ok: false,
            detail: format!("open failed (read-only): {err}"),
        },
    }
}

fn list_quarantine_readonly(path: &Path) -> Result<Vec<QuarantineRow>, String> {
    let conn = open_readonly(path)?;
    let has: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='job_quarantine'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare("SELECT job_id, reason FROM job_quarantine ORDER BY job_id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(QuarantineRow {
                job_id: row.get(0)?,
                reason: row.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// Build multi-DB inventory + quarantine report for an Optimus home (read-only).
pub fn inventory(home: &Path, product_version: &str) -> DoctorInventory {
    let mut issues = Vec::new();
    let mut databases = Vec::new();

    databases.push(inspect_meta_db(
        home,
        "work_graph",
        "optimus.db",
        "meta",
        Some(WORK_GRAPH_SCHEMA_VERSION),
    ));
    // Campaign schema lives in the same file under campaign_meta.
    let mut campaign = inspect_meta_db(
        home,
        "campaigns",
        "optimus.db",
        "campaign_meta",
        Some(CAMPAIGN_SCHEMA_VERSION),
    );
    if campaign.present {
        campaign.detail = format!("campaign plane in optimus.db; {}", campaign.detail);
    }
    databases.push(campaign);

    for (id, rel, expected) in [
        ("sessions", "sessions.db", None),
        ("memory", "memory.db", Some(MEMORY_SCHEMA_VERSION)),
        ("skills", "skills.db", None),
        ("execution", "execution.db", None),
        ("cron", "cron.db", None),
        ("gateway", "gateway/gateway.db", None),
        ("workflow_runs", "workflow-runs.db", None),
        ("agent_invocations", "agent-invocations.db", None),
        ("workflow_registry", "workflow-registry.db", None),
        ("agent_registry", "agent-registry.db", None),
        ("routing", "routing.db", None),
    ] {
        databases.push(inspect_meta_db(home, id, rel, "meta", expected));
    }

    let mut quarantined_jobs = Vec::new();
    let optimus_path = home.join("optimus.db");
    if optimus_path.is_file() {
        match list_quarantine_readonly(&optimus_path) {
            Ok(rows) => quarantined_jobs = rows,
            Err(err) => {
                issues.push(format!("quarantine scan failed: {err}"));
                // Mark work_graph row non-ok if still marked ok.
                if let Some(row) = databases.iter_mut().find(|r| r.id == "work_graph") {
                    row.ok = false;
                    row.detail = format!("{}; quarantine scan failed: {err}", row.detail);
                }
            }
        }
    }

    for db in &databases {
        if !db.ok {
            issues.push(format!("{}: {}", db.id, db.detail));
        }
    }
    if !quarantined_jobs.is_empty() {
        issues.push(format!(
            "{} quarantined job(s) in optimus.db (fail-closed until repaired)",
            quarantined_jobs.len()
        ));
    }

    let backup_paths = backup_relative_paths()
        .iter()
        .map(|(rel, _)| rel.to_string())
        .collect();

    DoctorInventory {
        product_version: product_version.into(),
        home: home.display().to_string(),
        scope: "process-local / local SQLite durability (external messaging exactly-once out of S+++ scope)".into(),
        databases,
        quarantined_jobs,
        backup_paths,
        issues,
        read_only: true,
    }
}

pub fn backup_list(home: &Path) -> BackupList {
    let paths = backup_relative_paths()
        .iter()
        .map(|(rel, kind)| {
            let absolute = home.join(rel);
            BackupPath {
                relative_path: (*rel).into(),
                absolute_path: absolute.display().to_string(),
                present: absolute.exists(),
                kind: (*kind).into(),
            }
        })
        .collect();
    BackupList {
        home: home.display().to_string(),
        scope: "process-local / local SQLite durability".into(),
        paths,
        notes: vec![
            "Copy the whole home directory when possible; at minimum every present path above.".into(),
            "Stop writers (CLI/desktop/gateway) before cold copy; include -wal/-shm when present.".into(),
            "auth.json is intentionally omitted from the minimum set (secrets); include only if restoring OAuth.".into(),
            "External channel exactly-once delivery is out of architecture Durability S+++ scope.".into(),
            "Doctor is read-only: it never migrates or creates databases.".into(),
            "See docs/architecture/durability-and-backup.md.".into(),
        ],
    }
}

pub fn print_inventory_text(report: &DoctorInventory) {
    println!(
        "optimus {} — durability doctor (P18, read-only)",
        report.product_version
    );
    println!("home: {}", report.home);
    println!("scope: {}", report.scope);
    println!("databases:");
    for db in &report.databases {
        let mark = if db.ok { "ok" } else { "ISSUE" };
        println!(
            "  [{mark}] {id}  {rel}  {detail}",
            id = db.id,
            rel = db.relative_path,
            detail = db.detail
        );
    }
    if report.quarantined_jobs.is_empty() {
        println!("quarantine: none");
    } else {
        println!("quarantine: {} job(s)", report.quarantined_jobs.len());
        for q in &report.quarantined_jobs {
            println!("  {} — {}", q.job_id, q.reason);
        }
    }
    if report.issues.is_empty() {
        println!("issues: none");
    } else {
        println!("issues:");
        for issue in &report.issues {
            println!("  - {issue}");
        }
    }
    println!(
        "backup-list: {} path patterns (run: optimus doctor backup-list)",
        report.backup_paths.len()
    );
}

pub fn print_backup_list_text(list: &BackupList) {
    println!("optimus durability backup set");
    println!("home: {}", list.home);
    println!("scope: {}", list.scope);
    for path in &list.paths {
        let mark = if path.present { "present" } else { "absent" };
        println!(
            "  [{mark}] {}  ({})",
            path.relative_path, path.kind
        );
    }
    for note in &list.notes {
        println!("note: {note}");
    }
}
