//! R30.6: what the classifier changes about a real broker decision.
//!
//! The unit tests in `command_class` prove the classification. These prove it
//! reaches the decision — that a project-scoped `Standard` grant, which is the
//! profile a durably trusted project runs at, stops covering a command that
//! leaves the project.

use optimus_policy::{
    build_effect_request, build_effect_request_for, ActionRequest, AuthorityDecision,
    AutonomyProfile, CapabilityBroker, CapabilityId, Externality, Reversibility,
};

fn request(program: &str, args: &[&str]) -> ActionRequest {
    let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    build_effect_request_for(
        "ProjectRunCommand",
        "deadbeef",
        Some("roothash".into()),
        format!("project run {program}"),
        None,
        Some((program, &args)),
    )
    .expect("ProjectRunCommand always maps to a capability")
}

fn decide(profile: AutonomyProfile, program: &str, args: &[&str]) -> AuthorityDecision {
    CapabilityBroker.decide(profile, &request(program, args))
}

fn allowed(decision: &AuthorityDecision) -> bool {
    matches!(decision, AuthorityDecision::Allow { .. })
}

#[test]
fn a_trusted_project_still_builds_and_tests_without_asking() {
    // The whole point of a durable Standard grant: routine work does not
    // re-prompt.
    for args in [
        vec!["build"],
        vec!["test", "--workspace"],
        vec!["clippy", "--", "-D", "warnings"],
    ] {
        let decision = decide(AutonomyProfile::Standard, "cargo", &args);
        assert!(allowed(&decision), "cargo {args:?} should not ask");
    }
}

#[test]
fn direct_remote_commands_and_opaque_shells_ask_at_the_standard_boundary() {
    for (program, args, capability, externality) in [
        (
            "git",
            vec!["push", "origin", "main"],
            CapabilityId::GitRemotePush,
            Externality::RemoteService,
        ),
        (
            "git",
            vec!["fetch", "origin"],
            CapabilityId::NetworkPublicRead,
            Externality::RemoteService,
        ),
        (
            "curl",
            vec!["https://example.test"],
            CapabilityId::NetworkPublicRead,
            Externality::RemoteService,
        ),
        (
            "wget",
            vec!["https://example.test"],
            CapabilityId::NetworkPublicRead,
            Externality::RemoteService,
        ),
        (
            "ssh",
            vec!["host.example"],
            CapabilityId::ExternalSend,
            Externality::RemoteService,
        ),
        (
            "scp",
            vec!["file", "host.example:path"],
            CapabilityId::ExternalSend,
            Externality::RemoteService,
        ),
        (
            "rsync",
            vec!["file", "host.example:path"],
            CapabilityId::ExternalSend,
            Externality::RemoteService,
        ),
        (
            "gh",
            vec!["pr", "create"],
            CapabilityId::GitRemotePullRequest,
            Externality::RemoteService,
        ),
        (
            "sh",
            vec!["-c", "cargo test"],
            CapabilityId::SystemModify,
            Externality::HostSystem,
        ),
    ] {
        let request = request(program, &args);
        assert_eq!(request.capability, capability, "{program} {args:?}");
        assert_eq!(request.externality, externality, "{program} {args:?}");
        assert!(
            CapabilityBroker
                .decide(AutonomyProfile::Standard, &request)
                .is_ask(),
            "Standard must ask for {program} {args:?}"
        );
    }
}

#[test]
fn executable_name_case_and_windows_extensions_do_not_bypass_the_boundary() {
    for (program, args) in [
        ("GIT.EXE", vec!["push", "origin", "main"]),
        ("CURL.EXE", vec!["https://example.test"]),
        ("CMD.EXE", vec!["/C", "cargo test"]),
        ("CMD.EXE", vec!["/K", "cargo test"]),
        ("PowerShell.EXE", vec!["-c", "cargo test"]),
        ("PowerShell.EXE", vec!["-e", "Y2FyZ28gdGVzdA=="]),
        ("PowerShell.EXE", vec!["-enc", "Y2FyZ28gdGVzdA=="]),
        ("fish", vec!["-C", "curl https://example.test"]),
        ("fish", vec!["--init-command", "curl https://example.test"]),
        ("fish", vec!["--command=curl https://example.test"]),
        ("fish", vec!["--init-command=curl https://example.test"]),
    ] {
        let request = request(program, &args);
        assert!(
            CapabilityBroker
                .decide(AutonomyProfile::Standard, &request)
                .is_ask(),
            "Standard must ask for {program} {args:?}"
        );
    }
}

