//! Built-in pack catalog and the descriptor constructors that build it.
//!
//! Split out of `lib.rs` under the ADR-0049 module-size ratchet: the catalog is
//! pure data plus its small builders, so it moves as one unit and leaves
//! `lib.rs` owning the pack types, errors, and `CapabilitySession`.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::{PackDesc, PackId, ToolDesc, ToolId, ToolInvocation, ToolPolicy};

/// Catalog of built-in packs (Hermes waist lesson: core small, edges on demand).
pub fn builtin_catalog() -> BTreeMap<PackId, PackDesc> {
    let mut m = BTreeMap::new();
    m.insert(
        PackId::Core,
        PackDesc {
            id: PackId::Core,
            summary: "Always-on waist: fs, terminal, web, memory, skills, packs, jobs, goals, clarify"
                .into(),
            tools: vec![
                tool(
                    ToolInvocation::ReadFile,
                    "Read a workspace file, optionally a line range",
                    150,
                    object_schema(
                        json!({
                            "path":{"type":"string"},
                            "offset":{"type":"integer"},
                            "limit":{"type":"integer"}
                        }),
                        &["path"],
                    ),
                ),
                tool(
                    ToolInvocation::SearchContent,
                    "Search file contents by regular expression. Prefer this over \
                     running grep or rg through the terminal",
                    200,
                    object_schema(
                        json!({
                            "pattern":{"type":"string"},
                            "path":{"type":"string"},
                            "glob":{"type":"string"},
                            "case_sensitive":{"type":"boolean"},
                            "max_results":{"type":"integer"}
                        }),
                        &["pattern"],
                    ),
                ),
                tool(
                    ToolInvocation::FindFiles,
                    "Find files by glob pattern. Prefer this over running find or \
                     fd through the terminal",
                    140,
                    object_schema(
                        json!({
                            "glob":{"type":"string"},
                            "path":{"type":"string"},
                            "max_results":{"type":"integer"}
                        }),
                        &["glob"],
                    ),
                ),
                tool(
                    ToolInvocation::ListDir,
                    "List one workspace directory. Prefer this over running ls \
                     through the terminal",
                    100,
                    object_schema(json!({"path":{"type":"string"}}), &[]),
                ),
                tool(
                    ToolInvocation::WriteFile,
                    "Write workspace file; returns relative_path and absolute_path without terminal",
                    120,
                    object_schema(
                        json!({"path":{"type":"string"},"contents":{"type":"string"}}),
                        &["path", "contents"],
                    ),
                ),
                tool(
                    ToolInvocation::Mkdir,
                    "Create workspace directory (and parents)",
                    80,
                    object_schema(json!({"path":{"type":"string"}}), &["path"]),
                ),
                tool(
                    ToolInvocation::DeletePath,
                    "Delete workspace file or empty directory",
                    80,
                    object_schema(json!({"path":{"type":"string"}}), &["path"]),
                ),
                tool(
                    ToolInvocation::RenamePath,
                    "Rename or move within the workspace",
                    100,
                    object_schema(
                        json!({
                            "from":{"type":"string"},
                            "to":{"type":"string"}
                        }),
                        &["from", "to"],
                    ),
                ),
                tool(
                    ToolInvocation::PatchFile,
                    "Exact single-occurrence string replace in a workspace file",
                    140,
                    object_schema(
                        json!({
                            "path":{"type":"string"},
                            "old_string":{"type":"string"},
                            "new_string":{"type":"string"}
                        }),
                        &["path", "old_string", "new_string"],
                    ),
                ),
                tool(
                    ToolInvocation::Terminal,
                    "Run bounded shell command",
                    150,
                    object_schema(
                        json!({
                            "program":{"type":"string"},
                            "args":{"type":"array","items":{"type":"string"}}
                        }),
                        &["program"],
                    ),
                ),
                tool(
                    ToolInvocation::SelfDevelopment,
                    "Build, smoke-test, and launch a separate Optimus development copy through the stable supervisor; requires active Developer Full Access. Defaults to the granted scope root and desktop surface.",
                    100,
                    object_schema(
                        json!({
                            "workspace":{"type":"string"},
                            "surface":{"type":"string","enum":["desktop","host"]},
                            "port":{"type":"integer","minimum":1024}
                        }),
                        &[],
                    ),
                ),
                tool(
                    ToolInvocation::WebSearch,
                    "Search the web",
                    140,
                    object_schema(
                        json!({"query":{"type":"string"},"limit":{"type":"integer","minimum":1}}),
                        &["query"],
                    ),
                ),
                tool(
                    ToolInvocation::MemoryRecall,
                    "Recall EvidencePacket",
                    130,
                    object_schema(
                        json!({"subject":{"type":"string"},"predicate":{"type":"string"}}),
                        &[],
                    ),
                ),
                tool(
                    ToolInvocation::SkillResolve,
                    "Resolve procedural skill by name",
                    120,
                    object_schema(json!({"name":{"type":"string"}}), &["name"]),
                ),
                tool(
                    ToolInvocation::Goal,
                    "Manage the session goal: a durable objective with optional token and time budgets. Actions: set (create or rewrite in idle), start (idle to active), status, pause, resume, complete. Budgets are enforced by the turn loop while the goal is active.",
                    60,
                    object_schema(
                        json!({
                            "action":{"type":"string","enum":["set","start","status","pause","resume","complete"]},
                            "objective":{"type":"string"},
                            "token_budget":{"type":"integer","minimum":1},
                            "time_budget_seconds":{"type":"integer","minimum":1}
                        }),
                        &["action"],
                    ),
                ),
                tool(
                    ToolInvocation::ActivatePack,
                    "Load an on-demand capability pack",
                    100,
                    object_schema(
                        json!({"name":{"type":"string","enum":["browser","desktop","media","devex","social","collaboration"]}}),
                        &["name"],
                    ),
                ),
                tool(
                    ToolInvocation::ReleasePack,
                    "Unload an on-demand capability pack (frees its slot and schema tokens)",
                    100,
                    object_schema(
                        json!({"name":{"type":"string","enum":["browser","desktop","media","devex","social","collaboration"]}}),
                        &["name"],
                    ),
                ),
                unavailable("clarify", "Ask the user", ToolPolicy::UserInteraction),
            ],
        },
    );
    m.insert(
        PackId::Collaboration,
        PackDesc {
            id: PackId::Collaboration,
            summary: "On-demand session collaboration: send, inbox, roster, review, policy (spec-025)".into(),
            tools: vec![
                tool(
                    ToolInvocation::SessionSend,
                    "Send a message to another session (spec-025). Returns a failure-honest receipt; the target's inbound policy decides the landing state (auto-accept delivers, hold-approval holds, deny refuses).",
                    80,
                    object_schema(
                        json!({
                            "to_session":{"type":"string"},
                            "payload":{"type":"string"},
                            "kind":{"type":"string","enum":["request","reply","notice"]},
                            "reply_to":{"type":"string"},
                            "mode":{"type":"string","enum":["auto","steer","follow_up"]}
                        }),
                        &["to_session", "payload"],
                    ),
                ),
                tool(
                    ToolInvocation::SessionInbox,
                    "List this session's inbox with permission-classification state (spec-025). Delivers queued messages and expires held ones first.",
                    60,
                    object_schema(json!({"limit":{"type":"integer","minimum":1}}), &[]),
                ),
                tool(
                    ToolInvocation::SessionRoster,
                    "List sessions opted into peer discovery (spec-025 R2). Opted-out sessions never appear.",
                    50,
                    object_schema(json!({}), &[]),
                ),
                tool(
                    ToolInvocation::SessionReview,
                    "Approve or deny a held session message (spec-025 R3): approve delivers it, deny refuses it.",
                    60,
                    object_schema(
                        json!({
                            "message_id":{"type":"string"},
                            "approve":{"type":"boolean"}
                        }),
                        &["message_id", "approve"],
                    ),
                ),
                tool(
                    ToolInvocation::SessionPolicy,
                    "Get or set this session's inbound policy (auto-accept|hold-approval|deny), peer-discovery opt-in, and dialog expiry seconds (spec-025 R2/R3).",
                    70,
                    object_schema(
                        json!({
                            "inbound_policy":{"type":"string","enum":["auto-accept","hold-approval","deny"]},
                            "discoverable":{"type":"boolean"},
                            "dialog_expiry_seconds":{"type":"integer","minimum":1}
                        }),
                        &[],
                    ),
                ),
            ],
        },
    );
    m.insert(
        PackId::Browser,
        PackDesc {
            id: PackId::Browser,
            summary: "CDP/browser automation".into(),
            tools: vec![
                tool(
                    ToolInvocation::BrowserNavigate,
                    "Navigate URL",
                    200,
                    object_schema(json!({"url":{"type":"string"}}), &["url"]),
                ),
                tool(
                    ToolInvocation::BrowserClick,
                    "Click element",
                    180,
                    object_schema(json!({"index":{"type":"integer","minimum":0}}), &["index"]),
                ),
                tool(
                    ToolInvocation::BrowserSnapshot,
                    "A11y snapshot",
                    220,
                    object_schema(json!({}), &[]),
                ),
            ],
        },
    );
    m.insert(
        PackId::Desktop,
        PackDesc {
            id: PackId::Desktop,
            summary: "Computer-use / OS UI automation".into(),
            tools: vec![
                unavailable("desktop_screenshot", "Capture screen", ToolPolicy::Desktop),
                unavailable("desktop_click", "Click coordinates", ToolPolicy::Desktop),
                unavailable("desktop_type", "Type keys", ToolPolicy::Desktop),
            ],
        },
    );
    m.insert(
        PackId::Media,
        PackDesc {
            id: PackId::Media,
            summary: "Vision analysis (imagegen/TTS return with their lane; ADR-0068)".into(),
            tools: vec![tool(
                ToolInvocation::VisionAnalyze,
                "Answer a question about one image via a vision model; pass \
                 question plus exactly one of artifact_sha256 or path",
                180,
                object_schema(
                    json!({
                        "question":{"type":"string"},
                        "artifact_sha256":{"type":"string"},
                        "path":{"type":"string"}
                    }),
                    &["question"],
                ),
            )],
        },
    );
    m.insert(
        PackId::Devex,
        PackDesc {
            id: PackId::Devex,
            summary: "Git/GH/deep dev workflows (no tools until designed; ADR-0068)".into(),
            tools: vec![],
        },
    );
    m.insert(
        PackId::Social,
        PackDesc {
            id: PackId::Social,
            summary: "Messaging (returns with a live gateway transport; ADR-0068)".into(),
            tools: vec![],
        },
    );
    m.insert(
        PackId::Home,
        PackDesc {
            id: PackId::Home,
            summary: "Home automation (no tools until integrated; ADR-0068)".into(),
            tools: vec![],
        },
    );
    m.insert(
        PackId::Office,
        PackDesc {
            id: PackId::Office,
            summary: "Office documents (no tools until integrated; ADR-0068)".into(),
            tools: vec![],
        },
    );
    m
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

pub fn canonical_tool_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "version": {"type":"integer"},
            "call_id": {"type":"string"},
            "tool_id": {"type":"string"},
            "kind": {"type":"string", "enum":["succeeded","failed","cancelled","ambiguous"]},
            "summary": {"type":"string"},
            "data": {},
            "artifacts": {"type":"array", "items":{"type":"object"}},
            "error": {"type":["object","null"]},
            "replay": {"type":"string", "enum":["deterministic","convergent","fixture_replayable","model_nondeterministic","external_nondeterministic","destructive","ambiguous"]},
            "provenance": {"type":["object","null"]}
        },
        "required": ["version","call_id","tool_id","kind","summary","data","artifacts","error","replay","provenance"],
        "additionalProperties": false
    })
}

fn tool(
    invocation: ToolInvocation,
    description: &str,
    schema_tokens: u32,
    input_schema: Value,
) -> ToolDesc {
    ToolDesc {
        id: ToolId::new(
            invocation
                .id()
                .expect("available tool invocation has an id"),
        ),
        description: description.into(),
        input_schema,
        output_schema: canonical_tool_output_schema(),
        replay: invocation.replay(),
        policy: invocation
            .policy()
            .expect("available tool invocation has a policy"),
        invocation,
        operations: invocation.operations(),
        schema_tokens,
    }
}

fn unavailable(id: &str, description: &str, policy: ToolPolicy) -> ToolDesc {
    ToolDesc {
        id: ToolId::new(id),
        description: description.into(),
        input_schema: object_schema(json!({}), &[]),
        output_schema: canonical_tool_output_schema(),
        replay: ToolInvocation::Unavailable.replay(),
        policy,
        invocation: ToolInvocation::Unavailable,
        operations: ToolInvocation::Unavailable.operations(),
        schema_tokens: 0,
    }
}
