//! Progressive packs: budget, limits, core pin.

use optimus_packs::{
    builtin_catalog, ArtifactRef, CapabilitySession, DurableEffectProvenance, PackBudgetConfig,
    PackError, PackId, ReplayClass, ToolErrorDetail, ToolInvocation, ToolOutcome, ToolOutcomeKind,
    ToolPolicy,
};
use serde_json::json;
use uuid::Uuid;

#[test]
fn starts_with_core_only_under_budget() {
    let s = CapabilitySession::with_defaults();
    assert_eq!(s.loaded_packs(), vec![PackId::Core]);
    assert!(s.schema_tokens() <= 2500);
    assert!(s.loaded_tools().iter().any(|t| t.id.as_str() == "terminal"));
    assert!(!s
        .loaded_tools()
        .iter()
        .any(|t| t.id.as_str() == "browser_navigate"));
}

#[test]
fn activate_browser_increases_schema_tokens() {
    let mut s = CapabilitySession::with_defaults();
    let before = s.schema_tokens();
    s.activate(PackId::Browser).unwrap();
    assert!(s.schema_tokens() > before);
    assert!(s
        .loaded_tools()
        .iter()
        .any(|t| t.id.as_str() == "browser_navigate"));
    assert_eq!(s.activations, vec![PackId::Core, PackId::Browser]);
}

#[test]
fn on_demand_pack_limit_is_two() {
    let mut s = CapabilitySession::with_defaults();
    s.activate(PackId::Browser).unwrap();
    s.activate(PackId::Devex).unwrap();
    let err = s.activate(PackId::Social).unwrap_err();
    assert_eq!(err, PackError::PackLimit { loaded: 2, max: 2 });
}

#[test]
fn schema_budget_blocks_heavy_stack() {
    // Tiny budget: core alone may fit; an invocable browser pack must not.
    let core_tokens = builtin_catalog()
        .get(&PackId::Core)
        .unwrap()
        .schema_tokens();
    let mut s = CapabilitySession::new(PackBudgetConfig {
        max_on_demand_packs: 5,
        max_schema_tokens: core_tokens + 100,
    })
    .unwrap();
    let err = s.activate(PackId::Browser).unwrap_err();
    match err {
        PackError::SchemaBudget { would_be, max } => {
            assert!(would_be > max);
            assert_eq!(max, core_tokens + 100);
        }
        other => panic!("unexpected {other:?}"),
    }
    // core still only
    assert_eq!(s.loaded_packs(), vec![PackId::Core]);
}

#[test]
fn cannot_deactivate_core() {
    let mut s = CapabilitySession::with_defaults();
    assert_eq!(s.deactivate(PackId::Core), Err(PackError::CorePinned));
}

#[test]
fn deactivate_on_demand_frees_slot() {
    let mut s = CapabilitySession::with_defaults();
    s.activate(PackId::Browser).unwrap();
    s.activate(PackId::Devex).unwrap();
    s.deactivate(PackId::Browser).unwrap();
    s.activate(PackId::Social).unwrap();
    assert!(s.loaded_packs().contains(&PackId::Social));
    assert!(!s.loaded_packs().contains(&PackId::Browser));
}

#[test]
fn canonical_descriptor_owns_schema_policy_and_invocation() {
    let s = CapabilitySession::with_defaults();
    let read = s.resolve_loaded_tool("read_file").unwrap();
    assert_eq!(read.id.as_str(), "read_file");
    assert_eq!(read.policy, ToolPolicy::WorkspaceRead);
    assert_eq!(read.invocation, ToolInvocation::ReadFile);
    assert_eq!(read.input_schema["type"], "object");
    assert_eq!(read.input_schema["additionalProperties"], false);
    assert!(read.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "path"));
}

#[test]
fn canonical_tool_outcome_roundtrips_and_validates() {
    let outcome = ToolOutcome {
        version: 1,
        call_id: "call-1".into(),
        tool_id: "write_file".into(),
        kind: ToolOutcomeKind::Succeeded,
        summary: "wrote causal.txt".into(),
        data: json!({"path":"causal.txt"}),
        artifacts: vec![ArtifactRef {
            id: "artifact-1".into(),
            media_type: "text/plain".into(),
            path: Some("causal.txt".into()),
            uri: None,
            sha256: Some("a".repeat(64)),
            bytes: Some(6),
        }],
        error: None,
        replay: ReplayClass::Convergent,
        provenance: Some(DurableEffectProvenance {
            job_id: Uuid::new_v4(),
            node_id: Uuid::new_v4(),
            effect_attempt_id: Uuid::new_v4(),
            effect_sha256: "b".repeat(64),
            receipt_sha256: Some("c".repeat(64)),
        }),
    };

    outcome.validate().unwrap();
    let encoded = serde_json::to_value(&outcome).unwrap();
    assert_eq!(encoded["kind"], "succeeded");
    assert_eq!(encoded["replay"], "convergent");
    assert_eq!(
        serde_json::from_value::<ToolOutcome>(encoded).unwrap(),
        outcome
    );
}

