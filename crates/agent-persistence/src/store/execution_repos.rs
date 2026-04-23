use super::*;
use crate::{RunSummaryRollup, SessionActiveJobCounts};
use agent_runtime::provider::ProviderUsage;
use rusqlite::OptionalExtension;

impl RunRepository for PersistenceStore {
    fn put_run(&self, record: &RunRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO runs (
                id, session_id, mission_id, status, error, result, provider_usage_json, active_processes_json, recent_steps_json, evidence_refs_json,
                pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                mission_id = excluded.mission_id,
                status = excluded.status,
                error = excluded.error,
                result = excluded.result,
                provider_usage_json = excluded.provider_usage_json,
                active_processes_json = excluded.active_processes_json,
                recent_steps_json = excluded.recent_steps_json,
                evidence_refs_json = excluded.evidence_refs_json,
                pending_approvals_json = excluded.pending_approvals_json,
                provider_loop_json = excluded.provider_loop_json,
                delegate_runs_json = excluded.delegate_runs_json,
                started_at = excluded.started_at,
                updated_at = excluded.updated_at,
                finished_at = excluded.finished_at",
            params![
                record.id,
                record.session_id,
                record.mission_id,
                record.status,
                record.error,
                record.result,
                record.provider_usage_json,
                record.active_processes_json,
                record.recent_steps_json,
                record.evidence_refs_json,
                record.pending_approvals_json,
                record.provider_loop_json,
                record.delegate_runs_json,
                record.started_at,
                record.updated_at,
                record.finished_at
            ],
        )?;
        Ok(())
    }

    fn get_run(&self, id: &str) -> Result<Option<RunRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, session_id, mission_id, status, error, result, provider_usage_json, active_processes_json, recent_steps_json,
                        evidence_refs_json, pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
                 FROM runs WHERE id = ?1",
                [id],
                |row| {
                    Ok(RunRecord {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        mission_id: row.get(2)?,
                        status: row.get(3)?,
                        error: row.get(4)?,
                        result: row.get(5)?,
                        provider_usage_json: row.get(6)?,
                        active_processes_json: row.get(7)?,
                        recent_steps_json: row.get(8)?,
                        evidence_refs_json: row.get(9)?,
                        pending_approvals_json: row.get(10)?,
                        provider_loop_json: row.get(11)?,
                        delegate_runs_json: row.get(12)?,
                        started_at: row.get(13)?,
                        updated_at: row.get(14)?,
                        finished_at: row.get(15)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_runs(&self) -> Result<Vec<RunRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, mission_id, status, error, result, provider_usage_json, active_processes_json, recent_steps_json, evidence_refs_json,
                    pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
             FROM runs
             ORDER BY started_at ASC, id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut runs = Vec::new();

        while let Some(row) = rows.next()? {
            runs.push(RunRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                mission_id: row.get(2)?,
                status: row.get(3)?,
                error: row.get(4)?,
                result: row.get(5)?,
                provider_usage_json: row.get(6)?,
                active_processes_json: row.get(7)?,
                recent_steps_json: row.get(8)?,
                evidence_refs_json: row.get(9)?,
                pending_approvals_json: row.get(10)?,
                provider_loop_json: row.get(11)?,
                delegate_runs_json: row.get(12)?,
                started_at: row.get(13)?,
                updated_at: row.get(14)?,
                finished_at: row.get(15)?,
            });
        }

        Ok(runs)
    }

    fn list_runs_for_session(&self, session_id: &str) -> Result<Vec<RunRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, mission_id, status, error, result, provider_usage_json, active_processes_json, recent_steps_json, evidence_refs_json,
                    pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
             FROM runs
             WHERE session_id = ?1
             ORDER BY started_at ASC, id ASC",
        )?;
        let mut rows = statement.query([session_id])?;
        let mut runs = Vec::new();

