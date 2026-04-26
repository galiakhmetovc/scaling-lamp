use super::*;
use crate::{
    KnowledgeRepository, KnowledgeSearchDocRecord, KnowledgeSourceRecord, SessionSearchDocRecord,
    SessionSearchRepository,
};

impl SessionRetentionRepository for PersistenceStore {
    fn put_session_retention(&self, record: &SessionRetentionRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO session_retention (
                session_id, tier, last_accessed_at, archived_at, archive_manifest_path,
                archive_version, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(session_id) DO UPDATE SET
                tier = excluded.tier,
                last_accessed_at = excluded.last_accessed_at,
                archived_at = excluded.archived_at,
                archive_manifest_path = excluded.archive_manifest_path,
                archive_version = excluded.archive_version,
                updated_at = excluded.updated_at",
            params![
                record.session_id,
                record.tier,
                record.last_accessed_at,
                record.archived_at,
                record.archive_manifest_path,
                record.archive_version,
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    fn get_session_retention(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionRetentionRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT session_id, tier, last_accessed_at, archived_at,
                        archive_manifest_path, archive_version, updated_at
                 FROM session_retention
                 WHERE session_id = ?1",
                [session_id],
                |row| {
                    Ok(SessionRetentionRecord {
                        session_id: row.get(0)?,
                        tier: row.get(1)?,
                        last_accessed_at: row.get(2)?,
                        archived_at: row.get(3)?,
                        archive_manifest_path: row.get(4)?,
                        archive_version: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_session_retentions(&self) -> Result<Vec<SessionRetentionRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT session_id, tier, last_accessed_at, archived_at,
                    archive_manifest_path, archive_version, updated_at
             FROM session_retention
             ORDER BY updated_at ASC, session_id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut retentions = Vec::new();

        while let Some(row) = rows.next()? {
            retentions.push(SessionRetentionRecord {
                session_id: row.get(0)?,
                tier: row.get(1)?,
                last_accessed_at: row.get(2)?,
                archived_at: row.get(3)?,
                archive_manifest_path: row.get(4)?,
                archive_version: row.get(5)?,
                updated_at: row.get(6)?,
            });
        }

        Ok(retentions)
    }
}

impl SessionSearchRepository for PersistenceStore {
    fn replace_session_search_docs(
        &self,
        session_id: &str,
        docs: &[SessionSearchDocRecord],
    ) -> Result<(), StoreError> {
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        transaction.execute(
            "DELETE FROM session_search_fts
             WHERE doc_id IN (
                 SELECT doc_id FROM session_search_docs WHERE session_id = ?1
             )",
            [session_id],
        )?;
        transaction.execute(
            "DELETE FROM session_search_docs WHERE session_id = ?1",
            [session_id],
        )?;

        for doc in docs {
            transaction.execute(
                "INSERT INTO session_search_docs (
                    doc_id, session_id, source_kind, source_ref, body, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    doc.doc_id,
                    doc.session_id,
                    doc.source_kind,
                    doc.source_ref,
                    doc.body,
                    doc.updated_at,
                ],
            )?;
            transaction.execute(
                "INSERT INTO session_search_fts (doc_id, session_id, source_kind, source_ref, body)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    doc.doc_id,
                    doc.session_id,
                    doc.source_kind,
                    doc.source_ref,
                    doc.body
                ],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    fn list_session_search_docs(&self) -> Result<Vec<SessionSearchDocRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT doc_id, session_id, source_kind, source_ref, body, updated_at
             FROM session_search_docs
             ORDER BY updated_at DESC, doc_id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut docs = Vec::new();

        while let Some(row) = rows.next()? {
            docs.push(SessionSearchDocRecord {
                doc_id: row.get(0)?,
                session_id: row.get(1)?,
                source_kind: row.get(2)?,
                source_ref: row.get(3)?,
                body: row.get(4)?,
                updated_at: row.get(5)?,
            });
        }

        Ok(docs)
    }
}

impl KnowledgeRepository for PersistenceStore {
    fn put_knowledge_source(&self, record: &KnowledgeSourceRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO knowledge_sources (
                source_id, path, kind, sha256, byte_len, mtime, indexed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(source_id) DO UPDATE SET
                path = excluded.path,
                kind = excluded.kind,
                sha256 = excluded.sha256,
                byte_len = excluded.byte_len,
                mtime = excluded.mtime,
                indexed_at = excluded.indexed_at",
            params![
                record.source_id,
                record.path,
                record.kind,
                record.sha256,
                record.byte_len,
                record.mtime,
                record.indexed_at,
            ],
        )?;
        Ok(())
    }

    fn get_knowledge_source_by_path(
        &self,
        path: &str,
    ) -> Result<Option<KnowledgeSourceRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT source_id, path, kind, sha256, byte_len, mtime, indexed_at
                 FROM knowledge_sources
                 WHERE path = ?1",
                [path],
                |row| {
                    Ok(KnowledgeSourceRecord {
                        source_id: row.get(0)?,
                        path: row.get(1)?,
                        kind: row.get(2)?,
                        sha256: row.get(3)?,
                        byte_len: row.get(4)?,
                        mtime: row.get(5)?,
                        indexed_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_knowledge_sources(&self) -> Result<Vec<KnowledgeSourceRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT source_id, path, kind, sha256, byte_len, mtime, indexed_at
             FROM knowledge_sources
             ORDER BY path ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut sources = Vec::new();

        while let Some(row) = rows.next()? {
            sources.push(KnowledgeSourceRecord {
                source_id: row.get(0)?,
                path: row.get(1)?,
                kind: row.get(2)?,
                sha256: row.get(3)?,
                byte_len: row.get(4)?,
                mtime: row.get(5)?,
                indexed_at: row.get(6)?,
            });
        }

        Ok(sources)
    }

    fn delete_knowledge_source(&self, source_id: &str) -> Result<bool, StoreError> {
        self.connection.execute(
            "DELETE FROM knowledge_search_fts
             WHERE doc_id IN (
                 SELECT doc_id FROM knowledge_search_docs WHERE source_id = ?1
             )",
            [source_id],
        )?;
        self.connection.execute(
            "DELETE FROM knowledge_search_docs WHERE source_id = ?1",
            [source_id],
        )?;
        let deleted = self.connection.execute(
            "DELETE FROM knowledge_sources WHERE source_id = ?1",
            [source_id],
        )?;
        Ok(deleted > 0)
    }

    fn replace_knowledge_search_docs(
        &self,
        source_id: &str,
        docs: &[KnowledgeSearchDocRecord],
    ) -> Result<(), StoreError> {
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        transaction.execute(
            "DELETE FROM knowledge_search_fts
             WHERE doc_id IN (
                 SELECT doc_id FROM knowledge_search_docs WHERE source_id = ?1
             )",
            [source_id],
        )?;
        transaction.execute(
            "DELETE FROM knowledge_search_docs WHERE source_id = ?1",
            [source_id],
        )?;

        for doc in docs {
            transaction.execute(
                "INSERT INTO knowledge_search_docs (
                    doc_id, source_id, path, kind, body, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    doc.doc_id,
                    doc.source_id,
                    doc.path,
                    doc.kind,
                    doc.body,
                    doc.updated_at,
                ],
            )?;
            transaction.execute(
                "INSERT INTO knowledge_search_fts (doc_id, path, kind, body)
                 VALUES (?1, ?2, ?3, ?4)",
                params![doc.doc_id, doc.path, doc.kind, doc.body],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    fn list_knowledge_search_docs(&self) -> Result<Vec<KnowledgeSearchDocRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT doc_id, source_id, path, kind, body, updated_at
             FROM knowledge_search_docs
             ORDER BY path ASC, doc_id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut docs = Vec::new();

        while let Some(row) = rows.next()? {
            docs.push(KnowledgeSearchDocRecord {
                doc_id: row.get(0)?,
                source_id: row.get(1)?,
                path: row.get(2)?,
                kind: row.get(3)?,
                body: row.get(4)?,
                updated_at: row.get(5)?,
            });
        }

        Ok(docs)
    }
}
