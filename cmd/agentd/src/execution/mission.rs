use super::*;
use agent_runtime::session::TranscriptEntry;

impl ExecutionService {
    pub fn execute_mission_turn_job(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        job_id: &str,
        now: i64,
    ) -> Result<MissionTurnExecutionReport, ExecutionError> {
        let mut job = JobSpec::try_from(
            store
                .get_job(job_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingJob {
                    id: job_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let mut mission = MissionSpec::try_from(
            store
                .get_mission(&job.mission_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingMission {
                    id: job.mission_id.clone(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let session_record = store
            .get_session(&mission.session_id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| ExecutionError::MissingSession {
                id: mission.session_id.clone(),
            })?;
        let session =
            Session::try_from(session_record).map_err(ExecutionError::RecordConversion)?;

        let goal = match &job.input {
            JobExecutionInput::MissionTurn { mission_id, goal } if mission_id == &mission.id => {
                goal.clone()
            }
            _ => {
                return Err(ExecutionError::UnsupportedJobInput {
                    id: job.id.clone(),
                    kind: job.kind.as_str().to_string(),
                });
            }
        };

        let run_id = job
            .run_id
            .clone()
            .unwrap_or_else(|| format!("run-{}", job.id));
        let mut run = RunEngine::new(
            run_id.clone(),
            session.id.clone(),
            Some(mission.id.as_str()),
            now,
        );
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

        mission.status = MissionStatus::Running;
        mission.updated_at = now;
        store
            .put_mission(
                &MissionRecord::try_from(&mission).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

        let user_entry = TranscriptEntry::user(
            format!("transcript-{}-01-user", job.id),
            session.id.clone(),
            Some(run_id.as_str()),
            &goal,
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
            &mut observer,
        ) {
            Ok(response) => response,
            Err(source @ ExecutionError::ApprovalRequired { .. }) => {
                job.status = JobStatus::Blocked;
                job.error = Some(source.to_string());
                job.updated_at = now;
                mission.updated_at = now;
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                store
                    .put_mission(
                        &MissionRecord::try_from(&mission)
                            .map_err(ExecutionError::RecordConversion)?,
                    )
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
            Err(source) => {
                if !matches!(
                    source,
                    ExecutionError::PermissionDenied { .. }
                        | ExecutionError::ApprovalRequired { .. }
                ) {
                    run.fail(source.to_string(), now)
                        .map_err(ExecutionError::RunTransition)?;
                    self.persist_run(store, &run)?;
                }
                job.status = JobStatus::Failed;
                job.error = Some(source.to_string());
                job.finished_at = Some(now);
                job.updated_at = now;
                mission.updated_at = now;
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                store
                    .put_mission(
                        &MissionRecord::try_from(&mission)
                            .map_err(ExecutionError::RecordConversion)?,
                    )
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
        };

        run.complete(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;

        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-{}-02-assistant", job.id),
            session.id,
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
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;

        Ok(MissionTurnExecutionReport {
            job_id: job.id,
            run_id,
            response_id: response.response_id,
            output_text: response.output_text,
        })
    }
}
