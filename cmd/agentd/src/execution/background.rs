use super::*;
use agent_runtime::inbox::SessionInboxEvent;
use agent_runtime::mission::JobKind;
use agent_runtime::run::RunStatus;
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
        let supervisor = self.supervisor_tick(store, now, &[])?;
        let mut report = BackgroundWorkerTickReport {
            queued_jobs: supervisor.queued_jobs,
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
            JobKind::ChatTurn => {
                self.execute_background_chat_turn_job(store, provider, job_id, now)?;
                Ok(())
            }
            JobKind::ApprovalContinuation => {
                self.execute_background_approval_job(store, provider, job_id, now)?;
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
        let event = match job.status {
            JobStatus::Completed => {
                let summary = match job.result.as_ref() {
                    Some(JobResult::Summary { outcome }) => outcome.clone(),
                    None => job
                        .last_progress_message
                        .clone()
                        .unwrap_or_else(|| "background job completed".to_string()),
                };
                Some(SessionInboxEvent::job_completed(
                    format!("inbox-{}-completed-{}", job.id, job.updated_at),
                    &job.session_id,
                    Some(job.id.as_str()),
                    summary,
                    now,
                ))
            }
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
