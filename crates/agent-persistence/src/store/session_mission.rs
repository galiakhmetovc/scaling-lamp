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

        self.connection.execute(
            "INSERT INTO sessions (
                id, title, prompt_override, settings_json, agent_profile_id, active_mission_id,
                parent_session_id, parent_job_id, delegation_label, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                prompt_override = excluded.prompt_override,
                settings_json = excluded.settings_json,
                agent_profile_id = excluded.agent_profile_id,
                active_mission_id = excluded.active_mission_id,
                parent_session_id = excluded.parent_session_id,
                parent_job_id = excluded.parent_job_id,
                delegation_label = excluded.delegation_label,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
            params![
                record.id,
                record.title,
                record.prompt_override,
                &record.settings_json,
                &record.agent_profile_id,
                record.active_mission_id,
                record.parent_session_id,
                record.parent_job_id,
                record.delegation_label,
                record.created_at,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    fn get_session(&self, id: &str) -> Result<Option<SessionRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, title, prompt_override, settings_json, agent_profile_id, active_mission_id,
                        parent_session_id, parent_job_id, delegation_label, created_at, updated_at
                 FROM sessions WHERE id = ?1",
                [id],
                |row| {
                    Ok(SessionRecord {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        prompt_override: row.get(2)?,
                        settings_json: row.get(3)?,
                        agent_profile_id: row.get(4)?,
                        active_mission_id: row.get(5)?,
                        parent_session_id: row.get(6)?,
                        parent_job_id: row.get(7)?,
                        delegation_label: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_sessions(&self) -> Result<Vec<SessionRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, title, prompt_override, settings_json, agent_profile_id, active_mission_id,
                    parent_session_id, parent_job_id, delegation_label, created_at, updated_at
             FROM sessions
             ORDER BY created_at ASC, id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut sessions = Vec::new();

        while let Some(row) = rows.next()? {
            sessions.push(SessionRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                prompt_override: row.get(2)?,
                settings_json: row.get(3)?,
                agent_profile_id: row.get(4)?,
                active_mission_id: row.get(5)?,
                parent_session_id: row.get(6)?,
                parent_job_id: row.get(7)?,
                delegation_label: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            });
        }

        Ok(sessions)
    }

    fn delete_session(&self, id: &str) -> Result<bool, StoreError> {
        let transcript_paths = self.session_transcript_payload_paths(id)?;
        let artifact_paths = self.session_artifact_payload_paths(id)?;
        self.connection.execute(
            "DELETE FROM session_search_fts
             WHERE doc_id IN (
                 SELECT doc_id FROM session_search_docs WHERE session_id = ?1
             )",
            [id],
        )?;
        let deleted = self
            .connection
            .execute("DELETE FROM sessions WHERE id = ?1", [id])?;

        if deleted == 0 {
            return Ok(false);
        }

        for path in transcript_paths.into_iter().chain(artifact_paths) {
            remove_payload_if_exists(&path)?;
            remove_payload_if_exists(&backup_path(&path))?;
        }

        Ok(true)
    }
}

impl MissionRepository for PersistenceStore {
    fn put_mission(&self, record: &MissionRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO missions (
                id, session_id, objective, status, execution_intent, schedule_json, acceptance_json,
                created_at, updated_at, completed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
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
            params![
                record.id,
                record.session_id,
                record.objective,
                record.status,
                record.execution_intent,
                record.schedule_json,
                record.acceptance_json,
                record.created_at,
                record.updated_at,
                record.completed_at
            ],
        )?;
        Ok(())
    }

    fn get_mission(&self, id: &str) -> Result<Option<MissionRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, session_id, objective, status, execution_intent, schedule_json,
                        acceptance_json, created_at, updated_at, completed_at
                 FROM missions WHERE id = ?1",
                [id],
                |row| {
                    Ok(MissionRecord {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        objective: row.get(2)?,
                        status: row.get(3)?,
                        execution_intent: row.get(4)?,
                        schedule_json: row.get(5)?,
                        acceptance_json: row.get(6)?,
                        created_at: row.get(7)?,
                        updated_at: row.get(8)?,
                        completed_at: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_missions(&self) -> Result<Vec<MissionRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, objective, status, execution_intent, schedule_json,
                    acceptance_json, created_at, updated_at, completed_at
             FROM missions
             ORDER BY created_at ASC, id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut missions = Vec::new();

        while let Some(row) = rows.next()? {
            missions.push(MissionRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                objective: row.get(2)?,
                status: row.get(3)?,
                execution_intent: row.get(4)?,
                schedule_json: row.get(5)?,
                acceptance_json: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                completed_at: row.get(9)?,
            });
        }

        Ok(missions)
    }
}

impl TranscriptRepository for PersistenceStore {
    fn put_transcript(&self, record: &TranscriptRecord) -> Result<(), StoreError> {
        let path = self.transcript_path(&record.id)?;
        let storage_key = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| StoreError::InvalidIdentifier {
                id: record.id.clone(),
                reason: "must produce a valid payload filename",
            })?
            .to_string();
        let sha256 = sha256_hex(record.content.as_bytes());

        persist_payload_with_commit(&path, record.content.as_bytes(), || {
            self.connection
                .execute(
                    "INSERT INTO transcripts (
                        id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(id) DO UPDATE SET
                        session_id = excluded.session_id,
                        run_id = excluded.run_id,
                        kind = excluded.kind,
                        storage_key = excluded.storage_key,
                        byte_len = excluded.byte_len,
                        sha256 = excluded.sha256,
                        created_at = excluded.created_at",
                    params![
                        record.id,
                        record.session_id,
                        record.run_id,
                        record.kind,
                        storage_key,
                        record.content.len() as i64,
                        sha256,
                        record.created_at
                    ],
                )
                .map(|_| ())
                .map_err(StoreError::from)
        })
    }

    fn get_transcript(&self, id: &str) -> Result<Option<TranscriptRecord>, StoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
                 FROM transcripts WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
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
            Some(row) => Ok(Some(self.hydrate_transcript_record(row)?)),
            None => Ok(None),
        }
    }

    fn get_latest_transcript_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<TranscriptRecord>, StoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
                 FROM transcripts
                 WHERE session_id = ?1
                 ORDER BY created_at DESC, id DESC
                 LIMIT 1",
                [session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
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
            Some(row) => Ok(Some(self.hydrate_transcript_record(row)?)),
            None => Ok(None),
        }
    }

    fn count_transcripts_for_session(&self, session_id: &str) -> Result<usize, StoreError> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM transcripts WHERE session_id = ?1",
                [session_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count.max(0) as usize)
            .map_err(StoreError::from)
    }

    fn list_transcripts_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<TranscriptRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
             FROM transcripts
             WHERE session_id = ?1
             ORDER BY created_at ASC, id ASC",
        )?;
        let mut rows = statement.query([session_id])?;
        let mut transcripts = Vec::new();

        while let Some(row) = rows.next()? {
            let row: TranscriptRow = (
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            );
            transcripts.push(self.hydrate_transcript_record(row)?);
        }

        Ok(transcripts)
    }
}
