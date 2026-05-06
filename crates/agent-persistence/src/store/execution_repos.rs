use super::*;
use crate::{RunSummaryRollup, SessionActiveJobCounts};
use agent_runtime::provider::ProviderUsage;

impl RunRepository for PersistenceStore {
    fn put_run(&self, record: &RunRecord) -> Result<(), StoreError> {
        self.with_client(|client| {
            client.execute(
                "INSERT INTO runs (
                    id, session_id, mission_id, status, error, result, provider_usage_json, active_processes_json, recent_steps_json, evidence_refs_json,
                    pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
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
                &[
                    &record.id,
                    &record.session_id,
                    &record.mission_id,
                    &record.status,
                    &record.error,
                    &record.result,
                    &record.provider_usage_json,
                    &record.active_processes_json,
                    &record.recent_steps_json,
                    &record.evidence_refs_json,
                    &record.pending_approvals_json,
                    &record.provider_loop_json,
                    &record.delegate_runs_json,
                    &record.started_at,
                    &record.updated_at,
                    &record.finished_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_run(&self, id: &str) -> Result<Option<RunRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(RUN_SELECT_WITH_WHERE_ID, &[&id])
                .map(|row| row.map(|row| run_record_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_runs(&self) -> Result<Vec<RunRecord>, StoreError> {
        self.query_runs(
            "SELECT id, session_id, mission_id, status, error, result, provider_usage_json, active_processes_json, recent_steps_json, evidence_refs_json,
                    pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
             FROM runs
             ORDER BY started_at ASC, id ASC",
            &[],
            false,
        )
    }

    fn list_runs_for_session(&self, session_id: &str) -> Result<Vec<RunRecord>, StoreError> {
        self.query_runs(
            "SELECT id, session_id, mission_id, status, error, result, provider_usage_json, active_processes_json, recent_steps_json, evidence_refs_json,
                    pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
             FROM runs
             WHERE session_id = $1
             ORDER BY started_at ASC, id ASC",
            &[&session_id],
            false,
        )
    }

    fn list_recent_runs_for_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<RunRecord>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = limit as i64;
        self.query_runs(
            "SELECT id, session_id, mission_id, status, error, result, provider_usage_json, active_processes_json, recent_steps_json, evidence_refs_json,
                    pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
             FROM runs
             WHERE session_id = $1
             ORDER BY updated_at DESC, started_at DESC, id DESC
             LIMIT $2",
            &[&session_id, &limit],
            true,
        )
    }

    fn get_latest_run_summary_rollup_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<RunSummaryRollup>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT id, session_id, provider_usage_json, pending_approvals_json, started_at, updated_at
                     FROM runs
                     WHERE session_id = $1
                     ORDER BY updated_at DESC, started_at DESC, id DESC
                     LIMIT 1",
                    &[&session_id],
                )
                .map(|row| row.map(|row| row_to_run_summary_rollup(&row)))
                .map_err(StoreError::from)
        })
    }

    fn session_has_pending_approval(&self, session_id: &str) -> Result<bool, StoreError> {
        self.with_client(|client| {
            client
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1
                        FROM runs
                        WHERE session_id = $1
                          AND pending_approvals_json NOT IN ('[]', 'null', '')
                        LIMIT 1
                    )",
                    &[&session_id],
                )
                .map(|row| row.get(0))
                .map_err(StoreError::from)
        })
    }

    fn list_run_summary_rollups(&self) -> Result<Vec<RunSummaryRollup>, StoreError> {
        self.query_run_rollups(
            "SELECT id, session_id, provider_usage_json, pending_approvals_json, started_at, updated_at
             FROM runs
             ORDER BY updated_at ASC, started_at ASC, id ASC",
            &[],
        )
    }

    fn list_run_summary_rollups_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<RunSummaryRollup>, StoreError> {
        self.query_run_rollups(
            "SELECT id, session_id, provider_usage_json, pending_approvals_json, started_at, updated_at
             FROM runs
             WHERE session_id = $1
             ORDER BY updated_at ASC, started_at ASC, id ASC",
            &[&session_id],
        )
    }
}

