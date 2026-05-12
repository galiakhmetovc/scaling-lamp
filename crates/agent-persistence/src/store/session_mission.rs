use super::*;

impl SessionRepository for PersistenceStore {
    fn put_session(&self, record: &SessionRecord) -> Result<(), StoreError> {
        if let Some(existing) = self.get_session(&record.id)?
            && existing.agent_profile_id != record.agent_profile_id
        {
            return Err(StoreError::ImmutableSessionAgentProfile {
                session_id: record.id.clone(),
                existing_agent_profile_id: existing.agent_profile_id,
                attempted_agent_profile_id: record.agent_profile_id.clone(),
            });
        }

        self.with_client(|client| {
            client.execute(
                "INSERT INTO sessions (
                    id, title, prompt_override, settings_json, workspace_root, agent_profile_id, active_mission_id,
                    parent_session_id, parent_job_id, delegation_label, created_at, updated_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title,
                    prompt_override = excluded.prompt_override,
                    settings_json = excluded.settings_json,
                    workspace_root = excluded.workspace_root,
                    agent_profile_id = excluded.agent_profile_id,
                    active_mission_id = excluded.active_mission_id,
                    parent_session_id = excluded.parent_session_id,
                    parent_job_id = excluded.parent_job_id,
                    delegation_label = excluded.delegation_label,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at",
                &[
                    &record.id,
                    &record.title,
                    &record.prompt_override,
                    &record.settings_json,
                    &record.workspace_root,
                    &record.agent_profile_id,
                    &record.active_mission_id,
                    &record.parent_session_id,
                    &record.parent_job_id,
                    &record.delegation_label,
                    &record.created_at,
                    &record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_session(&self, id: &str) -> Result<Option<SessionRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT id, title, prompt_override, settings_json, workspace_root, agent_profile_id, active_mission_id,
                            parent_session_id, parent_job_id, delegation_label, created_at, updated_at
                     FROM sessions WHERE id = $1",
                    &[&id],
                )
                .map(|row| row.map(|row| session_record_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_sessions(&self) -> Result<Vec<SessionRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(
                    "SELECT id, title, prompt_override, settings_json, workspace_root, agent_profile_id, active_mission_id,
                            parent_session_id, parent_job_id, delegation_label, created_at, updated_at
                     FROM sessions
                     ORDER BY created_at ASC, id ASC",
                    &[],
                )
                .map(|rows| rows.iter().map(session_record_from_row).collect())
                .map_err(StoreError::from)
        })
    }

    fn list_sessions_page(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SessionRecord>, StoreError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let offset = i64::try_from(offset).unwrap_or(i64::MAX);
        self.with_client(|client| {
            client
                .query(
                    "SELECT id, title, prompt_override, settings_json, workspace_root, agent_profile_id, active_mission_id,
                            parent_session_id, parent_job_id, delegation_label, created_at, updated_at
                     FROM sessions
                     ORDER BY updated_at DESC, created_at DESC, id ASC
                     LIMIT $1 OFFSET $2",
                    &[&limit, &offset],
                )
                .map(|rows| rows.iter().map(session_record_from_row).collect())
                .map_err(StoreError::from)
        })
    }

    fn delete_session(&self, id: &str) -> Result<bool, StoreError> {
        let transcript_paths = self.session_transcript_payload_paths(id)?;
        let artifact_paths = self.session_artifact_payload_paths(id)?;
        self.append_diagnostic_event(
            "delete_session.start",
            "deleting session from store",
            Some(id),
            std::collections::BTreeMap::from([
                (
                    "transcript_payloads".to_string(),
                    serde_json::json!(transcript_paths.len()),
                ),
                (
                    "artifact_payloads".to_string(),
                    serde_json::json!(artifact_paths.len()),
                ),
            ]),
        );

        let deleted = self.with_client(|client| {
            client
                .execute("DELETE FROM sessions WHERE id = $1", &[&id])
                .map_err(StoreError::from)
        })?;

        if deleted == 0 {
            self.append_diagnostic_event(
                "delete_session.finish",
                "session was not found during delete",
                Some(id),
                std::collections::BTreeMap::new(),
            );
            return Ok(false);
        }

        for path in transcript_paths.into_iter().chain(artifact_paths) {
            remove_payload_if_exists(&path)?;
            remove_payload_if_exists(&backup_path(&path))?;
        }

        self.append_diagnostic_event(
            "delete_session.finish",
            "deleted session from store",
            Some(id),
            std::collections::BTreeMap::from([("deleted".to_string(), serde_json::json!(deleted))]),
        );

        Ok(true)
    }
}

impl MissionRepository for PersistenceStore {
    fn put_mission(&self, record: &MissionRecord) -> Result<(), StoreError> {
        self.with_client(|client| {
            client.execute(
                "INSERT INTO missions (
                    id, session_id, objective, status, execution_intent, schedule_json, acceptance_json,
                    created_at, updated_at, completed_at
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT(id) DO UPDATE SET
                    session_id = excluded.session_id,
                    objective = excluded.objective,
                    status = excluded.status,
                    execution_intent = excluded.execution_intent,
                    schedule_json = excluded.schedule_json,
                    acceptance_json = excluded.acceptance_json,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    completed_at = excluded.completed_at",
                &[
                    &record.id,
                    &record.session_id,
                    &record.objective,
                    &record.status,
                    &record.execution_intent,
                    &record.schedule_json,
                    &record.acceptance_json,
                    &record.created_at,
                    &record.updated_at,
                    &record.completed_at,
                ],
            )?;
            Ok(())
        })
    }

    fn get_mission(&self, id: &str) -> Result<Option<MissionRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT id, session_id, objective, status, execution_intent, schedule_json,
                            acceptance_json, created_at, updated_at, completed_at
                     FROM missions WHERE id = $1",
                    &[&id],
                )
                .map(|row| row.map(|row| mission_record_from_row(&row)))
                .map_err(StoreError::from)
        })
    }

    fn list_missions(&self) -> Result<Vec<MissionRecord>, StoreError> {
        self.with_client(|client| {
            client
                .query(
                    "SELECT id, session_id, objective, status, execution_intent, schedule_json,
                            acceptance_json, created_at, updated_at, completed_at
                     FROM missions
                     ORDER BY created_at ASC, id ASC",
                    &[],
                )
                .map(|rows| rows.iter().map(mission_record_from_row).collect())
                .map_err(StoreError::from)
        })
    }
}

