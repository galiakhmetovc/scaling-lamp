use super::*;
use rusqlite::{Row, Rows};

impl ToolCallRepository for PersistenceStore {
    fn put_tool_call(&self, record: &ToolCallRecord) -> Result<(), StoreError> {
        validate_identifier(&record.id)?;
        validate_identifier(&record.session_id)?;
        validate_identifier(&record.run_id)?;

        self.connection.execute(
            "INSERT INTO tool_calls (
                id, session_id, run_id, provider_tool_call_id, tool_name, arguments_json,
                summary, status, error, result_summary, result_preview, result_artifact_id,
                result_truncated, result_byte_len, requested_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                run_id = excluded.run_id,
                provider_tool_call_id = excluded.provider_tool_call_id,
                tool_name = excluded.tool_name,
                arguments_json = excluded.arguments_json,
                summary = excluded.summary,
                status = excluded.status,
                error = excluded.error,
                result_summary = excluded.result_summary,
                result_preview = excluded.result_preview,
                result_artifact_id = excluded.result_artifact_id,
                result_truncated = excluded.result_truncated,
                result_byte_len = excluded.result_byte_len,
                requested_at = excluded.requested_at,
                updated_at = excluded.updated_at",
            params![
                record.id,
                record.session_id,
                record.run_id,
                record.provider_tool_call_id,
                record.tool_name,
                record.arguments_json,
                record.summary,
                record.status,
                record.error,
                record.result_summary,
                record.result_preview,
                record.result_artifact_id,
                record.result_truncated,
                record.result_byte_len,
                record.requested_at,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    fn get_tool_call(&self, id: &str) -> Result<Option<ToolCallRecord>, StoreError> {
        validate_identifier(id)?;
        self.connection
            .query_row(
                "SELECT id, session_id, run_id, provider_tool_call_id, tool_name, arguments_json,
                        summary, status, error, result_summary, result_preview, result_artifact_id,
                        result_truncated, result_byte_len, requested_at, updated_at
                 FROM tool_calls
                 WHERE id = ?1",
                [id],
                tool_call_record_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_tool_calls_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<ToolCallRecord>, StoreError> {
        validate_identifier(session_id)?;
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, run_id, provider_tool_call_id, tool_name, arguments_json,
                    summary, status, error, result_summary, result_preview, result_artifact_id,
                    result_truncated, result_byte_len, requested_at, updated_at
             FROM tool_calls
             WHERE session_id = ?1
             ORDER BY requested_at ASC, id ASC",
        )?;
        let mut rows = statement.query([session_id])?;
        collect_tool_call_records(&mut rows)
    }

    fn list_tool_calls_for_run(&self, run_id: &str) -> Result<Vec<ToolCallRecord>, StoreError> {
        validate_identifier(run_id)?;
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, run_id, provider_tool_call_id, tool_name, arguments_json,
                    summary, status, error, result_summary, result_preview, result_artifact_id,
                    result_truncated, result_byte_len, requested_at, updated_at
             FROM tool_calls
             WHERE run_id = ?1
             ORDER BY requested_at ASC, id ASC",
        )?;
        let mut rows = statement.query([run_id])?;
        collect_tool_call_records(&mut rows)
    }
}

fn collect_tool_call_records(rows: &mut Rows<'_>) -> Result<Vec<ToolCallRecord>, StoreError> {
    let mut records = Vec::new();
    while let Some(row) = rows.next()? {
        records.push(tool_call_record_from_row(row)?);
    }
    Ok(records)
}

fn tool_call_record_from_row(row: &Row<'_>) -> rusqlite::Result<ToolCallRecord> {
    Ok(ToolCallRecord {
        id: row.get(0)?,
        session_id: row.get(1)?,
        run_id: row.get(2)?,
        provider_tool_call_id: row.get(3)?,
        tool_name: row.get(4)?,
        arguments_json: row.get(5)?,
        summary: row.get(6)?,
        status: row.get(7)?,
        error: row.get(8)?,
        result_summary: row.get(9)?,
        result_preview: row.get(10)?,
        result_artifact_id: row.get(11)?,
        result_truncated: row.get(12)?,
        result_byte_len: row.get(13)?,
        requested_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}
