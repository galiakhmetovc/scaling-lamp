use super::*;
use crate::agents;
use crate::diagnostics::{DiagnosticEventBuilder, render_diagnostic_tail};
use agent_persistence::{ContextOffloadRepository, RunRepository};
use agent_runtime::context::CompactionPolicy;
use agent_runtime::plan::PlanSnapshot;
use agent_runtime::prompt::{PromptAssembly, PromptAssemblyInput, SessionHead};
use agent_runtime::provider::ProviderMessage;
use agent_runtime::run::{PendingProviderApproval, RunSnapshot};
use agent_runtime::session::{MessageRole, Session};
use agent_runtime::skills::{
    parse_skill_document, resolve_session_skill_status, scan_skill_catalog_with_overrides,
};
use agent_runtime::tool::{
    KnowledgeReadInput, KnowledgeReadOutput, KnowledgeSearchInput, KnowledgeSearchOutput,
    SessionReadInput, SessionReadOutput, SessionSearchInput, SessionSearchOutput,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn load_session_head_metadata(
    store: &PersistenceStore,
    session: &Session,
) -> Result<
    (
        String,
        Option<agent_runtime::prompt::SessionHeadScheduleSummary>,
    ),
    BootstrapError,
> {
    let agent_name = store
        .get_agent_profile(&session.agent_profile_id)?
        .map(|record| record.name)
        .unwrap_or_else(|| session.agent_profile_id.clone());
    let schedule = session
        .delegation_label
        .as_deref()
        .and_then(|label| label.strip_prefix("agent-schedule:"))
        .map(str::to_string)
        .map(|schedule_id| {
            store
                .get_agent_schedule(&schedule_id)?
                .map(AgentSchedule::try_from)
                .transpose()
                .map_err(BootstrapError::RecordConversion)
                .map(|maybe| maybe.map(SessionScheduleSummary::from))
        })
        .transpose()?
        .flatten()
        .as_ref()
        .map(crate::bootstrap::session_head_schedule_summary);
    Ok((agent_name, schedule))
}

impl App {
    fn compaction_policy(&self) -> CompactionPolicy {
        CompactionPolicy {
            min_messages: self.config.context.compaction_min_messages,
            keep_tail_messages: self.config.context.compaction_keep_tail_messages,
            max_output_tokens: self.config.context.compaction_max_output_tokens,
            max_summary_chars: self.config.context.compaction_max_summary_chars,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn session_head(&self, session_id: &str) -> Result<SessionHead, BootstrapError> {
        let store = self.store()?;
        let session = Session::try_from(store.get_session(session_id)?.ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            }
        })?)
        .map_err(BootstrapError::RecordConversion)?;
        let transcripts = store.list_transcripts_for_session(session_id)?;
        let context_summary = store
            .get_context_summary(session_id)?
            .map(ContextSummary::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)?;
        let runs = store
            .list_runs_for_session(session_id)?
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;
        let (agent_name, schedule) = load_session_head_metadata(&store, &session)?;
        let workspace = agent_runtime::workspace::WorkspaceRef::new(&session.workspace_root);

        Ok(prompting::build_session_head(
            &session,
            &agent_name,
            schedule,
            &transcripts,
            context_summary.as_ref(),
            &runs,
            &workspace,
        ))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn pending_approvals(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionPendingApproval>, BootstrapError> {
        let started = Instant::now();
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "pending_approvals.start",
            "loading pending approvals for session",
        )
        .session_id(session_id.to_string())
        .emit(&self.persistence.audit);
        let store = self.store()?;
        let runs_started = Instant::now();
        let runs = store
            .list_runs_for_session(session_id)?
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "pending_approvals.loaded_runs",
            "loaded session-scoped runs for pending approvals",
        )
        .session_id(session_id.to_string())
        .elapsed_ms(runs_started.elapsed().as_millis() as u64)
        .outcome("ok")
        .field("run_count", runs.len())
        .emit(&self.persistence.audit);
        let mut pending = Vec::new();

        for run in runs {
            if run.session_id != session_id
                || run.status != agent_runtime::run::RunStatus::WaitingApproval
            {
                continue;
            }
            for approval in run.pending_approvals {
                pending.push(SessionPendingApproval {
                    run_id: run.id.clone(),
                    approval_id: approval.id,
                    reason: approval.reason,
                    requested_at: approval.requested_at,
                });
            }
        }

        pending.sort_by_key(|approval| (approval.requested_at, approval.approval_id.clone()));
        DiagnosticEventBuilder::new(
            &self.config,
            "info",
            "session_ops",
            "pending_approvals.finish",
            "loaded pending approvals for session",
        )
        .session_id(session_id.to_string())
        .elapsed_ms(started.elapsed().as_millis() as u64)
        .outcome("ok")
        .field("approval_count", pending.len())
        .emit(&self.persistence.audit);
        Ok(pending)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn latest_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<SessionPendingApproval>, BootstrapError> {
        let pending = self.pending_approvals(session_id)?;
        if let Some(requested) = requested_approval_id {
            return Ok(pending
                .into_iter()
                .find(|approval| approval.approval_id == requested));
        }

        Ok(pending.into_iter().max_by(|left, right| {
            left.requested_at
                .cmp(&right.requested_at)
                .then_with(|| left.approval_id.cmp(&right.approval_id))
        }))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn context_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<ContextSummary>, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }

        store
            .get_context_summary(session_id)?
            .map(ContextSummary::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn plan_snapshot(&self, session_id: &str) -> Result<PlanSnapshot, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }

        Ok(store
            .get_plan(session_id)?
            .map(PlanSnapshot::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)?
            .unwrap_or_else(|| PlanSnapshot {
                session_id: session_id.to_string(),
                goal: None,
                items: Vec::new(),
                updated_at: 0,
            }))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        let snapshot = self.plan_snapshot(session_id)?;
        if snapshot.is_empty() {
            return Ok("план пуст".to_string());
        }

        let mut lines = vec!["План:".to_string()];
        if let Some(goal) = snapshot.goal {
            lines.push(format!("Цель: {goal}"));
        }
        for item in snapshot.items {
            lines.push(format!(
                "- [{}] {}: {}",
                item.status.as_str(),
                item.id,
                item.content
            ));
            if !item.depends_on.is_empty() {
                lines.push(format!("  зависит_от: {}", item.depends_on.join(", ")));
            }
            if let Some(blocked_reason) = item.blocked_reason {
                lines.push(format!("  причина_блокировки: {blocked_reason}"));
            }
            if let Some(parent_task_id) = item.parent_task_id {
                lines.push(format!("  родительская_задача: {parent_task_id}"));
            }
            for note in item.notes {
                lines.push(format!("  заметка: {note}"));
            }
        }
        Ok(lines.join("\n"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_session_memory_search(
        &self,
        input: SessionSearchInput,
    ) -> Result<String, BootstrapError> {
        let store = self.store()?;
        let output = self
            .execution_service()
            .search_sessions(&store, &input)
            .map_err(BootstrapError::Execution)?;
        Ok(render_session_search_output(&output))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_session_memory_read(
        &self,
        input: SessionReadInput,
    ) -> Result<String, BootstrapError> {
        let store = self.store()?;
        let output = self
            .execution_service()
            .read_session(&store, &input)
            .map_err(BootstrapError::Execution)?;
        Ok(render_session_read_output(&output))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_knowledge_search(
        &self,
        input: KnowledgeSearchInput,
    ) -> Result<String, BootstrapError> {
        let store = self.store()?;
        let output = self
            .execution_service()
            .search_knowledge(&store, &input)
            .map_err(BootstrapError::Execution)?;
        Ok(render_knowledge_search_output(&output))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_knowledge_read(
        &self,
        input: KnowledgeReadInput,
    ) -> Result<String, BootstrapError> {
        let store = self.store()?;
        let output = self
            .execution_service()
            .read_knowledge(&store, &input)
            .map_err(BootstrapError::Execution)?;
        Ok(render_knowledge_read_output(&output))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_context_state(&self, session_id: &str) -> Result<String, BootstrapError> {
        let store = self.store()?;
        let session = Session::try_from(store.get_session(session_id)?.ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            }
        })?)
        .map_err(BootstrapError::RecordConversion)?;
        let transcripts = store.list_transcripts_for_session(session_id)?;
        let context_summary = store
            .get_context_summary(session_id)?
            .map(ContextSummary::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)?;
        let context_offload = store
            .get_context_offload(session_id)?
            .map(agent_runtime::context::ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)?;
        let runs = store
            .list_runs_for_session(session_id)?
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;
        let (agent_name, schedule) = load_session_head_metadata(&store, &session)?;
        let workspace = agent_runtime::workspace::WorkspaceRef::new(&session.workspace_root);
        let session_head = prompting::build_session_head(
            &session,
            &agent_name,
            schedule,
            &transcripts,
            context_summary.as_ref(),
            &runs,
            &workspace,
        );
        let policy = self.compaction_policy();
        let uncovered_messages = transcripts.len().saturating_sub(
            context_summary
                .as_ref()
                .map_or(0, |summary| summary.covered_message_count as usize),
        );
        let provider_usage = crate::bootstrap::latest_provider_usage(&runs, &session.id);
        let offload_refs = context_offload
            .as_ref()
            .map_or(0usize, |snapshot| snapshot.refs.len());
        let offload_tokens = context_offload
            .as_ref()
            .map_or(0u32, |snapshot| snapshot.total_token_estimate());
        let offload_messages = context_offload
            .as_ref()
            .map_or(0u32, total_offload_message_count);

        let mut lines = vec![
            "Context:".to_string(),
            format!("session_id={}", session.id),
            match provider_usage {
                Some(ref usage) => format!(
                    "usage=input:{} output:{} total:{} (from latest provider run)",
                    usage.input_tokens, usage.output_tokens, usage.total_tokens
                ),
                None => format!(
                    "usage=<нет>; approx_ctx={} (tail + summary fallback)",
                    session_head.context_tokens
                ),
            },
            format!("messages_total={}", transcripts.len()),
            format!("messages_uncovered={uncovered_messages}"),
            format!(
                "summary_tokens={}",
                context_summary
                    .as_ref()
                    .map_or(0u32, |summary| summary.summary_token_estimate)
            ),
            format!("offload_tokens={offload_tokens}"),
            format!("offload_refs={offload_refs}"),
            format!("offload_messages={offload_messages}"),
            format!("compactifications={}", session.settings.compactifications),
            format!(
                "compaction_manual_available=true auto_trigger_ratio={:.2} context_window_override={}",
                self.config.context.auto_compaction_trigger_ratio,
                self.config
                    .context
                    .context_window_tokens_override
                    .map_or_else(|| "<resolver>".to_string(), |value| value.to_string())
            ),
            format!(
                "threshold_messages={} keep_tail={}",
                policy.min_messages, policy.keep_tail_messages
            ),
        ];

        if let Some(summary) = context_summary.as_ref() {
            lines.push(format!(
                "summary_covers_messages={}",
                summary.covered_message_count
            ));
            lines.push(format!("summary_updated_at={}", summary.updated_at));
        } else {
            lines.push("summary_covers_messages=0".to_string());
            lines.push("summary_updated_at=<none>".to_string());
        }
        if let Some(snapshot) = context_offload.as_ref() {
            lines.push(format!("offload_updated_at={}", snapshot.updated_at));
            lines.extend(render_offload_snapshot_lines(snapshot));
        } else {
            lines.push("offload_updated_at=<none>".to_string());
        }

        Ok(lines.join("\n"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_system_blocks(&self, session_id: &str) -> Result<String, BootstrapError> {
        let store = self.store()?;
        let session = Session::try_from(store.get_session(session_id)?.ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            }
        })?)
        .map_err(BootstrapError::RecordConversion)?;
        let transcripts = store.list_transcripts_for_session(session_id)?;
        let transcript_entries = transcripts
            .iter()
            .cloned()
            .map(agent_runtime::session::TranscriptEntry::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;
        let context_summary = store
            .get_context_summary(session_id)?
            .map(ContextSummary::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)?;
        let context_offload = store
            .get_context_offload(session_id)?
            .map(agent_runtime::context::ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)?;
        let plan_snapshot = store
            .get_plan(session_id)?
            .map(PlanSnapshot::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)?;
        let runs = store
            .list_runs_for_session(session_id)?
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;
        let (agent_name, schedule) = load_session_head_metadata(&store, &session)?;
        let workspace = agent_runtime::workspace::WorkspaceRef::new(&session.workspace_root);
        let session_head = prompting::build_session_head(
            &session,
            &agent_name,
            schedule,
            &transcripts,
            context_summary.as_ref(),
            &runs,
            &workspace,
        );
        let system_prompt =
            prompting::load_system_prompt(&self.config.data_dir, &session.agent_profile_id);
        let agents_prompt =
            prompting::load_agents_prompt(&self.config.data_dir, &session.agent_profile_id);
        let agent_skills_dir =
            agents::agent_home(&self.config.data_dir, &session.agent_profile_id).join("skills");
        let skills_catalog = scan_skill_catalog_with_overrides(
            &self.config.daemon.skills_dir,
            Some(agent_skills_dir.as_path()),
        )
        .map_err(|source| BootstrapError::Io {
            path: agent_skills_dir.clone(),
            source,
        })?;
        let active_skill_status = resolve_session_skill_status(
            &skills_catalog,
            &session.settings,
            &session.title,
            &transcript_entries,
        );
        let agent_profile = self.agent_profile(&session.agent_profile_id)?;
        let interagent = super::session_ops::load_session_interagent_summary(&store, session_id)?;

        let mut lines = vec![
            "Системные блоки:".to_string(),
            format!(
                "Агент: {} ({}) | home={}",
                agent_profile.name,
                agent_profile.id,
                agent_profile.agent_home.display()
            ),
            String::new(),
            "Порядок prompt assembly:".to_string(),
            "1. SYSTEM.md".to_string(),
            "2. AGENTS.md".to_string(),
            "3. active skill prompts".to_string(),
            "4. SessionHead".to_string(),
            "5. Plan".to_string(),
            "6. ContextSummary".to_string(),
            "7. offload refs".to_string(),
            "8. uncovered transcript tail".to_string(),
            String::new(),
            "[SYSTEM.md]".to_string(),
            system_prompt,
            String::new(),
            "[AGENTS.md]".to_string(),
            agents_prompt.unwrap_or_else(|| "<none>".to_string()),
            String::new(),
            "[InterAgent]".to_string(),
            String::new(),
            "[SessionHead]".to_string(),
            session_head.render(),
            String::new(),
            "[Plan]".to_string(),
        ];

        match interagent.as_ref() {
            Some(summary) => {
                lines.insert(
                    lines.len().saturating_sub(4),
                    format!(
                        "continuation_grant_pending={}",
                        summary.continuation_grant_pending
                    ),
                );
                if let Some(delegation_label) = summary.delegation_label.as_deref() {
                    lines.insert(
                        lines.len().saturating_sub(4),
                        format!("delegation_label={delegation_label}"),
                    );
                }
                if let Some(parent_session_id) = summary.parent_session_id.as_deref() {
                    lines.insert(
                        lines.len().saturating_sub(4),
                        format!("parent_session_id={parent_session_id}"),
                    );
                }
                if let Some(parent_interagent_session_id) =
                    summary.parent_interagent_session_id.as_deref()
                {
                    lines.insert(
                        lines.len().saturating_sub(4),
                        format!("parent_interagent_session_id={parent_interagent_session_id}"),
                    );
                }
                if let Some(recipient_session_id) = summary.recipient_session_id.as_deref() {
                    lines.insert(
                        lines.len().saturating_sub(4),
                        format!("recipient_session_id={recipient_session_id}"),
                    );
                }
                if let Some(target_agent_id) = summary.target_agent_id.as_deref() {
                    lines.insert(
                        lines.len().saturating_sub(4),
                        format!("target_agent_id={target_agent_id}"),
                    );
                }
                if let Some(origin_agent_id) = summary.origin_agent_id.as_deref() {
                    lines.insert(
                        lines.len().saturating_sub(4),
                        format!("origin_agent_id={origin_agent_id}"),
                    );
                }
                if let Some(origin_session_id) = summary.origin_session_id.as_deref() {
                    lines.insert(
                        lines.len().saturating_sub(4),
                        format!("origin_session_id={origin_session_id}"),
                    );
                }
                lines.insert(
                    lines.len().saturating_sub(4),
                    format!(
                        "chain_id={} state={} hop={} max_hops={}",
                        summary.chain_id,
                        summary.state,
                        summary
                            .hop_count
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "<unknown>".to_string()),
                        summary
                            .max_hops
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "<unknown>".to_string())
                    ),
                );
            }
            None => lines.insert(lines.len().saturating_sub(4), "<none>".to_string()),
        }

        match plan_snapshot {
            Some(snapshot) if !snapshot.is_empty() => lines.push(snapshot.system_message_text()),
            _ => lines.push("<none>".to_string()),
        }

        lines.push(String::new());
        lines.push("[ContextSummary]".to_string());
        match context_summary {
            Some(summary) if !summary.summary_text.trim().is_empty() => {
                lines.push(summary.system_message_text())
            }
            _ => lines.push("<none>".to_string()),
        }

        lines.push(String::new());
        lines.push("[OffloadRefs]".to_string());
        match context_offload {
            Some(snapshot) if !snapshot.refs.is_empty() => {
                for reference in snapshot.refs {
                    lines.push(format!(
                        "- [{}] {} | artifact_id={} | tokens={} | messages={} | summary={}",
                        reference.id,
                        reference.label,
                        reference.artifact_id,
                        reference.token_estimate,
                        reference.message_count,
                        reference.summary
                    ));
                }
            }
            _ => lines.push("<none>".to_string()),
        }

        lines.push(String::new());
        lines.push("[Active Skill Prompts]".to_string());
        let active_names = active_skill_status
            .iter()
            .filter(|skill| {
                matches!(
                    skill.mode,
                    agent_runtime::skills::SkillActivationMode::Automatic
                        | agent_runtime::skills::SkillActivationMode::Manual
                )
            })
            .map(|skill| skill.name.clone())
            .collect::<Vec<_>>();
        if active_names.is_empty() {
            lines.push("<none>".to_string());
        } else {
            for skill in skills_catalog.entries.iter().filter(|entry| {
                active_names
                    .iter()
                    .any(|candidate| entry.name.eq_ignore_ascii_case(candidate))
            }) {
                let contents = std::fs::read_to_string(&skill.skill_md_path).map_err(|source| {
                    BootstrapError::Io {
                        path: skill.skill_md_path.clone(),
                        source,
                    }
                })?;
                let document =
                    parse_skill_document(&skill.skill_md_path, &contents).map_err(|reason| {
                        BootstrapError::Usage {
                            reason: format!(
                                "invalid skill document {}: {reason}",
                                skill.skill_md_path.display()
                            ),
                        }
                    })?;
                lines.push(format!("[SKILL:{}]", skill.name));
                if document.body.trim().is_empty() {
                    lines.push("<empty>".to_string());
                } else {
                    lines.push(document.body);
                }
                lines.push(String::new());
            }
        }

        let assembled = PromptAssembly::build_messages(PromptAssemblyInput {
            system_prompt: Some(prompting::load_system_prompt(
                &self.config.data_dir,
                &session.agent_profile_id,
            )),
            agents_prompt: prompting::load_agents_prompt(
                &self.config.data_dir,
                &session.agent_profile_id,
            ),
            active_skill_prompts: prompting::load_active_skill_prompts(
                &skills_catalog,
                &active_skill_status,
            ),
            session_head: Some(session_head),
            plan_snapshot: store
                .get_plan(session_id)?
                .map(PlanSnapshot::try_from)
                .transpose()
                .map_err(BootstrapError::RecordConversion)?,
            context_summary: store
                .get_context_summary(session_id)?
                .map(ContextSummary::try_from)
                .transpose()
                .map_err(BootstrapError::RecordConversion)?,
            context_offload: store
                .get_context_offload(session_id)?
                .map(agent_runtime::context::ContextOffloadSnapshot::try_from)
                .transpose()
                .map_err(BootstrapError::RecordConversion)?,
            transcript_messages: transcripts
                .iter()
                .map(|record| {
                    let role = MessageRole::try_from(record.kind.as_str()).map_err(|_| {
                        BootstrapError::RecordConversion(
                            RecordConversionError::InvalidMessageRole {
                                value: record.kind.clone(),
                            },
                        )
                    })?;
                    Ok::<ProviderMessage, BootstrapError>(ProviderMessage {
                        role,
                        content: record.content.clone(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        });
        lines.push("[Assembled Prompt Messages]".to_string());
        for (index, message) in assembled.into_iter().enumerate() {
            lines.push(format!("{}. [{}]", index + 1, message.role.as_str()));
            lines.push(message.content);
            lines.push(String::new());
        }

        Ok(lines.join("\n"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_session_artifacts(&self, session_id: &str) -> Result<String, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        let snapshot = store
            .get_context_offload(session_id)?
            .map(agent_runtime::context::ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)?;

        let Some(snapshot) = snapshot else {
            return Ok("Артефакты: нет".to_string());
        };
        if snapshot.refs.is_empty() {
            return Ok("Артефакты: нет".to_string());
        }

        let open_hint = snapshot
            .refs
            .first()
            .map(|reference| {
                format!(
                    "подсказка: используйте \\артефакт {}, чтобы открыть конкретный payload",
                    reference.artifact_id
                )
            })
            .unwrap_or_else(|| {
                "подсказка: используйте \\артефакт <artifact_id>, чтобы открыть конкретный payload"
                    .to_string()
            });
        let mut lines = vec![
            "Артефакты:".to_string(),
            format!(
                "refs={} tokens={} messages={} updated_at={}",
                snapshot.refs.len(),
                snapshot.total_token_estimate(),
                total_offload_message_count(&snapshot),
                snapshot.updated_at
            ),
            open_hint,
        ];
        for reference in snapshot.refs {
            lines.push(format!(
                "- {} [{}] {}",
                reference.artifact_id, reference.id, reference.label
            ));
            lines.push(format!("  summary: {}", reference.summary));
            lines.push(format!(
                "  tokens={} messages={} created_at={}",
                reference.token_estimate, reference.message_count, reference.created_at
            ));
        }
        Ok(lines.join("\n"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn read_session_artifact(
        &self,
        session_id: &str,
        artifact_id: &str,
    ) -> Result<String, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        let snapshot = store
            .get_context_offload(session_id)?
            .map(agent_runtime::context::ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)?
            .ok_or_else(|| BootstrapError::Usage {
                reason: "в этой сессии нет offload-артефактов".to_string(),
            })?;
        let reference = snapshot
            .refs
            .into_iter()
            .find(|reference| reference.artifact_id == artifact_id)
            .ok_or_else(|| BootstrapError::Usage {
                reason: format!("артефакт {artifact_id} не найден в текущей сессии"),
            })?;
        let payload = store
            .get_context_offload_payload(artifact_id)?
            .ok_or_else(|| BootstrapError::Usage {
                reason: format!("payload для артефакта {artifact_id} отсутствует"),
            })?;
        Ok([
            format!("artifact_id={}", reference.artifact_id),
            format!("ref_id={}", reference.id),
            format!("label={}", reference.label),
            format!("summary={}", reference.summary),
            format!(
                "tokens={} messages={} created_at={}",
                reference.token_estimate, reference.message_count, reference.created_at
            ),
            String::new(),
            String::from_utf8_lossy(&payload.bytes).to_string(),
        ]
        .join("\n"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_provider_request_preview(
        &self,
        session_id: &str,
    ) -> Result<String, BootstrapError> {
        let store = self.store()?;
        let provider = self.provider_driver()?;
        let request = self
            .execution_service()
            .build_provider_request_preview(&store, provider.as_ref(), session_id)
            .map_err(BootstrapError::Execution)?;
        agent_runtime::provider::render_http_request_preview(&self.config.provider, &request)
            .map_err(BootstrapError::ProviderRequest)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn compact_session(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        let transcript_count = store.list_transcripts_for_session(session_id)?.len();
        let policy = CompactionPolicy {
            min_messages: self.config.context.compaction_min_messages,
            keep_tail_messages: self.config.context.compaction_keep_tail_messages,
            max_output_tokens: self.config.context.compaction_max_output_tokens,
            max_summary_chars: self.config.context.compaction_max_summary_chars,
        };
        if !policy.should_compact(transcript_count) {
            return self.session_summary(session_id);
        }
        let provider = self.provider_driver()?;
        self.execution_service()
            .compact_session_at(&store, provider.as_ref(), session_id, unix_timestamp()?)
            .map_err(BootstrapError::Execution)?;
        self.session_summary(session_id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn write_debug_bundle(&self, session_id: &str) -> Result<PathBuf, BootstrapError> {
        let bundle = self.render_debug_bundle(session_id)?;
        let path = self.debug_bundle_output_path(session_id)?;
        write_debug_bundle_file(&path, bundle.as_str())?;
        Ok(path)
    }

    fn debug_bundle_output_path(&self, session_id: &str) -> Result<PathBuf, BootstrapError> {
        Ok(self
            .config
            .data_dir
            .join("audit")
            .join("debug-bundles")
            .join(format!(
                "{}-{}.txt",
                sanitize_debug_filename(session_id),
                unique_timestamp_token()?
            )))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_diagnostics_tail(&self, max_lines: usize) -> Result<String, BootstrapError> {
        render_diagnostic_tail(&self.persistence.audit, max_lines)
    }

    fn render_debug_bundle(&self, session_id: &str) -> Result<String, BootstrapError> {
        let store = self.store()?;
        let session_record =
            store
                .get_session(session_id)?
                .ok_or_else(|| BootstrapError::MissingRecord {
                    kind: "session",
                    id: session_id.to_string(),
                })?;
        let summary = self.session_summary(session_id)?;
        let transcript = self.session_transcript(session_id)?;
        let context = self.render_context_state(session_id)?;
        let session_head = self.session_head(session_id)?;
        let system_blocks = self.render_system_blocks(session_id)?;
        let plan = self.render_plan(session_id)?;
        let jobs = self.render_session_background_jobs(session_id)?;
        let skills = self.render_session_skills(session_id)?;
        let pending_approvals = self.pending_approvals(session_id)?;
        let provider_http_preview = self
            .render_provider_request_preview(session_id)
            .unwrap_or_else(|error| format!("<unavailable: {error}>"));
        let diagnostics_tail = self
            .render_diagnostics_tail(self.config.runtime_limits.diagnostic_tail_lines)
            .unwrap_or_else(|error| format!("<unavailable: {error}>"));
        let runs = store
            .list_runs_for_session(session_id)?
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;
        let session_runs = runs;

        let mut lines = vec![
            "Debug Bundle".to_string(),
            format!("generated_at={}", unix_timestamp()?),
            format!("version={}", crate::about::APP_VERSION),
            format!("workspace_root={}", session_record.workspace_root),
            format!("data_dir={}", self.config.data_dir.display()),
            format!("state_db={}", self.persistence.stores.metadata_db.display()),
            String::new(),
            "Session Summary:".to_string(),
            format!("session_id={}", summary.id),
            format!("title={}", summary.title),
            format!(
                "agent={} ({})",
                summary.agent_name, summary.agent_profile_id
            ),
            format!(
                "model={}",
                summary.model.unwrap_or_else(|| "<default>".to_string())
            ),
            format!("reasoning_visible={}", summary.reasoning_visible),
            format!(
                "think_level={}",
                summary
                    .think_level
                    .unwrap_or_else(|| "<default>".to_string())
            ),
            match (
                summary.usage_input_tokens,
                summary.usage_output_tokens,
                summary.usage_total_tokens,
            ) {
                (Some(input), Some(output), Some(total)) => {
                    format!("usage=input:{input} output:{output} total:{total}")
                }
                _ => format!("approx_ctx={}", summary.context_tokens),
            },
            format!("messages={}", summary.message_count),
            format!(
                "background_jobs={} running={} queued={}",
                summary.background_job_count,
                summary.running_background_job_count,
                summary.queued_background_job_count
            ),
            format!("has_pending_approval={}", summary.has_pending_approval),
            format!("compactifications={}", summary.compactifications),
            format!("completion_nudges={:?}", summary.completion_nudges),
            format!("auto_approve={}", summary.auto_approve),
            String::new(),
            "Session Detail:".to_string(),
            format!("agent_profile_id={}", session_record.agent_profile_id),
            format!("prompt_override={:?}", session_record.prompt_override),
            format!("settings_json={}", session_record.settings_json),
            format!("active_mission_id={:?}", session_record.active_mission_id),
            format!("parent_session_id={:?}", session_record.parent_session_id),
            format!("parent_job_id={:?}", session_record.parent_job_id),
            format!("delegation_label={:?}", session_record.delegation_label),
            String::new(),
            "Session Head:".to_string(),
            session_head.render(),
            String::new(),
            "Context:".to_string(),
            context,
            String::new(),
            "System Blocks:".to_string(),
            system_blocks,
            String::new(),
            "Plan:".to_string(),
            plan,
            String::new(),
            "Jobs:".to_string(),
            jobs,
            String::new(),
            "Skills:".to_string(),
            skills,
            String::new(),
            "Pending Approvals:".to_string(),
        ];

        if pending_approvals.is_empty() {
            lines.push("- none".to_string());
        } else {
            for approval in pending_approvals {
                lines.push(format!(
                    "- run_id={} approval_id={} requested_at={} reason={}",
                    approval.run_id, approval.approval_id, approval.requested_at, approval.reason
                ));
            }
        }

        lines.push(String::new());
        lines.push("Runs:".to_string());
        if session_runs.is_empty() {
            lines.push("- none".to_string());
        } else {
            for run in session_runs.iter().rev().take(8).rev() {
                lines.extend(render_debug_run(run));
            }
        }

        lines.push(String::new());
        lines.push("Provider HTTP Preview:".to_string());
        lines.push(provider_http_preview);

        lines.push(String::new());
        lines.push("Diagnostic Tail:".to_string());
        lines.push(diagnostics_tail);

        lines.push(String::new());
        lines.push(format!(
            "Transcript Tail: total_entries={}",
            transcript.entries.len()
        ));
        for entry in transcript.entries.iter().rev().take(80).rev() {
            let descriptor = match entry.role.as_str() {
                "tool" => format!(
                    "tool:{}:{}",
                    entry.tool_name.as_deref().unwrap_or("tool"),
                    entry.tool_status.as_deref().unwrap_or("completed")
                ),
                "approval" => format!(
                    "approval:{}",
                    entry.approval_id.as_deref().unwrap_or("approval")
                ),
                role => role.to_string(),
            };
            lines.push(format!(
                "- [{}] {} {}",
                entry.created_at,
                descriptor,
                entry.content.replace('\n', "\\n")
            ));
        }

        Ok(lines.join("\n"))
    }
}

fn total_offload_message_count(snapshot: &agent_runtime::context::ContextOffloadSnapshot) -> u32 {
    snapshot
        .refs
        .iter()
        .map(|reference| reference.message_count)
        .sum()
}

fn render_offload_snapshot_lines(
    snapshot: &agent_runtime::context::ContextOffloadSnapshot,
) -> Vec<String> {
    if snapshot.refs.is_empty() {
        return vec!["Offload: none".to_string()];
    }
    let mut lines = vec!["Offload:".to_string()];
    for reference in &snapshot.refs {
        lines.push(format!(
            "- [{}] {} | artifact_id={} | tokens={} | messages={} | summary={}",
            reference.id,
            reference.label,
            reference.artifact_id,
            reference.token_estimate,
            reference.message_count,
            reference.summary
        ));
    }
    lines
}

fn render_session_search_output(output: &SessionSearchOutput) -> String {
    if output.results.is_empty() {
        return format!("Память сессий: ничего не найдено\nquery={}", output.query);
    }

    let mut lines = vec![
        "Память сессий:".to_string(),
        format!(
            "query={} offset={} limit={} total={} next_offset={} truncated={}",
            output.query,
            output.offset,
            output.limit,
            output.total_results,
            output
                .next_offset
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            output.truncated
        ),
    ];
    for result in &output.results {
        lines.push(format!(
            "- {} | title={} | agent={} | tier={} | source={} | updated_at={}",
            result.session_id,
            result.title,
            result.agent_profile_id,
            result.tier.as_str(),
            result.match_source.as_str(),
            result.updated_at
        ));
        lines.push(format!("  snippet: {}", result.snippet));
    }
    lines.join("\n")
}

fn render_session_read_output(output: &SessionReadOutput) -> String {
    let mut lines = vec![
        "Память сессии:".to_string(),
        format!(
            "session_id={} title={} agent={} mode={} tier={} from_archive={} cursor={} next_cursor={} total_items={} truncated={}",
            output.session_id,
            output.title,
            output.agent_profile_id,
            output.mode.as_str(),
            output.tier.as_str(),
            output.from_archive,
            output.cursor,
            output
                .next_cursor
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            output.total_items,
            output.truncated
        ),
    ];
    if let Some(summary) = output.summary.as_ref() {
        lines.push(format!(
            "summary covered={} tokens={} updated_at={}",
            summary.covered_message_count, summary.summary_token_estimate, summary.updated_at
        ));
        lines.push(summary.summary_text.clone());
    }
    for message in &output.messages {
        lines.push(format!(
            "- [{}] {}: {}",
            message.created_at, message.role, message.content
        ));
    }
    for artifact in &output.artifacts {
        lines.push(format!(
            "- artifact {} | kind={} | bytes={} | created_at={} | path={}",
            artifact.artifact_id,
            artifact.kind,
            artifact.byte_len,
            artifact.created_at,
            artifact.path
        ));
        if let Some(label) = artifact.label.as_ref() {
            lines.push(format!("  label: {label}"));
        }
        if let Some(summary) = artifact.summary.as_ref() {
            lines.push(format!("  summary: {summary}"));
        }
    }
    lines.join("\n")
}

fn render_knowledge_search_output(output: &KnowledgeSearchOutput) -> String {
    if output.results.is_empty() {
        return format!("Память знаний: ничего не найдено\nquery={}", output.query);
    }

    let mut lines = vec![
        "Память знаний:".to_string(),
        format!(
            "query={} offset={} limit={} total={} next_offset={} truncated={}",
            output.query,
            output.offset,
            output.limit,
            output.total_results,
            output
                .next_offset
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            output.truncated
        ),
    ];
    for result in &output.results {
        lines.push(format!(
            "- {} | kind={} | mtime={}",
            result.path,
            result.kind.as_str(),
            result.mtime
        ));
        lines.push(format!("  snippet: {}", result.snippet));
    }
    lines.join("\n")
}

fn render_knowledge_read_output(output: &KnowledgeReadOutput) -> String {
    [
        "Файл знаний:".to_string(),
        format!(
            "path={} kind={} mode={} lines={}-{} total_lines={} cursor={} next_cursor={} truncated={} mtime={}",
            output.path,
            output.kind.as_str(),
            output.mode.as_str(),
            output.start_line,
            output.end_line,
            output.total_lines,
            output.cursor,
            output
                .next_cursor
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            output.truncated,
            output.mtime
        ),
        format!("sha256={}", output.sha256),
        String::new(),
        output.text.clone(),
    ]
    .join("\n")
}

fn sanitize_debug_filename(session_id: &str) -> String {
    session_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn write_debug_bundle_file(path: &Path, content: &str) -> Result<(), BootstrapError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| BootstrapError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::write(path, content).map_err(|source| BootstrapError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn render_debug_run(run: &RunSnapshot) -> Vec<String> {
    let mut lines = vec![format!(
        "- id={} status={} started_at={} updated_at={} finished_at={:?}",
        run.id,
        run.status.as_str(),
        run.started_at,
        run.updated_at,
        run.finished_at
    )];
    if let Some(error) = run.error.as_ref() {
        lines.push(format!("  error={error}"));
    }
    if let Some(result) = run.result.as_ref() {
        lines.push(format!("  result={}", result.replace('\n', "\\n")));
    }
    if let Some(stream) = run.provider_stream.as_ref() {
        lines.push(format!(
            "  provider_stream=response_id={} model={} output_len={} updated_at={}",
            stream.response_id,
            stream.model,
            stream.output_text.len(),
            stream.updated_at
        ));
    }
    if let Some(loop_state) = run.provider_loop.as_ref() {
        lines.push(format!(
            "  provider_loop=next_round:{} previous_response_id:{:?} pending_tool_outputs:{} continuation_messages:{} seen_tool_signatures:{} completion_nudges_used:{} pending_approval:{}",
            loop_state.next_round,
            loop_state.previous_response_id,
            loop_state.pending_tool_outputs.len(),
            loop_state.continuation_input_messages.len(),
            loop_state.seen_tool_signatures.len(),
            loop_state.completion_nudges_used,
            describe_pending_provider_approval(loop_state.pending_approval.as_ref())
        ));
    }
    if !run.pending_approvals.is_empty() {
        for approval in &run.pending_approvals {
            lines.push(format!(
                "  pending_approval id={} tool_call_id={} requested_at={} reason={}",
                approval.id, approval.tool_call_id, approval.requested_at, approval.reason
            ));
        }
    }
    if !run.active_processes.is_empty() {
        for process in &run.active_processes {
            lines.push(format!(
                "  active_process id={} kind={} pid_ref={} started_at={}",
                process.id, process.kind, process.pid_ref, process.started_at
            ));
        }
    }
    if !run.delegate_runs.is_empty() {
        for delegate in &run.delegate_runs {
            lines.push(format!(
                "  delegate_run id={} label={} started_at={}",
                delegate.id, delegate.label, delegate.started_at
            ));
        }
    }
    if !run.recent_steps.is_empty() {
        lines.push("  recent_steps:".to_string());
        for step in run.recent_steps.iter().rev().take(12).rev() {
            lines.push(format!(
                "    - [{}] {:?}: {}",
                step.recorded_at, step.kind, step.detail
            ));
        }
    }
    lines
}

fn describe_pending_provider_approval(pending: Option<&PendingProviderApproval>) -> String {
    match pending {
        None => "none".to_string(),
        Some(PendingProviderApproval::Tool(approval)) => {
            format!("tool:{}:{}", approval.tool_name, approval.approval_id)
        }
        Some(PendingProviderApproval::LoopReset(approval)) => format!(
            "loop_reset:{} rounds={}/{}",
            approval.approval_id, approval.exhausted_rounds, approval.max_rounds
        ),
        Some(PendingProviderApproval::CompletionNudge(approval)) => format!(
            "completion:{} nudges={}/{}",
            approval.approval_id, approval.completion_nudges_used, approval.max_completion_nudges
        ),
        Some(PendingProviderApproval::ProviderRetry(approval)) => format!(
            "provider_retry:{} error={}",
            approval.approval_id, approval.error_summary
        ),
    }
}
