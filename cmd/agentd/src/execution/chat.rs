use super::*;
use agent_runtime::mission::{JobResult, MissionStatus};
use agent_runtime::provider::{ProviderContinuationMessage, ProviderToolOutput};
use agent_runtime::session::TranscriptEntry;
use agent_runtime::tool::{ToolCatalog, ToolRuntime};
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
            Some(
                MissionSpec::try_from(
                    store
                        .get_mission(&job.mission_id)
                        .map_err(ExecutionError::Store)?
                        .ok_or_else(|| ExecutionError::MissingMission {
                            id: job.mission_id.clone(),
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

        let mut tool_runtime = ToolRuntime::new(self.workspace.clone());
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
