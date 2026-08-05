//! Toolchain bind tier for the Developer Full Access command envelope
//! (spec-014 R1-R3, ADR-0080). RED-GREEN tests: classed binds, credential
//! exclusion, skip-if-absent, deterministic exec resolution.

use std::path::{Path, PathBuf};

use optimus_policy::{CapabilityId, DeveloperAccessGrant, DeveloperCapabilities, DeveloperScope};
use optimus_runtime::toolchain::{toolchain_bind_list, BindMode};

fn grant(scope: DeveloperScope, terminal: bool) -> DeveloperAccessGrant {
    let mut capabilities = DeveloperCapabilities::default();
    if !terminal {
        capabilities.terminal_execution = false;
    }
    DeveloperAccessGrant {
        enabled: true,
        scope,
        capabilities,
        issued_unix: 1,
        ..Default::default()
    }
}

fn selected_repo(home: &Path) -> DeveloperScope {
    DeveloperScope::SelectedRepository {
        root: home.join("project").display().to_string(),
        root_hash: None,
    }
}

fn entry<'a>(binds: &'a [(PathBuf, BindMode)], suffix: &str) -> Option<&'a (PathBuf, BindMode)> {
    binds.iter().find(|(p, _)| p.ends_with(suffix))
}

#[test]
fn toolchain_binds_only_for_terminal_enabled_grant() {
    let home = PathBuf::from("/tmp/fake-home");
    assert!(
        toolchain_bind_list(&home, &grant(selected_repo(&home), false)).is_empty(),
        "no toolchain binds without terminal execution"
    );
    let mut disabled = grant(selected_repo(&home), true);
    disabled.enabled = false;
    assert!(toolchain_bind_list(&home, &disabled).is_empty());
    assert!(!toolchain_bind_list(&home, &grant(selected_repo(&home), true)).is_empty());
}

#[test]
fn toolchain_binds_never_include_credential_or_identity_paths() {
    let home = PathBuf::from("/tmp/fake-home");
    let binds = toolchain_bind_list(&home, &grant(selected_repo(&home), true));
    for path in [
        ".cargo/credentials.toml",
        ".cargo/config.toml",
        ".gitconfig",
        ".config/git",
        ".config/gh",
        ".ssh",
    ] {
        assert!(
            binds.iter().all(|(p, _)| !p.ends_with(path)),
            "credential/identity path {path} must never be bound, got {binds:?}"
        );
    }
}

#[test]
fn toolchain_binds_are_empty_for_entire_machine_scope() {
    let home = PathBuf::from("/tmp/fake-home");
    let binds = toolchain_bind_list(&home, &grant(DeveloperScope::EntireLocalMachine, true));
    assert!(
        binds.is_empty(),
        "full-machine scope needs no toolchain binds: {binds:?}"
    );
}

#[test]
fn rw_caches_and_ro_toolchains_are_classed_correctly() {
    let dir = std::env::temp_dir().join(format!("optimus-toolchain-class-{}", std::process::id()));
    let home = dir.join("home");
    for suffix in [".cargo/bin", ".rustup", ".cache/ms-playwright"] {
        std::fs::create_dir_all(home.join(suffix)).unwrap();
    }
    let binds = toolchain_bind_list(&home, &grant(selected_repo(&home), true));
    for (suffix, mode) in [
        (".cargo/registry", BindMode::Rw),
        (".cargo/git", BindMode::Rw),
        (".bun", BindMode::Rw),
        (".cache/cargo", BindMode::Rw),
        (".cache/bun", BindMode::Rw),
        (".cargo/bin", BindMode::Ro),
        (".rustup", BindMode::Ro),
        (".cache/ms-playwright", BindMode::Ro),
    ] {
        let e = entry(&binds, suffix).unwrap_or_else(|| panic!("missing {suffix}: {binds:?}"));
        assert_eq!(e.1, mode, "{suffix} must be {mode:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rw_entries_are_created_ro_entries_skip_when_absent() {
    let dir = std::env::temp_dir().join(format!("optimus-toolchain-test-{}", std::process::id()));
    let home = dir.join("home");
    std::fs::create_dir_all(home.join(".rustup")).unwrap();
    let binds = toolchain_bind_list(&home, &grant(selected_repo(&home), true));
    // ro: absent paths skipped, present kept
    assert!(
        entry(&binds, ".rustup").is_some(),
        "present ro path must be bound"
    );
    assert!(
        entry(&binds, "ms-playwright").is_none(),
        "absent ro path must be skipped"
    );
    // rw: absent paths created and bound
    assert!(
        home.join(".cargo/registry").is_dir(),
        "rw source must be host-created"
    );
    assert!(entry(&binds, ".cargo/registry").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rw_binds_precede_ro_binds() {
    let home = PathBuf::from("/tmp/fake-home");
    let binds = toolchain_bind_list(&home, &grant(selected_repo(&home), true));
    let rw_last = binds
        .iter()
        .rposition(|(_, m)| *m == BindMode::Rw)
        .unwrap_or(0);
    let ro_first = binds
        .iter()
        .position(|(_, m)| *m == BindMode::Ro)
        .unwrap_or(usize::MAX);
    assert!(
        rw_last < ro_first,
        "all rw binds must precede ro over-binds: {binds:?}"
    );
}

#[test]
fn capability_toggle_is_respected() {
    // Sanity: the terminal toggle is the one governing the toolchain tier.
    let home = PathBuf::from("/tmp/fake-home");
    let mut caps = DeveloperCapabilities::default();
    caps.terminal_execution = true;
    let grant = DeveloperAccessGrant {
        enabled: true,
        scope: selected_repo(&home),
        capabilities: caps,
        issued_unix: 1,
        ..Default::default()
    };
    assert!(grant
        .capabilities
        .allows(CapabilityId::ProcessProjectExecute));
    assert!(!toolchain_bind_list(&home, &grant).is_empty());
}
