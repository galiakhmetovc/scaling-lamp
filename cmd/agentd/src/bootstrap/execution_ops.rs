use super::*;
use crate::http::types::{
    A2ADelegationAcceptedResponse, A2ADelegationCompletionOutcomeRequest,
    A2ADelegationCompletionRequest, A2ADelegationCreateRequest,
};
use agent_persistence::{JobRecord, SessionInboxRepository, TranscriptRecord};
use agent_runtime::inbox::SessionInboxEvent;
use agent_runtime::mission::{JobResult, JobSpec, JobStatus};
use agent_runtime::run::{RunEngine, RunSnapshot};
use agent_runtime::scheduler::MissionVerificationSummary;
use agent_runtime::session::{Session, SessionSettings, TranscriptEntry};
use agent_runtime::tool::{GrantAgentChainContinuationInput, MessageAgentInput, ToolCall};
use std::sync::atomic::AtomicBool;

impl App {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn supervisor_tick(
        &self,
        now: i64,
        verifications: &[MissionVerificationSummary],
    ) -> Result<execution::SupervisorTickReport, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .supervisor_tick(&store, now, verifications)
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn background_worker_tick(
        &self,
        now: i64,
    ) -> Result<execution::BackgroundWorkerTickReport, BootstrapError> {
        let store = self.store()?;
        let provider = self.provider_driver()?;
        self.execution_service()
            .background_worker_tick(&store, provider.as_ref(), now)
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn execute_mission_turn_job(
        &self,
        job_id: &str,
        now: i64,
    ) -> Result<execution::MissionTurnExecutionReport, BootstrapError> {
        let store = self.store()?;
        let provider = self.provider_driver()?;
        self.execution_service()
            .execute_mission_turn_job(&store, provider.as_ref(), job_id, now)
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn execute_chat_turn(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
    ) -> Result<execution::ChatTurnExecutionReport, BootstrapError> {
        let store = self.store()?;
        let provider = self.provider_driver()?;
        self.execution_service()
            .execute_chat_turn(&store, provider.as_ref(), session_id, message, now)
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn execute_chat_turn_with_observer(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        observer: &mut dyn FnMut(execution::ChatExecutionEvent),
    ) -> Result<execution::ChatTurnExecutionReport, BootstrapError> {
        self.execute_chat_turn_with_control_and_observer(session_id, message, now, None, observer)
    }

    pub fn execute_chat_turn_with_control_and_observer(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut dyn FnMut(execution::ChatExecutionEvent),
    ) -> Result<execution::ChatTurnExecutionReport, BootstrapError> {
        let store = self.store()?;
        let provider = self.provider_driver()?;
        let mut observer = Some(observer as &mut dyn FnMut(execution::ChatExecutionEvent));
        self.execution_service()
            .execute_chat_turn_with_control(
                &store,
                provider.as_ref(),
                session_id,
                message,
                now,
                interrupt_after_tool_step,
                &mut observer,
            )
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn approve_run(
        &self,
        run_id: &str,
        approval_id: &str,
        now: i64,
    ) -> Result<execution::ApprovalContinuationReport, BootstrapError> {
        let store = self.store()?;
        let snapshot = RunSnapshot::try_from(store.get_run(run_id)?.ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "run",
                id: run_id.to_string(),
            }
        })?)
        .map_err(BootstrapError::RecordConversion)?;

        if snapshot.provider_loop.is_some() {
            let provider = self.provider_driver()?;
            return self
                .execution_service()
                .approve_model_run(&store, provider.as_ref(), run_id, approval_id, now)
                .map_err(BootstrapError::Execution);
        }

        let mut engine = RunEngine::from_snapshot(snapshot);
        engine
            .resolve_approval(approval_id, now)
            .map_err(BootstrapError::RunTransition)?;
        let record =
            RunRecord::try_from(engine.snapshot()).map_err(BootstrapError::RecordConversion)?;
        store.put_run(&record)?;
        Ok(execution::ApprovalContinuationReport {
            run_id: run_id.to_string(),
            run_status: engine.snapshot().status,
            response_id: None,
            output_text: None,
            approval_id: None,
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn approve_run_with_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        now: i64,
        observer: &mut dyn FnMut(execution::ChatExecutionEvent),
    ) -> Result<execution::ApprovalContinuationReport, BootstrapError> {
        self.approve_run_with_control_and_observer(run_id, approval_id, now, None, observer)
    }

    pub fn approve_run_with_control_and_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut dyn FnMut(execution::ChatExecutionEvent),
    ) -> Result<execution::ApprovalContinuationReport, BootstrapError> {
        let store = self.store()?;
        let snapshot = RunSnapshot::try_from(store.get_run(run_id)?.ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "run",
                id: run_id.to_string(),
            }
        })?)
        .map_err(BootstrapError::RecordConversion)?;

