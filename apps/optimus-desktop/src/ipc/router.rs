//! Exact method ownership and shared dispatcher.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Domain {
    System,
    Sessions,
    Scheduling,
    Runtime,
    Files,
    Chat,
    Os,
    Consoles,
}

const METHOD_DOMAINS: &[(&str, Domain)] = &[
    ("ping", Domain::System),
    ("doctor", Domain::System),
    ("window_minimize", Domain::Os),
    ("window_maximize", Domain::Os),
    ("window_close", Domain::Os),
    ("window_drag", Domain::Os),
    ("window_outer_position", Domain::Os),
    ("window_set_outer_position", Domain::Os),
    ("pick_folder", Domain::Os),
    ("project_root_stage_native", Domain::Os),
    ("auth_status", Domain::System),
    ("auth_import_hermes", Domain::System),
    ("auth_import_cli", Domain::System),
    ("settings_get", Domain::System),
    ("settings_set", Domain::System),
    ("sessions", Domain::Sessions),
    ("delete_session", Domain::Sessions),
    ("rename_session", Domain::Sessions),
    ("session_search", Domain::Sessions),
    ("archive_session", Domain::Sessions),
    ("pin_session", Domain::Sessions),
    ("open_path", Domain::Os),
    ("open_url", Domain::Os),
    ("new_session", Domain::Sessions),
    ("get_session", Domain::Sessions),
    ("cron_list", Domain::Scheduling),
    ("cron_add", Domain::Scheduling),
    ("cron_tick", Domain::Scheduling),
    ("cron_set_enabled", Domain::Scheduling),
    ("cron_remove", Domain::Scheduling),
    ("cron_history", Domain::Scheduling),
    ("approvals_list", Domain::Runtime),
    ("approvals_grant", Domain::Runtime),
    ("jobs_list", Domain::Runtime),
    ("campaign_list", Domain::Runtime),
    ("campaign_create", Domain::Runtime),
    ("campaign_run", Domain::Runtime),
    ("campaign_status", Domain::Runtime),
    ("fs_roots", Domain::Files),
    ("fs_list", Domain::Files),
    ("fs_read", Domain::Files),
    ("project_scopes_list", Domain::Files),
    ("project_scopes_authorize", Domain::Files),
    ("artifacts_list", Domain::Files),
    ("artifacts_put_text", Domain::Files),
    ("artifacts_get", Domain::Files),
    ("artifacts_delete", Domain::Files),
    ("artifacts_delete_many", Domain::Files),
    ("artifacts_export", Domain::Files),
    ("artifacts_export_zip", Domain::Files),
    ("term_run", Domain::Runtime),
    ("browser_navigate", Domain::Runtime),
    ("browser_click", Domain::Runtime),
    ("browser_reload", Domain::Runtime),
    ("chat", Domain::Chat),
    ("chat_offline", Domain::Chat),
    ("chat_approval_resolve", Domain::Chat),
    ("skills_list", Domain::Consoles),
    ("skills_pin", Domain::Consoles),
    ("skills_deprecate", Domain::Consoles),
    ("memory_list", Domain::Consoles),
    ("memory_recall", Domain::Consoles),
    ("memory_correct", Domain::Consoles),
    ("memory_forget", Domain::Consoles),
    ("packs_state", Domain::Consoles),
    ("packs_activate", Domain::Consoles),
    ("packs_deactivate", Domain::Consoles),
    ("logs_tail", Domain::Consoles),
    ("commands_list", Domain::Consoles),
];

fn classify(method: &str) -> Option<Domain> {
    METHOD_DOMAINS
        .iter()
        .find_map(|(name, domain)| (*name == method).then_some(*domain))
}

#[cfg(test)]
fn domain_recognizes(domain: Domain, method: &str) -> bool {
    match domain {
        Domain::System => super::system::owns(method),
        Domain::Sessions => super::sessions::owns(method),
        Domain::Scheduling => super::scheduling::owns(method),
        Domain::Runtime => super::runtime_ops::owns(method),
        Domain::Files => super::files::owns(method),
        Domain::Chat => super::chat::owns(method),
        Domain::Os => super::os::owns(method),
        Domain::Consoles => super::consoles::owns(method),
    }
}