impl TranscriptRepository for PersistenceStore {
    fn put_transcript(&self, record: &TranscriptRecord) -> Result<(), StoreError> {
        let path = self.transcript_path(&record.session_id, &record.id)?;
        let storage_key = self.transcript_storage_key(&record.session_id, &record.id)?;
        let sha256 = sha256_hex(record.content.as_bytes());

        persist_payload_with_commit(&path, record.content.as_bytes(), || {
            self.with_client(|client| {
                client.execute(
                    "INSERT INTO transcripts (
                        id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                     ON CONFLICT(id) DO UPDATE SET
                        session_id = excluded.session_id,
                        run_id = excluded.run_id,
                        kind = excluded.kind,
                        storage_key = excluded.storage_key,
                        byte_len = excluded.byte_len,
                        sha256 = excluded.sha256,
                        created_at = excluded.created_at",
                    &[
                        &record.id,
                        &record.session_id,
                        &record.run_id,
                        &record.kind,
                        &storage_key,
                        &(record.content.len() as i64),
                        &sha256,
                        &record.created_at,
                    ],
                )?;
                Ok(())
            })
        })
    }

    fn get_transcript(&self, id: &str) -> Result<Option<TranscriptRecord>, StoreError> {
        let row = self.with_client(|client| {
            client
                .query_opt(
                    "SELECT id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
                     FROM transcripts WHERE id = $1",
                    &[&id],
                )
                .map(|row| row.map(|row| transcript_row_from_row(&row)))
                .map_err(StoreError::from)
        })?;

        row.map(|row| self.hydrate_transcript_record(row))
            .transpose()
    }