impl JobRepository for PersistenceStore {
    fn put_job(&self, record: &JobRecord) -> Result<(), StoreError> {
        self.with_client(|client| {
            client.execute(
                "INSERT INTO jobs (
                    id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
                    result_json, error, created_at, updated_at, started_at, finished_at,
                    attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
                    cancel_requested_at, last_progress_message, callback_json, callback_sent_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)
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
                &[
                    &record.id,
                    &record.session_id,
                    &record.mission_id,
                    &record.run_id,
                    &record.parent_job_id,
                    &record.kind,
                    &record.status,
                    &record.input_json,
                    &record.result_json,
                    &record.error,
                    &record.created_at,
                    &record.updated_at,
                    &record.started_at,
                    &record.finished_at,
                    &record.attempt_count,
                    &record.max_attempts,
                    &record.lease_owner,
                    &record.lease_expires_at,
                    &record.heartbeat_at,
                    &record.cancel_requested_at,
                    &record.last_progress_message,
                    &record.callback_json,
                    &record.callback_sent_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_job(&self, id: &str) -> Result<Option<JobRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(JOB_SELECT_WITH_WHERE_ID, &[&id])
                .map(|row| row.map(|row| job_record_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_jobs(&self) -> Result<Vec<JobRecord>, StoreError> {
        self.query_jobs(
            "SELECT id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
                    result_json, error, created_at, updated_at, started_at, finished_at,
                    attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
                    cancel_requested_at, last_progress_message, callback_json, callback_sent_at
             FROM jobs
             ORDER BY created_at ASC, id ASC",
            &[],
        )
    }

    fn list_jobs_for_session(&self, session_id: &str) -> Result<Vec<JobRecord>, StoreError> {
        self.query_jobs(
            "SELECT id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
                    result_json, error, created_at, updated_at, started_at, finished_at,
                    attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
                    cancel_requested_at, last_progress_message, callback_json, callback_sent_at
             FROM jobs
             WHERE session_id = $1
             ORDER BY created_at ASC, id ASC",
            &[&session_id],
        )
    }

    fn list_active_jobs_for_session(&self, session_id: &str) -> Result<Vec<JobRecord>, StoreError> {
        self.query_jobs(
            "SELECT id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
                    result_json, error, created_at, updated_at, started_at, finished_at,
                    attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
                    cancel_requested_at, last_progress_message, callback_json, callback_sent_at
             FROM jobs
             WHERE session_id = $1
               AND status IN ('queued', 'running', 'waiting_external', 'blocked')
             ORDER BY created_at ASC, id ASC",
            &[&session_id],
        )
    }

    fn list_active_job_counts(&self) -> Result<Vec<SessionActiveJobCounts>, StoreError> {
        self.query_active_job_counts(
            "SELECT session_id,
                    COUNT(*) AS active_count,
                    COALESCE(SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END), 0) AS running_count,
                    COALESCE(SUM(CASE WHEN status = 'queued' THEN 1 ELSE 0 END), 0) AS queued_count
             FROM jobs
             WHERE status IN ('queued', 'running', 'waiting_external', 'blocked')
             GROUP BY session_id
             ORDER BY session_id ASC",
            &[],
        )
    }

    fn get_active_job_counts_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionActiveJobCounts>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT session_id,
                            COUNT(*) AS active_count,
                            COALESCE(SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END), 0) AS running_count,
                            COALESCE(SUM(CASE WHEN status = 'queued' THEN 1 ELSE 0 END), 0) AS queued_count
                     FROM jobs
                     WHERE session_id = $1
                       AND status IN ('queued', 'running', 'waiting_external', 'blocked')
                     GROUP BY session_id",
                    &[&session_id],
                )
                .map(|row| row.map(|row| active_job_counts_from_row(&row)))
                .map_err(StoreError::from)
        })
    }
}