#[test]
fn transparent_interpreter_script_argv_remains_project_execution() {
    for (program, args) in [
        ("fish", vec!["scripts/check.fish"]),
        ("PowerShell.EXE", vec!["-File", "scripts/check.ps1"]),
    ] {
        let request = request(program, &args);
        assert_eq!(
            request.capability,
            CapabilityId::ProcessProjectExecute,
            "{program} {args:?}"
        );
        assert_eq!(
            request.externality,
            Externality::ProjectLocal,
            "{program} {args:?}"
        );
        assert!(
            CapabilityBroker
                .decide(AutonomyProfile::Standard, &request)
                .is_allow(),
            "transparent script argv should stay allowed: {program} {args:?}"
        );
    }
}

#[test]
fn git_remote_plumbing_and_inline_aliases_ask() {
    for (args, capability) in [
        (vec!["send-pack", "origin"], CapabilityId::GitRemotePush),
        (vec!["http-push", "origin"], CapabilityId::GitRemotePush),
        (
            vec!["fetch-pack", "origin"],
            CapabilityId::NetworkPublicRead,
        ),
        (
            vec!["http-fetch", "origin"],
            CapabilityId::NetworkPublicRead,
        ),
        (
            vec!["remote-https", "origin"],
            CapabilityId::NetworkPublicRead,
        ),
        (
            vec!["-c", "alias.ship=!curl https://example.test", "ship"],
            CapabilityId::SystemModify,
        ),
        (
            vec!["--config-env=alias.ship=SHIP_CMD", "ship"],
            CapabilityId::SystemModify,
        ),
    ] {
        let request = request("git", &args);
        assert_eq!(request.capability, capability, "git {args:?}");
        assert!(
            CapabilityBroker
                .decide(AutonomyProfile::Standard, &request)
                .is_ask(),
            "Standard must ask for git {args:?}"
        );
    }
}

#[test]
fn local_rsync_remains_project_execution_but_remote_rsync_asks() {
    let local = request("rsync", &["-a", "src/", "target/"]);
    assert_eq!(local.capability, CapabilityId::ProcessProjectExecute);
    assert_eq!(local.externality, Externality::ProjectLocal);
    assert!(
        CapabilityBroker
            .decide(AutonomyProfile::Standard, &local)
            .is_allow(),
        "local rsync should stay in the project lane"
    );

    for endpoint in [
        "host.example:path",
        "host.example::module",
        "rsync://host/module",
    ] {
        let remote = request("rsync", &["src/", endpoint]);
        assert_eq!(remote.capability, CapabilityId::ExternalSend, "{endpoint}");
        assert_eq!(remote.externality, Externality::RemoteService, "{endpoint}");
        assert!(
            CapabilityBroker
                .decide(AutonomyProfile::Standard, &remote)
                .is_ask(),
            "remote rsync must ask for {endpoint}"
        );
    }
}

#[test]
fn installing_a_host_binary_leaves_the_project_lane_and_asks() {
    // Before the classifier this was `ProcessProjectExecute` with
    // `Externality::ProjectLocal` — indistinguishable from `cargo test`, and so
    // covered by exactly the same project-scoped grant.
    let request = request("cargo", &["install", "ripgrep"]);
    assert_eq!(request.capability, CapabilityId::SystemModify);
    assert_eq!(request.externality, Externality::HostSystem);

    let decision = CapabilityBroker.decide(AutonomyProfile::Standard, &request);
    assert!(
        !allowed(&decision),
        "a host install must not ride on a project grant: {decision:?}"
    );
}

#[test]
fn pipx_installing_a_tool_is_a_host_change_not_a_project_run() {
    // Regression: `pipx install <pkg>` writes a standalone tool into the
    // user-level pipx environment (outside the project), but the classifier
    // used to fall through to `ProcessProjectExecute` — the same grant that
    // covers `cargo test`. It must leave the project lane and ask.
    for args in [
        vec!["install", "httpie"],
        vec!["uninstall", "httpie"],
        vec!["upgrade", "httpie"],
        vec!["ensurepath"],
    ] {
        let request = request("pipx", &args);
        assert_eq!(
            request.capability,
            CapabilityId::SystemModify,
            "pipx {args:?}"
        );
        assert_eq!(
            request.externality,
            Externality::HostSystem,
            "pipx {args:?}"
        );
        assert!(
            !allowed(&CapabilityBroker.decide(AutonomyProfile::Standard, &request)),
            "Standard must ask for pipx {args:?}"
        );
    }

    // `pipx run` executes a tool in place and stays a transparent project
    // execution, so it is not a host-install classification.
    let run = request("pipx", &["run", "httpie"]);
    assert_eq!(run.capability, CapabilityId::ProcessProjectExecute);
    assert_eq!(run.externality, Externality::ProjectLocal);
}

