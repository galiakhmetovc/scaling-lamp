use super::*;
use agent_persistence::ContextOffloadRepository;
use agent_runtime::context::CompactionPolicy;
use agent_runtime::plan::PlanSnapshot;
use agent_runtime::prompt::SessionHead;
use agent_runtime::provider::ProviderMessage;
use agent_runtime::run::{PendingProviderApproval, RunSnapshot};
use agent_runtime::session::{MessageRole, Session};
use std::path::PathBuf;

impl App {
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
            .load_execution_state()?
            .runs
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;

        Ok(prompting::build_session_head(
            &session,
            &transcripts,
            context_summary.as_ref(),
            &runs,
            &self.runtime.workspace,
        ))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn pending_approvals(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionPendingApproval>, BootstrapError> {
        let snapshot = self.store()?.load_execution_state()?;
        let mut pending = Vec::new();

        for record in snapshot.runs {
            let run = RunSnapshot::try_from(record).map_err(BootstrapError::RecordConversion)?;
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
            return Ok("plan is empty".to_string());
        }

        let mut lines = vec!["Plan:".to_string()];
        if let Some(goal) = snapshot.goal {
            lines.push(format!("Goal: {goal}"));
        }
        for item in snapshot.items {
            lines.push(format!(
                "- [{}] {}: {}",
                item.status.as_str(),
                item.id,
                item.content
            ));
            if !item.depends_on.is_empty() {
                lines.push(format!("  depends_on: {}", item.depends_on.join(", ")));
            }
            if let Some(blocked_reason) = item.blocked_reason {
                lines.push(format!("  blocked_reason: {blocked_reason}"));
            }
            if let Some(parent_task_id) = item.parent_task_id {
                lines.push(format!("  parent_task_id: {parent_task_id}"));
            }
            for note in item.notes {
                lines.push(format!("  note: {note}"));
            }
        }
        Ok(lines.join("\n"))
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
            .load_execution_state()?
            .runs
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;
        let session_head = prompting::build_session_head(
            &session,
            &transcripts,
            context_summary.as_ref(),
            &runs,
            &self.runtime.workspace,
        );
        let policy = CompactionPolicy::default();
        let uncovered_messages = transcripts.len().saturating_sub(
            context_summary
                .as_ref()
                .map_or(0, |summary| summary.covered_message_count as usize),
        );
        let offload_refs = context_offload
            .as_ref()
            .map_or(0usize, |snapshot| snapshot.refs.len());
        let offload_tokens = context_offload
            .as_ref()
            .map_or(0u32, |snapshot| snapshot.total_token_estimate());

        let mut lines = vec![
            "Context:".to_string(),
            format!("session_id={}", session.id),
            format!("ctx={} (tail + summary only)", session_head.context_tokens),
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
            format!("compactifications={}", session.settings.compactifications),
            format!(
                "compaction_manual={} threshold_messages={} keep_tail={}",
                true, policy.min_messages, policy.keep_tail_messages
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

        Ok(lines.join("\n"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn compact_session(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        let store = self.store()?;
        let session_record =
            store
                .get_session(session_id)?
                .ok_or_else(|| BootstrapError::MissingRecord {
                    kind: "session",
                    id: session_id.to_string(),
                })?;
        let mut session =
            Session::try_from(session_record).map_err(BootstrapError::RecordConversion)?;
        let transcripts = store.list_transcripts_for_session(session_id)?;
        let policy = CompactionPolicy::default();

        if !policy.should_compact(transcripts.len()) {
            return self.session_summary(session_id);
        }

        let covered_message_count = policy.covered_message_count(transcripts.len());
        let summary_messages = transcripts
            .iter()
            .take(covered_message_count)
            .map(|record| {
                let role = MessageRole::try_from(record.kind.as_str()).map_err(|_| {
                    BootstrapError::RecordConversion(RecordConversionError::InvalidMessageRole {
                        value: record.kind.clone(),
                    })
                })?;
                Ok::<ProviderMessage, BootstrapError>(ProviderMessage {
                    role,
                    content: record.content.clone(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let provider = self.provider_driver()?;
        let response = provider.complete(&agent_runtime::provider::ProviderRequest {
            model: session.settings.model.clone(),
            instructions: Some(compaction_instructions()),
            messages: summary_messages,
            previous_response_id: None,
            continuation_messages: Vec::new(),
            tools: Vec::new(),
            tool_outputs: Vec::new(),
            max_output_tokens: Some(policy.max_output_tokens),
            stream: agent_runtime::provider::ProviderStreamMode::Disabled,
        })?;
        let now = unix_timestamp()?;
        let summary_text = policy.trim_summary_text(&response.output_text);
        let context_summary = ContextSummary {
            session_id: session.id.clone(),
            summary_text: summary_text.clone(),
            covered_message_count: covered_message_count as u32,
            summary_token_estimate: approximate_token_count(&summary_text),
            updated_at: now,
        };
        store.put_context_summary(&agent_persistence::ContextSummaryRecord::from(
            &context_summary,
        ))?;

        session.settings.compactifications += 1;
        session.updated_at = now;
        let session_record = agent_persistence::SessionRecord::try_from(&session)
            .map_err(BootstrapError::RecordConversion)?;
        store.put_session(&session_record)?;
        self.session_summary(session_id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn write_debug_bundle(&self, session_id: &str) -> Result<PathBuf, BootstrapError> {
        let bundle = self.render_debug_bundle(session_id)?;
        let relative_path = format!(
            ".teamd-debug/{}-{}.txt",
            sanitize_debug_filename(session_id),
            unique_timestamp_token()?
        );
        self.runtime
            .workspace
            .write_text(relative_path.as_str(), bundle.as_str())
            .map_err(map_workspace_error)?;
        self.runtime
            .workspace
            .resolve(relative_path.as_str())
            .map_err(map_workspace_error)
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
        let plan = self.render_plan(session_id)?;
        let jobs = self.render_session_background_jobs(session_id)?;
        let skills = self.render_session_skills(session_id)?;
        let pending_approvals = self.pending_approvals(session_id)?;
        let runs = store
            .load_execution_state()?
            .runs
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;
        let session_runs = runs
            .into_iter()
            .filter(|run| run.session_id == session_id)
            .collect::<Vec<_>>();

        let mut lines = vec![
            "Debug Bundle".to_string(),
            format!("generated_at={}", unix_timestamp()?),
            format!("workspace_root={}", self.runtime.workspace.root.display()),
            format!("data_dir={}", self.config.data_dir.display()),
            format!("state_db={}", self.persistence.stores.metadata_db.display()),
            String::new(),
            "Session Summary:".to_string(),
            format!("session_id={}", summary.id),
            format!("title={}", summary.title),
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
            format!("ctx={}", summary.context_tokens),
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
            format!("prompt_override={:?}", session_record.prompt_override),
            format!("settings_json={}", session_record.settings_json),
            format!("active_mission_id={:?}", session_record.active_mission_id),
            format!("parent_session_id={:?}", session_record.parent_session_id),
            format!("parent_job_id={:?}", session_record.parent_job_id),
            format!("delegation_label={:?}", session_record.delegation_label),
            String::new(),
            "Context:".to_string(),
            context,
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

fn map_workspace_error(error: agent_runtime::workspace::WorkspaceError) -> BootstrapError {
    match error {
        agent_runtime::workspace::WorkspaceError::InvalidPath { path, reason } => {
            BootstrapError::InvalidPath {
                path: PathBuf::from(path),
                reason,
            }
        }
        agent_runtime::workspace::WorkspaceError::Io { path, source } => {
            BootstrapError::Io { path, source }
        }
    }
}
