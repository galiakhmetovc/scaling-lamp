use super::*;
use crate::http::types::{A2ADelegationCompletionOutcomeRequest, A2ADelegationCompletionRequest};
use agent_runtime::agent::AgentSchedule;
use agent_runtime::delegation::DelegateResultPackage;
use agent_runtime::inbox::SessionInboxEvent;
use agent_runtime::mission::JobKind;
use agent_runtime::run::RunStatus;
use agent_runtime::session::{Session, TranscriptEntry};
use std::collections::BTreeSet;

const DAEMON_WORKER_LEASE_OWNER: &str = "daemon";
const DAEMON_WORKER_LEASE_SECONDS: i64 = 60;

impl ExecutionService {
    pub fn background_worker_tick(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        now: i64,
    ) -> Result<BackgroundWorkerTickReport, ExecutionError> {
        let fired_schedules = self.dispatch_due_agent_schedules(store, now)?;
        let supervisor = self.supervisor_tick(store, now, &[])?;
        let mut report = BackgroundWorkerTickReport {
            queued_jobs: supervisor.queued_jobs + fired_schedules,
            dispatched_jobs: supervisor.dispatched_jobs,
            ..BackgroundWorkerTickReport::default()
        };

        let jobs = store
            .list_jobs()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(JobSpec::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;

        for job in jobs
            .into_iter()
            .filter(|job| self.should_run_background_job(job, now))
        {
            if job.cancel_requested_at.is_some() {
                self.cancel_background_job(store, &job.id, now)?;
                continue;
            }
            self.claim_background_job(store, &job.id, now)?;
            match self.execute_background_job(store, provider, &job.id, now) {
                Ok(()) => {
                    report.executed_jobs += 1;
                }
                Err(ExecutionError::ApprovalRequired { .. }) => {
                    report.executed_jobs += 1;
                }
                Err(error) => {
                    return Err(error);
                }
            }

            let updated_job = self.load_job(store, &job.id)?;
            self.deliver_callback_for_job(store, &updated_job, now)?;
            let updated_job = self.load_job(store, &job.id)?;
            if self.emit_inbox_event_for_job(store, &updated_job, now)? {
                report.emitted_inbox_events += 1;
            }
        }

        let wakeable_sessions = store
            .list_queued_session_inbox_events()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .filter(|event| event.available_at <= now)
            .map(|event| event.session_id)
            .collect::<BTreeSet<_>>();

        for session_id in wakeable_sessions {
            if self.session_has_active_run(store, &session_id)? {
                continue;
            }
            if self.execute_session_wakeup_turn(store, provider, &session_id, now + 1)? {
                report.woken_sessions += 1;
            }
        }

        Ok(report)
    }

    fn should_run_background_job(&self, job: &JobSpec, now: i64) -> bool {
        if job.status != JobStatus::Running {
            return false;
        }

        match job.lease_expires_at {
            Some(expires_at) => expires_at <= now,
            None => true,
        }
    }

    fn claim_background_job(
        &self,
        store: &PersistenceStore,
        job_id: &str,
        now: i64,
    ) -> Result<(), ExecutionError> {
        let mut job = self.load_job(store, job_id)?;
        job.lease_owner = Some(DAEMON_WORKER_LEASE_OWNER.to_string());
        job.lease_expires_at = Some(now + DAEMON_WORKER_LEASE_SECONDS);
        job.heartbeat_at = Some(now);
        job.attempt_count = job.attempt_count.saturating_add(1);
        if job.last_progress_message.is_none() {
            job.last_progress_message = Some("background worker picked up the job".to_string());
        }
        job.updated_at = now;
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)
    }

    fn execute_background_job(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        job_id: &str,
        now: i64,
    ) -> Result<(), ExecutionError> {
        let job = self.load_job(store, job_id)?;
        match job.kind {
            JobKind::MissionTurn => {
                self.execute_mission_turn_job(store, provider, job_id, now)?;
                Ok(())
            }
            JobKind::ChatTurn | JobKind::ScheduledChatTurn => {
                self.execute_background_chat_turn_job(store, provider, job_id, now)?;
                Ok(())
            }
            JobKind::InterAgentMessage => {
                self.execute_background_interagent_message_job(store, provider, job_id, now)?;
                Ok(())
            }
            JobKind::ApprovalContinuation => {
                self.execute_background_approval_job(store, provider, job_id, now)?;
                Ok(())
            }
            JobKind::Delegate => {
                self.execute_background_delegate_job(store, provider, job_id, now)?;
                Ok(())
            }
            _ => {
                let mut job = job;
                job.status = JobStatus::Failed;
                job.error = Some(format!(
                    "background execution is not implemented for job kind {}",
                    job.kind.as_str()
                ));
                job.finished_at = Some(now);
                job.updated_at = now;
                job.last_progress_message = Some("background execution failed".to_string());
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                Ok(())
            }
        }
    }

