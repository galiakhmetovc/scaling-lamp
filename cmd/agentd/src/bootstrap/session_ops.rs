use super::*;
use crate::agents;
use crate::diagnostics::DiagnosticEventBuilder;
use agent_persistence::{JobRepository, RunRepository};
use agent_runtime::interagent::{AgentChainState, AgentMessageChain};
use agent_runtime::mission::JobSpec;
use agent_runtime::run::{RunSnapshot, RunStatus, RunStepKind};
use agent_runtime::session::{
    MessageRole, Session, TranscriptEntry, parse_scheduled_input_metadata,
};
use agent_runtime::skills::{resolve_session_skill_status, scan_skill_catalog_with_overrides};
use agent_runtime::tool::ProcessOutputStream;
use std::collections::HashMap;
use std::time::Instant;
use time::OffsetDateTime;
use time::UtcOffset;
use time::format_description::well_known::Rfc3339;

impl App {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn session_transcript(
        &self,
        session_id: &str,
    ) -> Result<SessionTranscriptView, BootstrapError> {
        self.build_session_transcript_view(session_id, None, None)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn session_transcript_tail(
        &self,
        session_id: &str,
        max_entries: usize,
    ) -> Result<SessionTranscriptView, BootstrapError> {
        let run_limit = if max_entries == 0 {
            0
        } else {
            self.config.runtime_limits.transcript_tail_run_limit
        };
        self.build_session_transcript_view(session_id, Some(max_entries), Some(run_limit))
    }

    fn build_session_transcript_view(
        &self,
        session_id: &str,
        transcript_limit: Option<usize>,
        run_limit: Option<usize>,
    ) -> Result<SessionTranscriptView, BootstrapError> {
        let started = Instant::now();
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "session_transcript.start",
            "building session transcript view",
        )
        .session_id(session_id.to_string())
        .field("transcript_limit", transcript_limit)
        .field("run_limit", run_limit)
        .emit(&self.persistence.audit);
        let store_started = Instant::now();
        let store = self.store()?;
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "session_transcript.opened_store",
            "opened runtime store for session transcript view",
        )
        .session_id(session_id.to_string())
        .elapsed_ms(store_started.elapsed().as_millis() as u64)
        .outcome("ok")
        .emit(&self.persistence.audit);

        let exists_started = Instant::now();
        if !store.session_exists(session_id)? {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "session_transcript.checked_session_exists",
            "verified session existence for transcript view",
        )
        .session_id(session_id.to_string())
        .elapsed_ms(exists_started.elapsed().as_millis() as u64)
        .outcome("ok")
        .emit(&self.persistence.audit);

        let transcripts_started = Instant::now();
        let entries = match transcript_limit {
            Some(limit) => store.list_transcripts_tail_for_session(session_id, limit)?,
            None => store.list_transcripts_for_session(session_id)?,
        }
        .into_iter()
        .map(TranscriptEntry::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(BootstrapError::RecordConversion)?;
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "session_transcript.loaded_transcripts",
            "loaded transcript entries for session transcript view",
        )
        .session_id(session_id.to_string())
        .elapsed_ms(transcripts_started.elapsed().as_millis() as u64)
        .outcome("ok")
        .field("transcript_count", entries.len())
        .emit(&self.persistence.audit);
        let schedule_labels = entries
            .iter()
            .filter_map(|entry| {
                (entry.role == MessageRole::System)
                    .then(|| parse_scheduled_input_metadata(&entry.content))
                    .flatten()
                    .map(|metadata| (metadata.message_id, metadata.schedule_id))
            })
            .collect::<HashMap<_, _>>();
        let mut entries = entries
            .into_iter()
            .filter_map(|entry| {
                if entry.role == MessageRole::System
                    && parse_scheduled_input_metadata(&entry.content).is_some()
                {
                    return None;
                }

                let role = schedule_labels
                    .get(&entry.id)
                    .map(|schedule_id| format!("расписание: {schedule_id}"))
                    .unwrap_or_else(|| entry.role.as_str().to_string());
                Some(SessionTranscriptLine {
                    role,
                    content: entry.content,
                    run_id: entry.run_id,
                    created_at: entry.created_at,
                    tool_name: None,
                    tool_status: None,
                    approval_id: None,
                })
            })
            .collect::<Vec<_>>();

