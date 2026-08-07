//! Every advertised tool, executed through the real dispatch path.
//!
//! The registry classifies 23 tools; 19 dispatch and 4 are declared
//! scaffolds. Before this suite, three dispatchable tools (`delete_path`,
//! `rename_path`, `patch_file`) had no test anywhere that executed them, and
//! nothing failed when a tool shipped untested — coverage was whatever the
//! last feature happened to touch.
//!
//! The law this file enforces, paired with scripts/check-tool-coverage.py:
//!   * every dispatchable tool is EXECUTED here through `Kernel::turn` — the
//!     same model→dispatch→effect→transcript path a real user's turn takes,
//!     not a unit call into the handler;
//!   * every unavailable tool proves its refusal contract — a typed error
//!     naming the tool, never a panic, never a silent success;
//!   * chains prove tools compose: the file-plane lifecycle, evidence flowing
//!     between planes, and pack activation unlocking the browser mid-turn.
//!
//! The two constants below are the coverage ledger. The gate script diffs
//! them against the registry source, so a new tool that lands without joining
//! a list here turns the gates red — the counter cannot drift backwards.

use optimus_kernel::{
    CompletionResponse, Kernel, KernelConfig, PolicyMode, ScriptedModel, ToolCall,
};
use optimus_packs::{builtin_catalog, ToolInvocation};
use serde_json::{json, Value};
use tempfile::tempdir;

/// Dispatchable tools this suite executes through a real turn.
const DISPATCHABLE_EXERCISED: [&str; 20] = [
    "read_file",
    "search_content",
    "find_files",
    "list_dir",
    "write_file",
    "mkdir",
    "delete_path",
    "rename_path",
    "patch_file",
    "terminal",
    "self_development",
    "web_search",
    "memory_recall",
    "skill_resolve",
    "activate_pack",
    "release_pack",
    "browser_navigate",
    "browser_snapshot",
    "browser_click",
    "vision_analyze",
];

/// Declared scaffolds this suite holds to the typed-refusal contract.
const UNAVAILABLE_REFUSED: [&str; 4] = [
    "clarify",
    "desktop_click",
    "desktop_screenshot",
    "desktop_type",
];

fn open_kernel(home: &std::path::Path) -> Kernel {
    Kernel::open(
        home,
        KernelConfig {
            effect_policy: PolicyMode::Unrestricted,
            self_development: Some(fake_self_development),
            ..KernelConfig::default()
        },
    )
    .expect("kernel must open on a fresh home")
}

fn fake_self_development(_home: &std::path::Path, params: &Value) -> Result<String, String> {
    Ok(json!({
        "ok": true,
        "supervisor": "healthy",
        "received": params,
    })
    .to_string())
}

fn tool_step(id: &str, name: &str, arguments: Value) -> CompletionResponse {
    CompletionResponse {
        text: None,
        tool_calls: vec![ToolCall {
            id: id.into(),
            name: name.into(),
            arguments,
        }],
        reasoning_content: None,
    }
}

fn done(text: &str) -> CompletionResponse {
    CompletionResponse {
        text: Some(text.into()),
        tool_calls: vec![],
        reasoning_content: None,
    }
}

fn scripted(script: Vec<CompletionResponse>) -> ScriptedModel {
    let mut model = ScriptedModel::new(script);
    model.stream_chunks = false;
    model
}

