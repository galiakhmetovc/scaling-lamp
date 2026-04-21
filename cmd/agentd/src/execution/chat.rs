use super::*;
use crate::prompting;
use agent_runtime::delegation::{DelegateRequest, DelegateResultPackage};
use agent_runtime::mission::{JobResult, MissionStatus};
use agent_runtime::provider::{ProviderContinuationMessage, ProviderToolOutput};
use agent_runtime::session::TranscriptEntry;
use agent_runtime::tool::ToolCatalog;
use std::sync::atomic::AtomicBool;

impl ExecutionService {
    pub fn execute_chat_turn(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        session_id: &str,
        message: &str,
        now: i64,
    ) -> Result<ChatTurnExecutionReport, ExecutionError> {
        let mut observer = None;
        self.execute_chat_turn_with_observer(
            store,
            provider,
            session_id,
            message,
            now,
            &mut observer,
        )
    }

    pub fn execute_chat_turn_with_observer(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        session_id: &str,
        message: &str,
        now: i64,
        observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
    ) -> Result<ChatTurnExecutionReport, ExecutionError> {
        self.execute_chat_turn_with_control(
            store, provider, session_id, message, now, None, observer,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn execute_chat_turn_with_control(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        session_id: &str,
        message: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
    ) -> Result<ChatTurnExecutionReport, ExecutionError> {
        let mut session_record = store
            .get_session(session_id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| ExecutionError::MissingSession {
                id: session_id.to_string(),
            })?;
        let session =
            Session::try_from(session_record.clone()).map_err(ExecutionError::RecordConversion)?;
        let run_id = format!("run-chat-{session_id}-{now}");
        let mut run = RunEngine::new(run_id.clone(), session.id.clone(), None, now);
        run.start(now).map_err(ExecutionError::RunTransition)?;
        store
            .put_run(
                &RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

        let user_entry = TranscriptEntry::user(
            format!("transcript-chat-{session_id}-{now}-01-user"),
            session.id.clone(),
            Some(run_id.as_str()),
            message,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&user_entry))
            .map_err(ExecutionError::Store)?;

        session_record.updated_at = now;
        store
            .put_session(&session_record)
            .map_err(ExecutionError::Store)?;

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
            interrupt_after_tool_step,
            observer,
        ) {
            Ok(response) => response,
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
                return Err(source);
            }
        };

        run.complete(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;

        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-chat-{session_id}-{now}-02-assistant"),
            session.id.clone(),
            Some(run_id.as_str()),
            &response.output_text,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&assistant_entry))
            .map_err(ExecutionError::Store)?;