#[test]
fn a_global_npm_install_is_a_host_change_however_it_is_spelled() {
    for args in [
        vec!["install", "-g", "tsx"],
        vec!["install", "--global", "tsx"],
    ] {
        let request = request("npm", &args);
        assert_eq!(
            request.capability,
            CapabilityId::SystemModify,
            "npm {args:?}"
        );
        assert!(!allowed(
            &CapabilityBroker.decide(AutonomyProfile::Standard, &request)
        ));
    }
}

#[test]
fn adding_a_dependency_is_recorded_as_reaching_the_network() {
    let request = request("cargo", &["add", "serde"]);
    assert_eq!(request.capability, CapabilityId::PackageAdd);
    assert_eq!(
        request.externality,
        Externality::PublicNetwork,
        "choosing a new dependency reaches a registry, and the record should say so"
    );
}

#[test]
fn a_lockfile_sync_and_a_new_dependency_are_no_longer_the_same_request() {
    let sync = request("npm", &["ci"]);
    let add = request("npm", &["install", "left-pad"]);
    assert_eq!(sync.capability, CapabilityId::PackageSync);
    assert_eq!(add.capability, CapabilityId::PackageAdd);
    assert_ne!(sync.capability, add.capability);
}

#[test]
fn uv_pip_requirement_files_are_recorded_as_a_sync_like_pip() {
    // Regression: `uv pip install -r requirements.txt` was recorded as
    // `PackageAdd` (a new dependency choice) while the identical pip act was
    // `PackageSync`. The recorded capability must match what the command does.
    let sync = request("uv", &["pip", "install", "-r", "requirements.txt"]);
    assert_eq!(sync.capability, CapabilityId::PackageSync);
    assert_eq!(sync.externality, Externality::PublicNetwork);

    let pip_sync = request("pip", &["install", "-r", "requirements.txt"]);
    assert_eq!(
        sync.capability, pip_sync.capability,
        "uv and pip must record the same act the same way"
    );

    let add = request("uv", &["pip", "install", "requests"]);
    assert_eq!(add.capability, CapabilityId::PackageAdd);
    assert_ne!(sync.capability, add.capability);
}

#[test]
fn read_only_still_refuses_every_command_class() {
    for (program, args) in [
        ("cargo", vec!["test"]),
        ("cargo", vec!["add", "serde"]),
        ("npm", vec!["ci"]),
        ("cargo", vec!["install", "ripgrep"]),
    ] {
        let decision = decide(AutonomyProfile::ReadOnly, program, &args);
        assert!(
            !allowed(&decision),
            "read_only must not allow {program} {args:?}"
        );
    }
}

#[test]
fn an_unclassifiable_command_keeps_the_legacy_project_execution_answer() {
    // No regression for the long tail: unknown programs stay project execution
    // rather than being guessed into a narrower capability.
    let request = request("./deploy.sh", &["--prod"]);
    assert_eq!(request.capability, CapabilityId::ProcessProjectExecute);
    assert_eq!(request.externality, Externality::ProjectLocal);
}

#[test]
fn uv_sync_with_a_host_flag_leaves_the_project_lane_and_asks() {
    // Regression: `uv sync --system` installs into the host Python
    // environment, so it must not ride on the project-scoped lockfile-sync
    // grant that covers a plain `uv sync`. The classifier used to map every
    // `uv sync` to `PackageSync` regardless of the host flag.
    for args in [vec!["sync", "--system"], vec!["sync", "--user"]] {
        let request = request("uv", &args);
        assert_eq!(
            request.capability,
            CapabilityId::SystemModify,
            "uv {args:?}"
        );
        assert_eq!(request.externality, Externality::HostSystem, "uv {args:?}");
        assert!(
            !allowed(&CapabilityBroker.decide(AutonomyProfile::Standard, &request)),
            "Standard must ask for uv {args:?}"
        );
    }

    // Without a host flag a project-venv sync stays in the project lane.
    let sync = request("uv", &["sync"]);
    assert_eq!(sync.capability, CapabilityId::PackageSync);
    assert_eq!(sync.externality, Externality::PublicNetwork);
}

#[test]
fn uncheckpointed_delete_effects_ask_in_standard() {
    for kind in ["DeletePath", "ProjectDeletePath"] {
        let request = build_effect_request(
            kind,
            "deadbeef",
            Some("roothash".into()),
            format!("delete through {kind}"),
            Some("src/old.rs".into()),
        )
        .expect("delete effects map to a capability");

        assert_eq!(request.capability, CapabilityId::FsProjectDelete);
        assert_eq!(request.reversibility, Reversibility::Irreversible);
        assert!(
            CapabilityBroker
                .decide(AutonomyProfile::Standard, &request)
                .is_ask(),
            "{kind} must ask until checkpoint manifests exist"
        );
    }
}
