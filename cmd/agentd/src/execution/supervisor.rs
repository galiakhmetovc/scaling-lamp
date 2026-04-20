use super::*;
use agent_runtime::scheduler::SupervisorTickInput;
use std::collections::BTreeMap;

impl ExecutionService {
    pub fn supervisor_tick(
        &self,
        store: &PersistenceStore,
        now: i64,
        verifications: &[MissionVerificationSummary],
    ) -> Result<SupervisorTickReport, ExecutionError> {
        let state = store
            .load_execution_state()
            .map_err(ExecutionError::Store)?;
        let missions = state
            .missions
            .into_iter()
            .map(MissionSpec::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        let jobs = state
            .jobs
            .into_iter()
            .map(JobSpec::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        let runs = state
            .runs
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;

        let tick = self.supervisor.tick(SupervisorTickInput {
            now,
            missions: &missions,
            jobs: &jobs,
            runs: &runs,
            verifications,
        });

        let mission_by_id = missions
            .into_iter()
            .map(|mission| (mission.id.clone(), mission))
            .collect::<BTreeMap<_, _>>();
        let job_by_id = jobs
            .into_iter()
            .map(|job| (job.id.clone(), job))
            .collect::<BTreeMap<_, _>>();
        let mut report = SupervisorTickReport {
            actions: tick.actions.clone(),
            queued_jobs: 0,
            dispatched_jobs: 0,
            blocked_jobs: 0,
            deferred_missions: 0,
            completed_missions: 0,
            budget_remaining: tick.budget_remaining,
        };

        for action in &tick.actions {
            match action {
                SupervisorAction::QueueJob(job) => {
                    store
                        .put_job(
                            &JobRecord::try_from(job.as_ref())
                                .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                    touch_mission(store, &mission_by_id, &job.mission_id, now)?;
                    report.queued_jobs += 1;
                }
                SupervisorAction::DispatchJob { job_id, .. } => {
                    let mut job = job_by_id
                        .get(job_id)
                        .cloned()
                        .ok_or_else(|| ExecutionError::MissingJob { id: job_id.clone() })?;
                    job.status = JobStatus::Running;
                    job.updated_at = now;
                    if job.started_at.is_none() {
                        job.started_at = Some(now);
                    }
                    store
                        .put_job(
                            &JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                    touch_mission(store, &mission_by_id, &job.mission_id, now)?;
                    report.dispatched_jobs += 1;
                }
                SupervisorAction::RequestApproval { job_id, reason } => {
                    let mut job = job_by_id
                        .get(job_id)
                        .cloned()
                        .ok_or_else(|| ExecutionError::MissingJob { id: job_id.clone() })?;
                    job.status = JobStatus::Blocked;
                    job.error = Some(reason.clone());
                    job.updated_at = now;
                    store
                        .put_job(
                            &JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                    touch_mission(store, &mission_by_id, &job.mission_id, now)?;
                    report.blocked_jobs += 1;
                }
                SupervisorAction::DeferMission { mission_id, .. } => {
                    touch_mission(store, &mission_by_id, mission_id, now)?;
                    report.deferred_missions += 1;
                }
                SupervisorAction::CompleteMission { mission_id } => {
                    let mut mission = mission_by_id.get(mission_id).cloned().ok_or_else(|| {
                        ExecutionError::MissingMission {
                            id: mission_id.clone(),
                        }
                    })?;
                    mission.status = MissionStatus::Completed;
                    mission.updated_at = now;
                    mission.completed_at = Some(now);
                    store
                        .put_mission(
                            &MissionRecord::try_from(&mission)
                                .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                    report.completed_missions += 1;
                }
            }
        }

        Ok(report)
    }
}

fn touch_mission(
    store: &PersistenceStore,
    mission_by_id: &BTreeMap<String, MissionSpec>,
    mission_id: &str,
    now: i64,
) -> Result<(), ExecutionError> {
    let mut mission =
        mission_by_id
            .get(mission_id)
            .cloned()
            .ok_or_else(|| ExecutionError::MissingMission {
                id: mission_id.to_string(),
            })?;
    mission.updated_at = now;
    store
        .put_mission(&MissionRecord::try_from(&mission).map_err(ExecutionError::RecordConversion)?)
        .map_err(ExecutionError::Store)?;
    Ok(())
}
