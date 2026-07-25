//! P18 doctor multi-DB inventory + backup-list CLI smoke.

use std::process::Command;

use tempfile::tempdir;

#[test]
fn doctor_inventory_json_lists_core_databases() {
    let home = tempdir().unwrap();
    let exe = env!("CARGO_BIN_EXE_optimus");
    let output = Command::new(exe)
        .args([
            "--home",
            home.path().to_str().unwrap(),
            "doctor",
            "--json",
        ])
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
    assert!(value["scope"]
        .as_str()
        .unwrap()
        .contains("process-local"));
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
    assert!(rels.iter().any(|r| r.ends_with("-wal")));
}