        while let Some(row) = rows.next()? {
            runs.push(RunRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                mission_id: row.get(2)?,
                status: row.get(3)?,
                error: row.get(4)?,
                result: row.get(5)?,
                provider_usage_json: row.get(6)?,
                active_processes_json: row.get(7)?,
                recent_steps_json: row.get(8)?,
                evidence_refs_json: row.get(9)?,
                pending_approvals_json: row.get(10)?,
                provider_loop_json: row.get(11)?,
                delegate_runs_json: row.get(12)?,
                started_at: row.get(13)?,
                updated_at: row.get(14)?,
                finished_at: row.get(15)?,
            });
        }

        Ok(runs)
    }

    fn list_recent_runs_for_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<RunRecord>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut statement = self.connection.prepare(
            "SELECT id, session_id, mission_id, status, error, result, provider_usage_json, active_processes_json, recent_steps_json, evidence_refs_json,
                    pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
             FROM runs
             WHERE session_id = ?1
             ORDER BY updated_at DESC, started_at DESC, id DESC
             LIMIT ?2",
        )?;
        let mut rows = statement.query(params![session_id, limit as i64])?;
        let mut runs = Vec::new();

        while let Some(row) = rows.next()? {
            runs.push(RunRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                mission_id: row.get(2)?,
                status: row.get(3)?,
                error: row.get(4)?,
                result: row.get(5)?,
                provider_usage_json: row.get(6)?,
                active_processes_json: row.get(7)?,
                recent_steps_json: row.get(8)?,
                evidence_refs_json: row.get(9)?,
                pending_approvals_json: row.get(10)?,
                provider_loop_json: row.get(11)?,
                delegate_runs_json: row.get(12)?,
                started_at: row.get(13)?,
                updated_at: row.get(14)?,
                finished_at: row.get(15)?,
            });
        }

        runs.reverse();
        Ok(runs)
    }

    fn get_latest_run_summary_rollup_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<RunSummaryRollup>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, session_id, provider_usage_json, pending_approvals_json, started_at, updated_at
                 FROM runs
                 WHERE session_id = ?1
                 ORDER BY updated_at DESC, started_at DESC, id DESC
                 LIMIT 1",
                [session_id],
                row_to_run_summary_rollup,
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn session_has_pending_approval(&self, session_id: &str) -> Result<bool, StoreError> {
        self.connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1
                    FROM runs
                    WHERE session_id = ?1
                      AND pending_approvals_json NOT IN ('[]', 'null', '')
                    LIMIT 1
                )",
                [session_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|exists| exists != 0)
            .map_err(StoreError::from)
    }

    fn list_run_summary_rollups(&self) -> Result<Vec<RunSummaryRollup>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, provider_usage_json, pending_approvals_json, started_at, updated_at
             FROM runs
             ORDER BY updated_at ASC, started_at ASC, id ASC",
        )?;
        collect_run_summary_rollups(&mut statement, &[] as &[&dyn rusqlite::ToSql])
    }

    fn list_run_summary_rollups_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<RunSummaryRollup>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, provider_usage_json, pending_approvals_json, started_at, updated_at
             FROM runs
             WHERE session_id = ?1
             ORDER BY updated_at ASC, started_at ASC, id ASC",
        )?;
        collect_run_summary_rollups(&mut statement, &[&session_id])
    }
}

