use super::*;
use agent_runtime::session::{Session, TranscriptEntry};
use agent_runtime::skills::{resolve_session_skill_status, scan_skill_catalog};

impl App {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn session_transcript(
        &self,
        session_id: &str,
    ) -> Result<SessionTranscriptView, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }

        let entries = store
            .list_transcripts_for_session(session_id)?
            .into_iter()
            .map(TranscriptEntry::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?
            .into_iter()
            .map(|entry| SessionTranscriptLine {
                role: entry.role.as_str().to_string(),
                content: entry.content,
                run_id: entry.run_id,
                created_at: entry.created_at,
            })
            .collect();

        Ok(SessionTranscriptView {
            session_id: session_id.to_string(),
            entries,
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn create_session_auto(
        &self,
        title: Option<&str>,
    ) -> Result<SessionSummary, BootstrapError> {
        self.create_session(
            &format!("session-{}", unique_timestamp_token()?),
            title.unwrap_or("New Session").trim(),
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn create_session(&self, id: &str, title: &str) -> Result<SessionSummary, BootstrapError> {
        let store = self.store()?;
        let now = unix_timestamp()?;
        let session = Session {
            id: id.to_string(),
            title: title.trim().to_string(),
            prompt_override: None,
            settings: SessionSettings::default(),
            active_mission_id: None,
            created_at: now,
            updated_at: now,
        };
        let record = agent_persistence::SessionRecord::try_from(&session)
            .map_err(BootstrapError::RecordConversion)?;
        store.put_session(&record)?;
        self.session_summary(&session.id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError> {
        let store = self.store()?;
        build_session_summaries(&store, &self.config, &self.runtime.workspace)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn session_summary(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        self.list_session_summaries()?
            .into_iter()
            .find(|summary| summary.id == session_id)
            .ok_or_else(|| BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn session_skills(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        let store = self.store()?;
        let session = Session::try_from(store.get_session(session_id)?.ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            }
        })?)
        .map_err(BootstrapError::RecordConversion)?;
        let transcripts = store
            .list_transcripts_for_session(session_id)?
            .into_iter()
            .map(TranscriptEntry::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;

        let catalog = self.skill_catalog()?;
        Ok(
            resolve_session_skill_status(&catalog, &session.settings, &session.title, &transcripts)
                .into_iter()
                .map(SessionSkillStatus::from)
                .collect(),
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_session_skills(&self, session_id: &str) -> Result<String, BootstrapError> {
        let skills = self.session_skills(session_id)?;
        if skills.is_empty() {
            return Ok("skills: none discovered".to_string());
        }

        let mut lines = vec!["Skills:".to_string()];
        lines.extend(
            skills
                .into_iter()
                .map(|skill| format!("- [{}] {}: {}", skill.mode, skill.name, skill.description)),
        );
        Ok(lines.join("\n"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        self.update_session_skill_state(session_id, skill_name, SkillCommand::Enable)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        self.update_session_skill_state(session_id, skill_name, SkillCommand::Disable)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<SessionSummary, BootstrapError> {
        let store = self.store()?;
        let record =
            store
                .get_session(session_id)?
                .ok_or_else(|| BootstrapError::MissingRecord {
                    kind: "session",
                    id: session_id.to_string(),
                })?;
        let mut session = Session::try_from(record).map_err(BootstrapError::RecordConversion)?;

        if let Some(title) = patch.title {
            session.title = title.trim().to_string();
        }
        if let Some(model) = patch.model {
            session.settings.model = model.map(|value| value.trim().to_string());
        }
        if let Some(reasoning_visible) = patch.reasoning_visible {
            session.settings.reasoning_visible = reasoning_visible;
        }
        if let Some(think_level) = patch.think_level {
            session.settings.think_level = think_level.map(|value| value.trim().to_string());
        }
        if let Some(compactifications) = patch.compactifications {
            session.settings.compactifications = compactifications;
        }
        session.updated_at = unix_timestamp()?;

        let record = agent_persistence::SessionRecord::try_from(&session)
            .map_err(BootstrapError::RecordConversion)?;
        store.put_session(&record)?;
        self.session_summary(session_id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn delete_session(&self, session_id: &str) -> Result<(), BootstrapError> {
        let store = self.store()?;
        let deleted = store.delete_session(session_id)?;
        if !deleted {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn clear_session(
        &self,
        session_id: &str,
        title: Option<&str>,
    ) -> Result<SessionSummary, BootstrapError> {
        self.delete_session(session_id)?;
        self.create_session_auto(title)
    }

    fn update_session_skill_state(
        &self,
        session_id: &str,
        skill_name: &str,
        command: SkillCommand,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        let normalized_skill = skill_name.trim().to_lowercase();
        if normalized_skill.is_empty() {
            return Err(BootstrapError::Usage {
                reason: "skill name must not be empty".to_string(),
            });
        }

        let store = self.store()?;
        let record =
            store
                .get_session(session_id)?
                .ok_or_else(|| BootstrapError::MissingRecord {
                    kind: "session",
                    id: session_id.to_string(),
                })?;
        let mut session = Session::try_from(record).map_err(BootstrapError::RecordConversion)?;
        let catalog = self.skill_catalog()?;
        let matching = catalog
            .entries
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(normalized_skill.as_str()))
            .or_else(|| {
                catalog
                    .entries
                    .iter()
                    .find(|entry| entry.name.to_lowercase() == normalized_skill)
            })
            .ok_or_else(|| BootstrapError::Usage {
                reason: format!("unknown skill {skill_name}"),
            })?;

        match command {
            SkillCommand::Enable => {
                session.settings.enable_skill(&matching.name);
            }
            SkillCommand::Disable => {
                session.settings.disable_skill(&matching.name);
            }
        }
        session.updated_at = unix_timestamp()?;

        let record = agent_persistence::SessionRecord::try_from(&session)
            .map_err(BootstrapError::RecordConversion)?;
        store.put_session(&record)?;
        self.session_skills(session_id)
    }

    fn skill_catalog(&self) -> Result<agent_runtime::skills::SkillCatalog, BootstrapError> {
        scan_skill_catalog(&self.config.daemon.skills_dir).map_err(|source| BootstrapError::Io {
            path: self.config.daemon.skills_dir.clone(),
            source,
        })
    }
}
