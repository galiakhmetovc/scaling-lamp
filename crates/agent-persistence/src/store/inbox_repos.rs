use super::*;

impl SessionInboxRepository for PersistenceStore {
    fn put_session_inbox_event(&self, record: &SessionInboxEventRecord) -> Result<(), StoreError> {
        self.with_client(|client| {
            client.execute(
                "INSERT INTO session_inbox_events (
                    id, session_id, job_id, kind, payload_json, status, created_at, available_at,
                    claimed_at, processed_at, error
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                 ON CONFLICT(id) DO UPDATE SET
                    session_id = excluded.session_id,
                    job_id = excluded.job_id,
                    kind = excluded.kind,
                    payload_json = excluded.payload_json,
                    status = excluded.status,
                    created_at = excluded.created_at,
                    available_at = excluded.available_at,
                    claimed_at = excluded.claimed_at,
                    processed_at = excluded.processed_at,
                    error = excluded.error",
                &[
                    &record.id,
                    &record.session_id,
                    &record.job_id,
                    &record.kind,
                    &record.payload_json,
                    &record.status,
                    &record.created_at,
                    &record.available_at,
                    &record.claimed_at,
                    &record.processed_at,
                    &record.error,
                ],
            )?;
            Ok(())
        })
    }

    fn get_session_inbox_event(
        &self,
        id: &str,
    ) -> Result<Option<SessionInboxEventRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT id, session_id, job_id, kind, payload_json, status, created_at,
                            available_at, claimed_at, processed_at, error
                     FROM session_inbox_events
                     WHERE id = $1",
                    &[&id],
                )
                .map(|row| row.map(|row| inbox_event_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_session_inbox_events_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionInboxEventRecord>, StoreError> {
        self.query_inbox_events(
            "SELECT id, session_id, job_id, kind, payload_json, status, created_at,
                    available_at, claimed_at, processed_at, error
             FROM session_inbox_events
             WHERE session_id = $1
             ORDER BY created_at ASC, id ASC",
            &[&session_id],
        )
    }

    fn list_queued_session_inbox_events_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionInboxEventRecord>, StoreError> {
        self.query_inbox_events(
            "SELECT id, session_id, job_id, kind, payload_json, status, created_at,
                    available_at, claimed_at, processed_at, error
             FROM session_inbox_events
             WHERE session_id = $1
               AND status = 'queued'
             ORDER BY available_at ASC, created_at ASC, id ASC",
            &[&session_id],
        )
    }

    fn list_queued_session_inbox_events(&self) -> Result<Vec<SessionInboxEventRecord>, StoreError> {
        self.query_inbox_events(
            "SELECT id, session_id, job_id, kind, payload_json, status, created_at,
                    available_at, claimed_at, processed_at, error
             FROM session_inbox_events
             WHERE status = 'queued'
             ORDER BY available_at ASC, created_at ASC, id ASC",
            &[],
        )
    }
}

impl PersistenceStore {
    fn query_inbox_events(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<SessionInboxEventRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(inbox_event_from_row).collect())
                .map_err(StoreError::from)
        })
    }
}

fn inbox_event_from_row(row: &Row) -> SessionInboxEventRecord {
    SessionInboxEventRecord {
        id: row.get(0),
        session_id: row.get(1),
        job_id: row.get(2),
        kind: row.get(3),
        payload_json: row.get(4),
        status: row.get(5),
        created_at: row.get(6),
        available_at: row.get(7),
        claimed_at: row.get(8),
        processed_at: row.get(9),
        error: row.get(10),
    }
}