        Ok(ChatTurnExecutionReport {
            session_id: session.id,
            run_id,
            response_id: response.response_id,
            output_text: response.output_text,
        })
    }

    pub fn execute_background_chat_turn_job(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        job_id: &str,
        now: i64,
    ) -> Result<ChatTurnExecutionReport, ExecutionError> {
        let mut job = JobSpec::try_from(
            store
                .get_job(job_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingJob {
                    id: job_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let message = match &job.input {
            JobExecutionInput::ChatTurn { message } => message.clone(),
            _ => {
                return Err(ExecutionError::UnsupportedJobInput {
                    id: job.id.clone(),
                    kind: job.kind.as_str().to_string(),
                });
            }
        };
        let session_record = store
            .get_session(&job.session_id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| ExecutionError::MissingSession {
                id: job.session_id.clone(),
            })?;
        let session =
            Session::try_from(session_record.clone()).map_err(ExecutionError::RecordConversion)?;
        let run_id = job
            .run_id
            .clone()
            .unwrap_or_else(|| format!("run-{}", job.id));
        let mut run = RunEngine::new(run_id.clone(), session.id.clone(), None, now);
        run.start(now).map_err(ExecutionError::RunTransition)?;
        store
            .put_run(
                &RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

        job.status = JobStatus::Running;
        job.run_id = Some(run_id.clone());
        job.error = None;
        job.updated_at = now;
        if job.started_at.is_none() {
            job.started_at = Some(now);
        }
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;

        let user_entry = TranscriptEntry::user(
            format!("transcript-{}-01-user", job.id),
            session.id.clone(),
            Some(run_id.as_str()),
            &message,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&user_entry))
            .map_err(ExecutionError::Store)?;

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
                job.status = JobStatus::Blocked;
                job.error = Some(source.to_string());
                job.updated_at = now;
                job.last_progress_message =
                    Some("background chat turn requires approval".to_string());
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
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
                job.status = JobStatus::Failed;
                job.error = Some(source.to_string());
                job.finished_at = Some(now);
                job.updated_at = now;
                job.last_progress_message = Some("background chat turn failed".to_string());
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
        };

        run.complete(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;

        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-{}-02-assistant", job.id),
            session.id.clone(),
            Some(run_id.as_str()),
            &response.output_text,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&assistant_entry))
            .map_err(ExecutionError::Store)?;

        job.status = JobStatus::Completed;
        job.result = Some(JobResult::Summary {
            outcome: response.output_text.clone(),
        });
        job.finished_at = Some(now);
        job.updated_at = now;
        job.last_progress_message = Some("background chat turn completed".to_string());
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;

        Ok(ChatTurnExecutionReport {
            session_id: session.id,
            run_id,
            response_id: response.response_id,
            output_text: response.output_text,
        })
    }

    pub fn execute_background_approval_job(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        job_id: &str,
        now: i64,
    ) -> Result<ApprovalContinuationReport, ExecutionError> {
        let mut job = JobSpec::try_from(
            store
                .get_job(job_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingJob {
                    id: job_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let (run_id, approval_id) = match &job.input {
            JobExecutionInput::ApprovalContinuation {
                run_id,
                approval_id,
            } => (run_id.clone(), approval_id.clone()),
            _ => {
                return Err(ExecutionError::UnsupportedJobInput {
                    id: job.id.clone(),
                    kind: job.kind.as_str().to_string(),
                });
            }
        };

        job.status = JobStatus::Running;
        job.run_id = Some(run_id.clone());
        job.error = None;
        job.updated_at = now;
        if job.started_at.is_none() {
            job.started_at = Some(now);
        }
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;

        let mut observer = None;
        let report = match self.approve_model_run_with_control(
            store,
            provider,
            &run_id,
            &approval_id,
            now,
            None,
            &mut observer,
        ) {
            Ok(report) => report,
            Err(source @ ExecutionError::ApprovalRequired { .. }) => {
                job.status = JobStatus::Blocked;
                job.error = Some(source.to_string());
                job.updated_at = now;
                job.last_progress_message =
                    Some("background approval continuation requires approval".to_string());
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
            Err(source) => {
                job.status = JobStatus::Failed;
                job.error = Some(source.to_string());
                job.finished_at = Some(now);
                job.updated_at = now;
                job.last_progress_message =
                    Some("background approval continuation failed".to_string());
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
        };

        job.status = JobStatus::Completed;
        job.result = Some(JobResult::Summary {
            outcome: report
                .output_text
                .clone()
                .unwrap_or_else(|| "approval continuation completed".to_string()),
        });
        job.finished_at = Some(now);
        job.updated_at = now;
        job.last_progress_message = Some("background approval continuation completed".to_string());
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;
        Ok(report)
    }

    pub fn execute_background_delegate_job(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        job_id: &str,
        now: i64,
    ) -> Result<ChatTurnExecutionReport, ExecutionError> {
        let mut job = JobSpec::try_from(
            store
                .get_job(job_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingJob {
                    id: job_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let (label, goal, bounded_context, write_scope, expected_output, owner) = match &job.input {
            JobExecutionInput::Delegate {
                label,
                goal,
                bounded_context,
                write_scope,
                expected_output,
                owner,
            } => (
                label.clone(),
                goal.clone(),
                bounded_context.clone(),
                write_scope.clone(),
                expected_output.clone(),
                owner.clone(),
            ),
            _ => {
                return Err(ExecutionError::UnsupportedJobInput {
                    id: job.id.clone(),
                    kind: job.kind.as_str().to_string(),
                });
            }
        };

        let parent_session_record = store
            .get_session(&job.session_id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| ExecutionError::MissingSession {
                id: job.session_id.clone(),
            })?;
        let parent_session = Session::try_from(parent_session_record.clone())
            .map_err(ExecutionError::RecordConversion)?;
        let child_run_id = format!("run-delegate-child-{}", job.id);
        let parent_run_id = job
            .run_id
            .clone()
            .unwrap_or_else(|| format!("run-delegate-parent-{}", job.id));
        let request = DelegateRequest::new(
            job.id.clone(),
            parent_run_id,
            child_run_id.clone(),
            label.clone(),
            goal.clone(),
            bounded_context.clone(),
            write_scope.clone(),
            expected_output.clone(),
            owner.clone(),
        )
        .map_err(|source| ExecutionError::ProviderLoop {
            reason: source.to_string(),
        })?;

        job.status = JobStatus::Running;
        job.error = None;
        job.updated_at = now;
        if job.started_at.is_none() {
            job.started_at = Some(now);
        }
        job.last_progress_message = Some("delegation child session running".to_string());
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;

        let child_session =
            self.ensure_delegate_child_session(store, &parent_session, &job.id, &label, now)?;
        self.write_delegate_parent_started_transcript(
            store,
            &parent_session.id,
            &job.id,
            &child_session.id,
            &label,
            now,
        )?;

        let child_report = match self.restore_or_execute_delegate_child_turn(
            store,
            provider,
            &job.id,
            &child_session,
            &request,
            now,
        ) {
            Ok(report) => report,
            Err(source @ ExecutionError::ApprovalRequired { .. }) => {
                job.status = JobStatus::Blocked;
                job.error = Some(source.to_string());
                job.updated_at = now;
                job.last_progress_message =
                    Some("delegated child session requires approval".to_string());
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
            Err(source) => {
                job.status = JobStatus::Failed;
                job.error = Some(source.to_string());
                job.finished_at = Some(now);
                job.updated_at = now;
                job.last_progress_message = Some("delegated child session failed".to_string());
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
        };

        let artifact_refs = store
            .get_context_offload(&child_session.id)
            .map_err(ExecutionError::Store)?
            .map(agent_runtime::context::ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)?
            .map(|snapshot| {
                snapshot
                    .refs
                    .into_iter()
                    .map(|reference| reference.artifact_id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let package = DelegateResultPackage::new(
            prompting::preview_text(&child_report.output_text, 240),
            Vec::new(),
            artifact_refs,
            Vec::new(),
        )
        .map_err(|source| ExecutionError::ProviderLoop {
            reason: source.to_string(),
        })?;

        job.status = JobStatus::Completed;
        job.run_id = Some(child_report.run_id.clone());
        job.result = Some(JobResult::Delegation {
            child_session_id: child_session.id.clone(),
            package: package.clone(),
        });
        job.finished_at = Some(now);
        job.updated_at = now;
        job.last_progress_message = Some("delegated child session completed".to_string());
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;

        Ok(child_report)
    }

    fn ensure_delegate_child_session(
        &self,
        store: &PersistenceStore,
        parent_session: &Session,
        parent_job_id: &str,
        label: &str,
        now: i64,
    ) -> Result<Session, ExecutionError> {
        let child_session_id = format!("session-delegate-{parent_job_id}");
        if let Some(record) = store
            .get_session(&child_session_id)
            .map_err(ExecutionError::Store)?
        {
            return Session::try_from(record).map_err(ExecutionError::RecordConversion);
        }

        let child_session = Session {
            id: child_session_id,
            title: format!("Delegate: {label}"),
            prompt_override: parent_session.prompt_override.clone(),
            settings: parent_session.settings.clone(),
            active_mission_id: None,
            parent_session_id: Some(parent_session.id.clone()),
            parent_job_id: Some(parent_job_id.to_string()),
            delegation_label: Some(label.to_string()),
            created_at: now,
            updated_at: now,
        };
        store
            .put_session(
                &agent_persistence::SessionRecord::try_from(&child_session)
                    .map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;
        Ok(child_session)
    }

    fn write_delegate_parent_started_transcript(
        &self,
        store: &PersistenceStore,
        parent_session_id: &str,
        parent_job_id: &str,
        child_session_id: &str,
        label: &str,
        now: i64,
    ) -> Result<(), ExecutionError> {
        let entry = TranscriptEntry::system(
            format!("transcript-{parent_job_id}-delegate-started"),
            parent_session_id.to_string(),
            None,
            format!("delegation started: {label} (child session: {child_session_id})"),
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&entry))
            .map_err(ExecutionError::Store)
    }

    fn restore_or_execute_delegate_child_turn(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        parent_job_id: &str,
        child_session: &Session,
        request: &DelegateRequest,
        now: i64,
    ) -> Result<ChatTurnExecutionReport, ExecutionError> {
        if let Some(existing_run) = store
            .get_run(&request.child_run_id)
            .map_err(ExecutionError::Store)?
        {
            let snapshot =
                RunSnapshot::try_from(existing_run).map_err(ExecutionError::RecordConversion)?;
            if snapshot.status == RunStatus::Completed {
                let assistant_output = store
                    .list_transcripts_for_session(&child_session.id)
                    .map_err(ExecutionError::Store)?
                    .into_iter()
                    .rfind(|record| {
                        record.run_id.as_deref() == Some(request.child_run_id.as_str())
                            && record.kind == "assistant"
                    })
                    .map(|record| record.content)
                    .unwrap_or_default();
                return Ok(ChatTurnExecutionReport {
                    session_id: child_session.id.clone(),
                    run_id: request.child_run_id.clone(),
                    response_id: String::new(),
                    output_text: assistant_output,
                });
            }
        }

        let mut run = RunEngine::new(
            request.child_run_id.clone(),
            child_session.id.clone(),
            None,
            now,
        );
        run.start(now).map_err(ExecutionError::RunTransition)?;
        store
            .put_run(
                &RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

        let system_entry = TranscriptEntry::system(
            format!("transcript-{parent_job_id}-delegate-child-01-system"),
            child_session.id.clone(),
            Some(request.child_run_id.as_str()),
            format!(
                "Delegated task.\nlabel: {}\nowner: {}\nexpected_output: {}\nbounded_context: {}\nwrite_scope: {}",
                request.label,
                request.owner,
                request.expected_output,
                request.bounded_context.join(", "),
                request.write_scope.allowed_paths.join(", ")
            ),
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&system_entry))
            .map_err(ExecutionError::Store)?;

        let user_entry = TranscriptEntry::user(
            format!("transcript-{parent_job_id}-delegate-child-02-user"),
            child_session.id.clone(),
            Some(request.child_run_id.as_str()),
            &request.goal,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&user_entry))
            .map_err(ExecutionError::Store)?;

        let mut observer = None;
        let response = match self.execute_provider_turn_loop(
            store,
            provider,
            &child_session.id,
            child_session.settings.model.clone(),
            child_session
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
                return Err(source);
            }
        };

        run.complete(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;

        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-{parent_job_id}-delegate-child-03-assistant"),
            child_session.id.clone(),
            Some(request.child_run_id.as_str()),
            &response.output_text,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&assistant_entry))
            .map_err(ExecutionError::Store)?;

        Ok(ChatTurnExecutionReport {
            session_id: child_session.id.clone(),
            run_id: request.child_run_id.clone(),
            response_id: response.response_id,
            output_text: response.output_text,
        })
    }

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
        let run_id = format!("run-wakeup-{session_id}-{now}");
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

    pub fn approve_model_run(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        run_id: &str,
        approval_id: &str,
        now: i64,
    ) -> Result<ApprovalContinuationReport, ExecutionError> {
        let mut observer = None;
        self.approve_model_run_with_observer(
            store,
            provider,
            run_id,
            approval_id,
            now,
            &mut observer,
        )
    }

    pub fn approve_model_run_with_observer(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        run_id: &str,
        approval_id: &str,
        now: i64,
        observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
    ) -> Result<ApprovalContinuationReport, ExecutionError> {
        self.approve_model_run_with_control(
            store,
            provider,
            run_id,
            approval_id,
            now,
            None,
            observer,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn approve_model_run_with_control(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        run_id: &str,
        approval_id: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
    ) -> Result<ApprovalContinuationReport, ExecutionError> {
        let run_snapshot = RunSnapshot::try_from(
            store
                .get_run(run_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingRun {
                    id: run_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let mut run = RunEngine::from_snapshot(run_snapshot);
        let loop_state =
            run.snapshot()
                .provider_loop
                .clone()
                .ok_or_else(|| ExecutionError::ProviderLoop {
                    reason: format!("run {run_id} has no persisted provider continuation state"),
                })?;
        let pending_approval =
            loop_state
                .pending_approval
                .clone()
                .ok_or_else(|| ExecutionError::ProviderLoop {
                    reason: format!("run {run_id} has no pending provider approval to resume"),
                })?;
        if pending_approval.approval_id != approval_id {
            return Err(ExecutionError::ProviderLoop {
                reason: format!(
                    "approval {approval_id} does not match pending provider approval {}",
                    pending_approval.approval_id
                ),
            });
        }

        let session_record = store
            .get_session(&run.snapshot().session_id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| ExecutionError::MissingSession {
                id: run.snapshot().session_id.clone(),
            })?;
        let session =
            Session::try_from(session_record).map_err(ExecutionError::RecordConversion)?;
        let mut job = self.find_job_by_run_id(store, run_id)?;
        let mut mission = if let Some(job) = job.as_ref() {
            let mission_id =
                job.mission_id
                    .clone()
                    .ok_or_else(|| ExecutionError::UnsupportedJobInput {
                        id: job.id.clone(),
                        kind: job.kind.as_str().to_string(),
                    })?;
            Some(
                MissionSpec::try_from(
                    store
                        .get_mission(&mission_id)
                        .map_err(ExecutionError::Store)?
                        .ok_or_else(|| ExecutionError::MissingMission {
                            id: mission_id.clone(),
                        })?,
                )
                .map_err(ExecutionError::RecordConversion)?,
            )
        } else {
            None
        };

        let parsed = ToolCall::from_openai_function(
            &pending_approval.tool_name,
            &pending_approval.tool_arguments,
        )
        .map_err(|source| ExecutionError::ToolCallParse {
            name: pending_approval.tool_name.clone(),
            reason: source.to_string(),
        })?;
        let catalog = ToolCatalog::default();
        let definition =
            catalog
                .definition_for_call(&parsed)
                .ok_or_else(|| ExecutionError::ToolCallParse {
                    name: pending_approval.tool_name.clone(),
                    reason: "tool is not in the catalog".to_string(),
                })?;
        let permission = self.permissions.resolve(definition, &parsed);
        if matches!(permission.action, PermissionAction::Deny) {
            let reason = format!(
                "tool {} denied by permission policy: {}",
                parsed.name().as_str(),
                permission.reason
            );
            run.fail(reason.clone(), now)
                .map_err(ExecutionError::RunTransition)?;
            self.persist_run(store, &run)?;
            if let Some(job) = job.as_mut() {
                job.status = JobStatus::Failed;
                job.error = Some(reason.clone());
                job.finished_at = Some(now);
                job.updated_at = now;
                store
                    .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
            }
            if let Some(mission) = mission.as_mut() {
                mission.updated_at = now;
                store
                    .put_mission(
                        &MissionRecord::try_from(&*mission)
                            .map_err(ExecutionError::RecordConversion)?,
                    )
                    .map_err(ExecutionError::Store)?;
            }
            return Err(ExecutionError::PermissionDenied {
                tool: parsed.name().as_str().to_string(),
                reason,
            });
        }

        run.resolve_approval(approval_id, now)
            .map_err(ExecutionError::RunTransition)?;
        if run.snapshot().status == RunStatus::Resuming {
            run.resume(now).map_err(ExecutionError::RunTransition)?;
        }
        Self::emit_event(
            observer,
            ChatExecutionEvent::ToolStatus {
                tool_name: parsed.name().as_str().to_string(),
                summary: parsed.summary(),
                status: ToolExecutionStatus::Approved,
            },
        );

        if let Some(job) = job.as_mut() {
            job.status = JobStatus::Running;
            job.error = None;
            job.updated_at = now;
            if job.started_at.is_none() {
                job.started_at = Some(now);
            }
            store
                .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
                .map_err(ExecutionError::Store)?;
        }
        if let Some(mission) = mission.as_mut() {
            mission.status = MissionStatus::Running;
            mission.updated_at = now;
            store
                .put_mission(
                    &MissionRecord::try_from(&*mission)
                        .map_err(ExecutionError::RecordConversion)?,
                )
                .map_err(ExecutionError::Store)?;
        }

        let mut tool_runtime = self.tool_runtime();
        let model_output = self.invoke_provider_tool_call(
            super::provider_loop::ProviderToolExecutionContext {
                store,
                session_id: &session.id,
                now,
            },
            &mut run,
            &mut tool_runtime,
            pending_approval.provider_tool_call_id.as_str(),
            &parsed,
            observer,
        )?;
        if interrupt_after_tool_step
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::SeqCst))
        {
            run.interrupt("superseded by queued user input", now)
                .map_err(ExecutionError::RunTransition)?;
            self.persist_run(store, &run)?;
            return Err(ExecutionError::InterruptedByQueuedInput);
        }

        let mut resumed_loop_state = loop_state;
        resumed_loop_state.pending_approval = None;
        if provider
            .descriptor()
            .capabilities
            .supports_previous_response_id
        {
            resumed_loop_state
                .pending_tool_outputs
                .push(ProviderToolOutput {
                    call_id: pending_approval.provider_tool_call_id.clone(),
                    output: model_output,
                });
        } else {
            resumed_loop_state.continuation_messages.push(
                ProviderContinuationMessage::ToolResult {
                    tool_call_id: pending_approval.provider_tool_call_id.clone(),
                    content: model_output,
                },
            );
        }
        run.set_provider_loop_state(resumed_loop_state.clone(), now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;

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
            Some(resumed_loop_state),
            now,
            interrupt_after_tool_step,
            observer,
        ) {
            Ok(response) => response,
            Err(
                ref source @ ExecutionError::ApprovalRequired {
                    approval_id: ref next_approval_id,
                    ..
                },
            ) => {
                if let Some(job) = job.as_mut() {
                    job.status = JobStatus::Blocked;
                    job.error = Some(source.to_string());
                    job.updated_at = now;
                    store
                        .put_job(
                            &JobRecord::try_from(&*job)
                                .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                }
                if let Some(mission) = mission.as_mut() {
                    mission.updated_at = now;
                    store
                        .put_mission(
                            &MissionRecord::try_from(&*mission)
                                .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                }
                return Ok(ApprovalContinuationReport {
                    run_id: run_id.to_string(),
                    run_status: RunStatus::WaitingApproval,
                    response_id: None,
                    output_text: None,
                    approval_id: Some(next_approval_id.clone()),
                });
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
                if let Some(job) = job.as_mut() {
                    job.status = JobStatus::Failed;
                    job.error = Some(source.to_string());
                    job.finished_at = Some(now);
                    job.updated_at = now;
                    store
                        .put_job(
                            &JobRecord::try_from(&*job)
                                .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                }
                if let Some(mission) = mission.as_mut() {
                    mission.updated_at = now;
                    store
                        .put_mission(
                            &MissionRecord::try_from(&*mission)
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
            format!("transcript-run-{run_id}-{now}-assistant"),
            session.id.clone(),
            Some(run_id),
            &response.output_text,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&assistant_entry))
            .map_err(ExecutionError::Store)?;

        if let Some(job) = job.as_mut() {
            job.status = JobStatus::Completed;
            job.result = Some(JobResult::Summary {
                outcome: response.output_text.clone(),
            });
            job.finished_at = Some(now);
            job.updated_at = now;
            store
                .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
                .map_err(ExecutionError::Store)?;
        }

        Ok(ApprovalContinuationReport {
            run_id: run_id.to_string(),
            run_status: RunStatus::Completed,
            response_id: Some(response.response_id),
            output_text: Some(response.output_text),
            approval_id: None,
        })
    }
}