#[test]
fn canonical_tool_outcome_rejects_inconsistent_or_unbounded_values() {
    let base = ToolOutcome {
        version: 1,
        call_id: "call-1".into(),
        tool_id: "read_file".into(),
        kind: ToolOutcomeKind::Succeeded,
        summary: "read file".into(),
        data: json!({}),
        artifacts: vec![],
        error: None,
        replay: ReplayClass::Deterministic,
        provenance: None,
    };

    let mut failed_without_error = base.clone();
    failed_without_error.kind = ToolOutcomeKind::Failed;
    assert!(matches!(
        failed_without_error.validate(),
        Err(PackError::InvalidOutcome { .. })
    ));

    let mut success_with_error = base.clone();
    success_with_error.error = Some(ToolErrorDetail {
        code: "unexpected".into(),
        message: "must not be present".into(),
        retryable: false,
    });
    assert!(success_with_error.validate().is_err());

    let mut empty_call = base.clone();
    empty_call.call_id.clear();
    assert!(empty_call.validate().is_err());

    let mut oversized = base.clone();
    oversized.summary = "x".repeat(4097);
    assert!(oversized.validate().is_err());

    let mut bad_hash = base;
    bad_hash.artifacts.push(ArtifactRef {
        id: "artifact-1".into(),
        media_type: "text/plain".into(),
        path: None,
        uri: Some("artifact://one".into()),
        sha256: Some("not-a-hash".into()),
        bytes: None,
    });
    assert!(bad_hash.validate().is_err());
}

#[test]
fn ambiguous_outcome_requires_ambiguous_replay_class() {
    let outcome = ToolOutcome {
        version: 1,
        call_id: "call-1".into(),
        tool_id: "terminal".into(),
        kind: ToolOutcomeKind::Ambiguous,
        summary: "command outcome unknown".into(),
        data: json!({}),
        artifacts: vec![],
        error: Some(ToolErrorDetail {
            code: "external_outcome_unknown".into(),
            message: "command may have run".into(),
            retryable: false,
        }),
        replay: ReplayClass::ExternalNondeterministic,
        provenance: None,
    };
    assert!(outcome.validate().is_err());
}

#[test]
fn tool_resolution_fails_closed_for_unknown_unloaded_and_unavailable() {
    let mut s = CapabilitySession::with_defaults();
    assert_eq!(
        s.resolve_loaded_tool("does_not_exist").unwrap_err(),
        PackError::UnknownTool("does_not_exist".into())
    );
    assert_eq!(
        s.resolve_loaded_tool("browser_navigate").unwrap_err(),
        PackError::ToolNotLoaded {
            tool: "browser_navigate".into(),
            pack: "browser".into(),
        }
    );
    s.activate(PackId::Desktop).unwrap();
    assert_eq!(
        s.resolve_loaded_tool("desktop_click").unwrap_err(),
        PackError::ToolUnavailable("desktop_click".into())
    );
}

#[test]
fn catalog_rejects_duplicate_tool_identity() {
    let mut catalog = builtin_catalog();
    catalog.get_mut(&PackId::Browser).unwrap().tools[0].id = "read_file".into();
    assert!(matches!(
        CapabilitySession::try_from_catalog(PackBudgetConfig::default(), catalog).unwrap_err(),
        PackError::DuplicateTool { tool, .. } if tool == "read_file"
    ));
}

#[test]
fn catalog_rejects_policy_or_invocation_identity_drift() {
    let mut catalog = builtin_catalog();
    let read = &mut catalog.get_mut(&PackId::Core).unwrap().tools[0];
    read.policy = ToolPolicy::Process;
    assert!(matches!(
        CapabilitySession::try_from_catalog(PackBudgetConfig::default(), catalog).unwrap_err(),
        PackError::DescriptorPolicyMismatch { tool, .. } if tool == "read_file"
    ));
}