    fn emit_inbox_event_for_job(
        &self,
        store: &PersistenceStore,
        job: &JobSpec,
        now: i64,
    ) -> Result<bool, ExecutionError> {
        if job.callback.is_some() {
            return Ok(false);
        }
        if job.kind == JobKind::ScheduledChatTurn {
            return Ok(false);
        }
        if let JobExecutionInput::InterAgentMessage {
            source_session_id,
            target_agent_name,
            ..
        } = &job.input
        {
            let event = match job.status {
                JobStatus::Completed => match job.result.as_ref() {
                    Some(JobResult::Summary { outcome }) => {
                        Some(SessionInboxEvent::external_input_received(
                            format!("inbox-{}-interagent-completed-{}", job.id, job.updated_at),
                            source_session_id,
                            Some(job.id.as_str()),
                            target_agent_name.clone(),
                            outcome.clone(),
                            now,
                        ))
                    }
                    _ => None,
                },
                JobStatus::Failed | JobStatus::Blocked | JobStatus::Cancelled => {
                    Some(SessionInboxEvent::external_input_received(
                        format!("inbox-{}-interagent-terminal-{}", job.id, job.updated_at),
                        source_session_id,
                        Some(job.id.as_str()),
                        target_agent_name.clone(),
                        job.error
                            .clone()
                            .or_else(|| job.last_progress_message.clone())
                            .unwrap_or_else(|| "inter-agent message failed".to_string()),
                        now,
                    ))
                }
                _ => None,
            };

            let Some(event) = event else {
                return Ok(false);
            };
            let record = agent_persistence::SessionInboxEventRecord::try_from(&event)
                .map_err(ExecutionError::RecordConversion)?;
            store
                .put_session_inbox_event(&record)
                .map_err(ExecutionError::Store)?;
            return Ok(true);
        }
        let event = match job.status {
            JobStatus::Completed => match job.result.as_ref() {
                Some(JobResult::Delegation {
                    child_session_id: _,
                    package,
                }) => Some(SessionInboxEvent::delegation_result_ready(
                    format!("inbox-{}-delegation-{}", job.id, job.updated_at),
                    &job.session_id,
                    Some(job.id.as_str()),
                    package.summary.clone(),
                    package.artifact_refs.clone(),
                    now,
                )),
                Some(JobResult::Summary { outcome }) => Some(SessionInboxEvent::job_completed(
                    format!("inbox-{}-completed-{}", job.id, job.updated_at),
                    &job.session_id,
                    Some(job.id.as_str()),
                    outcome.clone(),
                    now,
                )),
                None => Some(SessionInboxEvent::job_completed(
                    format!("inbox-{}-completed-{}", job.id, job.updated_at),
                    &job.session_id,
                    Some(job.id.as_str()),
                    job.last_progress_message
                        .clone()
                        .unwrap_or_else(|| "background job completed".to_string()),
                    now,
                )),
            },
            JobStatus::Failed => Some(SessionInboxEvent::job_failed(
                format!("inbox-{}-failed-{}", job.id, job.updated_at),
                &job.session_id,
                Some(job.id.as_str()),
                job.error
                    .clone()
                    .unwrap_or_else(|| "background job failed".to_string()),
                now,
            )),
            JobStatus::Blocked => Some(SessionInboxEvent::job_blocked(
                format!("inbox-{}-blocked-{}", job.id, job.updated_at),
                &job.session_id,
                Some(job.id.as_str()),
                job.error
                    .clone()
                    .unwrap_or_else(|| "background job blocked".to_string()),
                now,
            )),
            _ => None,
        };

        let Some(event) = event else {
            return Ok(false);
        };
        let record = agent_persistence::SessionInboxEventRecord::try_from(&event)
            .map_err(ExecutionError::RecordConversion)?;
        store
            .put_session_inbox_event(&record)
            .map_err(ExecutionError::Store)?;
        Ok(true)
    }