impl PersistenceStore {
    fn query_runs(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        reverse: bool,
    ) -> Result<Vec<RunRecord>, StoreError> {
        let mut runs = self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(run_record_from_row).collect::<Vec<_>>())
                .map_err(StoreError::from)
        })?;
        if reverse {
            runs.reverse();
        }
        Ok(runs)
    }

    fn query_run_rollups(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<RunSummaryRollup>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(row_to_run_summary_rollup).collect())
                .map_err(StoreError::from)
        })
    }

    fn query_jobs(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<JobRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(job_record_from_row).collect())
                .map_err(StoreError::from)
        })
    }

    fn query_active_job_counts(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<SessionActiveJobCounts>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(active_job_counts_from_row).collect())
                .map_err(StoreError::from)
        })
    }
}

const RUN_SELECT_WITH_WHERE_ID: &str = "SELECT id, session_id, mission_id, status, error, result, provider_usage_json, active_processes_json, recent_steps_json,
        evidence_refs_json, pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
 FROM runs WHERE id = $1";

const JOB_SELECT_WITH_WHERE_ID: &str =
    "SELECT id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
        result_json, error, created_at, updated_at, started_at, finished_at,
        attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
        cancel_requested_at, last_progress_message, callback_json, callback_sent_at
 FROM jobs WHERE id = $1";

fn row_to_run_summary_rollup(row: &Row) -> RunSummaryRollup {
    let provider_usage_json: String = row.get(2);
    let pending_approvals_json: String = row.get(3);
    let latest_provider_usage =
        serde_json::from_str::<Option<ProviderUsage>>(&provider_usage_json).unwrap_or(None);
    let pending_approval_count =
        serde_json::from_str::<Vec<serde_json::Value>>(&pending_approvals_json)
            .map(|entries| entries.len())
            .unwrap_or(0);

    RunSummaryRollup {
        id: row.get(0),
        session_id: row.get(1),
        latest_provider_usage,
        pending_approval_count,
        started_at: row.get(4),
        updated_at: row.get(5),
    }
}

fn run_record_from_row(row: &Row) -> RunRecord {
    RunRecord {
        id: row.get(0),
        session_id: row.get(1),
        mission_id: row.get(2),
        status: row.get(3),
        error: row.get(4),
        result: row.get(5),
        provider_usage_json: row.get(6),
        active_processes_json: row.get(7),
        recent_steps_json: row.get(8),
        evidence_refs_json: row.get(9),
        pending_approvals_json: row.get(10),
        provider_loop_json: row.get(11),
        delegate_runs_json: row.get(12),
        started_at: row.get(13),
        updated_at: row.get(14),
        finished_at: row.get(15),
    }
}

fn job_record_from_row(row: &Row) -> JobRecord {
    JobRecord {
        id: row.get(0),
        session_id: row.get(1),
        mission_id: row.get(2),
        run_id: row.get(3),
        parent_job_id: row.get(4),
        kind: row.get(5),
        status: row.get(6),
        input_json: row.get(7),
        result_json: row.get(8),
        error: row.get(9),
        created_at: row.get(10),
        updated_at: row.get(11),
        started_at: row.get(12),
        finished_at: row.get(13),
        attempt_count: row.get(14),
        max_attempts: row.get(15),
        lease_owner: row.get(16),
        lease_expires_at: row.get(17),
        heartbeat_at: row.get(18),
        cancel_requested_at: row.get(19),
        last_progress_message: row.get(20),
        callback_json: row.get(21),
        callback_sent_at: row.get(22),
    }
}

fn active_job_counts_from_row(row: &Row) -> SessionActiveJobCounts {
    SessionActiveJobCounts {
        session_id: row.get(0),
        active_count: row.get::<_, i64>(1).max(0) as usize,
        running_count: row.get::<_, i64>(2).max(0) as usize,
        queued_count: row.get::<_, i64>(3).max(0) as usize,
    }
}