    fn list_transcript_session_stats(
        &self,
    ) -> Result<Vec<crate::repository::TranscriptSessionStats>, StoreError> {
        self.with_client(|client| {
            client
                .query(
                    "SELECT session_id, COUNT(*), MAX(created_at)
                     FROM transcripts
                     GROUP BY session_id
                     ORDER BY session_id ASC",
                    &[],
                )
                .map(|rows| {
                    rows.iter()
                        .map(|row| crate::repository::TranscriptSessionStats {
                            session_id: row.get(0),
                            transcript_count: row.get::<_, i64>(1).max(0) as usize,
                            latest_transcript_created_at: row.get(2),
                        })
                        .collect()
                })
                .map_err(StoreError::from)
        })
    }

    fn get_latest_transcript_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<TranscriptRecord>, StoreError> {
        let row = self.with_client(|client| {
            client
                .query_opt(
                    "SELECT id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
                     FROM transcripts
                     WHERE session_id = $1
                     ORDER BY created_at DESC, id DESC
                     LIMIT 1",
                    &[&session_id],
                )
                .map(|row| row.map(|row| transcript_row_from_row(&row)))
                .map_err(StoreError::from)
        })?;

        row.map(|row| self.hydrate_transcript_record(row))
            .transpose()
    }

    fn get_latest_transcript_created_at_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<i64>, StoreError> {
        self.with_client(|client| {
            client
                .query_opt(
                    "SELECT created_at
                     FROM transcripts
                     WHERE session_id = $1
                     ORDER BY created_at DESC, id DESC
                     LIMIT 1",
                    &[&session_id],
                )
                .map(|row| row.map(|row| row.get(0)))
                .map_err(StoreError::from)
        })
    }

    fn count_transcripts_for_session(&self, session_id: &str) -> Result<usize, StoreError> {
        self.with_client(|client| {
            client
                .query_one(
                    "SELECT COUNT(*) FROM transcripts WHERE session_id = $1",
                    &[&session_id],
                )
                .map(|row| row.get::<_, i64>(0).max(0) as usize)
                .map_err(StoreError::from)
        })
    }

    fn list_transcripts_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<TranscriptRecord>, StoreError> {
        self.query_transcripts(
            "SELECT id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
             FROM transcripts
             WHERE session_id = $1
             ORDER BY created_at ASC, id ASC",
            &[&session_id],
            false,
        )
    }

    fn list_transcripts_tail_for_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<TranscriptRecord>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let limit = limit as i64;
        self.query_transcripts(
            "SELECT id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
             FROM transcripts
             WHERE session_id = $1
             ORDER BY created_at DESC, id DESC
             LIMIT $2",
            &[&session_id, &limit],
            true,
        )
    }
}

impl PersistenceStore {
    fn query_transcripts(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
        reverse: bool,
    ) -> Result<Vec<TranscriptRecord>, StoreError> {
        let rows = self.with_client(|client| {
            client
                .query(sql, params)
                .map(|rows| rows.iter().map(transcript_row_from_row).collect::<Vec<_>>())
                .map_err(StoreError::from)
        })?;
        let mut transcripts = rows
            .into_iter()
            .map(|row| self.hydrate_transcript_record(row))
            .collect::<Result<Vec<_>, _>>()?;
        if reverse {
            transcripts.reverse();
        }
        Ok(transcripts)
    }
}

fn session_record_from_row(row: &Row) -> SessionRecord {
    SessionRecord {
        id: row.get(0),
        title: row.get(1),
        prompt_override: row.get(2),
        settings_json: row.get(3),
        workspace_root: row.get(4),
        agent_profile_id: row.get(5),
        active_mission_id: row.get(6),
        parent_session_id: row.get(7),
        parent_job_id: row.get(8),
        delegation_label: row.get(9),
        created_at: row.get(10),
        updated_at: row.get(11),
    }
}

fn mission_record_from_row(row: &Row) -> MissionRecord {
    MissionRecord {
        id: row.get(0),
        session_id: row.get(1),
        objective: row.get(2),
        status: row.get(3),
        execution_intent: row.get(4),
        schedule_json: row.get(5),
        acceptance_json: row.get(6),
        created_at: row.get(7),
        updated_at: row.get(8),
        completed_at: row.get(9),
    }
}

fn transcript_row_from_row(row: &Row) -> TranscriptRow {
    (
        row.get(0),
        row.get(1),
        row.get(2),
        row.get(3),
        row.get(4),
        row.get(5),
        row.get(6),
        row.get(7),
    )
}