        let runs_started = Instant::now();
        let runs = match run_limit {
            Some(limit) => store.list_recent_runs_for_session(session_id, limit)?,
            None => store.list_runs_for_session(session_id)?,
        }
        .into_iter()
        .map(RunSnapshot::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(BootstrapError::RecordConversion)?;
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "session_transcript.loaded_runs",
            "loaded session-scoped runs for transcript view",
        )
        .session_id(session_id.to_string())
        .elapsed_ms(runs_started.elapsed().as_millis() as u64)
        .outcome("ok")
        .field("run_count", runs.len())
        .emit(&self.persistence.audit);
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

        let transcript = SessionTranscriptView {
            session_id: session_id.to_string(),
            entries,
        };
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "session_transcript.finish",
            "built session transcript view",
        )
        .session_id(session_id.to_string())
        .elapsed_ms(started.elapsed().as_millis() as u64)
        .outcome("ok")
        .field("entry_count", transcript.entries.len())
        .field("transcript_limit", transcript_limit)
        .field("run_limit", run_limit)
        .emit(&self.persistence.audit);
        Ok(transcript)
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
        let started = Instant::now();
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "create_session.start",
            "creating session",
        )
        .session_id(id.to_string())
        .field("title", title.trim())
        .emit(&self.persistence.audit);
        let store = self.store()?;
        let now = unix_timestamp()?;
        let settings = SessionSettings {
            working_memory_limit: self.config.session_defaults.working_memory_limit,
            project_memory_enabled: self.config.session_defaults.project_memory_enabled,
            ..SessionSettings::default()
        };
        let agent_profile_id = self.current_agent_profile_id()?;
        let session = Session {
            id: id.to_string(),
            title: title.trim().to_string(),
            prompt_override: None,
            settings,
            agent_profile_id,
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
        let summary = self.session_summary(&session.id)?;
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "create_session.finish",
            "created session",
        )
        .session_id(summary.id.clone())
        .elapsed_ms(started.elapsed().as_millis() as u64)
        .outcome("ok")
        .emit(&self.persistence.audit);
        Ok(summary)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError> {
        let started = Instant::now();
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "list_session_summaries.start",
            "listing session summaries",
        )
        .emit(&self.persistence.audit);
        let store = self.store()?;
        let summaries = build_session_summaries(&store, &self.config, &self.runtime.workspace)?;
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "list_session_summaries.finish",
            "listed session summaries",
        )
        .elapsed_ms(started.elapsed().as_millis() as u64)
        .outcome("ok")
        .field("count", summaries.len())
        .emit(&self.persistence.audit);
        Ok(summaries)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn session_summary(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        let started = Instant::now();
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "session_summary.start",
            "building session summary",
        )
        .session_id(session_id.to_string())
        .emit(&self.persistence.audit);
        let store = self.store()?;
        let summary = build_single_session_summary(
            &store,
            &self.config,
            &self.runtime.workspace,
            session_id,
        )?;
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "session_summary.finish",
            "built session summary",
        )
        .session_id(session_id.to_string())
        .elapsed_ms(started.elapsed().as_millis() as u64)
        .outcome("ok")
        .field("message_count", summary.message_count)
        .emit(&self.persistence.audit);
        Ok(summary)
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

        let catalog = self.skill_catalog(&session.agent_profile_id)?;
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
            return Ok("Скиллы: ничего не найдено".to_string());
        }

        let mut lines = vec!["Скиллы:".to_string()];
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
            return Ok("Задачи: активных нет".to_string());
        }

        let mut lines = vec!["Задачи:".to_string()];
        for job in jobs {
            lines.push(format!("- [{}] {} ({})", job.status, job.id, job.kind));
            lines.push(format!(
                "  поставлена_в_очередь: {}",
                format_background_job_time(job.queued_at)
            ));
            if let Some(started_at) = job.started_at {
                lines.push(format!(
                    "  запущена: {}",
                    format_background_job_time(started_at)
                ));
            }
            if let Some(progress) = job.last_progress_message {
                lines.push(format!("  прогресс: {progress}"));
            }
        }
        Ok(lines.join("\n"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }

        let Some(run) = self
            .execution_service()
            .latest_active_session_run(&store, session_id)
            .map_err(BootstrapError::Execution)?
        else {
            return Ok("Ход: активного выполнения нет".to_string());
        };
        let summary = self.session_summary(session_id)?;
        let interagent = load_session_interagent_summary(&store, session_id)?;

        let mut lines = vec![
            "Ход:".to_string(),
            format!("- сессия: {}", summary.title),
            format!(
                "- агент: {} ({})",
                summary.agent_name, summary.agent_profile_id
            ),
            format!("- id: {}", run.id),
            format!("- статус: {}", run.status.as_str()),
            format!("- начат: {}", format_background_job_time(run.started_at)),
            format!("- обновлён: {}", format_background_job_time(run.updated_at)),
        ];
        lines.extend(render_session_schedule_lines(&summary));
        lines.extend(render_session_interagent_lines(interagent.as_ref()));
        if let Some(usage) = run.latest_provider_usage.as_ref() {
            lines.push(format!(
                "- usage: input={} output={} total={}",
                usage.input_tokens, usage.output_tokens, usage.total_tokens
            ));
        }
        let step_tail = active_run_step_tail(self, &run);
        if let Some(step) = step_tail.last() {
            lines.push(format!("- последний шаг: {}", step.detail));
            if step_tail.len() > 1 {
                lines.push("- предыдущие шаги:".to_string());
                for prior_step in step_tail[..step_tail.len() - 1].iter().rev() {
                    lines.push(format!("  - {}", prior_step.detail));
                }
            }
        }
        if !run.active_processes.is_empty() {
            lines.push("- активные процессы:".to_string());
            for process in &run.active_processes {
                lines.push(format!(
                    "  - {} ({}) {} c {}",
                    process.id,
                    process.kind,
                    process.pid_ref,
                    format_background_job_time(process.started_at)
                ));
                if let Some(command_display) = process.command_display.as_deref() {
                    lines.push(format!("    команда: {command_display}"));
                }
                if let Some(cwd) = process.cwd.as_deref() {
                    lines.push(format!("    cwd: {cwd}"));
                }
                if let Some(output_lines) =
                    render_active_process_output_tail(self, process.id.as_str())
                {
                    lines.push("    вывод:".to_string());
                    lines.extend(output_lines);
                }
            }
        }
        if let Some(error) = run.error.as_deref() {
            lines.push(format!("- ошибка: {error}"));
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
        let started = Instant::now();
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "delete_session.start",
            "deleting session",
        )
        .session_id(session_id.to_string())
        .emit(&self.persistence.audit);
        let store = self.store()?;
        let deleted = store.delete_session(session_id)?;
        if !deleted {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "delete_session.finish",
            "deleted session",
        )
        .session_id(session_id.to_string())
        .elapsed_ms(started.elapsed().as_millis() as u64)
        .outcome("ok")
        .emit(&self.persistence.audit);
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn clear_session(
        &self,
        session_id: &str,
        title: Option<&str>,
    ) -> Result<SessionSummary, BootstrapError> {
        let started = Instant::now();
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "clear_session.start",
            "clearing session by delete and recreate",
        )
        .session_id(session_id.to_string())
        .field("replacement_title", title)
        .emit(&self.persistence.audit);
        self.delete_session(session_id)?;
        let summary = self.create_session_auto(title)?;
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "clear_session.finish",
            "cleared session and created replacement",
        )
        .session_id(session_id.to_string())
        .elapsed_ms(started.elapsed().as_millis() as u64)
        .outcome("ok")
        .field("replacement_session_id", summary.id.clone())
        .emit(&self.persistence.audit);
        Ok(summary)
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
        let catalog = self.skill_catalog(&session.agent_profile_id)?;
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

    fn skill_catalog(
        &self,
        agent_profile_id: &str,
    ) -> Result<agent_runtime::skills::SkillCatalog, BootstrapError> {
        let agent_skills_dir =
            agents::agent_home(&self.config.data_dir, agent_profile_id).join("skills");
        scan_skill_catalog_with_overrides(
            &self.config.daemon.skills_dir,
            Some(agent_skills_dir.as_path()),
        )
        .map_err(|source| BootstrapError::Io {
            path: agent_skills_dir,
            source,
        })
    }
}

