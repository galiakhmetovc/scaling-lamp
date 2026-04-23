use super::*;

impl ContextSummaryRepository for PersistenceStore {
    fn put_context_summary(&self, record: &ContextSummaryRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO context_summaries (
                session_id, summary_text, covered_message_count, summary_token_estimate, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(session_id) DO UPDATE SET
                summary_text = excluded.summary_text,
                covered_message_count = excluded.covered_message_count,
                summary_token_estimate = excluded.summary_token_estimate,
                updated_at = excluded.updated_at",
            params![
                record.session_id,
                record.summary_text,
                record.covered_message_count,
                record.summary_token_estimate,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    fn list_context_summaries(&self) -> Result<Vec<ContextSummaryRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT session_id, summary_text, covered_message_count, summary_token_estimate, updated_at
             FROM context_summaries
             ORDER BY updated_at ASC, session_id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut summaries = Vec::new();

        while let Some(row) = rows.next()? {
            summaries.push(ContextSummaryRecord {
                session_id: row.get(0)?,
                summary_text: row.get(1)?,
                covered_message_count: row.get(2)?,
                summary_token_estimate: row.get(3)?,
                updated_at: row.get(4)?,
            });
        }

        Ok(summaries)
    }

    fn get_context_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<ContextSummaryRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT session_id, summary_text, covered_message_count, summary_token_estimate, updated_at
                 FROM context_summaries WHERE session_id = ?1",
                [session_id],
                |row| {
                    Ok(ContextSummaryRecord {
                        session_id: row.get(0)?,
                        summary_text: row.get(1)?,
                        covered_message_count: row.get(2)?,
                        summary_token_estimate: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }
}

impl ContextOffloadRepository for PersistenceStore {
    fn put_context_offload(
        &self,
        record: &ContextOffloadRecord,
        payloads: &[ContextOffloadPayload],
    ) -> Result<(), StoreError> {
        let snapshot = ContextOffloadSnapshot::try_from(record.clone()).map_err(|source| {
            StoreError::InvalidContextOffload {
                session_id: record.session_id.clone(),
                reason: source.to_string(),
            }
        })?;
        let referenced_artifact_ids = snapshot
            .refs
            .iter()
            .map(|reference| reference.artifact_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let payload_artifact_ids = payloads
            .iter()
            .map(|payload| payload.artifact_id.clone())
            .collect::<std::collections::BTreeSet<_>>();

        if referenced_artifact_ids != payload_artifact_ids {
            return Err(StoreError::InvalidContextOffload {
                session_id: record.session_id.clone(),
                reason: "payload artifact ids must exactly match snapshot refs".to_string(),
            });
        }

        let obsolete_artifact_ids = self
            .get_context_offload(&record.session_id)?
            .map(ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(|source| StoreError::InvalidContextOffload {
                session_id: record.session_id.clone(),
                reason: source.to_string(),
            })?
            .map(|existing| {
                existing
                    .refs
                    .into_iter()
                    .filter(|reference| !referenced_artifact_ids.contains(&reference.artifact_id))
                    .map(|reference| reference.artifact_id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for payload in payloads {
            let reference = snapshot
                .refs
                .iter()
                .find(|reference| reference.artifact_id == payload.artifact_id)
                .ok_or_else(|| StoreError::InvalidContextOffload {
                    session_id: record.session_id.clone(),
                    reason: format!(
                        "missing ref metadata for payload artifact {}",
                        payload.artifact_id
                    ),
                })?;
            self.put_artifact(&ArtifactRecord {
                id: payload.artifact_id.clone(),
                session_id: record.session_id.clone(),
                kind: "context_offload".to_string(),
                metadata_json: serde_json::json!({
                    "offload_ref_id": reference.id,
                    "label": reference.label,
                    "summary": reference.summary,
                    "token_estimate": reference.token_estimate,
                    "message_count": reference.message_count,
                    "created_at": reference.created_at,
                })
                .to_string(),
                path: self.artifact_relative_path(&payload.artifact_id)?,
                bytes: payload.bytes.clone(),
                created_at: reference.created_at,
            })?;
        }

        self.connection.execute(
            "INSERT INTO context_offloads (session_id, refs_json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET
                refs_json = excluded.refs_json,
                updated_at = excluded.updated_at",
            params![record.session_id, record.refs_json, record.updated_at],
        )?;

        for artifact_id in obsolete_artifact_ids {
            self.delete_artifact_by_id(&artifact_id)?;
        }

        Ok(())
    }

    fn get_context_offload(
        &self,
        session_id: &str,
    ) -> Result<Option<ContextOffloadRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT session_id, refs_json, updated_at
                 FROM context_offloads WHERE session_id = ?1",
                [session_id],
                |row| {
                    Ok(ContextOffloadRecord {
                        session_id: row.get(0)?,
                        refs_json: row.get(1)?,
                        updated_at: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn get_context_offload_payload(
        &self,
        artifact_id: &str,
    ) -> Result<Option<ContextOffloadPayload>, StoreError> {
        match self.get_artifact(artifact_id)? {
            Some(record) if record.kind == "context_offload" => Ok(Some(ContextOffloadPayload {
                artifact_id: record.id,
                bytes: record.bytes,
            })),
            Some(_) => Ok(None),
            None => Ok(None),
        }
    }
}

impl PlanRepository for PersistenceStore {
    fn put_plan(&self, record: &PlanRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO plans (session_id, items_json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET
                items_json = excluded.items_json,
                updated_at = excluded.updated_at",
            params![record.session_id, record.items_json, record.updated_at],
        )?;
        Ok(())
    }

    fn get_plan(&self, session_id: &str) -> Result<Option<PlanRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT session_id, items_json, updated_at FROM plans WHERE session_id = ?1",
                [session_id],
                |row| {
                    Ok(PlanRecord {
                        session_id: row.get(0)?,
                        items_json: row.get(1)?,
                        updated_at: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }
}

impl ArtifactRepository for PersistenceStore {
    fn put_artifact(&self, record: &ArtifactRecord) -> Result<(), StoreError> {
        let path = self.artifact_path(&record.id)?;
        let relative_path = self.artifact_relative_path(&record.id)?;

        if record.path != relative_path {
            return Err(StoreError::InvalidIdentifier {
                id: record.id.clone(),
                reason: "artifact path must match the canonical storage path",
            });
        }
        let sha256 = sha256_hex(&record.bytes);

        persist_payload_with_commit(&path, &record.bytes, || {
            self.connection
                .execute(
                    "INSERT INTO artifacts (
                        id, session_id, kind, path, metadata_json, byte_len, sha256, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(id) DO UPDATE SET
                        session_id = excluded.session_id,
                        kind = excluded.kind,
                        path = excluded.path,
                        metadata_json = excluded.metadata_json,
                        byte_len = excluded.byte_len,
                        sha256 = excluded.sha256,
                        created_at = excluded.created_at",
                    params![
                        record.id,
                        &record.session_id,
                        record.kind,
                        record.path.to_string_lossy().to_string(),
                        &record.metadata_json,
                        record.bytes.len() as i64,
                        sha256,
                        record.created_at
                    ],
                )
                .map(|_| ())
                .map_err(StoreError::from)
        })
    }

    fn get_artifact(&self, id: &str) -> Result<Option<ArtifactRecord>, StoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, session_id, kind, path, metadata_json, byte_len, sha256, created_at
                 FROM artifacts WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            )
            .optional()?;

        match row {
            Some((id, session_id, kind, path, metadata_json, byte_len, sha256, created_at)) => {
                let path = self
                    .layout
                    .metadata_db
                    .parent()
                    .unwrap_or(self.layout.metadata_db.as_path())
                    .join(&path);
                let bytes = read_binary_payload(&path)?;
                validate_integrity(&path, bytes.len() as u64, &bytes, byte_len as u64, &sha256)?;

                Ok(Some(ArtifactRecord {
                    id,
                    session_id,
                    kind,
                    metadata_json,
                    path: PathBuf::from(
                        path.strip_prefix(
                            self.layout
                                .metadata_db
                                .parent()
                                .unwrap_or(self.layout.metadata_db.as_path()),
                        )
                        .unwrap_or(path.as_path()),
                    ),
                    bytes,
                    created_at,
                }))
            }
            None => Ok(None),
        }
    }
}