        if snapshot.provider_loop.is_some() {
            let provider = self.provider_driver()?;
            let mut observer = Some(observer as &mut dyn FnMut(execution::ChatExecutionEvent));
            return self
                .execution_service()
                .approve_model_run_with_control(
                    &store,
                    provider.as_ref(),
                    run_id,
                    approval_id,
                    now,
                    interrupt_after_tool_step,
                    &mut observer,
                )
                .map_err(BootstrapError::Execution);
        }

        self.approve_run(run_id, approval_id, now)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn cancel_latest_session_run(
        &self,
        session_id: &str,
        now: i64,
    ) -> Result<String, BootstrapError> {
        let store = self.store()?;
        let Some(run) = self
            .execution_service()
            .cancel_latest_session_run(&store, session_id, now)
            .map_err(BootstrapError::Execution)?
        else {
            return Ok("Ход: активного выполнения нет".to_string());
        };
        Ok(format!(
            "ход {} остановлен оператором (статус: {})",
            run.id,
            run.status.as_str()
        ))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn cancel_all_session_work(
        &self,
        session_id: &str,
        now: i64,
    ) -> Result<String, BootstrapError> {
        let store = self.store()?;
        if store
            .get_session(session_id)
            .map_err(BootstrapError::Store)?
            .is_none()
        {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        let report = self
            .execution_service()
            .cancel_all_session_work(&store, session_id, now)
            .map_err(BootstrapError::Execution)?;
        Ok(format!(
            "отмена выполнена: sessions={} runs={} jobs={} missions={} inbox_events={}",
            report.session_count,
            report.run_count,
            report.job_count,
            report.mission_count,
            report.inbox_event_count
        ))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn request_tool_approval(
        &self,
        job_id: &str,
        run_id: &str,
        tool_call: &ToolCall,
        now: i64,
    ) -> Result<execution::ToolExecutionReport, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .request_tool_approval(&store, job_id, run_id, tool_call, now)
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn send_session_agent_message(
        &self,
        session_id: &str,
        target_agent_id: &str,
        message: &str,
        now: i64,
    ) -> Result<String, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }

        let output = self
            .execution_service()
            .queue_interagent_message(
                &store,
                session_id,
                &MessageAgentInput {
                    target_agent_id: target_agent_id.to_string(),
                    message: message.to_string(),
                },
                now,
            )
            .map_err(BootstrapError::Execution)?;
        Ok(format!(
            "сообщение отправлено агенту {}: recipient_session={} recipient_job={} chain_id={} hop_count={}",
            output.target_agent_id,
            output.recipient_session_id,
            output.recipient_job_id,
            output.chain_id,
            output.hop_count
        ))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn grant_session_chain_continuation(
        &self,
        session_id: &str,
        chain_id: &str,
        reason: &str,
        now: i64,
    ) -> Result<String, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }

        let output = self
            .execution_service()
            .grant_agent_chain_continuation(
                &store,
                &GrantAgentChainContinuationInput {
                    chain_id: chain_id.to_string(),
                    reason: reason.to_string(),
                },
                now,
            )
            .map_err(BootstrapError::Execution)?;
        Ok(format!(
            "цепочка {} продолжена: granted_hops={}",
            output.chain_id, output.granted_hops
        ))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn resume_tool_call(
        &self,
        request: execution::ToolResumeRequest<'_>,
    ) -> Result<execution::ToolExecutionReport, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .resume_tool_call(&store, request)
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn accept_remote_delegation(
        &self,
        request: A2ADelegationCreateRequest,
    ) -> Result<A2ADelegationAcceptedResponse, BootstrapError> {
        let store = self.store()?;
        let remote_session_id = format!("session-a2a-{}", request.parent_job_id);
        let remote_job_id = format!("job-a2a-{}", request.parent_job_id);

        if store.get_job(&remote_job_id)?.is_some() {
            return Ok(A2ADelegationAcceptedResponse {
                accepted: true,
                remote_session_id,
                remote_job_id,
            });
        }

        if store.get_session(&remote_session_id)?.is_none() {
            let session = Session {
                id: remote_session_id.clone(),
                title: format!("A2A Delegate: {}", request.label),
                prompt_override: None,
                settings: SessionSettings::default(),
                agent_profile_id: "default".to_string(),
                active_mission_id: None,
                parent_session_id: Some(request.parent_session_id.clone()),
                parent_job_id: Some(request.parent_job_id.clone()),
                delegation_label: Some(request.label.clone()),
                created_at: request.now,
                updated_at: request.now,
            };
            store.put_session(
                &agent_persistence::SessionRecord::try_from(&session)
                    .map_err(BootstrapError::RecordConversion)?,
            )?;

            let system_entry = TranscriptEntry::system(
                format!("transcript-{}-a2a-01-system", request.parent_job_id),
                remote_session_id.clone(),
                None,
                format!(
                    "Delegated A2A task.\nlabel: {}\nowner: {}\nexpected_output: {}\nbounded_context: {}\nwrite_scope: {}",
                    request.label,
                    request.owner,
                    request.expected_output,
                    request.bounded_context.join(", "),
                    request.write_scope.allowed_paths.join(", ")
                ),
                request.now,
            );
            store.put_transcript(&TranscriptRecord::from(&system_entry))?;
        }

        let mut job = JobSpec::chat_turn(
            &remote_job_id,
            &remote_session_id,
            None,
            None,
            &request.goal,
            request.now,
        );
        job.status = JobStatus::Running;
        job.last_progress_message = Some("accepted via a2a".to_string());
        job.callback = Some(agent_runtime::mission::JobCallbackTarget {
            url: request.callback.url,
            bearer_token: request.callback.bearer_token,
            parent_session_id: request.parent_session_id,
            parent_job_id: request.parent_job_id,
        });
        store.put_job(&JobRecord::try_from(&job).map_err(BootstrapError::RecordConversion)?)?;

        Ok(A2ADelegationAcceptedResponse {
            accepted: true,
            remote_session_id,
            remote_job_id,
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn complete_remote_delegation(
        &self,
        parent_job_id: &str,
        request: A2ADelegationCompletionRequest,
    ) -> Result<(), BootstrapError> {
        let store = self.store()?;
        let mut job = JobSpec::try_from(store.get_job(parent_job_id)?.ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "job",
                id: parent_job_id.to_string(),
            }
        })?)
        .map_err(BootstrapError::RecordConversion)?;

        if job.status == JobStatus::Cancelled || job.cancel_requested_at.is_some() {
            return Ok(());
        }

        let inbox_event = match request.outcome {
            A2ADelegationCompletionOutcomeRequest::Completed {
                remote_session_id,
                remote_job_id: _,
                package,
            } => {
                job.status = JobStatus::Completed;
                job.result = Some(JobResult::Delegation {
                    child_session_id: remote_session_id,
                    package: package.clone(),
                });
                job.error = None;
                job.finished_at = Some(request.now);
                job.updated_at = request.now;
                job.last_progress_message = Some("remote delegation completed".to_string());
                SessionInboxEvent::delegation_result_ready(
                    format!("inbox-{}-delegation-{}", job.id, request.now),
                    &job.session_id,
                    Some(job.id.as_str()),
                    package.summary,
                    package.artifact_refs,
                    request.now,
                )
            }
            A2ADelegationCompletionOutcomeRequest::Failed {
                remote_session_id: _,
                remote_job_id: _,
                reason,
            } => {
                job.status = JobStatus::Failed;
                job.error = Some(reason.clone());
                job.finished_at = Some(request.now);
                job.updated_at = request.now;
                job.last_progress_message = Some("remote delegation failed".to_string());
                SessionInboxEvent::job_failed(
                    format!("inbox-{}-failed-{}", job.id, request.now),
                    &job.session_id,
                    Some(job.id.as_str()),
                    reason,
                    request.now,
                )
            }
            A2ADelegationCompletionOutcomeRequest::Blocked {
                remote_session_id: _,
                remote_job_id: _,
                reason,
            } => {
                job.status = JobStatus::Blocked;
                job.error = Some(reason.clone());
                job.updated_at = request.now;
                job.last_progress_message = Some("remote delegation blocked".to_string());
                SessionInboxEvent::job_blocked(
                    format!("inbox-{}-blocked-{}", job.id, request.now),
                    &job.session_id,
                    Some(job.id.as_str()),
                    reason,
                    request.now,
                )
            }
        };

        store.put_job(&JobRecord::try_from(&job).map_err(BootstrapError::RecordConversion)?)?;
        store.put_session_inbox_event(
            &agent_persistence::SessionInboxEventRecord::try_from(&inbox_event)
                .map_err(BootstrapError::RecordConversion)?,
        )?;
        Ok(())
    }
}
