use super::*;

impl TaskRegistryRepository for PersistenceStore {
    fn put_task_registry(&self, record: &TaskRegistryRecord) -> Result<(), StoreError> {
        validate_identifier(&record.task_id)?;
        validate_non_empty("task kind", &record.kind)?;
        validate_non_empty("task status", &record.status)?;
        validate_non_empty("task dependency_json", &record.dependency_json)?;
        validate_non_empty("task context_ref_json", &record.context_ref_json)?;
        validate_non_empty("task retry_policy_json", &record.retry_policy_json)?;
        self.with_client(|client| {
            client.execute(
                "INSERT INTO task_registry (
                    task_id, kind, source_session_id, owner_agent_id, executor_agent_id,
                    parent_task_id, status, dependency_json, context_ref_json, result_ref_json,
                    retry_policy_json, attempt_count, max_attempts, timeout_at, chain_id,
                    hop_count, max_hops, trace_id, created_at, updated_at, started_at,
                    finished_at, error
                 ) VALUES (
                    $1, $2, $3, $4, $5,
                    $6, $7, $8, $9, $10,
                    $11, $12, $13, $14, $15,
                    $16, $17, $18, $19, $20, $21,
                    $22, $23
                 )
                 ON CONFLICT(task_id) DO UPDATE SET
                    kind = excluded.kind,
                    source_session_id = excluded.source_session_id,
                    owner_agent_id = excluded.owner_agent_id,
                    executor_agent_id = excluded.executor_agent_id,
                    parent_task_id = excluded.parent_task_id,
                    status = excluded.status,
                    dependency_json = excluded.dependency_json,
                    context_ref_json = excluded.context_ref_json,
                    result_ref_json = excluded.result_ref_json,
                    retry_policy_json = excluded.retry_policy_json,
                    attempt_count = excluded.attempt_count,
                    max_attempts = excluded.max_attempts,
                    timeout_at = excluded.timeout_at,
                    chain_id = excluded.chain_id,
                    hop_count = excluded.hop_count,
                    max_hops = excluded.max_hops,
                    trace_id = excluded.trace_id,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    started_at = excluded.started_at,
                    finished_at = excluded.finished_at,
                    error = excluded.error",
                &[
                    &record.task_id,
                    &record.kind,
                    &record.source_session_id,
                    &record.owner_agent_id,
                    &record.executor_agent_id,
                    &record.parent_task_id,
                    &record.status,
                    &record.dependency_json,
                    &record.context_ref_json,
                    &record.result_ref_json,
                    &record.retry_policy_json,
                    &record.attempt_count,
                    &record.max_attempts,
                    &record.timeout_at,
                    &record.chain_id,
                    &record.hop_count,
                    &record.max_hops,
                    &record.trace_id,
                    &record.created_at,
                    &record.updated_at,
                    &record.started_at,
                    &record.finished_at,
                    &record.error,
                ],
            )?;
            Ok(())
        })
    }

    fn get_task_registry(&self, task_id: &str) -> Result<Option<TaskRegistryRecord>, StoreError> {
        validate_identifier(task_id)?;
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT task_id, kind, source_session_id, owner_agent_id, executor_agent_id,
                            parent_task_id, status, dependency_json, context_ref_json, result_ref_json,
                            retry_policy_json, attempt_count, max_attempts, timeout_at, chain_id,
                            hop_count, max_hops, trace_id, created_at, updated_at, started_at,
                            finished_at, error
                     FROM task_registry
                     WHERE task_id = $1",
                    &[&task_id],
                )
                .map(|row| row.map(|row| task_registry_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_task_registry_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<TaskRegistryRecord>, StoreError> {
        validate_identifier(session_id)?;
        self.with_client(|client| {
            client
                .query(
                    "SELECT task_id, kind, source_session_id, owner_agent_id, executor_agent_id,
                            parent_task_id, status, dependency_json, context_ref_json, result_ref_json,
                            retry_policy_json, attempt_count, max_attempts, timeout_at, chain_id,
                            hop_count, max_hops, trace_id, created_at, updated_at, started_at,
                            finished_at, error
                     FROM task_registry
                     WHERE source_session_id = $1
                     ORDER BY updated_at DESC, task_id ASC",
                    &[&session_id],
                )
                .map(|rows| {
                    rows.into_iter()
                        .map(|row| task_registry_from_row(&row))
                        .collect()
                })
                .map_err(StoreError::from)
        })
    }
}

fn task_registry_from_row(row: &Row) -> TaskRegistryRecord {
    TaskRegistryRecord {
        task_id: row.get(0),
        kind: row.get(1),
        source_session_id: row.get(2),
        owner_agent_id: row.get(3),
        executor_agent_id: row.get(4),
        parent_task_id: row.get(5),
        status: row.get(6),
        dependency_json: row.get(7),
        context_ref_json: row.get(8),
        result_ref_json: row.get(9),
        retry_policy_json: row.get(10),
        attempt_count: row.get(11),
        max_attempts: row.get(12),
        timeout_at: row.get(13),
        chain_id: row.get(14),
        hop_count: row.get(15),
        max_hops: row.get(16),
        trace_id: row.get(17),
        created_at: row.get(18),
        updated_at: row.get(19),
        started_at: row.get(20),
        finished_at: row.get(21),
        error: row.get(22),
    }
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), StoreError> {
    if value.trim().is_empty() {
        return Err(StoreError::InvalidIdentifier {
            id: value.to_string(),
            reason: field,
        });
    }
    Ok(())
}
