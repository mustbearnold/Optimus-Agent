//! Fail-closed recovery for the narrow gap between accepting a session turn
//! and durably creating its execution manifest.

use super::*;

pub(crate) fn settle_manifestless_turn(
    sessions: &SessionStore,
    executions: &ExecutionStore,
    session_id: Uuid,
    title: &str,
    packs: &[String],
    messages: &[Message],
) -> Result<()> {
    let Some(turn) = sessions.active_turn(session_id)? else {
        return Ok(());
    };
    if executions.find_by_turn(turn.id)?.is_some() {
        return Ok(());
    }
    sessions.finish_turn(
        turn.id,
        session_id,
        title,
        packs,
        messages,
        TurnStatus::Failed,
        Some("execution_manifest_missing"),
        &[],
    )
}

impl Kernel {
    pub(crate) fn begin_recorded_execution(
        &mut self,
        model: &dyn ModelProvider,
        prompt: &str,
        start_message_count: usize,
    ) -> Result<(Uuid, RecordedExecution)> {
        let packs = pack_names(&self.packs);
        let turn_id = self.sessions.begin_turn(
            self.session_id,
            &self.session_title,
            &packs,
            &self.messages,
            start_message_count,
        )?;
        match self.begin_execution_manifest(turn_id, model, prompt) {
            Ok(execution) => Ok((turn_id, execution)),
            Err(error) => {
                self.sessions.finish_turn(
                    turn_id,
                    self.session_id,
                    &self.session_title,
                    &packs,
                    &self.messages,
                    TurnStatus::Failed,
                    Some(kernel_error_code(&error)),
                    &[],
                )?;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn message(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.into(),
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }
    }

    #[test]
    fn manifestless_active_turn_is_failed_once_and_unblocks_the_session() {
        let dir = tempdir().unwrap();
        let sessions = SessionStore::open(dir.path().join("sessions.db")).unwrap();
        let executions = ExecutionStore::open(dir.path().join("execution.db")).unwrap();
        let session_id = sessions.create("live").unwrap();
        let messages = vec![
            message(Role::System, "system"),
            message(Role::User, "hello"),
        ];
        sessions
            .save(session_id, "live", &[], &messages[..1])
            .unwrap();
        let turn_id = sessions
            .begin_turn(session_id, "live", &[], &messages, 1)
            .unwrap();

        settle_manifestless_turn(&sessions, &executions, session_id, "live", &[], &messages)
            .unwrap();
        settle_manifestless_turn(&sessions, &executions, session_id, "live", &[], &messages)
            .unwrap();

        assert!(sessions.active_turn(session_id).unwrap().is_none());
        let turns = sessions.turns(session_id).unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].id, turn_id);
        assert_eq!(turns[0].status, TurnStatus::Failed);
        assert_eq!(
            turns[0].error_code.as_deref(),
            Some("execution_manifest_missing")
        );
        assert_eq!(sessions.turn_event_count(turn_id).unwrap(), 2);
    }
}
