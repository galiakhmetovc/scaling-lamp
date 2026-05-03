use super::*;
use crate::{KvEntryRecord, KvRepository};

impl KvRepository for PersistenceStore {
    fn put_kv_entry(
        &self,
        record: &KvEntryRecord,
        expected_revision: Option<i64>,
    ) -> Result<KvEntryRecord, StoreError> {
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let existing = select_kv_entry(
            &transaction,
            record.scope.as_str(),
            record.namespace_id.as_str(),
            record.key.as_str(),
        )?;
        if let Some(expected_revision) = expected_revision {
            let actual_revision = existing.as_ref().map(|entry| entry.revision);
            if actual_revision.unwrap_or(0) != expected_revision {
                return Err(StoreError::KvRevisionConflict {
                    scope: record.scope.clone(),
                    namespace_id: record.namespace_id.clone(),
                    key: record.key.clone(),
                    expected_revision,
                    actual_revision,
                });
            }
        }

        let revision = existing
            .as_ref()
            .map(|entry| entry.revision.saturating_add(1))
            .unwrap_or(1);
        let created_at = existing
            .as_ref()
            .map(|entry| entry.created_at)
            .unwrap_or(record.created_at);
        let stored = KvEntryRecord {
            revision,
            created_at,
            ..record.clone()
        };
        transaction.execute(
            "INSERT INTO kv_entries (
                scope, namespace_id, key, value_json, metadata_json, revision,
                created_at, updated_at, expires_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(scope, namespace_id, key) DO UPDATE SET
                value_json = excluded.value_json,
                metadata_json = excluded.metadata_json,
                revision = excluded.revision,
                updated_at = excluded.updated_at,
                expires_at = excluded.expires_at",
            params![
                stored.scope,
                stored.namespace_id,
                stored.key,
                stored.value_json,
                stored.metadata_json,
                stored.revision,
                stored.created_at,
                stored.updated_at,
                stored.expires_at,
            ],
        )?;
        transaction.commit()?;
        Ok(stored)
    }

    fn get_kv_entry(
        &self,
        scope: &str,
        namespace_id: &str,
        key: &str,
        now: i64,
    ) -> Result<Option<KvEntryRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT scope, namespace_id, key, value_json, metadata_json, revision,
                        created_at, updated_at, expires_at
                 FROM kv_entries
                 WHERE scope = ?1
                   AND namespace_id = ?2
                   AND key = ?3
                   AND (expires_at IS NULL OR expires_at > ?4)",
                params![scope, namespace_id, key, now],
                kv_entry_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_kv_entries(
        &self,
        scope: &str,
        namespace_id: &str,
        prefix: Option<&str>,
        limit: usize,
        offset: usize,
        now: i64,
    ) -> Result<Vec<KvEntryRecord>, StoreError> {
        let prefix = prefix.unwrap_or("");
        let mut statement = self.connection.prepare(
            "SELECT scope, namespace_id, key, value_json, metadata_json, revision,
                    created_at, updated_at, expires_at
             FROM kv_entries
             WHERE scope = ?1
               AND namespace_id = ?2
               AND key LIKE ?3 ESCAPE '\\'
               AND (expires_at IS NULL OR expires_at > ?4)
             ORDER BY key ASC
             LIMIT ?5 OFFSET ?6",
        )?;
        let pattern = format!("{}%", escape_like(prefix));
        let mut rows =
            statement.query(params![scope, namespace_id, pattern, now, limit, offset])?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next()? {
            entries.push(kv_entry_from_row(row)?);
        }
        Ok(entries)
    }

    fn delete_kv_entry(
        &self,
        scope: &str,
        namespace_id: &str,
        key: &str,
        expected_revision: Option<i64>,
    ) -> Result<bool, StoreError> {
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let existing = select_kv_entry(&transaction, scope, namespace_id, key)?;
        if let Some(expected_revision) = expected_revision {
            let actual_revision = existing.as_ref().map(|entry| entry.revision);
            if actual_revision.unwrap_or(0) != expected_revision {
                return Err(StoreError::KvRevisionConflict {
                    scope: scope.to_string(),
                    namespace_id: namespace_id.to_string(),
                    key: key.to_string(),
                    expected_revision,
                    actual_revision,
                });
            }
        }
        let changed = transaction.execute(
            "DELETE FROM kv_entries
             WHERE scope = ?1 AND namespace_id = ?2 AND key = ?3",
            params![scope, namespace_id, key],
        )?;
        transaction.commit()?;
        Ok(changed > 0)
    }
}

fn select_kv_entry(
    connection: &rusqlite::Connection,
    scope: &str,
    namespace_id: &str,
    key: &str,
) -> Result<Option<KvEntryRecord>, StoreError> {
    connection
        .query_row(
            "SELECT scope, namespace_id, key, value_json, metadata_json, revision,
                    created_at, updated_at, expires_at
             FROM kv_entries
             WHERE scope = ?1 AND namespace_id = ?2 AND key = ?3",
            params![scope, namespace_id, key],
            kv_entry_from_row,
        )
        .optional()
        .map_err(StoreError::from)
}

fn kv_entry_from_row(row: &rusqlite::Row<'_>) -> Result<KvEntryRecord, rusqlite::Error> {
    Ok(KvEntryRecord {
        scope: row.get(0)?,
        namespace_id: row.get(1)?,
        key: row.get(2)?,
        value_json: row.get(3)?,
        metadata_json: row.get(4)?,
        revision: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        expires_at: row.get(8)?,
    })
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
