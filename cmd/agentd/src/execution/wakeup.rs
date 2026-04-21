use super::*;
use agent_runtime::session::TranscriptEntry;

impl ExecutionService {
    pub fn execute_session_wakeup_turn(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        session_id: &str,
        now: i64,
    ) -> Result<bool, ExecutionError> {
        let queued_events = store
            .list_queued_session_inbox_events_for_session(session_id)
            .map_err(ExecutionError::Store)?
            .into_iter()
            .filter(|record| record.available_at <= now)
            .map(agent_runtime::inbox::SessionInboxEvent::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        if queued_events.is_empty() {
            return Ok(false);
        }

        let session_record = store
            .get_session(session_id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| ExecutionError::MissingSession {
                id: session_id.to_string(),
            })?;
        let session =
            Session::try_from(session_record).map_err(ExecutionError::RecordConversion)?;
        let run_id = ensure_unique_run_id(store, format!("run-wakeup-{session_id}-{now}"))?;
        let mut run = RunEngine::new(run_id.clone(), session.id.clone(), None, now);
        run.start(now).map_err(ExecutionError::RunTransition)?;
        store
            .put_run(
                &RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

        for event in &queued_events {
            store
                .put_session_inbox_event(
                    &agent_persistence::SessionInboxEventRecord::try_from(
                        &event.clone().mark_claimed(now),
                    )
                    .map_err(ExecutionError::RecordConversion)?,
                )
                .map_err(ExecutionError::Store)?;
            let system_entry = TranscriptEntry::system(
                format!("transcript-{}-system", event.id),
                session.id.clone(),
                Some(run_id.as_str()),
                event.transcript_summary(),
                now,
            );
            store
                .put_transcript(&TranscriptRecord::from(&system_entry))
                .map_err(ExecutionError::Store)?;
        }

        let mut observer = None;
        let response = match self.execute_provider_turn_loop(
            store,
            provider,
            &session.id,
            session.settings.model.clone(),
            session
                .prompt_override
                .as_ref()
                .map(|override_text| override_text.as_str().to_string()),
            &mut run,
            None,
            now,
            None,
            &mut observer,
        ) {
            Ok(response) => response,
            Err(source @ ExecutionError::ApprovalRequired { .. }) => {
                for event in &queued_events {
                    store
                        .put_session_inbox_event(
                            &agent_persistence::SessionInboxEventRecord::try_from(
                                &event.clone().mark_processed(now),
                            )
                            .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                }
                return Err(source);
            }
            Err(source) => {
                if !matches!(
                    source,
                    ExecutionError::PermissionDenied { .. }
                        | ExecutionError::ApprovalRequired { .. }
                        | ExecutionError::InterruptedByQueuedInput
                ) {
                    run.fail(source.to_string(), now)
                        .map_err(ExecutionError::RunTransition)?;
                    self.persist_run(store, &run)?;
                }
                for event in &queued_events {
                    store
                        .put_session_inbox_event(
                            &agent_persistence::SessionInboxEventRecord::try_from(
                                &event.clone().requeue(now, source.to_string()),
                            )
                            .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                }
                return Err(source);
            }
        };

        run.complete(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;
        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-run-{run_id}-assistant"),
            session.id,
            Some(run_id.as_str()),
            &response.output_text,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&assistant_entry))
            .map_err(ExecutionError::Store)?;
        for event in &queued_events {
            store
                .put_session_inbox_event(
                    &agent_persistence::SessionInboxEventRecord::try_from(
                        &event.clone().mark_processed(now),
                    )
                    .map_err(ExecutionError::RecordConversion)?,
                )
                .map_err(ExecutionError::Store)?;
        }
        Ok(true)
    }
}
