use super::*;

impl TraceRepository for PersistenceStore {
    fn put_trace_link(&self, record: &TraceLinkRecord) -> Result<(), StoreError> {
        validate_identifier(&record.entity_kind)?;
        validate_identifier(&record.entity_id)?;
        validate_identifier(&record.trace_id)?;
        validate_identifier(&record.span_id)?;
        if let Some(parent_span_id) = record.parent_span_id.as_deref() {
            validate_identifier(parent_span_id)?;
        }

        self.with_client(|client| {
            client.execute(
                "INSERT INTO trace_links (
                    entity_kind, entity_id, trace_id, span_id, parent_span_id, surface, entrypoint,
                    attributes_json, created_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT(entity_kind, entity_id) DO UPDATE SET
                    trace_id = excluded.trace_id,
                    span_id = excluded.span_id,
                    parent_span_id = excluded.parent_span_id,
                    surface = excluded.surface,
                    entrypoint = excluded.entrypoint,
                    attributes_json = excluded.attributes_json,
                    created_at = excluded.created_at",
                &[
                    &record.entity_kind,
                    &record.entity_id,
                    &record.trace_id,
                    &record.span_id,
                    &record.parent_span_id,
                    &record.surface,
                    &record.entrypoint,
                    &record.attributes_json,
                    &record.created_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_trace_link(
        &self,
        entity_kind: &str,
        entity_id: &str,
    ) -> Result<Option<TraceLinkRecord>, StoreError> {
        validate_identifier(entity_kind)?;
        validate_identifier(entity_id)?;

        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT entity_kind, entity_id, trace_id, span_id, parent_span_id, surface,
                            entrypoint, attributes_json, created_at
                     FROM trace_links
                     WHERE entity_kind = $1 AND entity_id = $2",
                    &[&entity_kind, &entity_id],
                )
                .map(|row| row.map(|row| trace_link_record_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_trace_links_for_trace(
        &self,
        trace_id: &str,
    ) -> Result<Vec<TraceLinkRecord>, StoreError> {
        validate_identifier(trace_id)?;
        self.with_client(|client| {
            client
                .query(
                    "SELECT entity_kind, entity_id, trace_id, span_id, parent_span_id, surface,
                            entrypoint, attributes_json, created_at
                     FROM trace_links
                     WHERE trace_id = $1
                     ORDER BY created_at ASC, entity_kind ASC, entity_id ASC",
                    &[&trace_id],
                )
                .map(|rows| rows.iter().map(trace_link_record_from_row).collect())
                .map_err(StoreError::from)
        })
    }

    fn list_recent_trace_links(&self, limit: usize) -> Result<Vec<TraceLinkRecord>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = limit as i64;
        self.with_client(|client| {
            client
                .query(
                    "SELECT entity_kind, entity_id, trace_id, span_id, parent_span_id, surface,
                            entrypoint, attributes_json, created_at
                     FROM trace_links
                     ORDER BY created_at DESC, entity_kind ASC, entity_id ASC
                     LIMIT $1",
                    &[&limit],
                )
                .map(|rows| rows.iter().map(trace_link_record_from_row).collect())
                .map_err(StoreError::from)
        })
    }
}

fn trace_link_record_from_row(row: &Row) -> TraceLinkRecord {
    TraceLinkRecord {
        entity_kind: row.get(0),
        entity_id: row.get(1),
        trace_id: row.get(2),
        span_id: row.get(3),
        parent_span_id: row.get(4),
        surface: row.get(5),
        entrypoint: row.get(6),
        attributes_json: row.get(7),
        created_at: row.get(8),
    }
}
