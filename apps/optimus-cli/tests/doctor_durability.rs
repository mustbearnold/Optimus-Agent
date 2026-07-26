//! P18 doctor multi-DB inventory + backup-list CLI smoke.

use std::process::Command;

use tempfile::tempdir;

#[test]
fn doctor_inventory_json_lists_core_databases() {
    let home = tempdir().unwrap();
    let exe = env!("CARGO_BIN_EXE_optimus");
    let output = Command::new(exe)
        .args(["--home", home.path().to_str().unwrap(), "doctor", "--json"])
        .output()
        .expect("run doctor");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert!(value["databases"].as_array().unwrap().len() >= 6);
    assert!(value["backup_paths"].as_array().unwrap().len() >= 10);
    assert!(value["scope"].as_str().unwrap().contains("process-local"));
    let ids: Vec<&str> = value["databases"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert!(ids.contains(&"work_graph"));
    assert!(ids.contains(&"campaigns"));
    assert!(ids.contains(&"sessions"));
    assert!(ids.contains(&"memory"));
}

#[test]
fn doctor_backup_list_includes_wal_and_gateway() {
    let home = tempdir().unwrap();
    let exe = env!("CARGO_BIN_EXE_optimus");
    let output = Command::new(exe)
        .args([
            "--home",
            home.path().to_str().unwrap(),
            "doctor",
            "backup-list",
            "--json",
        ])
        .output()
        .expect("run backup-list");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    let rels: Vec<&str> = value["paths"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["relative_path"].as_str())
        .collect();
    assert!(rels.contains(&"optimus.db"));
    assert!(rels.contains(&"sessions.db"));
    assert!(rels.contains(&"gateway/gateway.db"));
    assert!(rels.contains(&"workflow-runs.db"));
    assert!(rels.contains(&"agent-invocations.db"));
    assert!(rels.contains(&"agent-registry.db"));
    assert!(rels.iter().any(|r| r.ends_with("-wal")));
}

#[test]
fn doctor_exits_nonzero_on_work_graph_schema_skew_without_migrating() {
    let home = tempdir().unwrap();
    let exe = env!("CARGO_BIN_EXE_optimus");
    // Empty home: absent DBs are OK (read-only doctor).
    let ok = Command::new(exe)
        .args(["--home", home.path().to_str().unwrap(), "doctor", "--json"])
        .output()
        .expect("doctor empty");
    assert!(
        ok.status.success(),
        "empty home should pass; stderr={}",
        String::from_utf8_lossy(&ok.stderr)
    );

    let db = home.path().join("optimus.db");
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO meta(key,value) VALUES('schema_version','3');",
        )
        .unwrap();
    }
    let output = Command::new(exe)
        .args(["--home", home.path().to_str().unwrap(), "doctor", "--json"])
        .output()
        .expect("doctor skew");
    assert!(
        !output.status.success(),
        "doctor must fail-closed on schema skew; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(
        body.contains("schema skew") || body.contains("got 3"),
        "expected skew detail: {body}"
    );
    // Must not have migrated the on-disk version.
    let conn = rusqlite::Connection::open(&db).unwrap();
    let version: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, "3", "doctor must not migrate on diagnose");
}

#[test]
fn doctor_exits_nonzero_when_quarantine_rows_present() {
    let home = tempdir().unwrap();
    let exe = env!("CARGO_BIN_EXE_optimus");
    let db = home.path().join("optimus.db");
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO meta(key,value) VALUES('schema_version','7');
             CREATE TABLE campaign_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO campaign_meta(key,value) VALUES('schema_version','4');
             CREATE TABLE job_quarantine(job_id TEXT PRIMARY KEY, reason TEXT NOT NULL);
             INSERT INTO job_quarantine(job_id, reason) VALUES('00000000-0000-0000-0000-000000000001','test quarantine');",
        )
        .unwrap();
    }
    let output = Command::new(exe)
        .args(["--home", home.path().to_str().unwrap(), "doctor", "--json"])
        .output()
        .expect("doctor quarantine");
    assert!(
        !output.status.success(),
        "quarantine must fail-closed; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let body = String::from_utf8_lossy(&output.stdout);
    assert!(
        body.contains("quarantine") || body.contains("00000000-0000-0000-0000-000000000001"),
        "expected quarantine in report: {body}"
    );
}
