use super::*;

const CURRENT_AGENT_PROFILE_KEY: &str = "current_agent_profile_id";

impl AgentRepository for PersistenceStore {
    fn put_agent_profile(&self, record: &AgentProfileRecord) -> Result<(), StoreError> {
        validate_identifier(&record.id)?;
        self.connection.execute(
            "INSERT INTO agent_profiles (
                id, name, template_kind, agent_home, allowed_tools_json, default_workspace_root,
                created_from_template_id, created_by_session_id, created_by_agent_profile_id,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                template_kind = excluded.template_kind,
                agent_home = excluded.agent_home,
                allowed_tools_json = excluded.allowed_tools_json,
                default_workspace_root = excluded.default_workspace_root,
                created_from_template_id = excluded.created_from_template_id,
                created_by_session_id = excluded.created_by_session_id,
                created_by_agent_profile_id = excluded.created_by_agent_profile_id,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
            params![
                record.id,
                record.name,
                record.template_kind,
                record.agent_home,
                record.allowed_tools_json,
                record.default_workspace_root,
                record.created_from_template_id,
                record.created_by_session_id,
                record.created_by_agent_profile_id,
                record.created_at,
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    fn get_agent_profile(&self, id: &str) -> Result<Option<AgentProfileRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, name, template_kind, agent_home, allowed_tools_json, default_workspace_root,
                        created_from_template_id, created_by_session_id, created_by_agent_profile_id,
                        created_at, updated_at
                 FROM agent_profiles
                 WHERE id = ?1",
                [id],
                |row| {
                    Ok(AgentProfileRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        template_kind: row.get(2)?,
                        agent_home: row.get(3)?,
                        allowed_tools_json: row.get(4)?,
                        default_workspace_root: row.get(5)?,
                        created_from_template_id: row.get(6)?,
                        created_by_session_id: row.get(7)?,
                        created_by_agent_profile_id: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_agent_profiles(&self) -> Result<Vec<AgentProfileRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, template_kind, agent_home, allowed_tools_json, default_workspace_root,
                    created_from_template_id, created_by_session_id, created_by_agent_profile_id,
                    created_at, updated_at
             FROM agent_profiles
             ORDER BY created_at ASC, id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut profiles = Vec::new();

        while let Some(row) = rows.next()? {
            profiles.push(AgentProfileRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                template_kind: row.get(2)?,
                agent_home: row.get(3)?,
                allowed_tools_json: row.get(4)?,
                default_workspace_root: row.get(5)?,
                created_from_template_id: row.get(6)?,
                created_by_session_id: row.get(7)?,
                created_by_agent_profile_id: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            });
        }

        Ok(profiles)
    }

    fn delete_agent_profile(&self, id: &str) -> Result<bool, StoreError> {
        Ok(self
            .connection
            .execute("DELETE FROM agent_profiles WHERE id = ?1", [id])?
            > 0)
    }

    fn get_current_agent_profile_id(&self) -> Result<Option<String>, StoreError> {
        self.connection
            .query_row(
                "SELECT value FROM daemon_state WHERE key = ?1",
                [CURRENT_AGENT_PROFILE_KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn set_current_agent_profile_id(&self, id: Option<&str>) -> Result<(), StoreError> {
        match id {
            Some(id) => {
                validate_identifier(id)?;
                self.connection.execute(
                    "INSERT INTO daemon_state (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![CURRENT_AGENT_PROFILE_KEY, id],
                )?;
            }
            None => {
                self.connection.execute(
                    "DELETE FROM daemon_state WHERE key = ?1",
                    [CURRENT_AGENT_PROFILE_KEY],
                )?;
            }
        }
        Ok(())
    }

    fn put_agent_chain_continuation(
        &self,
        record: &AgentChainContinuationRecord,
    ) -> Result<(), StoreError> {
        validate_identifier(&record.chain_id)?;
        self.connection.execute(
            "INSERT INTO agent_chain_continuations (
                chain_id, reason, granted_hops, granted_at
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(chain_id) DO UPDATE SET
                reason = excluded.reason,
                granted_hops = excluded.granted_hops,
                granted_at = excluded.granted_at",
            params![
                record.chain_id,
                record.reason,
                record.granted_hops,
                record.granted_at,
            ],
        )?;
        Ok(())
    }

    fn get_agent_chain_continuation(
        &self,
        chain_id: &str,
    ) -> Result<Option<AgentChainContinuationRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT chain_id, reason, granted_hops, granted_at
                 FROM agent_chain_continuations
                 WHERE chain_id = ?1",
                [chain_id],
                |row| {
                    Ok(AgentChainContinuationRecord {
                        chain_id: row.get(0)?,
                        reason: row.get(1)?,
                        granted_hops: row.get(2)?,
                        granted_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn delete_agent_chain_continuation(&self, chain_id: &str) -> Result<bool, StoreError> {
        Ok(self.connection.execute(
            "DELETE FROM agent_chain_continuations WHERE chain_id = ?1",
            [chain_id],
        )? > 0)
    }

    fn put_agent_schedule(&self, record: &AgentScheduleRecord) -> Result<(), StoreError> {
        validate_identifier(&record.id)?;
        validate_identifier(&record.agent_profile_id)?;
        if let Some(target_session_id) = record.target_session_id.as_deref() {
            validate_identifier(target_session_id)?;
        }
        if let Some(last_session_id) = record.last_session_id.as_deref() {
            validate_identifier(last_session_id)?;
        }
        if let Some(last_job_id) = record.last_job_id.as_deref() {
            validate_identifier(last_job_id)?;
        }
        self.connection.execute(
            "INSERT INTO agent_schedules (
                id, agent_profile_id, workspace_root, prompt, mode, delivery_mode,
                target_session_id, interval_seconds, next_fire_at, enabled, last_triggered_at,
                last_finished_at, last_session_id, last_job_id, last_result, last_error,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
             ON CONFLICT(id) DO UPDATE SET
                agent_profile_id = excluded.agent_profile_id,
                workspace_root = excluded.workspace_root,
                prompt = excluded.prompt,
                mode = excluded.mode,
                delivery_mode = excluded.delivery_mode,
                target_session_id = excluded.target_session_id,
                interval_seconds = excluded.interval_seconds,
                next_fire_at = excluded.next_fire_at,
                enabled = excluded.enabled,
                last_triggered_at = excluded.last_triggered_at,
                last_finished_at = excluded.last_finished_at,
                last_session_id = excluded.last_session_id,
                last_job_id = excluded.last_job_id,
                last_result = excluded.last_result,
                last_error = excluded.last_error,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
            params![
                record.id,
                record.agent_profile_id,
                record.workspace_root,
                record.prompt,
                record.mode,
                record.delivery_mode,
                record.target_session_id,
                record.interval_seconds,
                record.next_fire_at,
                record.enabled,
                record.last_triggered_at,
                record.last_finished_at,
                record.last_session_id,
                record.last_job_id,
                record.last_result,
                record.last_error,
                record.created_at,
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    fn get_agent_schedule(&self, id: &str) -> Result<Option<AgentScheduleRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, agent_profile_id, workspace_root, prompt, mode, delivery_mode,
                        target_session_id, interval_seconds, next_fire_at, enabled,
                        last_triggered_at, last_finished_at, last_session_id, last_job_id,
                        last_result, last_error, created_at, updated_at
                 FROM agent_schedules
                 WHERE id = ?1",
                [id],
                |row| {
                    Ok(AgentScheduleRecord {
                        id: row.get(0)?,
                        agent_profile_id: row.get(1)?,
                        workspace_root: row.get(2)?,
                        prompt: row.get(3)?,
                        mode: row.get(4)?,
                        delivery_mode: row.get(5)?,
                        target_session_id: row.get(6)?,
                        interval_seconds: row.get(7)?,
                        next_fire_at: row.get(8)?,
                        enabled: row.get(9)?,
                        last_triggered_at: row.get(10)?,
                        last_finished_at: row.get(11)?,
                        last_session_id: row.get(12)?,
                        last_job_id: row.get(13)?,
                        last_result: row.get(14)?,
                        last_error: row.get(15)?,
                        created_at: row.get(16)?,
                        updated_at: row.get(17)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_agent_schedules(&self) -> Result<Vec<AgentScheduleRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, agent_profile_id, workspace_root, prompt, mode, delivery_mode,
                    target_session_id, interval_seconds, next_fire_at, enabled,
                    last_triggered_at, last_finished_at, last_session_id, last_job_id,
                    last_result, last_error, created_at, updated_at
             FROM agent_schedules
             ORDER BY created_at ASC, id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut schedules = Vec::new();

        while let Some(row) = rows.next()? {
            schedules.push(AgentScheduleRecord {
                id: row.get(0)?,
                agent_profile_id: row.get(1)?,
                workspace_root: row.get(2)?,
                prompt: row.get(3)?,
                mode: row.get(4)?,
                delivery_mode: row.get(5)?,
                target_session_id: row.get(6)?,
                interval_seconds: row.get(7)?,
                next_fire_at: row.get(8)?,
                enabled: row.get(9)?,
                last_triggered_at: row.get(10)?,
                last_finished_at: row.get(11)?,
                last_session_id: row.get(12)?,
                last_job_id: row.get(13)?,
                last_result: row.get(14)?,
                last_error: row.get(15)?,
                created_at: row.get(16)?,
                updated_at: row.get(17)?,
            });
        }

        Ok(schedules)
    }

    fn delete_agent_schedule(&self, id: &str) -> Result<bool, StoreError> {
        Ok(self
            .connection
            .execute("DELETE FROM agent_schedules WHERE id = ?1", [id])?
            > 0)
    }
}