    fn dispatch_due_agent_schedules(
        &self,
        store: &PersistenceStore,
        now: i64,
    ) -> Result<usize, ExecutionError> {
        let current_workspace = canonical_workspace_root(&self.workspace.root);
        let schedules = store
            .list_agent_schedules()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(AgentSchedule::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        let mut fired = 0usize;

        for mut schedule in schedules {
            if schedule.workspace_root != current_workspace || !schedule.is_due(now) {
                continue;
            }

            if let Some(last_job_id) = schedule.last_job_id.as_deref()
                && let Some(record) = store.get_job(last_job_id).map_err(ExecutionError::Store)?
            {
                let job = JobSpec::try_from(record).map_err(ExecutionError::RecordConversion)?;
                if job.status.is_active() {
                    continue;
                }
            }

            let Some(agent_record) = store
                .get_agent_profile(&schedule.agent_profile_id)
                .map_err(ExecutionError::Store)?
            else {
                schedule.next_fire_at = now
                    .saturating_add(i64::try_from(schedule.interval_seconds).unwrap_or(i64::MAX));
                schedule.updated_at = now;
                store
                    .put_agent_schedule(&agent_persistence::AgentScheduleRecord::from(&schedule))
                    .map_err(ExecutionError::Store)?;
                continue;
            };
            let agent =
                AgentProfile::try_from(agent_record).map_err(ExecutionError::RecordConversion)?;

            let session_id = format!(
                "session-schedule-{}-{}",
                schedule.id,
                unique_execution_token()
            );
            let job_id = format!("job-schedule-{}-{}", schedule.id, unique_execution_token());
            let session = Session {
                id: session_id.clone(),
                title: format!("Расписание: {}", schedule.id),
                prompt_override: None,
                settings: self.config.session_defaults.clone(),
                agent_profile_id: schedule.agent_profile_id.clone(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: Some(format!("agent-schedule:{}", schedule.id)),
                created_at: now,
                updated_at: now,
            };
            store
                .put_session(
                    &agent_persistence::SessionRecord::try_from(&session)
                        .map_err(ExecutionError::RecordConversion)?,
                )
                .map_err(ExecutionError::Store)?;

            let system_entry = TranscriptEntry::system(
                format!("transcript-{}-schedule-system", job_id),
                session_id.clone(),
                None,
                format!(
                    "Scheduled agent launch.\nschedule_id: {}\nagent_profile_id: {}\nagent_name: {}\nworkspace_root: {}",
                    schedule.id,
                    schedule.agent_profile_id,
                    agent.name,
                    schedule.workspace_root.display()
                ),
                now,
            );
            store
                .put_transcript(&TranscriptRecord::from(&system_entry))
                .map_err(ExecutionError::Store)?;

            let mut job = JobSpec::scheduled_chat_turn(
                &job_id,
                &session_id,
                None,
                None,
                &schedule.id,
                &schedule.prompt,
                now,
            );
            job.status = JobStatus::Running;
            job.last_progress_message =
                Some(format!("scheduled launch queued from {}", schedule.id));
            store
                .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                .map_err(ExecutionError::Store)?;

            schedule.last_triggered_at = Some(now);
            schedule.last_session_id = Some(session_id);
            schedule.last_job_id = Some(job_id);
            schedule.next_fire_at =
                now.saturating_add(i64::try_from(schedule.interval_seconds).unwrap_or(i64::MAX));
            schedule.updated_at = now;
            store
                .put_agent_schedule(&agent_persistence::AgentScheduleRecord::from(&schedule))
                .map_err(ExecutionError::Store)?;
            fired += 1;
        }

        Ok(fired)
    }

    fn deliver_callback_for_job(
        &self,
        store: &PersistenceStore,
        job: &JobSpec,
        now: i64,
    ) -> Result<(), ExecutionError> {
        let Some(callback) = job.callback.as_ref() else {
            return Ok(());
        };
        if job.callback_sent_at.is_some() {
            return Ok(());
        }
        if !matches!(
            job.status,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Blocked | JobStatus::Cancelled
        ) {
            return Ok(());
        }

        let request = self.build_a2a_completion_request(store, job, now)?;
        match self
            .a2a
            .send_completion(&callback.url, callback.bearer_token.as_deref(), &request)
        {
            Ok(()) => {
                let mut updated = job.clone();
                updated.callback_sent_at = Some(now);
                updated.updated_at = now;
                updated.last_progress_message =
                    Some("a2a completion callback delivered".to_string());
                store
                    .put_job(
                        &JobRecord::try_from(&updated).map_err(ExecutionError::RecordConversion)?,
                    )
                    .map_err(ExecutionError::Store)?;
            }
            Err(reason) => {
                let mut updated = job.clone();
                updated.updated_at = now;
                updated.last_progress_message =
                    Some(format!("a2a completion callback failed: {reason}"));
                store
                    .put_job(
                        &JobRecord::try_from(&updated).map_err(ExecutionError::RecordConversion)?,
                    )
                    .map_err(ExecutionError::Store)?;
            }
        }

        Ok(())
    }

    fn build_a2a_completion_request(
        &self,
        store: &PersistenceStore,
        job: &JobSpec,
        now: i64,
    ) -> Result<A2ADelegationCompletionRequest, ExecutionError> {
        let outcome = match job.status {
            JobStatus::Completed => {
                let package = match job.result.as_ref() {
                    Some(JobResult::Delegation {
                        child_session_id: _,
                        package,
                    }) => package.clone(),
                    Some(JobResult::Summary { outcome }) => DelegateResultPackage::new(
                        outcome.clone(),
                        Vec::new(),
                        self.collect_job_artifact_refs(store, &job.session_id)?,
                        Vec::new(),
                    )
                    .map_err(|source| ExecutionError::ProviderLoop {
                        reason: source.to_string(),
                    })?,
                    None => DelegateResultPackage::new(
                        job.last_progress_message
                            .clone()
                            .unwrap_or_else(|| "remote delegated job completed".to_string()),
                        Vec::new(),
                        self.collect_job_artifact_refs(store, &job.session_id)?,
                        Vec::new(),
                    )
                    .map_err(|source| ExecutionError::ProviderLoop {
                        reason: source.to_string(),
                    })?,
                };
                A2ADelegationCompletionOutcomeRequest::Completed {
                    remote_session_id: job.session_id.clone(),
                    remote_job_id: job.id.clone(),
                    package,
                }
            }
            JobStatus::Failed => A2ADelegationCompletionOutcomeRequest::Failed {
                remote_session_id: job.session_id.clone(),
                remote_job_id: job.id.clone(),
                reason: job
                    .error
                    .clone()
                    .unwrap_or_else(|| "remote delegated job failed".to_string()),
            },
            JobStatus::Blocked => {
                let reason = job
                    .error
                    .clone()
                    .or_else(|| job.last_progress_message.clone())
                    .unwrap_or_else(|| "remote delegated job blocked".to_string());
                A2ADelegationCompletionOutcomeRequest::Blocked {
                    remote_session_id: job.session_id.clone(),
                    remote_job_id: job.id.clone(),
                    reason,
                }
            }
            JobStatus::Cancelled => A2ADelegationCompletionOutcomeRequest::Blocked {
                remote_session_id: job.session_id.clone(),
                remote_job_id: job.id.clone(),
                reason: "remote delegated job cancelled".to_string(),
            },
            _ => {
                return Err(ExecutionError::ProviderLoop {
                    reason: format!(
                        "cannot build a2a completion request from non-terminal status {}",
                        job.status.as_str()
                    ),
                });
            }
        };

        Ok(A2ADelegationCompletionRequest { outcome, now })
    }

    fn collect_job_artifact_refs(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<Vec<String>, ExecutionError> {
        Ok(store
            .get_context_offload(session_id)
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
            .unwrap_or_default())
    }

    fn load_job(&self, store: &PersistenceStore, job_id: &str) -> Result<JobSpec, ExecutionError> {
        JobSpec::try_from(
            store
                .get_job(job_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingJob {
                    id: job_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)
    }

    fn session_has_active_run(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<bool, ExecutionError> {
        Ok(store
            .list_runs()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?
            .into_iter()
            .any(|run| {
                run.session_id == session_id
                    && matches!(
                        run.status,
                        RunStatus::Queued
                            | RunStatus::Running
                            | RunStatus::WaitingApproval
                            | RunStatus::WaitingProcess
                            | RunStatus::Resuming
                    )
            }))
    }

    fn cancel_background_job(
        &self,
        store: &PersistenceStore,
        job_id: &str,
        now: i64,
    ) -> Result<(), ExecutionError> {
        let mut job = self.load_job(store, job_id)?;
        job.status = JobStatus::Cancelled;
        job.updated_at = now;
        job.finished_at = Some(now);
        job.lease_owner = None;
        job.lease_expires_at = None;
        job.heartbeat_at = Some(now);
        job.last_progress_message = Some("background job cancelled".to_string());
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)
    }
}

fn canonical_workspace_root(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
