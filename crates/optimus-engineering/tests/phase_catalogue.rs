//! Program P40 exit gate, E40.9: a phase runs the repository's commands.
//!
//! The unit tests in `catalogue.rs` cover the rules against constructed
//! profiles. These run against *this* repository — real `just --summary`, real
//! `git` — because the thing worth proving is that the resolved profile and
//! the phase table actually meet: that a run started here would know what to
//! execute, without a prompt guessing at it.
//!
//! `gh` is the one command that is stubbed. It reaches the network, and branch
//! protection has no bearing on which commands a phase runs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use optimus_engineering::{
    plan_for, plans_for_run, resolve_profile, CommandError, CommandOutcome, CommandRunner,
    DeclaredPolicy, DevPhase, EvidenceKind, ProcessRunner,
};

/// Real commands, except `gh`, which would reach the forge.
struct OfflineRunner;

impl CommandRunner for OfflineRunner {
    fn run(
        &self,
        cwd: &Path,
        program: &str,
        args: &[String],
        timeout: Duration,
    ) -> Result<CommandOutcome, CommandError> {
        if program == "gh" {
            return Ok(CommandOutcome {
                program: program.to_string(),
                args: args.to_vec(),
                exit_code: Some(1),
                stdout: Vec::new(),
                stderr: b"gh: Branch not protected (HTTP 404)".to_vec(),
                stdout_truncated: false,
                stderr_truncated: false,
                timed_out: false,
                duration_ms: 0,
            });
        }
        ProcessRunner.run(cwd, program, args, timeout)
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn this_repository_supplies_every_command_its_phases_need() {
    let root = repo_root();
    let profile = resolve_profile(&OfflineRunner, &root, &DeclaredPolicy::default())
        .expect("this repository resolves");

    let plans = plans_for_run(&profile);
    let blocked: Vec<_> = plans
        .iter()
        .filter(|plan| !plan.can_drive())
        .map(|plan| plan.blocking_reason().unwrap_or_default())
        .collect();
    assert!(
        blocked.is_empty(),
        "a run started in this repository could not proceed: {blocked:?}"
    );
}

#[test]
fn the_focused_gate_comes_from_the_justfile_not_from_a_guess() {
    let root = repo_root();
    let profile = resolve_profile(&OfflineRunner, &root, &DeclaredPolicy::default())
        .expect("this repository resolves");

    let plan = plan_for(DevPhase::FocusedVerify, &profile);
    let step = plan
        .steps
        .iter()
        .find(|s| s.evidence == EvidenceKind::FocusedTestRun)
        .expect("a focused step");

    // Whatever the recipe is called, it has to be one this repository really
    // has — the point of E41.3 is that nothing here is invented.
    assert_eq!(step.program, "just");
    let recipe = step.args.first().expect("a recipe name");
    let summary = std::process::Command::new("just")
        .current_dir(&root)
        .arg("--summary")
        .output()
        .expect("just --summary");
    let recipes = String::from_utf8_lossy(&summary.stdout);
    assert!(
        recipes.split_whitespace().any(|name| name == recipe),
        "planned `just {recipe}`, which this repository does not define: {recipes}"
    );
}

#[test]
fn the_full_gate_is_the_one_managed_land_runs() {
    let root = repo_root();
    let profile = resolve_profile(&OfflineRunner, &root, &DeclaredPolicy::default())
        .expect("this repository resolves");

    let plan = plan_for(DevPhase::FullVerify, &profile);
    let step = &plan.steps[0];
    assert_eq!(step.evidence, EvidenceKind::FullVerification);
    assert_eq!(step.program, "just");
    assert_eq!(step.args, vec!["verify".to_string()]);
}

#[test]
fn the_baseline_and_the_focused_run_are_the_same_command_at_different_times() {
    // The comparison only means something if both sides run the same gate:
    // a red focused verify after a green baseline is attributable to the patch
    // precisely because nothing else changed, including the command.
    let root = repo_root();
    let profile = resolve_profile(&OfflineRunner, &root, &DeclaredPolicy::default())
        .expect("this repository resolves");

    let baseline = plan_for(DevPhase::PrepareWorktree, &profile);
    let focused = plan_for(DevPhase::FocusedVerify, &profile);

    let baseline_step = baseline
        .steps
        .iter()
        .find(|s| s.evidence == EvidenceKind::BaselineVerification)
        .expect("baseline step");
    let focused_step = focused
        .steps
        .iter()
        .find(|s| s.evidence == EvidenceKind::FocusedTestRun)
        .expect("focused step");

    assert_eq!(baseline_step.program, focused_step.program);
    assert_eq!(baseline_step.args, focused_step.args);
}

#[test]
fn the_hardest_evidence_is_never_satisfied_by_a_command() {
    let root = repo_root();
    let profile = resolve_profile(&OfflineRunner, &root, &DeclaredPolicy::default())
        .expect("this repository resolves");

    let plan = plan_for(DevPhase::FocusedVerify, &profile);
    assert!(
        !plan
            .evidence_from_commands()
            .contains(&EvidenceKind::DifferentialProof),
        "a differential proof cannot come from one command at one commit"
    );
    assert!(plan
        .from_elsewhere
        .contains(&EvidenceKind::DifferentialProof));
}