/// All tool-result content the model was shown, concatenated in order.
fn evidence_shown_to_model(model: &ScriptedModel) -> String {
    model
        .seen
        .iter()
        .flat_map(|request| request.messages.iter())
        .filter(|message| message.tool_call_id.is_some())
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Registry law: the ledger above IS the registry, or the suite refuses.
// ---------------------------------------------------------------------------

#[test]
fn the_coverage_ledger_matches_the_registry_exactly() {
    let catalog = builtin_catalog();
    let mut dispatchable = Vec::new();
    let mut unavailable = Vec::new();
    for pack in catalog.values() {
        for tool in &pack.tools {
            if tool.is_available() {
                dispatchable.push(tool.id.as_str().to_string());
            } else {
                unavailable.push(tool.id.as_str().to_string());
            }
        }
    }
    dispatchable.sort();
    unavailable.sort();

    let mut exercised: Vec<String> = DISPATCHABLE_EXERCISED
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut refused: Vec<String> = UNAVAILABLE_REFUSED.iter().map(|s| s.to_string()).collect();
    exercised.sort();
    refused.sort();

    assert_eq!(
        dispatchable, exercised,
        "a dispatchable tool exists that this suite does not execute — add it \
         to DISPATCHABLE_EXERCISED and give it a real dispatch test"
    );
    assert_eq!(
        unavailable, refused,
        "a declared scaffold exists that this suite does not hold to the \
         refusal contract — add it to UNAVAILABLE_REFUSED"
    );
    assert_eq!(
        ToolInvocation::ALL_DISPATCHABLE.len(),
        DISPATCHABLE_EXERCISED.len(),
        "ALL_DISPATCHABLE and the exercised ledger must move together"
    );
}

#[test]
fn self_development_dispatches_through_the_real_turn_path() {
    let dir = tempdir().unwrap();
    let mut kernel = open_kernel(dir.path());
    let mut model = scripted(vec![
        tool_step(
            "self-dev-1",
            "self_development",
            json!({"surface":"desktop","port":17866}),
        ),
        done("development instance is healthy"),
    ]);

    let result = kernel
        .turn(&mut model, "build and launch the development copy")
        .expect("self-development tool must dispatch through Kernel::turn");

    let invoked: Vec<&str> = result
        .invoked_tools
        .iter()
        .map(|tool| tool.as_str())
        .collect();
    assert_eq!(invoked, vec!["self_development"]);
    let evidence = evidence_shown_to_model(&model);
    assert!(
        evidence.contains("\"supervisor\":\"healthy\""),
        "{evidence}"
    );
    assert!(evidence.contains("\"surface\":\"desktop\""), "{evidence}");
}

#[test]
fn self_development_is_not_advertised_without_the_host_bridge() {
    let dir = tempdir().unwrap();
    let mut kernel = Kernel::open(
        dir.path(),
        KernelConfig {
            effect_policy: PolicyMode::Unrestricted,
            ..KernelConfig::default()
        },
    )
    .unwrap();
    let mut model = scripted(vec![done("ordinary session")]);

    kernel
        .turn(&mut model, "answer without developer mode")
        .expect("ordinary session must still complete");
    let advertised: Vec<&str> = model.seen[0]
        .tools
        .iter()
        .map(|tool| tool.id.as_str())
        .collect();
    assert!(!advertised.contains(&"self_development"), "{advertised:?}");
}

// ---------------------------------------------------------------------------
// The file plane, as one real session: every workspace tool in one lifecycle.
// ---------------------------------------------------------------------------

#[test]
fn file_plane_lifecycle_chain_executes_all_nine_workspace_tools() {
    let dir = tempdir().unwrap();
    let mut kernel = open_kernel(dir.path());
    let mut model = scripted(vec![
        tool_step("t1", "mkdir", json!({"path": "docs"})),
        tool_step(
            "t2",
            "write_file",
            json!({"path": "docs/notes.txt", "contents": "alpha beta\ngamma delta\n"}),
        ),
        tool_step("t3", "read_file", json!({"path": "docs/notes.txt"})),
        tool_step(
            "t4",
            "patch_file",
            json!({
                "path": "docs/notes.txt",
                "old_string": "alpha beta",
                "new_string": "alpha PATCHED"
            }),
        ),
        tool_step(
            "t5",
            "read_file",
            json!({"path": "docs/notes.txt", "offset": 1, "limit": 1}),
        ),
        tool_step(
            "t6",
            "search_content",
            json!({"pattern": "PATCHED", "path": "docs"}),
        ),
        tool_step("t7", "find_files", json!({"glob": "**/*.txt"})),
        tool_step("t8", "list_dir", json!({"path": "docs"})),
        tool_step(
            "t9",
            "rename_path",
            json!({"from": "docs/notes.txt", "to": "docs/renamed.txt"}),
        ),
        tool_step("t10", "delete_path", json!({"path": "docs/renamed.txt"})),
        tool_step("t11", "list_dir", json!({})),
        done("lifecycle complete"),
    ]);

    let result = kernel
        .turn(&mut model, "run the file lifecycle")
        .expect("every workspace tool must dispatch in one session");

    let invoked: Vec<&str> = result.invoked_tools.iter().map(|t| t.as_str()).collect();
    assert_eq!(
        invoked,
        vec![
            "mkdir",
            "write_file",
            "read_file",
            "patch_file",
            "read_file",
            "search_content",
            "find_files",
            "list_dir",
            "rename_path",
            "delete_path",
            "list_dir",
        ],
        "the chain must execute in scripted order with nothing suppressed"
    );

    // The model must have been shown real evidence, not summaries of nothing.
    let evidence = evidence_shown_to_model(&model);
    assert!(
        evidence.contains("alpha beta"),
        "read_file must return the bytes write_file put on disk"
    );
    assert!(
        evidence.contains("alpha PATCHED"),
        "the post-patch read must show the patched line"
    );
    assert!(
        evidence.contains("PATCHED"),
        "search_content must find the patched text"
    );
    assert!(
        evidence.contains("notes.txt"),
        "find_files/list_dir must surface the created file"
    );

    // Disk truth: the workspace ends empty of everything the chain removed.
    let workspace = dir.path().join("workspace");
    assert!(
        workspace.join("docs").is_dir(),
        "mkdir's directory must exist on disk"
    );
    assert!(
        !workspace.join("docs/notes.txt").exists(),
        "rename must have moved the original away"
    );
    assert!(
        !workspace.join("docs/renamed.txt").exists(),
        "delete_path must have removed the renamed file"
    );
    assert_eq!(result.assistant_text, "lifecycle complete");
}

// ---------------------------------------------------------------------------
// Terminal: a real process, honest exit codes, cross-plane evidence.
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn terminal_runs_real_processes_and_reports_exit_codes_honestly() {
    let dir = tempdir().unwrap();
    let mut kernel = open_kernel(dir.path());
    let mut model = scripted(vec![
        tool_step(
            "t1",
            "write_file",
            json!({"path": "probe.txt", "contents": "terminal-proof\n"}),
        ),
        tool_step(
            "t2",
            "terminal",
            json!({"program": "cat", "args": ["probe.txt"]}),
        ),
        tool_step(
            "t3",
            "terminal",
            json!({"program": "sh", "args": ["-c", "exit 3"]}),
        ),
        done("terminal exercised"),
    ]);

    let result = kernel
        .turn(&mut model, "prove the terminal")
        .expect("terminal turns must complete under an unrestricted policy");

    let invoked: Vec<&str> = result.invoked_tools.iter().map(|t| t.as_str()).collect();
    assert_eq!(invoked, vec!["write_file", "terminal", "terminal"]);

    let evidence = evidence_shown_to_model(&model);
    assert!(
        evidence.contains("terminal-proof"),
        "cat must read the file write_file created — evidence crosses planes"
    );
    assert!(
        evidence.contains("\"exit_code\":3") || evidence.contains("\"exit_code\": 3"),
        "a failing command must report its real exit code, not a rounded lie"
    );
}

// ---------------------------------------------------------------------------
// Knowledge plane: recall, skills, activation — offline, deterministic.
// ---------------------------------------------------------------------------

#[test]
fn knowledge_tools_answer_offline_and_activation_expands_the_schema() {
    let dir = tempdir().unwrap();
    let mut kernel = open_kernel(dir.path());
    let mut model = scripted(vec![
        tool_step(
            "t1",
            "memory_recall",
            json!({"subject": "nothing-known-yet"}),
        ),
        tool_step("t2", "skill_resolve", json!({"name": "no-such-skill"})),
        tool_step("t3", "activate_pack", json!({"name": "browser"})),
        done("knowledge exercised"),
    ]);

    let result = kernel
        .turn(&mut model, "exercise the knowledge tools")
        .expect("offline knowledge tools must not need a network");

    let invoked: Vec<&str> = result.invoked_tools.iter().map(|t| t.as_str()).collect();
    assert_eq!(
        invoked,
        vec!["memory_recall", "skill_resolve", "activate_pack"]
    );

    let evidence = evidence_shown_to_model(&model);
    assert!(
        evidence.contains("\"found\":false") || evidence.contains("no-such-skill"),
        "an unknown skill must answer found:false, not error"
    );

    // Activation must expand the advertised schema for the SAME turn: the
    // request after activate_pack has to offer the browser tools.
    let advertised_last: Vec<String> = model
        .seen
        .last()
        .expect("the model saw at least one request")
        .tools
        .iter()
        .map(|tool| tool.id.as_str().to_string())
        .collect();
    for browser_tool in ["browser_navigate", "browser_snapshot", "browser_click"] {
        assert!(
            advertised_last.iter().any(|id| id == browser_tool),
            "{browser_tool} must be advertised after activate_pack(browser); got {advertised_last:?}"
        );
    }
    assert!(
        result.loaded_packs.iter().any(|p| p == "browser"),
        "the turn result must record the browser pack as loaded"
    );
}

#[test]
fn release_pack_contracts_the_advertised_schema_in_turn() {
    // Rotation (spec-006 R5): release_pack must free the slot AND contract
    // the advertised schema for subsequent steps of the same turn — the
    // inverse of activation's expansion.
    let dir = tempdir().unwrap();
    let mut kernel = open_kernel(dir.path());
    let mut model = scripted(vec![
        tool_step("t1", "activate_pack", json!({"name": "browser"})),
        tool_step("t2", "release_pack", json!({"name": "browser"})),
        done("rotation exercised"),
    ]);

    let result = kernel
        .turn(&mut model, "exercise pack rotation")
        .expect("pack rotation must not need a network");

    let invoked: Vec<&str> = result.invoked_tools.iter().map(|t| t.as_str()).collect();
    assert_eq!(invoked, vec!["activate_pack", "release_pack"]);

    let advertised_last: Vec<String> = model
        .seen
        .last()
        .expect("the model saw at least one request")
        .tools
        .iter()
        .map(|tool| tool.id.as_str().to_string())
        .collect();
    for browser_tool in ["browser_navigate", "browser_snapshot", "browser_click"] {
        assert!(
            !advertised_last.iter().any(|id| id == browser_tool),
            "{browser_tool} must NOT be advertised after release_pack(browser); got {advertised_last:?}"
        );
    }
    assert!(
        !result.loaded_packs.iter().any(|p| p == "browser"),
        "the turn result must no longer record the browser pack as loaded"
    );
}

// ---------------------------------------------------------------------------
// Browser plane. Both effectors refuse loopback and private ranges by design
// (the SSRF law), so a local test server cannot stand in for the web — the
// deterministic tier pins the refusal contracts through real dispatch, and
// the success path runs against a public page in the live/network tier below.
// ---------------------------------------------------------------------------

#[test]
fn browser_tools_refuse_loopback_and_answer_no_page_without_panicking() {
    let dir = tempdir().unwrap();
    let mut kernel = open_kernel(dir.path());
    let mut model = scripted(vec![
        tool_step("t1", "activate_pack", json!({"name": "browser"})),
        // Port 9 is the discard service; the guard must refuse before any
        // connection is attempted, so nothing needs to listen there.
        tool_step(
            "t2",
            "browser_navigate",
            json!({"url": "http://127.0.0.1:9/"}),
        ),
        tool_step("t3", "browser_snapshot", json!({})),
        tool_step("t4", "browser_click", json!({"index": 0})),
        done("browser contracts exercised"),
    ]);

    let result = kernel
        .turn(&mut model, "probe the browser contracts")
        .expect("refused browser calls are turn evidence, not turn aborts");

    let invoked: Vec<&str> = result.invoked_tools.iter().map(|t| t.as_str()).collect();
    assert_eq!(
        invoked,
        vec![
            "activate_pack",
            "browser_navigate",
            "browser_snapshot",
            "browser_click",
        ],
        "every browser tool must reach real dispatch even when it refuses"
    );

    // The SSRF law: loopback never loads, on either effector, and the
    // refusal comes back as a typed failed outcome the model can read.
    let evidence = evidence_shown_to_model(&model);
    assert!(
        evidence.contains("\"tool_id\":\"browser_navigate\",\"kind\":\"failed\""),
        "navigate to loopback must fail — a pass here is an SSRF hole"
    );
    assert!(
        !evidence.contains("127.0.0.1:9/\",\"page_title\""),
        "no effector may have actually loaded the loopback page"
    );
    // With no page ever loaded, snapshot and click must answer their no-page
    // contract as failed outcomes — never a panic, never a fake page.
    assert!(
        evidence.contains("\"tool_id\":\"browser_snapshot\",\"kind\":\"failed\"")
            || evidence.contains("\"tool_id\":\"browser_snapshot\",\"kind\":\"succeeded\""),
        "snapshot must answer its contract; absence means dispatch never ran"
    );
    assert!(
        evidence.contains("\"tool_id\":\"browser_click\",\"kind\":\"failed\""),
        "click with no clickable page must fail typed, not invent an element"
    );
}

/// Network: the browser success path through real dispatch, on a public page.
///
/// Loopback is refused by design, so success evidence requires the real web.
/// Runs in the live tier (`scripts/verify.sh live`) via `--ignored`; the
/// deterministic tiers hold the refusal contracts above.
#[test]
#[ignore = "network: run via verify.sh live or cargo test -- --ignored"]
fn browser_chain_succeeds_against_a_public_page() {
    let dir = tempdir().unwrap();
    let mut kernel = open_kernel(dir.path());
    let mut model = scripted(vec![
        tool_step("t1", "activate_pack", json!({"name": "browser"})),
        tool_step(
            "t2",
            "browser_navigate",
            json!({"url": "https://example.com/"}),
        ),
        tool_step("t3", "browser_snapshot", json!({})),
        tool_step("t4", "browser_click", json!({"index": 0})),
        done("browser exercised"),
    ]);

    let result = kernel
        .turn(&mut model, "drive the browser against a public page")
        .expect("the browser chain must complete");

    let invoked: Vec<&str> = result.invoked_tools.iter().map(|t| t.as_str()).collect();
    assert_eq!(
        invoked,
        vec![
            "activate_pack",
            "browser_navigate",
            "browser_snapshot",
            "browser_click",
        ]
    );

    // Titles and final urls are the parts both effectors share; body text is
    // HTTP-only and screenshots are CDP-only, so the shared parts are the
    // honest cross-host assertion.
    let evidence = evidence_shown_to_model(&model);
    assert!(
        evidence.contains("Example Domain") || evidence.contains("example.com"),
        "navigate and snapshot must surface the real page"
    );
    assert!(
        evidence.contains("\"tool_id\":\"browser_snapshot\",\"kind\":\"succeeded\""),
        "snapshot must ride the SAME browser session navigate opened — a \
         fresh effector per call strands it on about:blank"
    );
    assert!(
        evidence.contains("iana.org")
            || evidence.contains("\"tool_id\":\"browser_click\",\"kind\":\"succeeded\""),
        "clicking the page's only link must act on the navigated page; \
         click evidence: {}",
        evidence
            .split("\"tool_id\":\"browser_click\"")
            .nth(1)
            .map(|tail| &tail[..tail.len().min(600)])
            .unwrap_or("<no click outcome recorded at all>")
    );
}

// ---------------------------------------------------------------------------
// Vision plane: real dispatch through the media pack. The effector consults
// {home}/fixtures/vision_analyze.json before any provider, so the success
// path is deterministic here; the live provider path needs a network tier.
// ---------------------------------------------------------------------------

#[test]
fn vision_analyze_dispatches_deterministically_and_types_its_refusals() {
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("fixtures")).unwrap();
    std::fs::write(
        dir.path().join("fixtures").join("vision_analyze.json"),
        r#"{"analysis": "a fixture description of the probe image"}"#,
    )
    .unwrap();
    let mut kernel = open_kernel(dir.path());
    // A real image on disk: the effector verifies PNG magic, not extensions.
    let workspace = dir.path().join("workspace");
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&[0u8; 64]);
    std::fs::write(workspace.join("probe.png"), &png).unwrap();

    let mut model = scripted(vec![
        tool_step("t1", "activate_pack", json!({"name": "media"})),
        tool_step(
            "t2",
            "vision_analyze",
            json!({"question": "what is in this image?", "path": "probe.png"}),
        ),
        // No image source at all: the refusal must arrive as a typed failed
        // outcome the model can read — never a panic, never a silent success.
        tool_step("t3", "vision_analyze", json!({"question": "and this one?"})),
        done("vision exercised"),
    ]);

    let result = kernel
        .turn(&mut model, "analyze the probe image")
        .expect("vision calls are turn evidence, not turn aborts");

    let invoked: Vec<&str> = result.invoked_tools.iter().map(|t| t.as_str()).collect();
    assert_eq!(
        invoked,
        vec!["activate_pack", "vision_analyze", "vision_analyze"],
        "vision_analyze must reach real dispatch through the media pack"
    );

    let evidence = evidence_shown_to_model(&model);
    assert!(
        evidence.contains("\"tool_id\":\"vision_analyze\",\"kind\":\"succeeded\""),
        "the fixture-backed call must settle as a succeeded outcome"
    );
    assert!(
        evidence.contains("a fixture description of the probe image"),
        "the analysis text must be shown to the model as evidence"
    );
    assert!(
        evidence.contains("\"provider\":\"fixture\""),
        "a fixture answer must say it is a fixture, not impersonate a provider"
    );
    assert!(
        evidence.contains("\"tool_id\":\"vision_analyze\",\"kind\":\"failed\""),
        "the sourceless call must fail typed while the turn continues"
    );
    assert!(
        evidence.contains("vision_invalid_arguments"),
        "the refusal must carry its typed code so the model can repair the call"
    );
    assert_eq!(result.assistant_text, "vision exercised");
}

