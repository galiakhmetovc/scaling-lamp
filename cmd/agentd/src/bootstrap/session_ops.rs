use super::*;
use agent_persistence::JobRepository;
use agent_runtime::mission::JobSpec;
use agent_runtime::run::{RunSnapshot, RunStatus, RunStepKind};
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

        let mut entries = store
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
                tool_name: None,
                tool_status: None,
                approval_id: None,
            })
            .collect::<Vec<_>>();

        let runs = store
            .load_execution_state()?
            .runs
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;
        entries.extend(build_synthetic_run_lines(session_id, &runs));
        entries.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| {
                    transcript_line_sort_weight(left).cmp(&transcript_line_sort_weight(right))
                })
                .then_with(|| left.role.cmp(&right.role))
                .then_with(|| left.content.cmp(&right.content))
        });

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
        if let Some(completion_nudges) = patch.completion_nudges {
            session.settings.completion_nudges = completion_nudges;
        }
        if let Some(auto_approve) = patch.auto_approve {
            session.settings.auto_approve = auto_approve;
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

fn build_synthetic_run_lines(session_id: &str, runs: &[RunSnapshot]) -> Vec<SessionTranscriptLine> {
    let mut lines = Vec::new();
    for run in runs.iter().filter(|run| run.session_id == session_id) {
        for step in &run.recent_steps {
            match step.kind {
                RunStepKind::ToolCompleted => {
                    let (tool_name, status, summary) = parse_tool_step_detail(step.detail.as_str());
                    lines.push(SessionTranscriptLine {
                        role: "tool".to_string(),
                        content: summary,
                        run_id: Some(run.id.clone()),
                        created_at: step.recorded_at,
                        tool_name: Some(tool_name),
                        tool_status: Some(status),
                        approval_id: None,
                    });
                }
                RunStepKind::WaitingApproval
                | RunStepKind::ApprovalResolved
                | RunStepKind::WaitingProcess
                | RunStepKind::ProcessCompleted
                | RunStepKind::WaitingDelegate
                | RunStepKind::DelegateCompleted
                | RunStepKind::Cancelled
                | RunStepKind::Interrupted => {
                    lines.push(SessionTranscriptLine {
                        role: "system".to_string(),
                        content: step.detail.clone(),
                        run_id: Some(run.id.clone()),
                        created_at: step.recorded_at,
                        tool_name: None,
                        tool_status: None,
                        approval_id: None,
                    });
                }
                _ => {}
            }
        }

        if let Some(provider_stream) = run.provider_stream.as_ref()
            && !provider_stream.output_text.trim().is_empty()
        {
            lines.push(SessionTranscriptLine {
                role: "assistant".to_string(),
                content: provider_stream.output_text.clone(),
                run_id: Some(run.id.clone()),
                created_at: provider_stream.updated_at,
                tool_name: None,
                tool_status: None,
                approval_id: None,
            });
        }

        if matches!(
            run.status,
            RunStatus::Failed | RunStatus::Cancelled | RunStatus::Interrupted
        ) && let Some(error) = run.error.as_deref()
        {
            lines.push(SessionTranscriptLine {
                role: "system".to_string(),
                content: format!("chat failed: {error}"),
                run_id: Some(run.id.clone()),
                created_at: run.finished_at.unwrap_or(run.updated_at),
                tool_name: None,
                tool_status: None,
                approval_id: None,
            });
        }
    }
    lines
}

fn transcript_line_sort_weight(line: &SessionTranscriptLine) -> u8 {
    match line.role.as_str() {
        "user" => 0,
        "reasoning" => 1,
        "tool" => 2,
        "approval" => 3,
        "system" => 4,
        "assistant" => 5,
        _ => 6,
    }
}

fn parse_tool_step_detail(detail: &str) -> (String, String, String) {
    let summary = detail.trim().to_string();
    let tool_summary = detail
        .split_once(" -> ")
        .map(|(head, _)| head.trim())
        .unwrap_or_else(|| detail.trim());
    let tool_name = tool_summary
        .split_whitespace()
        .next()
        .unwrap_or("tool")
        .to_string();
    let detail_lower = detail.to_ascii_lowercase();
    let status = if detail_lower.contains("retryable error:")
        || detail_lower.contains(" invalid arguments:")
        || detail_lower.contains(" failed:")
    {
        "failed"
    } else {
        "completed"
    };
    (tool_name, status.to_string(), summary)
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
