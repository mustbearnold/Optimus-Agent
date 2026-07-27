//! Project-scope assertions for the method registry (criterion C2 in
//! docs/architecture/north-star-2026-07.md).
//!
//! Every registry method eventually declares a `ScopePolicy`; dispatch
//! enforces the declaration before any handler runs, so an asserted method
//! cannot silently operate outside (or ignore) a project scope. Methods with
//! no declaration yet are unasserted — tracked by the shrinking allowlist in
//! scripts/check-project-scope-assertions.py, which counts assertions 0→82.

use std::path::Path;

use optimus_kernel::ProjectAuthorityStore;

/// A method's declared relationship to project scope. Declared in
/// `router::METHOD_DOMAINS`; enforced by [`enforce`] at dispatch.
// Variants are constructed by per-method conversions in METHOD_DOMAINS; until
// the first assertion lands they appear only in tests. Drop the allow then.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopePolicy {
    /// The method operates on project state: dispatch refuses to run it
    /// without a `project_id` naming an authorized `ProjectScope`.
    Project,
    /// The method is explicitly project-agnostic (host-level state only):
    /// dispatch refuses a `project_id`, so callers cannot pass one that
    /// would be silently ignored.
    Host,
}

/// Shared `project_id` param validation (shape only; authorization is the
/// authority store's job). Also used by the chat handlers, which currently
/// accept an *optional* project id — that looseness is theirs, not ours.
pub(crate) fn optional_project_id(params: &serde_json::Value) -> Result<Option<String>, String> {
    let Some(value) = params.get("project_id") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value
        .as_str()
        .ok_or_else(|| "project_id must be a string".to_string())?;
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("project_id must be a 1-128 ASCII identifier".into());
    }
    Ok(Some(value.to_string()))
}

/// Enforce a method's declared scope policy against the incoming params.
/// Runs before the handler; an error here means the handler is never reached.
pub(crate) fn enforce(
    policy: Option<ScopePolicy>,
    home: &Path,
    method: &str,
    params: &serde_json::Value,
) -> Result<(), String> {
    match policy {
        None => Ok(()),
        Some(ScopePolicy::Host) => match optional_project_id(params)? {
            None => Ok(()),
            Some(_) => Err(format!(
                "method {method} is host-scoped and does not accept project_id"
            )),
        },
        Some(ScopePolicy::Project) => {
            let Some(project_id) = optional_project_id(params)? else {
                return Err(format!(
                    "method {method} is project-scoped and requires project_id"
                ));
            };
            let store = ProjectAuthorityStore::open(home).map_err(|error| error.to_string())?;
            match store
                .scope(&project_id)
                .map_err(|error| error.to_string())?
            {
                Some(_) => Ok(()),
                None => Err(format!(
                    "project_id {project_id} is not an authorized project scope"
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{enforce, ScopePolicy};

    #[test]
    fn unasserted_methods_pass_through_untouched() {
        let home = tempfile::tempdir().unwrap();
        let params = json!({ "project_id": 42 });
        assert_eq!(enforce(None, home.path(), "ping", &params), Ok(()));
    }

    #[test]
    fn host_scoped_methods_reject_a_project_id() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            enforce(Some(ScopePolicy::Host), home.path(), "ping", &json!({})),
            Ok(())
        );
        assert_eq!(
            enforce(
                Some(ScopePolicy::Host),
                home.path(),
                "ping",
                &json!({ "project_id": null })
            ),
            Ok(())
        );
        let err = enforce(
            Some(ScopePolicy::Host),
            home.path(),
            "ping",
            &json!({ "project_id": "p1" }),
        )
        .unwrap_err();
        assert_eq!(
            err,
            "method ping is host-scoped and does not accept project_id"
        );
    }

    #[test]
    fn project_scoped_methods_require_a_project_id() {
        let home = tempfile::tempdir().unwrap();
        let err = enforce(
            Some(ScopePolicy::Project),
            home.path(),
            "fs_list",
            &json!({}),
        )
        .unwrap_err();
        assert_eq!(
            err,
            "method fs_list is project-scoped and requires project_id"
        );
    }

    #[test]
    fn project_scoped_methods_reject_a_malformed_project_id() {
        let home = tempfile::tempdir().unwrap();
        let err = enforce(
            Some(ScopePolicy::Project),
            home.path(),
            "fs_list",
            &json!({ "project_id": ["not-a-string"] }),
        )
        .unwrap_err();
        assert_eq!(err, "project_id must be a string");
    }

    #[test]
    fn project_scoped_methods_reject_an_unauthorized_project_id() {
        let home = tempfile::tempdir().unwrap();
        let err = enforce(
            Some(ScopePolicy::Project),
            home.path(),
            "fs_list",
            &json!({ "project_id": "ghost" }),
        )
        .unwrap_err();
        assert_eq!(err, "project_id ghost is not an authorized project scope");
    }

    #[test]
    fn project_scoped_methods_pass_with_an_authorized_project_id() {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let authority = optimus_kernel::ProjectAuthorityStore::open(home.path()).unwrap();
        let selection = authority.stage_native_selection(project.path()).unwrap();
        authority
            .authorize_project(
                "project-a",
                std::slice::from_ref(&selection.path),
                Some(&selection.path),
                std::slice::from_ref(&selection.grant_token),
            )
            .unwrap();
        assert_eq!(
            enforce(
                Some(ScopePolicy::Project),
                home.path(),
                "fs_list",
                &json!({ "project_id": "project-a" }),
            ),
            Ok(())
        );
    }
}
