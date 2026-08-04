//! Exact method ownership and shared dispatcher.

use std::path::PathBuf;

use crate::scope::ScopePolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Domain {
    System,
    Developer,
    Sessions,
    Scheduling,
    Runtime,
    Files,
    Chat,
    Os,
    Consoles,
    Messaging,
    Extensibility,
}

// Third column: the method's project-scope assertion (criterion C2).
// `None` = unasserted (allowlisted, shrink-only, in
// scripts/check-project-scope-assertions.py); `Some(_)` is enforced by
// `crate::scope::enforce` before dispatch, so a declaration here is
// load-bearing, never documentation. New methods must declare at birth.
const METHOD_DOMAINS: &[(&str, Domain, Option<ScopePolicy>)] = &[
    ("ping", Domain::System, None),
    ("doctor", Domain::System, None),
    ("window_minimize", Domain::Os, None),
    ("window_maximize", Domain::Os, None),
    ("window_close", Domain::Os, None),
    ("window_drag", Domain::Os, None),
    ("window_outer_position", Domain::Os, None),
    ("window_set_outer_position", Domain::Os, None),
    ("pick_folder", Domain::Os, None),
    ("startup_context", Domain::Os, Some(ScopePolicy::Host)),
    ("project_root_stage_native", Domain::Os, None),
    ("auth_status", Domain::System, None),
    ("auth_import_hermes", Domain::System, None),
    ("auth_import_cli", Domain::System, None),
    (
        "provider_keys_status",
        Domain::System,
        Some(ScopePolicy::Host),
    ),
    ("provider_key_set", Domain::System, Some(ScopePolicy::Host)),
    (
        "provider_key_clear",
        Domain::System,
        Some(ScopePolicy::Host),
    ),
    ("settings_get", Domain::System, None),
    ("settings_set", Domain::System, None),
    (
        "developer_access_get",
        Domain::Developer,
        Some(ScopePolicy::Host),
    ),
    (
        "developer_access_enable",
        Domain::Developer,
        Some(ScopePolicy::Host),
    ),
    (
        "developer_access_revoke",
        Domain::Developer,
        Some(ScopePolicy::Host),
    ),
    (
        "developer_supervisor_status",
        Domain::Developer,
        Some(ScopePolicy::Host),
    ),
    (
        "developer_supervisor_launch",
        Domain::Developer,
        Some(ScopePolicy::Host),
    ),
    (
        "developer_supervisor_build_launch",
        Domain::Developer,
        Some(ScopePolicy::Host),
    ),
    (
        "developer_supervisor_stop",
        Domain::Developer,
        Some(ScopePolicy::Host),
    ),
    (
        "developer_supervisor_restart",
        Domain::Developer,
        Some(ScopePolicy::Host),
    ),
    (
        "developer_supervisor_rollback",
        Domain::Developer,
        Some(ScopePolicy::Host),
    ),
    (
        "developer_supervisor_log",
        Domain::Developer,
        Some(ScopePolicy::Host),
    ),
    (
        "developer_emergency_stop",
        Domain::Developer,
        Some(ScopePolicy::Host),
    ),
    ("sessions", Domain::Sessions, None),
    ("delete_session", Domain::Sessions, None),
    ("rename_session", Domain::Sessions, None),
    ("session_search", Domain::Sessions, None),
    ("archive_session", Domain::Sessions, None),
    ("pin_session", Domain::Sessions, None),
    ("open_path", Domain::Os, None),
    ("open_url", Domain::Os, None),
    ("new_session", Domain::Sessions, None),
    ("get_session", Domain::Sessions, None),
    ("cron_list", Domain::Scheduling, None),
    ("cron_add", Domain::Scheduling, None),
    ("cron_tick", Domain::Scheduling, None),
    ("cron_set_enabled", Domain::Scheduling, None),
    ("cron_remove", Domain::Scheduling, None),
    ("cron_history", Domain::Scheduling, None),
    ("approvals_list", Domain::Runtime, None),
    ("approvals_grant", Domain::Runtime, None),
    ("approvals_release_yolo", Domain::Runtime, None),
    ("jobs_list", Domain::Runtime, None),
    ("campaign_list", Domain::Runtime, None),
    ("campaign_create", Domain::Runtime, None),
    ("campaign_run", Domain::Runtime, None),
    ("campaign_status", Domain::Runtime, None),
    ("fs_roots", Domain::Files, None),
    ("fs_list", Domain::Files, None),
    ("fs_read", Domain::Files, None),
    ("project_scopes_list", Domain::Files, None),
    ("project_scopes_authorize", Domain::Files, None),
    ("artifacts_list", Domain::Files, None),
    ("artifacts_put_text", Domain::Files, None),
    ("artifacts_get", Domain::Files, None),
    ("artifacts_delete", Domain::Files, None),
    ("artifacts_delete_many", Domain::Files, None),
    ("artifacts_export", Domain::Files, None),
    ("artifacts_export_zip", Domain::Files, None),
    ("term_run", Domain::Runtime, None),
    ("browser_navigate", Domain::Runtime, None),
    ("browser_click", Domain::Runtime, None),
    ("browser_reload", Domain::Runtime, None),
    ("chat", Domain::Chat, None),
    ("chat_offline", Domain::Chat, None),
    ("chat_approval_resolve", Domain::Chat, None),
    ("skills_list", Domain::Consoles, None),
    ("skills_pin", Domain::Consoles, None),
    ("skills_deprecate", Domain::Consoles, None),
    ("memory_list", Domain::Consoles, None),
    ("memory_recall", Domain::Consoles, None),
    ("memory_search", Domain::Consoles, Some(ScopePolicy::Host)),
    ("memory_correct", Domain::Consoles, None),
    ("memory_forget", Domain::Consoles, None),
    ("packs_state", Domain::Consoles, None),
    ("packs_activate", Domain::Consoles, None),
    ("packs_deactivate", Domain::Consoles, None),
    ("logs_tail", Domain::Consoles, None),
    ("commands_list", Domain::Consoles, None),
    ("gateway_status", Domain::Messaging, None),
    ("gateway_inbox", Domain::Messaging, None),
    ("gateway_outbox", Domain::Messaging, None),
    ("gateway_enqueue", Domain::Messaging, None),
    ("gateway_ambiguous", Domain::Messaging, None),
    ("gateway_ack_delivery", Domain::Messaging, None),
    ("gateway_telegram_status", Domain::Messaging, None),
    ("providers_catalog", Domain::Extensibility, None),
    ("providers_route_preview", Domain::Extensibility, None),
    ("mcp_status", Domain::Extensibility, None),
    ("mcp_tools", Domain::Extensibility, None),
    ("packs_verify_signed", Domain::Extensibility, None),
];

