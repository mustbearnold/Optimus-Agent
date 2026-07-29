//! R30.6: what the classifier changes about a real broker decision.
//!
//! The unit tests in `command_class` prove the classification. These prove it
//! reaches the decision — that a project-scoped `Standard` grant, which is the
//! profile a durably trusted project runs at, stops covering a command that
//! leaves the project.

use optimus_policy::{
    build_effect_request_for, ActionRequest, AuthorityDecision, AutonomyProfile, CapabilityBroker,
    CapabilityId, Externality,
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
fn an_unclassifiable_command_keeps_the_old_conservative_answer() {
    // No regression for the long tail: unknown programs stay project execution
    // rather than being guessed into a narrower capability.
    let request = request("./deploy.sh", &["--prod"]);
    assert_eq!(request.capability, CapabilityId::ProcessProjectExecute);
    assert_eq!(request.externality, Externality::ProjectLocal);
}
