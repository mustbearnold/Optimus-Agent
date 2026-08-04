//! Criterion C1 (docs/architecture/north-star-2026-07.md): N projects in one
//! core, zero bleed; project selection changes only via an explicit IPC call,
//! never as a turn side effect.
//!
//! BLEED_RATCHET is the C1 counter: the number of projects this test proves
//! against each other inside ONE home. scripts/check-project-bleed.py pins the
//! value and lets it move only toward 5. Every cross-project assertion is
//! phrased pairwise, so raising the ratchet strengthens the same invariants
//! rather than redefining them. The counter sits at the target: the one
//! conversion that blocked every N ≥ 2 was session visibility, closed by the
//! lifetime session↔project binding (kernel session/project.rs) behind the
//! project-scoped `sessions` view — remove either and assertion (D) fails the
//! moment two projects exist, which is the regression this pin now guards.

use std::fs;
use std::path::{Path, PathBuf};

use optimus_host::handle_ipc;
use optimus_kernel::{
    CompletionResponse, Kernel, KernelConfig, Memory, ProjectAuthorityStore, RecallPurpose,
    RecallQuery, ScriptedModel, ToolCall, WriteContext,
};
use serde_json::json;

/// Enforced by scripts/check-project-bleed.py; may only grow (C1 target: 5).
const BLEED_RATCHET: usize = 5;

struct ProjectFixture {
    id: String,
    // Keeps the project root directory alive for the fixture's lifetime.
    _root: tempfile::TempDir,
    session_id: String,
}

fn authorize_project(home: &Path, index: usize) -> ProjectFixture {
    let root = tempfile::tempdir().unwrap();
    let id = format!("project-{index}");
    let authority = ProjectAuthorityStore::open(home).unwrap();
    let selection = authority.stage_native_selection(root.path()).unwrap();
    authority
        .authorize_project(
            &id,
            std::slice::from_ref(&selection.path),
            Some(&selection.path),
            std::slice::from_ref(&selection.grant_token),
        )
        .unwrap()
        .expect("authorization must produce a scope");
    ProjectFixture {
        id,
        _root: root,
        session_id: String::new(),
    }
}

/// The canonical root the kernel actually writes under (authorization
/// canonicalizes the picked path, e.g. through /tmp symlinks).
fn canonical_root(home: &Path, project_id: &str) -> PathBuf {
    ProjectAuthorityStore::open(home)
        .unwrap()
        .scope(project_id)
        .unwrap()
        .expect("scope must exist")
        .primary_root
}

/// Mirror of the kernel's default memory write context with only the project
/// swapped — the recall side of the pairwise memory assertion.
fn memory_ctx_for(project_id: &str) -> WriteContext {
    let defaults = KernelConfig::default();
    WriteContext {
        project: project_id.to_string(),
        ..defaults.memory_ctx
    }
}