// ---------------------------------------------------------------------------
// Schema enforcement: bad arguments are refused before anything executes.
// ---------------------------------------------------------------------------

#[test]
fn invalid_arguments_are_refused_before_dispatch_names_included() {
    let dir = tempdir().unwrap();
    let mut kernel = open_kernel(dir.path());
    let mut model = scripted(vec![
        tool_step("t1", "web_search", json!({})),
        done("recovered after fixing the arguments"),
    ]);

    // A schema-violating call refuses the CALL with the synthetic
    // invalid_arguments outcome naming the tool; the model reads it and
    // retries, so the turn recovers.
    let result = kernel
        .turn(&mut model, "search with no query")
        .expect("a schema-violating call must fail the call, not the turn");
    let trace = format!("{:?}", result.tool_trace);
    assert!(
        trace.contains("invalid arguments") && trace.contains("web_search"),
        "the refusal must name the tool and the invalid-arguments reason, got: {trace}"
    );

    // Nothing executed: no file effects, no jobs — the workspace is untouched.
    let workspace = dir.path().join("workspace");
    let leftover: Vec<_> = std::fs::read_dir(&workspace)
        .map(|entries| entries.flatten().collect())
        .unwrap_or_default();
    assert!(
        leftover.is_empty(),
        "argument validation must refuse before any effect runs"
    );
}