pub(crate) fn handle_ipc(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match classify(method) {
        Some(Domain::System) => super::system::handle(home, method, params),
        Some(Domain::Sessions) => super::sessions::handle(home, method, params),
        Some(Domain::Scheduling) => super::scheduling::handle(home, method, params),
        Some(Domain::Runtime) => super::runtime_ops::handle(home, method, params),
        Some(Domain::Files) => super::files::handle(home, method, params),
        Some(Domain::Chat) => super::chat::handle(home, method, params),
        Some(Domain::Os) => super::os::handle(home, method, params),
        Some(Domain::Consoles) => super::consoles::handle(home, method, params),
        None => Err(format!("unknown method: {method}")),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{domain_recognizes, Domain, METHOD_DOMAINS};

    const EXPECTED: &[(&str, Domain)] = &[
        ("ping", Domain::System),
        ("doctor", Domain::System),
        ("window_minimize", Domain::Os),
        ("window_maximize", Domain::Os),
        ("window_close", Domain::Os),
        ("window_drag", Domain::Os),
        ("window_outer_position", Domain::Os),
        ("window_set_outer_position", Domain::Os),
        ("pick_folder", Domain::Os),
        ("project_root_stage_native", Domain::Os),
        ("auth_status", Domain::System),
        ("auth_import_hermes", Domain::System),
        ("auth_import_cli", Domain::System),
        ("settings_get", Domain::System),
        ("settings_set", Domain::System),
        ("sessions", Domain::Sessions),
        ("delete_session", Domain::Sessions),
        ("rename_session", Domain::Sessions),
        ("session_search", Domain::Sessions),
        ("archive_session", Domain::Sessions),
        ("pin_session", Domain::Sessions),
        ("open_path", Domain::Os),
        ("open_url", Domain::Os),
        ("new_session", Domain::Sessions),
        ("get_session", Domain::Sessions),
        ("cron_list", Domain::Scheduling),
        ("cron_add", Domain::Scheduling),
        ("cron_tick", Domain::Scheduling),
        ("cron_set_enabled", Domain::Scheduling),
        ("cron_remove", Domain::Scheduling),
        ("cron_history", Domain::Scheduling),
        ("approvals_list", Domain::Runtime),
        ("approvals_grant", Domain::Runtime),
        ("jobs_list", Domain::Runtime),
        ("campaign_list", Domain::Runtime),
        ("campaign_create", Domain::Runtime),
        ("campaign_run", Domain::Runtime),
        ("campaign_status", Domain::Runtime),
        ("fs_roots", Domain::Files),
        ("fs_list", Domain::Files),
        ("fs_read", Domain::Files),
        ("project_scopes_list", Domain::Files),
        ("project_scopes_authorize", Domain::Files),
        ("artifacts_list", Domain::Files),
        ("artifacts_put_text", Domain::Files),
        ("artifacts_get", Domain::Files),
        ("artifacts_delete", Domain::Files),
        ("artifacts_delete_many", Domain::Files),
        ("artifacts_export", Domain::Files),
        ("artifacts_export_zip", Domain::Files),
        ("term_run", Domain::Runtime),
        ("browser_navigate", Domain::Runtime),
        ("browser_click", Domain::Runtime),
        ("browser_reload", Domain::Runtime),
        ("chat", Domain::Chat),
        ("chat_offline", Domain::Chat),
        ("chat_approval_resolve", Domain::Chat),
        ("skills_list", Domain::Consoles),
        ("skills_pin", Domain::Consoles),
        ("skills_deprecate", Domain::Consoles),
        ("memory_list", Domain::Consoles),
        ("memory_recall", Domain::Consoles),
        ("memory_correct", Domain::Consoles),
        ("memory_forget", Domain::Consoles),
        ("packs_state", Domain::Consoles),
        ("packs_activate", Domain::Consoles),
        ("packs_deactivate", Domain::Consoles),
        ("logs_tail", Domain::Consoles),
        ("commands_list", Domain::Consoles),
    ];

    #[test]
    fn method_registry_matches_the_frozen_contract_and_handlers() {
        assert_eq!(METHOD_DOMAINS, EXPECTED);
        assert_eq!(
            METHOD_DOMAINS
                .iter()
                .map(|(method, _)| *method)
                .collect::<HashSet<_>>()
                .len(),
            METHOD_DOMAINS.len()
        );
        for (method, domain) in METHOD_DOMAINS {
            assert!(domain_recognizes(*domain, method), "{method} -> {domain:?}");
        }
    }
}