impl JobRepository for PersistenceStore {
    fn put_job(&self, record: &JobRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO jobs (
                id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
                result_json, error, created_at, updated_at, started_at, finished_at,
                attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
                cancel_requested_at, last_progress_message, callback_json, callback_sent_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
             ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                mission_id = excluded.mission_id,
                run_id = excluded.run_id,
                parent_job_id = excluded.parent_job_id,
                kind = excluded.kind,
                status = excluded.status,
                input_json = excluded.input_json,
                result_json = excluded.result_json,
                error = excluded.error,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                started_at = excluded.started_at,
                finished_at = excluded.finished_at,
                attempt_count = excluded.attempt_count,
                max_attempts = excluded.max_attempts,
                lease_owner = excluded.lease_owner,
                lease_expires_at = excluded.lease_expires_at,
                heartbeat_at = excluded.heartbeat_at,
                cancel_requested_at = excluded.cancel_requested_at,
                last_progress_message = excluded.last_progress_message,
                callback_json = excluded.callback_json,
                callback_sent_at = excluded.callback_sent_at",
            params![
                record.id,
                record.session_id,
                record.mission_id,
                record.run_id,
                record.parent_job_id,
                record.kind,
                record.status,
                record.input_json,
                record.result_json,
                record.error,
                record.created_at,
                record.updated_at,
                record.started_at,
                record.finished_at,
                record.attempt_count,
                record.max_attempts,
                record.lease_owner,
                record.lease_expires_at,
                record.heartbeat_at,
                record.cancel_requested_at,
                record.last_progress_message,
                record.callback_json,
                record.callback_sent_at
            ],
        )?;
        Ok(())
    }