#[test]
fn canonical_descriptor_owns_output_schema_and_replay_class() {
    let catalog = builtin_catalog();
    let core = &catalog[&PackId::Core];
    let read = core
        .tools
        .iter()
        .find(|tool| tool.id.as_str() == "read_file")
        .unwrap();
    let write = core
        .tools
        .iter()
        .find(|tool| tool.id.as_str() == "write_file")
        .unwrap();
    assert_eq!(read.output_schema["properties"]["kind"]["type"], "string");
    assert_eq!(read.output_schema["additionalProperties"], false);
    assert_eq!(read.replay, ReplayClass::Deterministic);
    assert_eq!(write.replay, ReplayClass::Convergent);
}

#[test]
fn descriptor_rejects_outcome_identity_or_replay_drift() {
    let catalog = builtin_catalog();
    let read = catalog[&PackId::Core]
        .tools
        .iter()
        .find(|tool| tool.id.as_str() == "read_file")
        .unwrap();
    let mut outcome = ToolOutcome::succeeded(
        "call-1",
        "write_file",
        "wrong tool",
        json!({}),
        ReplayClass::Deterministic,
    );
    assert!(read.validate_outcome(&outcome).is_err());
    outcome.tool_id = "read_file".into();
    outcome.replay = ReplayClass::Destructive;
    assert!(read.validate_outcome(&outcome).is_err());
}

#[test]
fn catalog_rejects_output_schema_or_replay_drift() {
    let mut bad_schema = builtin_catalog();
    bad_schema.get_mut(&PackId::Core).unwrap().tools[0].output_schema = json!({});
    assert!(matches!(
        CapabilitySession::try_from_catalog(PackBudgetConfig::default(), bad_schema).unwrap_err(),
        PackError::InvalidOutputSchema { tool, .. } if tool == "read_file"
    ));

    let mut bad_replay = builtin_catalog();
    bad_replay.get_mut(&PackId::Core).unwrap().tools[0].replay = ReplayClass::Destructive;
    assert!(matches!(
        CapabilitySession::try_from_catalog(PackBudgetConfig::default(), bad_replay).unwrap_err(),
        PackError::DescriptorReplayMismatch { tool, .. } if tool == "read_file"
    ));
}

#[test]
fn catalog_rejects_schema_constraints_runtime_does_not_enforce() {
    let mut catalog = builtin_catalog();
    catalog.get_mut(&PackId::Core).unwrap().tools[0].input_schema["properties"]["path"]
        ["minLength"] = serde_json::json!(1);
    assert!(matches!(
        CapabilitySession::try_from_catalog(PackBudgetConfig::default(), catalog).unwrap_err(),
        PackError::InvalidInputSchema { tool, reason }
            if tool == "read_file" && reason.contains("unsupported keyword minLength")
    ));
}

#[test]
fn catalog_rejects_malformed_required_properties() {
    let mut catalog = builtin_catalog();
    catalog.get_mut(&PackId::Core).unwrap().tools[0].input_schema["required"] =
        serde_json::json!(["missing_property", 7]);
    assert!(matches!(
        CapabilitySession::try_from_catalog(PackBudgetConfig::default(), catalog).unwrap_err(),
        PackError::InvalidInputSchema { tool, .. } if tool == "read_file"
    ));
}

#[test]
fn catalog_rejects_availability_schema_token_drift() {
    let mut unavailable_costs_tokens = builtin_catalog();
    unavailable_costs_tokens
        .get_mut(&PackId::Desktop)
        .unwrap()
        .tools[0]
        .schema_tokens = 1;
    assert!(matches!(
        CapabilitySession::try_from_catalog(
            PackBudgetConfig::default(),
            unavailable_costs_tokens,
        )
        .unwrap_err(),
        PackError::DescriptorSchemaTokensMismatch { tool, .. }
            if tool == "desktop_screenshot"
    ));

    let mut available_is_free = builtin_catalog();
    available_is_free.get_mut(&PackId::Core).unwrap().tools[0].schema_tokens = 0;
    assert!(matches!(
        CapabilitySession::try_from_catalog(PackBudgetConfig::default(), available_is_free)
            .unwrap_err(),
        PackError::DescriptorSchemaTokensMismatch { tool, .. } if tool == "read_file"
    ));
}