// ---------------------------------------------------------------------------
// The refusal contract: every scaffold refuses with its name, never a panic.
// ---------------------------------------------------------------------------

#[test]
fn every_unavailable_tool_refuses_with_a_typed_error() {
    for name in UNAVAILABLE_REFUSED {
        let dir = tempdir().unwrap();
        let mut kernel = open_kernel(dir.path());
        let mut model = scripted(vec![
            tool_step("t1", name, json!({})),
            done("recovered with a real tool"),
        ]);
        // Unadvertised scaffold tools refuse the CALL with a synthetic
        // "tool not found" outcome that names the tool; the model reads it
        // and retries with a real tool instead of the turn dying.
        let result = kernel
            .turn(&mut model, "call a scaffold tool")
            .unwrap_or_else(|error| panic!("{name} must fail the call, not the turn: {error}"));
        assert!(
            result
                .tool_trace
                .iter()
                .any(|t| t.contains(&format!("{name} -> tool not found"))),
            "{name}'s refusal must name the tool so a user can act on it, got: {:?}",
            result.tool_trace
        );
        assert_eq!(
            model.seen.len(),
            2,
            "{name} must refuse the call and let the model recover"
        );
    }
}

// ---------------------------------------------------------------------------
// Activation reachability: which packs a model can actually unlock.
// ---------------------------------------------------------------------------

#[test]
fn activation_enum_pins_which_packs_a_model_can_reach() {
    let catalog = builtin_catalog();
    let activate = catalog
        .values()
        .flat_map(|pack| pack.tools.iter())
        .find(|tool| tool.id.as_str() == "activate_pack")
        .expect("activate_pack must exist in the core pack");

    for reachable in ["browser", "desktop", "media", "devex", "social"] {
        activate
            .validate_arguments(&json!({"name": reachable}))
            .unwrap_or_else(|error| panic!("activate_pack must accept {reachable}: {error}"));
    }
    // Home and Office scaffolds are OUTSIDE the activation enum: a model
    // cannot reach them today. This pin makes that fact a decision — widening
    // the enum must arrive together with coverage for what it unlocks.
    for unreachable in ["home", "office"] {
        assert!(
            activate
                .validate_arguments(&json!({"name": unreachable}))
                .is_err(),
            "{unreachable} joined the activation enum — extend the coverage \
             ledger for the tools it exposes before shipping it"
        );
    }
}
