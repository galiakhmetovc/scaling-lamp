use super::*;
use crate::agents;
use agent_persistence::JobRepository;
use agent_runtime::mission::JobSpec;
use agent_runtime::run::{RunSnapshot, RunStatus, RunStepKind};
use agent_runtime::session::{
    MessageRole, Session, TranscriptEntry, parse_scheduled_input_metadata,
};
use agent_runtime::skills::{resolve_session_skill_status, scan_skill_catalog_with_overrides};
use agent_runtime::tool::ProcessOutputStream;
use std::collections::HashMap;
use time::OffsetDateTime;
use time::UtcOffset;
use time::format_description::well_known::Rfc3339;

const ACTIVE_RUN_STEP_TAIL_LIMIT: usize = 3;
const ACTIVE_PROCESS_OUTPUT_TAIL_MAX_BYTES: usize = 2 * 1024;
const ACTIVE_PROCESS_OUTPUT_TAIL_MAX_LINES: usize = 8;

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
            .map_err(BootstrapError::RecordConversion)?;
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

        let mut lines = vec![
            "Ход:".to_string(),
            format!("- id: {}", run.id),
            format!("- статус: {}", run.status.as_str()),
            format!("- начат: {}", format_background_job_time(run.started_at)),
            format!("- обновлён: {}", format_background_job_time(run.updated_at)),
        ];
        if let Some(usage) = run.latest_provider_usage.as_ref() {
            lines.push(format!(
                "- usage: input={} output={} total={}",
                usage.input_tokens, usage.output_tokens, usage.total_tokens
            ));
        }
        let step_tail = active_run_step_tail(&run);
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

fn active_run_step_tail(run: &RunSnapshot) -> Vec<&agent_runtime::run::RunStep> {
    let relevant_steps = run
        .recent_steps
        .iter()
        .filter(|step| active_run_step_is_relevant(step.kind))
        .collect::<Vec<_>>();
    if relevant_steps.is_empty() {
        run.recent_steps
            .iter()
            .rev()
            .take(ACTIVE_RUN_STEP_TAIL_LIMIT)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    } else {
        relevant_steps
            .into_iter()
            .rev()
            .take(ACTIVE_RUN_STEP_TAIL_LIMIT)
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
            Some(ACTIVE_PROCESS_OUTPUT_TAIL_MAX_BYTES),
            Some(ACTIVE_PROCESS_OUTPUT_TAIL_MAX_LINES),
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