#[test]
fn n_projects_in_one_core_zero_bleed() {
    let home_dir = tempfile::tempdir().unwrap();
    let home = home_dir.path().to_path_buf();
    let mut projects: Vec<ProjectFixture> = (0..BLEED_RATCHET)
        .map(|index| authorize_project(&home, index))
        .collect();

    // The explicit authorize calls above are the ONLY selection mutations in
    // this test. Snapshot the authority document they produced.
    let authority_file = home.join("project-authority.json");
    let authority_before = fs::read(&authority_file).unwrap();

    // One real turn per project, through the same IPC entry every surface
    // uses. The reply's session id is that project's session for the
    // visibility assertion below.
    for (index, project) in projects.iter_mut().enumerate() {
        let reply = handle_ipc(
            &home,
            "chat",
            json!({
                "message": format!("bleed probe turn {index}"),
                "provider": "offline",
                "project_id": project.id,
            }),
        )
        .expect("a project turn must run");
        project.session_id = reply["session_id"]
            .as_str()
            .expect("turn must name its session")
            .to_string();
        assert!(
            reply["assistant_text"]
                .as_str()
                .is_some_and(|text| text.contains(&format!("bleed probe turn {index}"))),
            "the offline turn must actually complete: {reply}"
        );
    }

    // A. Turns are never selection mutations: the authority document is
    // byte-identical after N real turns.
    assert_eq!(
        fs::read(&authority_file).unwrap(),
        authority_before,
        "a chat turn mutated project authority — selection changed as a turn side effect"
    );
    // Control: the explicit path DOES mutate the same document, so the
    // assertion above cannot pass by watching the wrong file.
    let control = authorize_project(&home, BLEED_RATCHET);
    assert_ne!(
        fs::read(&authority_file).unwrap(),
        authority_before,
        "control failed: explicit authorization must change the authority document"
    );
    drop(control);

    // B. Write locality: a tool write inside project i's session lands under
    // root_i and under no other project's root.
    for (index, project) in projects.iter().enumerate() {
        let mut kernel = Kernel::open_project_session(
            &home,
            KernelConfig {
                effect_policy: optimus_graph::PolicyMode::Unrestricted,
                ..KernelConfig::default()
            },
            None,
            &project.id,
        )
        .unwrap();
        let mut model = ScriptedModel::new(vec![
            CompletionResponse {
                text: None,
                tool_calls: vec![ToolCall {
                    id: format!("write-{index}"),
                    name: "write_file".into(),
                    arguments: json!({
                        "path": format!("proof-{index}.txt"),
                        "contents": format!("bleed-proof-{index}"),
                    }),
                }],
                reasoning_content: None,
            },
            CompletionResponse {
                text: Some("written".into()),
                tool_calls: vec![],
                reasoning_content: None,
            },
        ]);
        kernel
            .turn(&mut model, &format!("write proof {index}"))
            .expect("the write turn must run");
    }
    for (index, project) in projects.iter().enumerate() {
        let own = canonical_root(&home, &project.id).join(format!("proof-{index}.txt"));
        assert_eq!(
            fs::read_to_string(&own).unwrap_or_else(|_| panic!(
                "project {index}'s write must land under its own root at {}",
                own.display()
            )),
            format!("bleed-proof-{index}"),
        );
        for (other_index, other) in projects.iter().enumerate() {
            if other_index == index {
                continue;
            }
            let foreign = canonical_root(&home, &other.id).join(format!("proof-{index}.txt"));
            assert!(
                !foreign.exists(),
                "project {index}'s write bled into project {other_index}'s root"
            );
        }
    }

    // C. Memory isolation: a claim remembered inside project i's session is
    // recalled by project i and invisible to project j — the store's project
    // filter, fed by the kernel's per-project write context.
    for (index, project) in projects.iter().enumerate() {
        let kernel =
            Kernel::open_project_session(&home, KernelConfig::default(), None, &project.id)
                .unwrap();
        kernel
            .remember_demo("user", "bleed_probe", &format!("secret-{index}"))
            .expect("remember must succeed");
    }
    let memory = Memory::open(home.join("memory.db")).unwrap();
    for (index, project) in projects.iter().enumerate() {
        let packet = memory
            .recall(
                &memory_ctx_for(&project.id),
                RecallQuery {
                    purpose: RecallPurpose::Inform,
                    subject: Some("user".into()),
                    predicate: Some("bleed_probe".into()),
                    as_of_valid: None,
                    as_of_tx: None,
                    limit: 16,
                },
            )
            .unwrap();
        let objects: Vec<&str> = packet
            .current
            .iter()
            .map(|claim| claim.object.as_str())
            .collect();
        assert!(
            objects.contains(&format!("secret-{index}").as_str()),
            "project {index} must recall its own memory, got {objects:?}"
        );
        for other_index in 0..projects.len() {
            if other_index == index {
                continue;
            }
            assert!(
                !objects.contains(&format!("secret-{other_index}").as_str()),
                "project {index} recalled project {other_index}'s memory: {objects:?}"
            );
        }
    }

    // D. Session visibility: project j's sessions view must not contain a
    // session created by project i. The view is the `sessions` method asked
    // for one project — the target API shape; a handler that ignores the
    // parameter and returns every session fails this the moment two projects
    // exist, which is the known blocker for ratchet=2.
    for (index, project) in projects.iter().enumerate() {
        let view = handle_ipc(&home, "sessions", json!({ "project_id": project.id }))
            .expect("sessions view must answer");
        let visible: Vec<&str> = view["sessions"]
            .as_array()
            .expect("sessions must be a list")
            .iter()
            .filter_map(|row| row["id"].as_str())
            .collect();
        for (other_index, other) in projects.iter().enumerate() {
            if other_index == index {
                continue;
            }
            assert!(
                !visible.contains(&other.session_id.as_str()),
                "project {index}'s sessions view shows project {other_index}'s session"
            );
        }
    }
}
