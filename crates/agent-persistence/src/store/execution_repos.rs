use super::*;

impl RunRepository for PersistenceStore {
    fn put_run(&self, record: &RunRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO runs (
                id, session_id, mission_id, status, error, result, recent_steps_json, evidence_refs_json,
                pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                mission_id = excluded.mission_id,
                status = excluded.status,
                error = excluded.error,
                result = excluded.result,
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
                "SELECT id, session_id, mission_id, status, error, result, recent_steps_json,
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
                        recent_steps_json: row.get(6)?,
                        evidence_refs_json: row.get(7)?,
                        pending_approvals_json: row.get(8)?,
                        provider_loop_json: row.get(9)?,
                        delegate_runs_json: row.get(10)?,
                        started_at: row.get(11)?,
                        updated_at: row.get(12)?,
                        finished_at: row.get(13)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_runs(&self) -> Result<Vec<RunRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, mission_id, status, error, result, recent_steps_json, evidence_refs_json,
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
                recent_steps_json: row.get(6)?,
                evidence_refs_json: row.get(7)?,
                pending_approvals_json: row.get(8)?,
                provider_loop_json: row.get(9)?,
                delegate_runs_json: row.get(10)?,
                started_at: row.get(11)?,
                updated_at: row.get(12)?,
                finished_at: row.get(13)?,
            });
        }

        Ok(runs)
    }
}

impl JobRepository for PersistenceStore {
    fn put_job(&self, record: &JobRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO jobs (
                id, mission_id, run_id, parent_job_id, kind, status, input_json, result_json, error,
                created_at, updated_at, started_at, finished_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
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
                finished_at = excluded.finished_at",
            params![
                record.id,
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
                record.finished_at
            ],
        )?;
        Ok(())
    }

    fn get_job(&self, id: &str) -> Result<Option<JobRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, mission_id, run_id, parent_job_id, kind, status, input_json,
                        result_json, error, created_at, updated_at, started_at, finished_at
                 FROM jobs WHERE id = ?1",
                [id],
                |row| {
                    Ok(JobRecord {
                        id: row.get(0)?,
                        mission_id: row.get(1)?,
                        run_id: row.get(2)?,
                        parent_job_id: row.get(3)?,
                        kind: row.get(4)?,
                        status: row.get(5)?,
                        input_json: row.get(6)?,
                        result_json: row.get(7)?,
                        error: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                        started_at: row.get(11)?,
                        finished_at: row.get(12)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_jobs(&self) -> Result<Vec<JobRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, mission_id, run_id, parent_job_id, kind, status, input_json,
                    result_json, error, created_at, updated_at, started_at, finished_at
             FROM jobs
             ORDER BY created_at ASC, id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut jobs = Vec::new();

        while let Some(row) = rows.next()? {
            jobs.push(JobRecord {
                id: row.get(0)?,
                mission_id: row.get(1)?,
                run_id: row.get(2)?,
                parent_job_id: row.get(3)?,
                kind: row.get(4)?,
                status: row.get(5)?,
                input_json: row.get(6)?,
                result_json: row.get(7)?,
                error: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                started_at: row.get(11)?,
                finished_at: row.get(12)?,
            });
        }

        Ok(jobs)
    }
}
