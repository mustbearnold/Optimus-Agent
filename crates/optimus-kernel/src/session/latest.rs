//! Recency queries whose order must not inherit pin/archive presentation.

use rusqlite::OptionalExtension;

use super::{SessionMeta, SessionStore};
use crate::Result;

impl SessionStore {
    /// Most recently touched active session, with insertion order breaking
    /// legacy timestamp ties. Pinning is presentation and cannot change which
    /// conversation a relaunch restores.
    pub fn latest(&self) -> Result<Option<SessionMeta>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, title, created_at, updated_at, packs_json, messages_json,
                        COALESCE(pinned, 0), COALESCE(archived, 0), project
                 FROM sessions
                 WHERE COALESCE(archived, 0) = 0
                 ORDER BY updated_at DESC, rowid DESC
                 LIMIT 1",
                [],
                |row| self.meta_from_row(row),
            )
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn pinning_does_not_win_a_legacy_timestamp_tie() {
        let dir = tempdir().unwrap();
        let store = SessionStore::open(dir.path().join("sessions.db")).unwrap();
        let older = store.create("older").unwrap();
        let newer = store.create("newer").unwrap();
        store.set_pinned(older, true).unwrap();
        store
            .conn
            .execute("UPDATE sessions SET updated_at = 'ts:1'", [])
            .unwrap();
        assert_eq!(store.latest().unwrap().unwrap().id, newer);

        store.set_archived(newer, true).unwrap();
        assert_eq!(store.latest().unwrap().unwrap().id, older);
    }
}
