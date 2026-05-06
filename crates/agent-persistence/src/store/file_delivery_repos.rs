use super::*;

impl FileDeliveryRepository for PersistenceStore {
    fn put_file_delivery_request(
        &self,
        record: &FileDeliveryRequestRecord,
    ) -> Result<(), StoreError> {
        validate_identifier(&record.id)?;
        validate_identifier(&record.artifact_id)?;
        self.with_client(|client| {
            client.execute(
                "INSERT INTO file_delivery_requests (
                    id, session_id, run_id, artifact_id, target, file_name, caption,
                    status, created_at, updated_at, delivered_at, error
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT(id) DO UPDATE SET
                    session_id = excluded.session_id,
                    run_id = excluded.run_id,
                    artifact_id = excluded.artifact_id,
                    target = excluded.target,
                    file_name = excluded.file_name,
                    caption = excluded.caption,
                    status = excluded.status,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    delivered_at = excluded.delivered_at,
                    error = excluded.error",
                &[
                    &record.id,
                    &record.session_id,
                    &record.run_id,
                    &record.artifact_id,
                    &record.target,
                    &record.file_name,
                    &record.caption,
                    &record.status,
                    &record.created_at,
                    &record.updated_at,
                    &record.delivered_at,
                    &record.error,
                ],
            )?;
            Ok(())
        })
    }

    fn get_file_delivery_request(
        &self,
        id: &str,
    ) -> Result<Option<FileDeliveryRequestRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT id, session_id, run_id, artifact_id, target, file_name, caption,
                            status, created_at, updated_at, delivered_at, error
                     FROM file_delivery_requests
                     WHERE id = $1",
                    &[&id],
                )
                .map(|row| row.map(|row| file_delivery_request_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_queued_file_delivery_requests_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<FileDeliveryRequestRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(
                    "SELECT id, session_id, run_id, artifact_id, target, file_name, caption,
                            status, created_at, updated_at, delivered_at, error
                     FROM file_delivery_requests
                     WHERE session_id = $1 AND status = 'queued'
                     ORDER BY created_at ASC, id ASC",
                    &[&session_id],
                )
                .map(|rows| rows.iter().map(file_delivery_request_from_row).collect())
                .map_err(StoreError::from)
        })
    }
}

fn file_delivery_request_from_row(row: &Row) -> FileDeliveryRequestRecord {
    FileDeliveryRequestRecord {
        id: row.get(0),
        session_id: row.get(1),
        run_id: row.get(2),
        artifact_id: row.get(3),
        target: row.get(4),
        file_name: row.get(5),
        caption: row.get(6),
        status: row.get(7),
        created_at: row.get(8),
        updated_at: row.get(9),
        delivered_at: row.get(10),
        error: row.get(11),
    }
}