fn classify(method: &str) -> Option<Domain> {
    METHOD_DOMAINS
        .iter()
        .find_map(|(name, domain, _)| (*name == method).then_some(*domain))
}

fn scope_policy(method: &str) -> Option<ScopePolicy> {
    METHOD_DOMAINS
        .iter()
        .find_map(|(name, _, policy)| (*name == method).then_some(*policy))
        .flatten()
}

#[cfg(test)]
fn domain_recognizes(domain: Domain, method: &str) -> bool {
    match domain {
        Domain::System => crate::system::owns(method),
        Domain::Developer => crate::developer::owns(method),
        Domain::Sessions => crate::sessions::owns(method),
        Domain::Scheduling => crate::scheduling::owns(method),
        Domain::Runtime => crate::runtime_ops::owns(method),
        Domain::Files => crate::files::owns(method),
        Domain::Chat => crate::chat::owns(method),
        Domain::Os => crate::os::owns(method),
        Domain::Consoles => crate::consoles::owns(method),
        Domain::Messaging => crate::messaging::owns(method),
        Domain::Extensibility => crate::extensibility::owns(method),
    }
}

pub fn handle_ipc(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let Some(domain) = classify(method) else {
        return Err(format!("unknown method: {method}"));
    };
    crate::scope::enforce(scope_policy(method), home, method, &params)?;
    match domain {
        Domain::System => crate::system::handle(home, method, params),
        Domain::Developer => crate::developer::handle(home, method, params),
        Domain::Sessions => crate::sessions::handle(home, method, params),
        Domain::Scheduling => crate::scheduling::handle(home, method, params),
        Domain::Runtime => crate::runtime_ops::handle(home, method, params),
        Domain::Files => crate::files::handle(home, method, params),
        Domain::Chat => crate::chat::handle(home, method, params),
        Domain::Os => crate::os::handle(home, method, params),
        Domain::Consoles => crate::consoles::handle(home, method, params),
        Domain::Messaging => crate::messaging::handle(home, method, params),
        Domain::Extensibility => crate::extensibility::handle(home, method, params),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::{domain_recognizes, handle_ipc, Domain, ScopePolicy, METHOD_DOMAINS};

    const EXPECTED: &[(&str, Domain, Option<ScopePolicy>)] = &[
        ("ping", Domain::System, None),
        ("doctor", Domain::System, None),
        ("window_minimize", Domain::Os, None),
        ("window_maximize", Domain::Os, None),
        ("window_close", Domain::Os, None),
        ("window_drag", Domain::Os, None),
        ("window_outer_position", Domain::Os, None),
        ("window_set_outer_position", Domain::Os, None),
        ("pick_folder", Domain::Os, None),
        ("startup_context", Domain::Os, Some(ScopePolicy::Host)),
        ("project_root_stage_native", Domain::Os, None),
        ("auth_status", Domain::System, None),
        ("auth_import_hermes", Domain::System, None),
        ("auth_import_cli", Domain::System, None),
        (
            "provider_keys_status",
            Domain::System,
            Some(ScopePolicy::Host),
        ),
        ("provider_key_set", Domain::System, Some(ScopePolicy::Host)),
        (
            "provider_key_clear",
            Domain::System,
            Some(ScopePolicy::Host),
        ),
        ("settings_get", Domain::System, None),
        ("settings_set", Domain::System, None),
        (
            "developer_access_get",
            Domain::Developer,
            Some(ScopePolicy::Host),
        ),
        (
            "developer_access_enable",
            Domain::Developer,
            Some(ScopePolicy::Host),
        ),
        (
            "developer_access_revoke",
            Domain::Developer,
            Some(ScopePolicy::Host),
        ),
        (
            "developer_supervisor_status",
            Domain::Developer,
            Some(ScopePolicy::Host),
        ),
        (
            "developer_supervisor_launch",
            Domain::Developer,
            Some(ScopePolicy::Host),
        ),
        (
            "developer_supervisor_build_launch",
            Domain::Developer,
            Some(ScopePolicy::Host),
        ),
        (
            "developer_supervisor_stop",
            Domain::Developer,
            Some(ScopePolicy::Host),
        ),
        (
            "developer_supervisor_restart",
            Domain::Developer,
            Some(ScopePolicy::Host),
        ),
        (
            "developer_supervisor_rollback",
            Domain::Developer,
            Some(ScopePolicy::Host),
        ),
        (
            "developer_supervisor_log",
            Domain::Developer,
            Some(ScopePolicy::Host),
        ),
        (
            "developer_emergency_stop",
            Domain::Developer,
            Some(ScopePolicy::Host),
        ),
        ("sessions", Domain::Sessions, None),
        ("delete_session", Domain::Sessions, None),
        ("rename_session", Domain::Sessions, None),
        ("session_search", Domain::Sessions, None),
        ("archive_session", Domain::Sessions, None),
        ("pin_session", Domain::Sessions, None),
        ("open_path", Domain::Os, None),
        ("open_url", Domain::Os, None),
        ("new_session", Domain::Sessions, None),
        ("get_session", Domain::Sessions, None),
        ("cron_list", Domain::Scheduling, None),
        ("cron_add", Domain::Scheduling, None),
        ("cron_tick", Domain::Scheduling, None),
        ("cron_set_enabled", Domain::Scheduling, None),
        ("cron_remove", Domain::Scheduling, None),
        ("cron_history", Domain::Scheduling, None),
        ("approvals_list", Domain::Runtime, None),
        ("approvals_grant", Domain::Runtime, None),
        ("approvals_release_yolo", Domain::Runtime, None),
        ("jobs_list", Domain::Runtime, None),
        ("campaign_list", Domain::Runtime, None),
        ("campaign_create", Domain::Runtime, None),
        ("campaign_run", Domain::Runtime, None),
        ("campaign_status", Domain::Runtime, None),
        ("fs_roots", Domain::Files, None),
        ("fs_list", Domain::Files, None),
        ("fs_read", Domain::Files, None),
        ("project_scopes_list", Domain::Files, None),
        ("project_scopes_authorize", Domain::Files, None),
        ("artifacts_list", Domain::Files, None),
        ("artifacts_put_text", Domain::Files, None),
        ("artifacts_get", Domain::Files, None),
        ("artifacts_delete", Domain::Files, None),
        ("artifacts_delete_many", Domain::Files, None),
        ("artifacts_export", Domain::Files, None),
        ("artifacts_export_zip", Domain::Files, None),
        ("term_run", Domain::Runtime, None),
        ("browser_navigate", Domain::Runtime, None),
        ("browser_click", Domain::Runtime, None),
        ("browser_reload", Domain::Runtime, None),
        ("chat", Domain::Chat, None),
        ("chat_offline", Domain::Chat, None),
        ("chat_approval_resolve", Domain::Chat, None),
        ("skills_list", Domain::Consoles, None),
        ("skills_pin", Domain::Consoles, None),
        ("skills_deprecate", Domain::Consoles, None),
        ("memory_list", Domain::Consoles, None),
        ("memory_recall", Domain::Consoles, None),
        ("memory_search", Domain::Consoles, Some(ScopePolicy::Host)),
        ("memory_correct", Domain::Consoles, None),
        ("memory_forget", Domain::Consoles, None),
        ("packs_state", Domain::Consoles, None),
        ("packs_activate", Domain::Consoles, None),
        ("packs_deactivate", Domain::Consoles, None),
        ("logs_tail", Domain::Consoles, None),
        ("commands_list", Domain::Consoles, None),
        ("gateway_status", Domain::Messaging, None),
        ("gateway_inbox", Domain::Messaging, None),
        ("gateway_outbox", Domain::Messaging, None),
        ("gateway_enqueue", Domain::Messaging, None),
        ("gateway_ambiguous", Domain::Messaging, None),
        ("gateway_ack_delivery", Domain::Messaging, None),
        ("gateway_telegram_status", Domain::Messaging, None),
        ("providers_catalog", Domain::Extensibility, None),
        ("providers_route_preview", Domain::Extensibility, None),
        ("mcp_status", Domain::Extensibility, None),
        ("mcp_tools", Domain::Extensibility, None),
        ("packs_verify_signed", Domain::Extensibility, None),
    ];

    #[test]
    fn method_registry_matches_the_frozen_contract_and_handlers() {
        assert_eq!(METHOD_DOMAINS, EXPECTED);
        assert_eq!(
            METHOD_DOMAINS
                .iter()
                .map(|(method, _, _)| *method)
                .collect::<HashSet<_>>()
                .len(),
            METHOD_DOMAINS.len()
        );
        for (method, domain, _) in METHOD_DOMAINS {
            assert!(domain_recognizes(*domain, method), "{method} -> {domain:?}");
        }
    }

    // Behavioural teeth for the scope column: every declared policy must be
    // observable at dispatch, without reaching the handler. Vacuous while all
    // methods are unasserted; each conversion is covered here automatically.
    #[test]
    fn declared_scope_policies_are_enforced_at_dispatch() {
        let home = tempfile::tempdir().unwrap();
        let home_buf = home.path().to_path_buf();
        for (method, _, policy) in METHOD_DOMAINS {
            match policy {
                None => {}
                Some(ScopePolicy::Project) => {
                    let err = handle_ipc(&home_buf, method, json!({})).unwrap_err();
                    assert_eq!(
                        err,
                        format!("method {method} is project-scoped and requires project_id")
                    );
                }
                Some(ScopePolicy::Host) => {
                    let err =
                        handle_ipc(&home_buf, method, json!({ "project_id": "p1" })).unwrap_err();
                    assert_eq!(
                        err,
                        format!("method {method} is host-scoped and does not accept project_id")
                    );
                }
            }
        }
    }

    // `memory_search` reads the claim ledger through the fixed console write
    // context, which never consults `project_id`. Declaring Host keeps a
    // caller from passing a project id that would be silently ignored — the
    // console would answer from a different scope than the caller asked for.
    #[test]
    fn memory_search_is_host_scoped_and_refuses_a_project_id() {
        let home = tempfile::tempdir().unwrap();
        let err = handle_ipc(
            &home.path().to_path_buf(),
            "memory_search",
            json!({ "query": "kettle", "project_id": "p1" }),
        )
        .unwrap_err();
        assert_eq!(
            err,
            "method memory_search is host-scoped and does not accept project_id"
        );
    }
}
