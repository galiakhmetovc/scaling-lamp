use super::*;
use crate::{
    KnowledgeRepository, KnowledgeSearchDocRecord, KnowledgeSourceRecord, SessionSearchDocRecord,
    SessionSearchRepository,
};

impl SessionRetentionRepository for PersistenceStore {
    fn put_session_retention(&self, record: &SessionRetentionRecord) -> Result<(), StoreError> {
        self.with_client(|client| {
            client.execute(
                "INSERT INTO session_retention (
                    session_id, tier, last_accessed_at, archived_at, archive_manifest_path,
                    archive_version, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT(session_id) DO UPDATE SET
                    tier = excluded.tier,
                    last_accessed_at = excluded.last_accessed_at,
                    archived_at = excluded.archived_at,
                    archive_manifest_path = excluded.archive_manifest_path,
                    archive_version = excluded.archive_version,
                    updated_at = excluded.updated_at",
                &[
                    &record.session_id,
                    &record.tier,
                    &record.last_accessed_at,
                    &record.archived_at,
                    &record.archive_manifest_path,
                    &record.archive_version,
                    &record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_session_retention(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionRetentionRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT session_id, tier, last_accessed_at, archived_at,
                            archive_manifest_path, archive_version, updated_at
                     FROM session_retention
                     WHERE session_id = $1",
                    &[&session_id],
                )
                .map(|row| row.map(|row| session_retention_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_session_retentions(&self) -> Result<Vec<SessionRetentionRecord>, StoreError> {
        self.query_session_retentions(
            "SELECT session_id, tier, last_accessed_at, archived_at,
                    archive_manifest_path, archive_version, updated_at
             FROM session_retention
             ORDER BY updated_at ASC, session_id ASC",
            &[],
        )
    }
}

impl SessionSearchRepository for PersistenceStore {
    fn replace_session_search_docs(
        &self,
        session_id: &str,
        docs: &[SessionSearchDocRecord],
    ) -> Result<(), StoreError> {
        let mut client = self.client()?;
        let mut transaction = client.transaction()?;
        transaction.execute(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&session_id],
        )?;
        transaction.execute(
            "DELETE FROM session_search_docs WHERE session_id = $1",
            &[&session_id],
        )?;

        for doc in docs {
            transaction.execute(
                "INSERT INTO session_search_docs (
                    doc_id, session_id, source_kind, source_ref, body, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT(doc_id) DO UPDATE SET
                    session_id = excluded.session_id,
                    source_kind = excluded.source_kind,
                    source_ref = excluded.source_ref,
                    body = excluded.body,
                    updated_at = excluded.updated_at",
                &[
                    &doc.doc_id,
                    &doc.session_id,
                    &doc.source_kind,
                    &doc.source_ref,
                    &doc.body,
                    &doc.updated_at,
                ],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    fn list_session_search_docs(&self) -> Result<Vec<SessionSearchDocRecord>, StoreError> {
        self.query_session_search_docs(
            "SELECT doc_id, session_id, source_kind, source_ref, body, updated_at
             FROM session_search_docs
             ORDER BY updated_at DESC, doc_id ASC",
            &[],
        )
    }
}

impl KnowledgeRepository for PersistenceStore {
    fn put_knowledge_source(&self, record: &KnowledgeSourceRecord) -> Result<(), StoreError> {
        self.with_client(|client| {
            client.execute(
                "INSERT INTO knowledge_sources (
                    source_id, path, kind, sha256, byte_len, mtime, indexed_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT(source_id) DO UPDATE SET
                    path = excluded.path,
                    kind = excluded.kind,
                    sha256 = excluded.sha256,
                    byte_len = excluded.byte_len,
                    mtime = excluded.mtime,
                    indexed_at = excluded.indexed_at",
                &[
                    &record.source_id,
                    &record.path,
                    &record.kind,
                    &record.sha256,
                    &record.byte_len,
                    &record.mtime,
                    &record.indexed_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_knowledge_source_by_path(
        &self,
        path: &str,
    ) -> Result<Option<KnowledgeSourceRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT source_id, path, kind, sha256, byte_len, mtime, indexed_at
                     FROM knowledge_sources
                     WHERE path = $1",
                    &[&path],
                )
                .map(|row| row.map(|row| knowledge_source_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_knowledge_sources(&self) -> Result<Vec<KnowledgeSourceRecord>, StoreError> {
        self.query_knowledge_sources(
            "SELECT source_id, path, kind, sha256, byte_len, mtime, indexed_at
             FROM knowledge_sources
             ORDER BY path ASC",
            &[],
        )
    }

    fn delete_knowledge_source(&self, source_id: &str) -> Result<bool, StoreError> {
        self.with_client(|client| {
            client
                .execute(
                    "DELETE FROM knowledge_sources WHERE source_id = $1",
                    &[&source_id],
                )
                .map(|affected| affected > 0)
                .map_err(StoreError::from)
        })
    }

    fn replace_knowledge_search_docs(
        &self,
        source_id: &str,
        docs: &[KnowledgeSearchDocRecord],
    ) -> Result<(), StoreError> {
        let mut client = self.client()?;
        let mut transaction = client.transaction()?;
        transaction.execute(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&source_id],
        )?;
        transaction.execute(
            "DELETE FROM knowledge_search_docs WHERE source_id = $1",
            &[&source_id],
        )?;

        for doc in docs {
            if doc.body.len() > MAX_KNOWLEDGE_SEARCH_DOC_BODY_BYTES {
                continue;
            }

            transaction.execute(
                "INSERT INTO knowledge_search_docs (
                    doc_id, source_id, path, kind, body, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT(doc_id) DO UPDATE SET
                    source_id = excluded.source_id,
                    path = excluded.path,
                    kind = excluded.kind,
                    body = excluded.body,
                    updated_at = excluded.updated_at",
                &[
                    &doc.doc_id,
                    &doc.source_id,
                    &doc.path,
                    &doc.kind,
                    &doc.body,
                    &doc.updated_at,
                ],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    fn list_knowledge_search_docs(&self) -> Result<Vec<KnowledgeSearchDocRecord>, StoreError> {
        self.query_knowledge_search_docs(
            "SELECT doc_id, source_id, path, kind, body, updated_at
             FROM knowledge_search_docs
             ORDER BY path ASC, doc_id ASC",
            &[],
        )
    }

    fn search_knowledge_search_docs(
        &self,
        fts_query: &str,
    ) -> Result<Vec<KnowledgeSearchDocRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(
                    "SELECT doc_id, source_id, path, kind, body, updated_at
                     FROM knowledge_search_docs
                     WHERE to_tsvector('simple', body) @@ plainto_tsquery('simple', $1)
                     ORDER BY ts_rank_cd(to_tsvector('simple', body), plainto_tsquery('simple', $1)) DESC,
                              path ASC,
                              doc_id ASC",
                    &[&fts_query],
                )
                .map(|rows| rows.iter().map(knowledge_search_doc_from_row).collect())
                .map_err(StoreError::from)
        })
    }
}

const MAX_KNOWLEDGE_SEARCH_DOC_BODY_BYTES: usize = 1_000_000;

impl PersistenceStore {
    fn query_session_retentions(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<SessionRetentionRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(session_retention_from_row).collect())
                .map_err(StoreError::from)
        })
    }

    fn query_session_search_docs(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<SessionSearchDocRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(session_search_doc_from_row).collect())
                .map_err(StoreError::from)
        })
    }

    fn query_knowledge_sources(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<KnowledgeSourceRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(knowledge_source_from_row).collect())
                .map_err(StoreError::from)
        })
    }

    fn query_knowledge_search_docs(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<KnowledgeSearchDocRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(knowledge_search_doc_from_row).collect())
                .map_err(StoreError::from)
        })
    }
}

fn session_retention_from_row(row: &Row) -> SessionRetentionRecord {
    SessionRetentionRecord {
        session_id: row.get(0),
        tier: row.get(1),
        last_accessed_at: row.get(2),
        archived_at: row.get(3),
        archive_manifest_path: row.get(4),
        archive_version: row.get(5),
        updated_at: row.get(6),
    }
}

fn session_search_doc_from_row(row: &Row) -> SessionSearchDocRecord {
    SessionSearchDocRecord {
        doc_id: row.get(0),
        session_id: row.get(1),
        source_kind: row.get(2),
        source_ref: row.get(3),
        body: row.get(4),
        updated_at: row.get(5),
    }
}

fn knowledge_source_from_row(row: &Row) -> KnowledgeSourceRecord {
    KnowledgeSourceRecord {
        source_id: row.get(0),
        path: row.get(1),
        kind: row.get(2),
        sha256: row.get(3),
        byte_len: row.get(4),
        mtime: row.get(5),
        indexed_at: row.get(6),
    }
}

fn knowledge_search_doc_from_row(row: &Row) -> KnowledgeSearchDocRecord {
    KnowledgeSearchDocRecord {
        doc_id: row.get(0),
        source_id: row.get(1),
        path: row.get(2),
        kind: row.get(3),
        body: row.get(4),
        updated_at: row.get(5),
    }
}