#[test]
fn catalog_and_loaded_totals_reject_schema_token_overflow() {
    let mut overflowing_pack = builtin_catalog();
    let core = &mut overflowing_pack.get_mut(&PackId::Core).unwrap().tools;
    core[0].schema_tokens = u32::MAX;
    assert!(matches!(
        CapabilitySession::try_from_catalog(PackBudgetConfig::default(), overflowing_pack)
            .unwrap_err(),
        PackError::SchemaTokenOverflow { scope } if scope == "core"
    ));

    let mut overflowing_loaded = builtin_catalog();
    let core = &mut overflowing_loaded.get_mut(&PackId::Core).unwrap().tools;
    let available_count = core.iter().filter(|tool| tool.is_available()).count();
    let remainder = (available_count - 1) as u32;
    for tool in core.iter_mut().filter(|tool| tool.is_available()) {
        tool.schema_tokens = 1;
    }
    core[0].schema_tokens = u32::MAX - remainder;
    let mut session = CapabilitySession::try_from_catalog(
        PackBudgetConfig {
            max_on_demand_packs: 2,
            max_schema_tokens: u32::MAX,
        },
        overflowing_loaded,
    )
    .unwrap();
    assert!(matches!(
        session.activate(PackId::Browser),
        Err(PackError::SchemaTokenOverflow { scope }) if scope == "loaded packs"
    ));
    assert_eq!(session.loaded_packs(), vec![PackId::Core]);
}

#[test]
fn restore_reapplies_current_schema_budget() {
    let core_tokens = builtin_catalog()[&PackId::Core].schema_tokens();
    let mut s = CapabilitySession::new(PackBudgetConfig {
        max_on_demand_packs: 2,
        max_schema_tokens: core_tokens + 100,
    })
    .unwrap();
    assert!(matches!(
        s.restore_loaded(&[PackId::Browser]),
        Err(PackError::SchemaBudget { .. })
    ));
    assert_eq!(s.loaded_packs(), vec![PackId::Core]);
}

#[test]
fn canonical_schema_validates_runtime_arguments() {
    let s = CapabilitySession::with_defaults();
    let read = s.resolve_loaded_tool("read_file").unwrap();
    read.validate_arguments(&serde_json::json!({"path":"notes.txt"}))
        .unwrap();
    assert!(matches!(
        read.validate_arguments(&serde_json::json!({"path":"notes.txt","escape":true})),
        Err(PackError::InvalidArguments { tool, .. }) if tool == "read_file"
    ));
    let terminal = s.resolve_loaded_tool("terminal").unwrap();
    assert!(matches!(
        terminal.validate_arguments(&serde_json::json!({"program":42})),
        Err(PackError::InvalidArguments { tool, .. }) if tool == "terminal"
    ));
}

#[test]
fn canonical_schema_enforces_enum_minimum_and_string_array_items() {
    let s = CapabilitySession::with_defaults();

    let activate = s.resolve_loaded_tool("activate_pack").unwrap();
    activate
        .validate_arguments(&serde_json::json!({"name":"browser"}))
        .unwrap();
    assert!(matches!(
        activate.validate_arguments(&serde_json::json!({"name":"core"})),
        Err(PackError::InvalidArguments { tool, .. }) if tool == "activate_pack"
    ));

    let search = s.resolve_loaded_tool("web_search").unwrap();
    search
        .validate_arguments(&serde_json::json!({"query":"x","limit":1}))
        .unwrap();
    assert!(matches!(
        search.validate_arguments(&serde_json::json!({"query":"x","limit":0})),
        Err(PackError::InvalidArguments { tool, .. }) if tool == "web_search"
    ));

    let terminal = s.resolve_loaded_tool("terminal").unwrap();
    terminal
        .validate_arguments(&serde_json::json!({"program":"cmd","args":["/C","echo ok"]}))
        .unwrap();
    assert!(matches!(
        terminal.validate_arguments(&serde_json::json!({"program":"cmd","args":["/C",7]})),
        Err(PackError::InvalidArguments { tool, .. }) if tool == "terminal"
    ));
}

#[test]
fn unavailable_catalog_entries_retain_honest_future_policy_identity() {
    let catalog = builtin_catalog();
    assert!(catalog[&PackId::Desktop]
        .tools
        .iter()
        .all(|tool| tool.policy == ToolPolicy::Desktop));
    assert!(catalog[&PackId::Media]
        .tools
        .iter()
        .all(|tool| tool.policy == ToolPolicy::Media));
    assert_eq!(
        catalog[&PackId::Social].tools[1].policy,
        ToolPolicy::NetworkWrite
    );
}

#[test]
fn construction_rejects_core_over_schema_budget() {
    let core_tokens = builtin_catalog()[&PackId::Core].schema_tokens();
    assert!(matches!(
        CapabilitySession::new(PackBudgetConfig {
            max_on_demand_packs: 2,
            max_schema_tokens: core_tokens - 1,
        }),
        Err(PackError::SchemaBudget { would_be, max })
            if would_be == core_tokens && max == core_tokens - 1
    ));
}