fn build_synthetic_run_lines(session_id: &str, runs: &[RunSnapshot]) -> Vec<SessionTranscriptLine> {
    let mut lines = Vec::new();
    for run in runs.iter().filter(|run| run.session_id == session_id) {
        let mut reasoning_started_at = None;
        let mut reasoning_buffer = String::new();
        for step in &run.recent_steps {
            match step.kind {
                RunStepKind::ProviderReasoningDelta => {
                    if reasoning_started_at.is_none() {
                        reasoning_started_at = Some(step.recorded_at);
                    }
                    let delta = parse_provider_reasoning_step(step.detail.as_str());
                    reasoning_buffer.push_str(delta.as_str());
                }
                RunStepKind::ToolCompleted => {
                    flush_reasoning_lines(
                        &mut lines,
                        run.id.as_str(),
                        &mut reasoning_started_at,
                        &mut reasoning_buffer,
                    );
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
                RunStepKind::SystemNote
                | RunStepKind::WaitingApproval
                | RunStepKind::ApprovalResolved
                | RunStepKind::WaitingProcess
                | RunStepKind::ProcessCompleted
                | RunStepKind::WaitingDelegate
                | RunStepKind::DelegateCompleted
                | RunStepKind::Cancelled
                | RunStepKind::Interrupted => {
                    flush_reasoning_lines(
                        &mut lines,
                        run.id.as_str(),
                        &mut reasoning_started_at,
                        &mut reasoning_buffer,
                    );
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
        flush_reasoning_lines(
            &mut lines,
            run.id.as_str(),
            &mut reasoning_started_at,
            &mut reasoning_buffer,
        );

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

fn active_run_step_tail<'a>(
    app: &App,
    run: &'a RunSnapshot,
) -> Vec<&'a agent_runtime::run::RunStep> {
    let relevant_steps = run
        .recent_steps
        .iter()
        .filter(|step| active_run_step_is_relevant(step.kind))
        .collect::<Vec<_>>();
    if relevant_steps.is_empty() {
        run.recent_steps
            .iter()
            .rev()
            .take(app.config.runtime_limits.active_run_step_tail_limit)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    } else {
        relevant_steps
            .into_iter()
            .rev()
            .take(app.config.runtime_limits.active_run_step_tail_limit)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

fn render_active_process_output_tail(app: &App, process_id: &str) -> Option<Vec<String>> {
    let output = app
        .processes
        .read_exec_output(
            process_id,
            ProcessOutputStream::Merged,
            None,
            Some(
                app.config
                    .runtime_limits
                    .active_process_output_tail_max_bytes,
            ),
            Some(
                app.config
                    .runtime_limits
                    .active_process_output_tail_max_lines,
            ),
        )
        .ok()?;
    let text = output.text.trim_end();
    if text.is_empty() {
        return None;
    }

    let mut lines = text
        .lines()
        .map(|line| format!("      {line}"))
        .collect::<Vec<_>>();
    if output.truncated {
        lines.insert(0, "      ...".to_string());
    }
    Some(lines)
}

fn render_session_schedule_lines(summary: &SessionSummary) -> Vec<String> {
    if let Some(schedule) = summary.schedule.as_ref() {
        let mut lines = vec![format!(
            "- расписание: {} mode={} delivery={} enabled={} next_fire_at={}",
            schedule.id,
            schedule.mode.as_str(),
            schedule.delivery_mode.as_str(),
            schedule.enabled,
            format_background_job_time(schedule.next_fire_at)
        )];
        if let Some(target_session_id) = schedule.target_session_id.as_deref() {
            lines.push(format!("  target_session: {target_session_id}"));
        }
        if let Some(last_result) = schedule.last_result.as_deref() {
            lines.push(format!("  last_result: {last_result}"));
        }
        if let Some(last_error) = schedule.last_error.as_deref() {
            lines.push(format!("  last_error: {last_error}"));
        }
        return lines;
    }
    summary
        .scheduled_by
        .as_deref()
        .map(|schedule_id| vec![format!("- расписание: {schedule_id}")])
        .unwrap_or_default()
}

pub(super) fn load_session_interagent_summary(
    store: &PersistenceStore,
    session_id: &str,
) -> Result<Option<SessionInteragentSummary>, BootstrapError> {
    let Some(session_record) = store.get_session(session_id)? else {
        return Ok(None);
    };
    let session = Session::try_from(session_record).map_err(BootstrapError::RecordConversion)?;
    let transcripts = store.list_transcripts_for_session(session_id)?;
    let latest_chain = transcripts
        .iter()
        .rev()
        .find_map(|record| AgentMessageChain::from_transcript_metadata(&record.content));
    if let Some(chain) = latest_chain {
        return Ok(Some(SessionInteragentSummary {
            chain_id: chain.chain_id.clone(),
            hop_count: Some(chain.hop_count),
            max_hops: Some(chain.max_hops),
            state: describe_interagent_chain_state(&chain.state).to_string(),
            origin_session_id: Some(chain.origin_session_id.clone()),
            origin_agent_id: Some(chain.origin_agent_id.clone()),
            target_agent_id: None,
            recipient_session_id: None,
            parent_interagent_session_id: chain.parent_interagent_session_id.clone(),
            parent_session_id: session.parent_session_id.clone(),
            delegation_label: session.delegation_label.clone(),
            continuation_grant_pending: store
                .get_agent_chain_continuation(&chain.chain_id)?
                .is_some(),
        }));
    }

    if let Some(summary) = transcripts
        .iter()
        .rev()
        .find_map(|record| parse_outbound_interagent_summary(record.content.as_str()))
    {
        return Ok(Some(SessionInteragentSummary {
            chain_id: summary.chain_id.clone(),
            hop_count: Some(summary.hop_count),
            max_hops: None,
            state: "queued".to_string(),
            origin_session_id: None,
            origin_agent_id: None,
            target_agent_id: Some(summary.target_agent_id),
            recipient_session_id: Some(summary.recipient_session_id),
            parent_interagent_session_id: None,
            parent_session_id: session.parent_session_id.clone(),
            delegation_label: session.delegation_label.clone(),
            continuation_grant_pending: store
                .get_agent_chain_continuation(&summary.chain_id)?
                .is_some(),
        }));
    }

    if let Some(chain_id) = session
        .delegation_label
        .as_deref()
        .and_then(|label| label.strip_prefix("agent-chain:"))
    {
        return Ok(Some(SessionInteragentSummary {
            chain_id: chain_id.to_string(),
            hop_count: None,
            max_hops: None,
            state: "delegated".to_string(),
            origin_session_id: None,
            origin_agent_id: None,
            target_agent_id: None,
            recipient_session_id: None,
            parent_interagent_session_id: None,
            parent_session_id: session.parent_session_id.clone(),
            delegation_label: session.delegation_label.clone(),
            continuation_grant_pending: store.get_agent_chain_continuation(chain_id)?.is_some(),
        }));
    }

    Ok(None)
}

fn render_session_interagent_lines(summary: Option<&SessionInteragentSummary>) -> Vec<String> {
    let Some(summary) = summary else {
        return Vec::new();
    };

    let mut headline = format!(
        "- межагент: chain_id={} state={}",
        summary.chain_id, summary.state
    );
    if let Some(hop_count) = summary.hop_count {
        match summary.max_hops {
            Some(max_hops) => {
                headline.push_str(&format!(" hop={hop_count}/{max_hops}"));
            }
            None => {
                headline.push_str(&format!(" hop={hop_count}"));
            }
        }
    }

    let mut lines = vec![headline];
    if let Some(origin_session_id) = summary.origin_session_id.as_deref() {
        lines.push(format!("  origin_session: {origin_session_id}"));
    }
    if let Some(origin_agent_id) = summary.origin_agent_id.as_deref() {
        lines.push(format!("  origin_agent: {origin_agent_id}"));
    }
    if let Some(target_agent_id) = summary.target_agent_id.as_deref() {
        lines.push(format!("  target_agent: {target_agent_id}"));
    }
    if let Some(recipient_session_id) = summary.recipient_session_id.as_deref() {
        lines.push(format!("  recipient_session: {recipient_session_id}"));
    }
    if let Some(parent_interagent_session_id) = summary.parent_interagent_session_id.as_deref() {
        lines.push(format!(
            "  parent_interagent_session: {parent_interagent_session_id}"
        ));
    }
    if let Some(parent_session_id) = summary.parent_session_id.as_deref() {
        lines.push(format!("  parent_session: {parent_session_id}"));
    }
    if let Some(delegation_label) = summary.delegation_label.as_deref() {
        lines.push(format!("  delegation_label: {delegation_label}"));
    }
    if summary.continuation_grant_pending {
        lines.push("  continuation_grant: pending".to_string());
    }
    lines
}

fn active_run_step_is_relevant(kind: RunStepKind) -> bool {
    matches!(
        kind,
        RunStepKind::ToolCompleted
            | RunStepKind::SystemNote
            | RunStepKind::WaitingApproval
            | RunStepKind::ApprovalResolved
            | RunStepKind::WaitingProcess
            | RunStepKind::ProcessCompleted
            | RunStepKind::WaitingDelegate
            | RunStepKind::DelegateCompleted
            | RunStepKind::Resumed
            | RunStepKind::EvidenceRecorded
            | RunStepKind::Failed
            | RunStepKind::Cancelled
            | RunStepKind::Interrupted
    )
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

#[derive(Debug, Clone)]
struct OutboundInteragentSummary {
    chain_id: String,
    hop_count: u32,
    target_agent_id: String,
    recipient_session_id: String,
}

fn describe_interagent_chain_state(state: &AgentChainState) -> &'static str {
    match state {
        AgentChainState::Active => "active",
        AgentChainState::BlockedMaxHops => "blocked_max_hops",
        AgentChainState::ContinuedOnce => "continued_once",
    }
}

fn parse_outbound_interagent_summary(content: &str) -> Option<OutboundInteragentSummary> {
    let body = content.strip_prefix("message_agent queued: ")?;
    let mut target_agent_id = None;
    let mut recipient_session_id = None;
    let mut chain_id = None;
    let mut hop_count = None;

    for token in body.split_whitespace() {
        let (key, value) = token.split_once('=')?;
        match key {
            "target" => target_agent_id = Some(value.to_string()),
            "recipient_session" => recipient_session_id = Some(value.to_string()),
            "chain_id" => chain_id = Some(value.to_string()),
            "hop_count" => hop_count = value.parse::<u32>().ok(),
            _ => {}
        }
    }

    Some(OutboundInteragentSummary {
        chain_id: chain_id?,
        hop_count: hop_count?,
        target_agent_id: target_agent_id?,
        recipient_session_id: recipient_session_id?,
    })
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
        || detail_lower.contains("tool error:")
        || detail_lower.contains("invalid arguments:")
        || detail_lower.contains("failed:")
    {
        "failed"
    } else {
        "completed"
    };
    (tool_name, status.to_string(), summary)
}

fn parse_provider_reasoning_step(detail: &str) -> String {
    detail
        .strip_prefix("provider reasoning: ")
        .unwrap_or(detail)
        .to_string()
}

fn flush_reasoning_lines(
    lines: &mut Vec<SessionTranscriptLine>,
    run_id: &str,
    reasoning_started_at: &mut Option<i64>,
    reasoning_buffer: &mut String,
) {
    let Some(created_at) = reasoning_started_at.take() else {
        return;
    };
    if reasoning_buffer.trim().is_empty() {
        reasoning_buffer.clear();
        return;
    }
    lines.push(SessionTranscriptLine {
        role: "reasoning".to_string(),
        content: reasoning_buffer.trim().to_string(),
        run_id: Some(run_id.to_string()),
        created_at,
        tool_name: None,
        tool_status: None,
        approval_id: None,
    });
    reasoning_buffer.clear();
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

#[cfg(test)]
mod tests {
    use super::{build_synthetic_run_lines, parse_tool_step_detail};
    use agent_runtime::run::{RunSnapshot, RunStatus, RunStep, RunStepKind};

    #[test]
    fn parse_tool_step_detail_marks_tool_error_as_failed() {
        let (tool_name, status, summary) = parse_tool_step_detail(
            "fs_read_text path=projects/adqm/infra/ansible/group_vars/all.yml tool error: execution tool error: workspace filesystem error at ./projects/adqm/infra/ansible/group_vars/all.yml: No such file or directory (os error 2)",
        );

        assert_eq!(tool_name, "fs_read_text");
        assert_eq!(status, "failed");
        assert!(summary.contains("tool error:"));
    }

    #[test]
    fn build_synthetic_run_lines_marks_tool_error_as_failed() {
        let run = RunSnapshot {
            id: "run-1".to_string(),
            session_id: "session-1".to_string(),
            status: RunStatus::Running,
            started_at: 100,
            updated_at: 101,
            recent_steps: vec![RunStep {
                kind: RunStepKind::ToolCompleted,
                detail: "fs_read_text path=projects/adqm/infra/ansible/group_vars/all.yml tool error: execution tool error: workspace filesystem error at ./projects/adqm/infra/ansible/group_vars/all.yml: No such file or directory (os error 2)".to_string(),
                recorded_at: 101,
            }],
            ..RunSnapshot::default()
        };

        let lines = build_synthetic_run_lines("session-1", &[run]);
        let tool_line = lines
            .iter()
            .find(|line| line.role == "tool")
            .expect("synthetic tool transcript line");

        assert_eq!(tool_line.tool_name.as_deref(), Some("fs_read_text"));
        assert_eq!(tool_line.tool_status.as_deref(), Some("failed"));
        assert!(tool_line.content.contains("tool error:"));
    }
}
