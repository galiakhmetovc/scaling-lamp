use super::*;
use agent_runtime::interagent::AgentMessageChain;
use agent_runtime::mission::{JobResult, MissionStatus};
use agent_runtime::provider::{
    FinishReason, ProviderContinuationMessage, ProviderResponse, ProviderToolOutput,
};
use agent_runtime::session::{TranscriptEntry, scheduled_input_metadata};
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
        let run_id = ensure_unique_run_id(store, format!("run-chat-{session_id}-{now}"))?;
        let mut run = RunEngine::new(run_id.clone(), session.id.clone(), None, now);
        run.start(now).map_err(ExecutionError::RunTransition)?;
        store
            .put_run(
                &RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

        let user_entry = TranscriptEntry::user(
            format!("transcript-{run_id}-01-user"),
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
                        | ExecutionError::CancelledByOperator
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
        let assistant_at = completed_run_timestamp(&run);

        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-{run_id}-02-assistant"),
            session.id.clone(),
            Some(run_id.as_str()),
            &response.output_text,
            assistant_at,
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
        let (message, progress_prefix, schedule_id) = match &job.input {
            JobExecutionInput::ChatTurn { message } => {
                (message.clone(), "background chat turn", None)
            }
            JobExecutionInput::ScheduledChatTurn {
                schedule_id,
                message,
            } => (
                message.clone(),
                "scheduled chat turn",
                Some(schedule_id.clone()),
            ),
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

        let user_entry_id = format!("transcript-{}-01-user", job.id);
        if let Some(schedule_id) = schedule_id.as_deref() {
            let schedule_label = format!("agent-schedule:{schedule_id}");
            if session.delegation_label.as_deref() != Some(schedule_label.as_str()) {
                let metadata_entry = TranscriptEntry::system(
                    format!("transcript-{}-00-schedule-meta", job.id),
                    session.id.clone(),
                    Some(run_id.as_str()),
                    scheduled_input_metadata(schedule_id, &user_entry_id),
                    now,
                );
                store
                    .put_transcript(&TranscriptRecord::from(&metadata_entry))
                    .map_err(ExecutionError::Store)?;
            }
        }

        let user_entry = TranscriptEntry::user(
            user_entry_id,
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
            Some(true),
            now,
            None,
            &mut observer,
        ) {
            Ok(response) => response,
            Err(source @ ExecutionError::ApprovalRequired { .. }) => {
                job.status = JobStatus::Blocked;
                job.error = Some(source.to_string());
                job.updated_at = now;
                job.last_progress_message = Some(format!("{progress_prefix} requires approval"));
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
                        | ExecutionError::CancelledByOperator
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
                job.last_progress_message = Some(format!("{progress_prefix} failed"));
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
        };

        run.complete(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;
        let assistant_at = completed_run_timestamp(&run);

        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-{}-02-assistant", job.id),
            session.id.clone(),
            Some(run_id.as_str()),
            &response.output_text,
            assistant_at,
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
        job.last_progress_message = Some(format!("{progress_prefix} completed"));
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

    pub fn execute_background_interagent_message_job(
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
        let (
            source_session_id,
            source_agent_id,
            source_agent_name,
            target_agent_name,
            message,
            chain,
        ) = match &job.input {
            JobExecutionInput::InterAgentMessage {
                source_session_id,
                source_agent_id,
                source_agent_name,
                target_agent_name,
                message,
                chain,
                ..
            } => (
                source_session_id.clone(),
                source_agent_id.clone(),
                source_agent_name.clone(),
                target_agent_name.clone(),
                message.clone(),
                chain.clone(),
            ),
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

        self.write_interagent_recipient_transcripts(
            store,
            &job.id,
            &session.id,
            &run_id,
            source_session_id.as_str(),
            source_agent_id.as_str(),
            source_agent_name.as_str(),
            target_agent_name.as_str(),
            &chain,
            &message,
            now,
        )?;

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
                    Some("inter-agent recipient turn requires approval".to_string());
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
                        | ExecutionError::CancelledByOperator
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
                job.last_progress_message = Some("inter-agent recipient turn failed".to_string());
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
        };

        run.complete(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;
        let assistant_at = completed_run_timestamp(&run);

        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-{}-03-assistant", job.id),
            session.id.clone(),
            Some(run_id.as_str()),
            &response.output_text,
            assistant_at,
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
        job.last_progress_message = Some("inter-agent recipient turn completed".to_string());
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

    #[allow(clippy::too_many_arguments)]
    fn write_interagent_recipient_transcripts(
        &self,
        store: &PersistenceStore,
        job_id: &str,
        session_id: &str,
        run_id: &str,
        source_session_id: &str,
        source_agent_id: &str,
        source_agent_name: &str,
        target_agent_name: &str,
        chain: &AgentMessageChain,
        message: &str,
        now: i64,
    ) -> Result<(), ExecutionError> {
        let chain_entry = TranscriptEntry::system(
            format!("transcript-{job_id}-01-system-chain"),
            session_id.to_string(),
            Some(run_id),
            chain.to_transcript_metadata(),
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&chain_entry))
            .map_err(ExecutionError::Store)?;

        let system_entry = TranscriptEntry::system(
            format!("transcript-{job_id}-02-system-interagent"),
            session_id.to_string(),
            Some(run_id),
            format!(
                "Inter-agent message.\nsource_session_id: {source_session_id}\nsource_agent_id: {source_agent_id}\nsource_agent_name: {source_agent_name}\ntarget_agent_name: {target_agent_name}\nchain_id: {}\nhop_count: {}",
                chain.chain_id, chain.hop_count
            ),
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&system_entry))
            .map_err(ExecutionError::Store)?;

        let user_entry = TranscriptEntry::user(
            format!("transcript-{job_id}-03-user"),
            session_id.to_string(),
            Some(run_id),
            self.interagent_origin_user_message(source_agent_name, message),
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&user_entry))
            .map_err(ExecutionError::Store)
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
        if pending_approval.approval_id() != approval_id {
            return Err(ExecutionError::ProviderLoop {
                reason: format!(
                    "approval {approval_id} does not match pending provider approval {}",
                    pending_approval.approval_id()
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

        run.resolve_approval(approval_id, now)
            .map_err(ExecutionError::RunTransition)?;
        if run.snapshot().status == RunStatus::Resuming {
            run.resume(now).map_err(ExecutionError::RunTransition)?;
        }

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

        let mut resumed_loop_state = loop_state;
        resumed_loop_state.pending_approval = None;
        match pending_approval {
            agent_runtime::run::PendingProviderApproval::Tool(pending_tool_approval) => {
                let parsed = ToolCall::from_openai_function(
                    &pending_tool_approval.tool_name,
                    &pending_tool_approval.tool_arguments,
                )
                .map_err(|source| ExecutionError::ToolCallParse {
                    name: pending_tool_approval.tool_name.clone(),
                    reason: source.to_string(),
                })?;
                let catalog = ToolCatalog::default();
                let definition = catalog.definition_for_call(&parsed).ok_or_else(|| {
                    ExecutionError::ToolCallParse {
                        name: pending_tool_approval.tool_name.clone(),
                        reason: "tool is not in the catalog".to_string(),
                    }
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
                    return Err(ExecutionError::PermissionDenied {
                        tool: parsed.name().as_str().to_string(),
                        reason,
                    });
                }

                Self::emit_event(
                    observer,
                    ChatExecutionEvent::ToolStatus {
                        tool_name: parsed.name().as_str().to_string(),
                        summary: parsed.summary(),
                        status: ToolExecutionStatus::Approved,
                    },
                );

                let mut tool_runtime = self.tool_runtime();
                let model_output = self.invoke_provider_tool_call(
                    super::provider_loop::ProviderToolExecutionContext {
                        store,
                        session_id: &session.id,
                        now,
                    },
                    &mut run,
                    &mut tool_runtime,
                    pending_tool_approval.provider_tool_call_id.as_str(),
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

                if provider
                    .descriptor()
                    .capabilities
                    .supports_previous_response_id
                {
                    resumed_loop_state
                        .pending_tool_outputs
                        .push(ProviderToolOutput {
                            call_id: pending_tool_approval.provider_tool_call_id.clone(),
                            output: model_output,
                        });
                } else {
                    resumed_loop_state.continuation_messages.push(
                        ProviderContinuationMessage::ToolResult {
                            tool_call_id: pending_tool_approval.provider_tool_call_id.clone(),
                            content: model_output,
                        },
                    );
                }
            }
            agent_runtime::run::PendingProviderApproval::LoopReset(pending_loop_reset) => {
                resumed_loop_state.next_round = 0;
                Self::emit_event(
                    observer,
                    ChatExecutionEvent::ProviderLoopProgress {
                        current_round: 1,
                        max_rounds: pending_loop_reset.max_rounds,
                    },
                );
            }
            agent_runtime::run::PendingProviderApproval::CompletionNudge(
                pending_completion_approval,
            ) => {
                let synthetic_response = ProviderResponse {
                    response_id: run
                        .snapshot()
                        .provider_stream
                        .as_ref()
                        .map(|stream| stream.response_id.clone())
                        .unwrap_or_else(|| format!("completion-gate-{run_id}")),
                    model: run
                        .snapshot()
                        .provider_stream
                        .as_ref()
                        .map(|stream| stream.model.clone())
                        .unwrap_or_else(|| {
                            session
                                .settings
                                .model
                                .clone()
                                .unwrap_or_else(|| "provider".to_string())
                        }),
                    output_text: run
                        .snapshot()
                        .provider_stream
                        .as_ref()
                        .map(|stream| stream.output_text.clone())
                        .unwrap_or_default(),
                    tool_calls: Vec::new(),
                    finish_reason: FinishReason::Completed,
                    usage: None,
                };
                let decision = self
                    .completion_gate_decision(store, &session.id, &run, &synthetic_response)?
                    .ok_or_else(|| ExecutionError::ProviderLoop {
                        reason: format!(
                            "approval {approval_id} requested completion continuation but the gate is no longer active"
                        ),
                    })?;
                resumed_loop_state.continuation_input_messages = self
                    .completion_continuation_messages(
                        provider
                            .descriptor()
                            .capabilities
                            .supports_previous_response_id,
                        &synthetic_response,
                        decision.nudge_message.as_str(),
                    );
                resumed_loop_state.completion_nudges_used =
                    pending_completion_approval.completion_nudges_used;
            }
            agent_runtime::run::PendingProviderApproval::ProviderRetry(_) => {}
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
            None,
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
                        | ExecutionError::CancelledByOperator
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
        let assistant_at = completed_run_timestamp(&run);

        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-run-{run_id}-{now}-assistant"),
            session.id.clone(),
            Some(run_id),
            &response.output_text,
            assistant_at,
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

fn completed_run_timestamp(run: &RunEngine) -> i64 {
    run.snapshot()
        .finished_at
        .unwrap_or(run.snapshot().updated_at)
}
