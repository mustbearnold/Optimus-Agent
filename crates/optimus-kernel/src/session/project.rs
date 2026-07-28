//! Session ↔ project binding (criterion C1 in
//! docs/architecture/north-star-2026-07.md).
//!
//! A session belongs to at most one project for its whole lifetime. The
//! binding is recorded at creation and enforced on every project-scoped
//! resume: an unbound session (pre-column rows, and host-created sessions
//! reaching their first project turn) is adopted by the resuming project;
//! a session already bound to a different project is refused. This is what
//! makes the project-scoped `sessions` view trustworthy — a row can never
//! drift between projects as a side effect of being opened.

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{KernelError, Message, Result, SessionStore};

impl SessionStore {
    /// Create a session bound to `project` (`None` creates a host session).
    pub fn create_scoped(&self, title: &str, project: Option<&str>) -> Result<Uuid> {
        let id = self.create(title)?;
        if let Some(project) = project {
            self.bind_project(id, project)?;
        }
        Ok(id)
    }

    /// Bind `id` to `project`: adopt when unbound, no-op when already bound
    /// to the same project, refuse when bound to a different one.
    pub fn bind_project(&self, id: Uuid, project: &str) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE sessions SET project = ?1
             WHERE id = ?2 AND (project IS NULL OR project = ?1)",
            params![project, id.to_string()],
        )?;
        if n == 1 {
            return Ok(());
        }
        let bound: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT project FROM sessions WHERE id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        match bound.flatten() {
            None => Err(KernelError::Model(format!("session {id} not found"))),
            Some(other) => Err(KernelError::Tool(format!(
                "session {id} is bound to project {other}, not {project}"
            ))),
        }
    }

    /// [`Self::load_repairing_effect_transcript`] behind the project binding:
    /// when the caller resumes under a project scope, the binding is enforced
    /// before any transcript bytes are handed over.
    pub fn load_bound_transcript(
        &self,
        id: Uuid,
        project: Option<&str>,
    ) -> Result<(Vec<String>, Vec<Message>, String, usize)> {
        if let Some(project) = project {
            self.bind_project(id, project)?;
        }
        self.load_repairing_effect_transcript(id)
    }
}

#[cfg(test)]
mod tests {
    use crate::{ListFilter, SessionStore};

    #[test]
    fn a_session_serves_exactly_one_project_for_life() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::open(dir.path().join("sessions.db")).unwrap();

        // Creation records the binding; a host session stays unbound.
        let alpha = store.create_scoped("alpha", Some("project-a")).unwrap();
        let host = store.create_scoped("host", None).unwrap();
        let by_id = |id| {
            store
                .list_filtered(ListFilter {
                    include_archived: true,
                })
                .unwrap()
                .into_iter()
                .find(|meta| meta.id == id)
                .unwrap()
        };
        assert_eq!(by_id(alpha).project.as_deref(), Some("project-a"));
        assert_eq!(by_id(host).project, None);

        // Resume under the same project is a no-op; an unbound session is
        // adopted by its first project resume.
        store.bind_project(alpha, "project-a").unwrap();
        store.bind_project(host, "project-b").unwrap();
        assert_eq!(by_id(host).project.as_deref(), Some("project-b"));

        // A different project is refused — the binding never migrates.
        let err = store.bind_project(alpha, "project-b").unwrap_err();
        assert!(
            err.to_string().contains("bound to project project-a"),
            "refusal must name the existing binding: {err}"
        );
        assert_eq!(by_id(alpha).project.as_deref(), Some("project-a"));

        // Unknown sessions fail loudly rather than binding nothing.
        let ghost = uuid::Uuid::new_v4();
        assert!(store.bind_project(ghost, "project-a").is_err());
    }
}
