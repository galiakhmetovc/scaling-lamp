use super::*;
use agent_runtime::context::CompactionPolicy;
use agent_runtime::plan::PlanSnapshot;
use agent_runtime::prompt::SessionHead;
use agent_runtime::provider::ProviderMessage;
use agent_runtime::run::RunSnapshot;
use agent_runtime::session::{MessageRole, Session};

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
}
