use super::*;
use rusqlite::{Row, Rows};

impl TraceRepository for PersistenceStore {
    fn put_trace_link(&self, record: &TraceLinkRecord) -> Result<(), StoreError> {
        validate_identifier(&record.entity_kind)?;
        validate_identifier(&record.entity_id)?;
        validate_identifier(&record.trace_id)?;
        validate_identifier(&record.span_id)?;
        if let Some(parent_span_id) = record.parent_span_id.as_deref() {
            validate_identifier(parent_span_id)?;
        }

        self.connection.execute(
            "INSERT INTO trace_links (
                entity_kind, entity_id, trace_id, span_id, parent_span_id, surface, entrypoint,
                attributes_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(entity_kind, entity_id) DO UPDATE SET
                trace_id = excluded.trace_id,
                span_id = excluded.span_id,
                parent_span_id = excluded.parent_span_id,
                surface = excluded.surface,
                entrypoint = excluded.entrypoint,
                attributes_json = excluded.attributes_json,
                created_at = excluded.created_at",
            params![
                record.entity_kind,
                record.entity_id,
                record.trace_id,
                record.span_id,
                record.parent_span_id,
                record.surface,
                record.entrypoint,
                record.attributes_json,
                record.created_at,
            ],
        )?;
        Ok(())
    }

    fn get_trace_link(
        &self,
        entity_kind: &str,
        entity_id: &str,
    ) -> Result<Option<TraceLinkRecord>, StoreError> {
        validate_identifier(entity_kind)?;
        validate_identifier(entity_id)?;

        self.connection
            .query_row(
                "SELECT entity_kind, entity_id, trace_id, span_id, parent_span_id, surface,
                        entrypoint, attributes_json, created_at
                 FROM trace_links
                 WHERE entity_kind = ?1 AND entity_id = ?2",
                params![entity_kind, entity_id],
                trace_link_record_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_trace_links_for_trace(
        &self,
        trace_id: &str,
    ) -> Result<Vec<TraceLinkRecord>, StoreError> {
        validate_identifier(trace_id)?;
        let mut statement = self.connection.prepare(
            "SELECT entity_kind, entity_id, trace_id, span_id, parent_span_id, surface,
                    entrypoint, attributes_json, created_at
             FROM trace_links
             WHERE trace_id = ?1
             ORDER BY created_at ASC, entity_kind ASC, entity_id ASC",
        )?;
        let mut rows = statement.query([trace_id])?;
        collect_trace_links(&mut rows)
    }

    fn list_recent_trace_links(&self, limit: usize) -> Result<Vec<TraceLinkRecord>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut statement = self.connection.prepare(
            "SELECT entity_kind, entity_id, trace_id, span_id, parent_span_id, surface,
                    entrypoint, attributes_json, created_at
             FROM trace_links
             ORDER BY created_at DESC, entity_kind ASC, entity_id ASC
             LIMIT ?1",
        )?;
        let mut rows = statement.query([limit as i64])?;
        collect_trace_links(&mut rows)
    }
}

fn collect_trace_links(rows: &mut Rows<'_>) -> Result<Vec<TraceLinkRecord>, StoreError> {
    let mut records = Vec::new();
    while let Some(row) = rows.next()? {
        records.push(trace_link_record_from_row(row)?);
    }
    Ok(records)
}

fn trace_link_record_from_row(row: &Row<'_>) -> rusqlite::Result<TraceLinkRecord> {
    Ok(TraceLinkRecord {
        entity_kind: row.get(0)?,
        entity_id: row.get(1)?,
        trace_id: row.get(2)?,
        span_id: row.get(3)?,
        parent_span_id: row.get(4)?,
        surface: row.get(5)?,
        entrypoint: row.get(6)?,
        attributes_json: row.get(7)?,
        created_at: row.get(8)?,
    })
}
