//! Operator durability inventory (P18).
//!
//! Multi-DB homes do not share a transaction. Doctor reports schema/version
//! presence, quarantine, and the backup file set operators must copy together.

use std::path::Path;

use optimus_graph::Store;
use optimus_runtime::{CampaignStore, CAMPAIGN_SCHEMA_VERSION};
use rusqlite::Connection;
use serde::Serialize;

/// Expected work-graph store schema after current migrations.
pub const WORK_GRAPH_SCHEMA_VERSION: &str = "7";
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
        ("project-authority.json", "project_root_authority"),
        ("artifacts", "content_addressed_artifacts_dir"),
    ]
}

fn meta_schema_version(path: &Path) -> Result<Option<String>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    // Prefer meta table; fall back to absence.
    let has_meta: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='meta'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has_meta {
        return Ok(None);
    }
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .ok();
    Ok(value)
}

fn file_row(
    home: &Path,
    id: &str,
    rel: &str,
    expected: Option<&str>,
    open_ok: Option<Result<Option<String>, String>>,
) -> DbInventoryRow {
    let absolute = home.join(rel);
    let present = absolute.exists();
    let (schema_version, ok, detail) = match open_ok {
        None if !present => (None, true, "absent (created on first use)".into()),
        None => (None, true, "present".into()),
        Some(Ok(version)) => {
            let ok = match (expected, version.as_deref()) {
                (Some(exp), Some(got)) => exp == got,
                (Some(_), None) if present => false,
                _ => true,
            };
            let detail = if !present {
                "absent (created on first use)".into()
            } else if let (Some(exp), Some(got)) = (expected, version.as_deref()) {
                if exp == got {
                    format!("schema_version={got}")
                } else {
                    format!("schema skew: got {got}, expected {exp}")
                }
            } else if let Some(got) = version.as_deref() {
                format!("schema_version={got}")
            } else {
                "present (no meta.schema_version)".into()
            };
            (version, ok, detail)
        }
        Some(Err(err)) => (None, false, format!("open/inspect failed: {err}")),
    };
    DbInventoryRow {
        id: id.into(),
        relative_path: rel.into(),
        absolute_path: absolute.display().to_string(),
        present,
        schema_version,
        expected_schema: expected.map(str::to_string),
        ok,
        detail,
    }
}

/// Build multi-DB inventory + quarantine report for an Optimus home.
pub fn inventory(home: &Path, product_version: &str) -> DoctorInventory {
    let mut issues = Vec::new();
    let mut databases = Vec::new();

    // Work graph / campaigns (optimus.db)
    let optimus_path = home.join("optimus.db");
    let work_graph = if optimus_path.exists() {
        match Store::open(&optimus_path) {
            Ok(store) => match store.schema_version() {
                Ok(v) => Some(Ok(Some(v))),
                Err(e) => Some(Err(e.to_string())),
            },
            Err(e) => Some(Err(e.to_string())),
        }
    } else {
        Some(Ok(None))
    };
    databases.push(file_row(
        home,
        "work_graph",
        "optimus.db",
        Some(WORK_GRAPH_SCHEMA_VERSION),
        work_graph,
    ));

    let campaign = if optimus_path.exists() || home.exists() {
        match CampaignStore::open(home) {
            Ok(store) => match store.schema_version() {
                Ok(v) => Some(Ok(Some(v.to_string()))),
                Err(e) => Some(Err(e.to_string())),
            },
            Err(e) => Some(Err(e.to_string())),
        }
    } else {
        Some(Ok(None))
    };
    // Campaign schema lives inside optimus.db; report as logical plane.
    let mut campaign_row = file_row(
        home,
        "campaigns",
        "optimus.db",
        Some(&CAMPAIGN_SCHEMA_VERSION.to_string()),
        campaign,
    );
    campaign_row.detail = format!("campaign plane in optimus.db; {}", campaign_row.detail);
    databases.push(campaign_row);

    for (id, rel, expected) in [
        ("sessions", "sessions.db", None),
        ("memory", "memory.db", Some(MEMORY_SCHEMA_VERSION)),
        ("skills", "skills.db", None),
        ("execution", "execution.db", None),
        ("cron", "cron.db", None),
        ("gateway", "gateway/gateway.db", None),
    ] {
        let open = Some(meta_schema_version(&home.join(rel)));
        databases.push(file_row(home, id, rel, expected, open));
    }

    let mut quarantined_jobs = Vec::new();
    if optimus_path.exists() {
        if let Ok(store) = Store::open(&optimus_path) {
            if let Ok(rows) = store.list_quarantined_jobs() {
                for row in rows {
                    quarantined_jobs.push(QuarantineRow {
                        job_id: row.job_id.to_string(),
                        reason: row.reason,
                    });
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
            "External channel exactly-once delivery is out of architecture Durability S+++ scope.".into(),
            "See docs/architecture/durability-and-backup.md.".into(),
        ],
    }
}

pub fn print_inventory_text(report: &DoctorInventory) {
    println!(
        "optimus {} — durability doctor (P18)",
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

