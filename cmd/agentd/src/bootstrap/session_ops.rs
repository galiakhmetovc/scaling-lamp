use super::*;
use agent_persistence::JobRepository;
use agent_runtime::mission::JobSpec;
use agent_runtime::session::{Session, TranscriptEntry};
use agent_runtime::skills::{resolve_session_skill_status, scan_skill_catalog};
use time::OffsetDateTime;
use time::UtcOffset;
use time::format_description::well_known::Rfc3339;

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
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
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
    pub fn session_background_jobs(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionBackgroundJob>, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }

        store
            .list_active_jobs_for_session(session_id)?
            .into_iter()
            .map(JobSpec::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)
            .map(|jobs| {
                jobs.into_iter()
                    .map(|job| SessionBackgroundJob {
                        id: job.id,
                        kind: job.kind.as_str().to_string(),
                        status: job.status.as_str().to_string(),
                        queued_at: job.created_at,
                        started_at: job.started_at,
                        last_progress_message: job.last_progress_message,
                    })
                    .collect()
            })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_session_background_jobs(
        &self,
        session_id: &str,
    ) -> Result<String, BootstrapError> {
        let jobs = self.session_background_jobs(session_id)?;
        if jobs.is_empty() {
            return Ok("jobs: none active".to_string());
        }

        let mut lines = vec!["Jobs:".to_string()];
        for job in jobs {
            lines.push(format!("- [{}] {} ({})", job.status, job.id, job.kind));
            lines.push(format!(
                "  queued: {}",
                format_background_job_time(job.queued_at)
            ));
            if let Some(started_at) = job.started_at {
                lines.push(format!(
                    "  started: {}",
                    format_background_job_time(started_at)
                ));
            }
            if let Some(progress) = job.last_progress_message {
                lines.push(format!("  progress: {progress}"));
            }
        }
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

fn format_background_job_time(timestamp: i64) -> String {
    OffsetDateTime::from_unix_timestamp(timestamp)
        .map(|value| {
            value
                .to_offset(UtcOffset::UTC)
                .format(&Rfc3339)
                .unwrap_or_else(|_| timestamp.to_string())
        })
        .unwrap_or_else(|_| timestamp.to_string())
}