    fn get_job(&self, id: &str) -> Result<Option<JobRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
                        result_json, error, created_at, updated_at, started_at, finished_at,
                        attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
                        cancel_requested_at, last_progress_message, callback_json, callback_sent_at
                 FROM jobs WHERE id = ?1",
                [id],
                |row| {
                    Ok(JobRecord {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        mission_id: row.get(2)?,
                        run_id: row.get(3)?,
                        parent_job_id: row.get(4)?,
                        kind: row.get(5)?,
                        status: row.get(6)?,
                        input_json: row.get(7)?,
                        result_json: row.get(8)?,
                        error: row.get(9)?,
                        created_at: row.get(10)?,
                        updated_at: row.get(11)?,
                        started_at: row.get(12)?,
                        finished_at: row.get(13)?,
                        attempt_count: row.get(14)?,
                        max_attempts: row.get(15)?,
                        lease_owner: row.get(16)?,
                        lease_expires_at: row.get(17)?,
                        heartbeat_at: row.get(18)?,
                        cancel_requested_at: row.get(19)?,
                        last_progress_message: row.get(20)?,
                        callback_json: row.get(21)?,
                        callback_sent_at: row.get(22)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_jobs(&self) -> Result<Vec<JobRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
                    result_json, error, created_at, updated_at, started_at, finished_at,
                    attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
                    cancel_requested_at, last_progress_message, callback_json, callback_sent_at
             FROM jobs
             ORDER BY created_at ASC, id ASC",
        )?;
        collect_jobs(&mut statement, &[] as &[&dyn rusqlite::ToSql])
    }

    fn list_jobs_for_session(&self, session_id: &str) -> Result<Vec<JobRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
                    result_json, error, created_at, updated_at, started_at, finished_at,
                    attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
                    cancel_requested_at, last_progress_message, callback_json, callback_sent_at
             FROM jobs
             WHERE session_id = ?1
             ORDER BY created_at ASC, id ASC",
        )?;
        collect_jobs(&mut statement, &[&session_id])
    }

    fn list_active_jobs_for_session(&self, session_id: &str) -> Result<Vec<JobRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
                    result_json, error, created_at, updated_at, started_at, finished_at,
                    attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
                    cancel_requested_at, last_progress_message, callback_json, callback_sent_at
             FROM jobs
             WHERE session_id = ?1
               AND status IN ('queued', 'running', 'waiting_external', 'blocked')
             ORDER BY created_at ASC, id ASC",
        )?;
        collect_jobs(&mut statement, &[&session_id])
    }

    fn list_active_job_counts(&self) -> Result<Vec<SessionActiveJobCounts>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT session_id,
                    COUNT(*) AS active_count,
                    SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END) AS running_count,
                    SUM(CASE WHEN status = 'queued' THEN 1 ELSE 0 END) AS queued_count
             FROM jobs
             WHERE status IN ('queued', 'running', 'waiting_external', 'blocked')
             GROUP BY session_id
             ORDER BY session_id ASC",
        )?;
        collect_active_job_counts(&mut statement, &[] as &[&dyn rusqlite::ToSql])
    }

    fn get_active_job_counts_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionActiveJobCounts>, StoreError> {
        self.connection
            .query_row(
                "SELECT session_id,
                        COUNT(*) AS active_count,
                        SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END) AS running_count,
                        SUM(CASE WHEN status = 'queued' THEN 1 ELSE 0 END) AS queued_count
                 FROM jobs
                 WHERE session_id = ?1
                   AND status IN ('queued', 'running', 'waiting_external', 'blocked')
                 GROUP BY session_id",
                [session_id],
                |row| {
                    Ok(SessionActiveJobCounts {
                        session_id: row.get(0)?,
                        active_count: row.get::<_, i64>(1)?.max(0) as usize,
                        running_count: row.get::<_, i64>(2)?.max(0) as usize,
                        queued_count: row.get::<_, i64>(3)?.max(0) as usize,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }
}

fn collect_run_summary_rollups(
    statement: &mut rusqlite::Statement<'_>,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<RunSummaryRollup>, StoreError> {
    let mut rows = statement.query(params)?;
    let mut rollups = Vec::new();

    while let Some(row) = rows.next()? {
        rollups.push(row_to_run_summary_rollup(row)?);
    }

    Ok(rollups)
}

fn row_to_run_summary_rollup(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunSummaryRollup> {
    let provider_usage_json: String = row.get(2)?;
    let pending_approvals_json: String = row.get(3)?;
    let latest_provider_usage =
        serde_json::from_str::<Option<ProviderUsage>>(&provider_usage_json).unwrap_or(None);
    let pending_approval_count =
        serde_json::from_str::<Vec<serde_json::Value>>(&pending_approvals_json)
            .map(|entries| entries.len())
            .unwrap_or(0);

    Ok(RunSummaryRollup {
        id: row.get(0)?,
        session_id: row.get(1)?,
        latest_provider_usage,
        pending_approval_count,
        started_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn collect_jobs(
    statement: &mut rusqlite::Statement<'_>,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<JobRecord>, StoreError> {
    let mut rows = statement.query(params)?;
    let mut jobs = Vec::new();

    while let Some(row) = rows.next()? {
        jobs.push(JobRecord {
            id: row.get(0)?,
            session_id: row.get(1)?,
            mission_id: row.get(2)?,
            run_id: row.get(3)?,
            parent_job_id: row.get(4)?,
            kind: row.get(5)?,
            status: row.get(6)?,
            input_json: row.get(7)?,
            result_json: row.get(8)?,
            error: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
            started_at: row.get(12)?,
            finished_at: row.get(13)?,
            attempt_count: row.get(14)?,
            max_attempts: row.get(15)?,
            lease_owner: row.get(16)?,
            lease_expires_at: row.get(17)?,
            heartbeat_at: row.get(18)?,
            cancel_requested_at: row.get(19)?,
            last_progress_message: row.get(20)?,
            callback_json: row.get(21)?,
            callback_sent_at: row.get(22)?,
        });
    }

    Ok(jobs)
}

fn collect_active_job_counts(
    statement: &mut rusqlite::Statement<'_>,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<SessionActiveJobCounts>, StoreError> {
    let mut rows = statement.query(params)?;
    let mut counts = Vec::new();

    while let Some(row) = rows.next()? {
        counts.push(SessionActiveJobCounts {
            session_id: row.get(0)?,
            active_count: row.get::<_, i64>(1)?.max(0) as usize,
            running_count: row.get::<_, i64>(2)?.max(0) as usize,
            queued_count: row.get::<_, i64>(3)?.max(0) as usize,
        });
    }

    Ok(counts)
}
