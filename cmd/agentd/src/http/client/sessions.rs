use super::*;
use crate::bootstrap::{AgentScheduleCreateOptions, AgentScheduleUpdatePatch, AgentScheduleView};
use crate::http::types::{
    AgentCreateRequest, AgentRenderResponse, AgentResolveRequest, AgentScheduleCreateRequest,
    AgentScheduleDetailResponse, AgentScheduleResolveRequest, AgentScheduleUpdateRequest,
    AgentSelectRequest, ClearSessionRequest, CreateSessionRequest, DebugBundleResponse,
    MemoryRenderResponse, SessionAgentMessageRequest, SessionArtifactResponse,
    SessionArtifactsResponse, SessionBackgroundJobsResponse, SessionChainGrantRequest,
    SessionDetailResponse, SessionRunControlResponse, SessionRunStatusResponse,
    SessionSystemResponse, SkillCommandRequest,
};
use agent_runtime::tool::{
    KnowledgeReadInput, KnowledgeSearchInput, SessionReadInput, SessionSearchInput,
};

impl DaemonClient {
    pub fn render_agents(&self) -> Result<String, BootstrapError> {
        let response: AgentRenderResponse = self.get_json("/v1/agents")?;
        Ok(response.message)
    }

    pub fn render_current_agent(&self) -> Result<String, BootstrapError> {
        let response: AgentRenderResponse = self.get_json("/v1/agents/current")?;
        Ok(response.message)
    }

    pub fn render_agent(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        let response: AgentRenderResponse = self.post_json(
            "/v1/agents/show",
            &AgentResolveRequest {
                identifier: identifier.map(str::to_string),
            },
        )?;
        Ok(response.message)
    }

    pub fn select_agent(&self, identifier: &str) -> Result<String, BootstrapError> {
        let response: AgentRenderResponse = self.post_json(
            "/v1/agents/select",
            &AgentSelectRequest {
                identifier: identifier.to_string(),
            },
        )?;
        Ok(response.message)
    }

    pub fn create_agent(
        &self,
        name: &str,
        template_identifier: Option<&str>,
    ) -> Result<String, BootstrapError> {
        let response: AgentRenderResponse = self.post_json(
            "/v1/agents",
            &AgentCreateRequest {
                name: name.to_string(),
                template_identifier: template_identifier.map(str::to_string),
            },
        )?;
        Ok(response.message)
    }

    pub fn open_agent_home(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        let response: AgentRenderResponse = self.post_json(
            "/v1/agents/open",
            &AgentResolveRequest {
                identifier: identifier.map(str::to_string),
            },
        )?;
        Ok(response.message)
    }

    pub fn render_agent_schedules(&self) -> Result<String, BootstrapError> {
        let response: AgentRenderResponse = self.get_json("/v1/agent-schedules")?;
        Ok(response.message)
    }

    pub fn render_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
        let response: AgentRenderResponse = self.post_json(
            "/v1/agent-schedules/show",
            &AgentScheduleResolveRequest { id: id.to_string() },
        )?;
        Ok(response.message)
    }

    pub fn create_agent_schedule(
        &self,
        id: &str,
        interval_seconds: u64,
        prompt: &str,
        agent_identifier: Option<&str>,
    ) -> Result<String, BootstrapError> {
        self.create_agent_schedule_with_options(
            id,
            AgentScheduleCreateOptions {
                agent_identifier: agent_identifier.map(str::to_string),
                prompt: prompt.to_string(),
                mode: agent_runtime::agent::AgentScheduleMode::Interval,
                delivery_mode: agent_runtime::agent::AgentScheduleDeliveryMode::FreshSession,
                target_session_id: None,
                interval_seconds,
                enabled: true,
            },
        )
    }

    pub fn create_agent_schedule_with_options(
        &self,
        id: &str,
        options: AgentScheduleCreateOptions,
    ) -> Result<String, BootstrapError> {
        let response: AgentRenderResponse = self.post_json(
            "/v1/agent-schedules",
            &AgentScheduleCreateRequest {
                id: id.to_string(),
                options,
            },
        )?;
        Ok(response.message)
    }

    pub fn resolve_agent_schedule(&self, id: &str) -> Result<AgentScheduleView, BootstrapError> {
        let response: AgentScheduleDetailResponse = self.post_json(
            "/v1/agent-schedules/resolve",
            &AgentScheduleResolveRequest { id: id.to_string() },
        )?;
        Ok(response.schedule)
    }

    pub fn update_agent_schedule(
        &self,
        id: &str,
        patch: AgentScheduleUpdatePatch,
    ) -> Result<String, BootstrapError> {
        let response: AgentRenderResponse = self.patch_json(
            &format!("/v1/agent-schedules/{id}"),
            &AgentScheduleUpdateRequest { patch },
        )?;
        Ok(response.message)
    }

    pub fn delete_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
        let response: AgentRenderResponse =
            self.delete_json(&format!("/v1/agent-schedules/{id}"))?;
        Ok(response.message)
    }

    pub fn create_session_auto(
        &self,
        title: Option<&str>,
    ) -> Result<SessionSummary, BootstrapError> {
        self.create_session(None, title)
    }

    pub fn create_session(
        &self,
        id: Option<&str>,
        title: Option<&str>,
    ) -> Result<SessionSummary, BootstrapError> {
        let session: SessionSummaryResponse = self.post_json(
            "/v1/sessions",
            &CreateSessionRequest {
                id: id.map(str::to_string),
                title: title.map(str::to_string),
            },
        )?;
        Ok(session.into())
    }

    pub fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError> {
        let sessions: Vec<SessionSummaryResponse> = self.get_json("/v1/sessions")?;
        Ok(sessions.into_iter().map(SessionSummary::from).collect())
    }

    pub fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<SessionSummary, BootstrapError> {
        let summary: SessionSummaryResponse =
            self.patch_json(&format!("/v1/sessions/{session_id}/preferences"), &patch)?;
        Ok(summary.into())
    }

    pub fn delete_session(&self, session_id: &str) -> Result<(), BootstrapError> {
        let _: serde_json::Value = self.delete_json(&format!("/v1/sessions/{session_id}"))?;
        Ok(())
    }

    pub fn clear_session(
        &self,
        session_id: &str,
        title: Option<&str>,
    ) -> Result<SessionSummary, BootstrapError> {
        let summary: SessionSummaryResponse = self.post_json(
            &format!("/v1/sessions/{session_id}/clear"),
            &ClearSessionRequest {
                title: title.map(str::to_string),
            },
        )?;
        Ok(summary.into())
    }

    pub fn send_agent_message(
        &self,
        session_id: &str,
        target_agent_id: &str,
        message: &str,
    ) -> Result<String, BootstrapError> {
        let response: SessionRunControlResponse = self.post_json(
            &format!("/v1/sessions/{session_id}/agent-message"),
            &SessionAgentMessageRequest {
                target_agent_id: target_agent_id.to_string(),
                message: message.to_string(),
            },
        )?;
        Ok(response.message)
    }

    pub fn grant_chain_continuation(
        &self,
        session_id: &str,
        chain_id: &str,
        reason: &str,
    ) -> Result<String, BootstrapError> {
        let response: SessionRunControlResponse = self.post_json(
            &format!("/v1/sessions/{session_id}/chain-grant"),
            &SessionChainGrantRequest {
                chain_id: chain_id.to_string(),
                reason: reason.to_string(),
            },
        )?;
        Ok(response.message)
    }

    pub fn session_summary(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        let summary: SessionSummaryResponse =
            self.get_json(&format!("/v1/sessions/{session_id}"))?;
        Ok(summary.into())
    }

    pub fn session_detail(
        &self,
        session_id: &str,
    ) -> Result<SessionDetailResponse, BootstrapError> {
        self.get_json(&format!("/v1/sessions/{session_id}/detail"))
    }

    pub fn session_transcript(
        &self,
        session_id: &str,
    ) -> Result<SessionTranscriptView, BootstrapError> {
        self.get_json(&format!("/v1/sessions/{session_id}/transcript"))
    }

    pub fn session_transcript_tail(
        &self,
        session_id: &str,
        max_entries: usize,
    ) -> Result<SessionTranscriptView, BootstrapError> {
        self.get_json(&format!(
            "/v1/sessions/{session_id}/transcript-tail/{max_entries}"
        ))
    }

    pub fn pending_approvals(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionPendingApproval>, BootstrapError> {
        self.get_json(&format!("/v1/sessions/{session_id}/approvals"))
    }

    pub fn session_skills(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        self.get_json(&format!("/v1/sessions/{session_id}/skills"))
    }

    pub fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        self.post_json(
            &format!("/v1/sessions/{session_id}/skills/enable"),
            &SkillCommandRequest {
                name: skill_name.to_string(),
            },
        )
    }

    pub fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
        self.post_json(
            &format!("/v1/sessions/{session_id}/skills/disable"),
            &SkillCommandRequest {
                name: skill_name.to_string(),
            },
        )
    }

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

    pub fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        let value: serde_json::Value = self.get_json(&format!("/v1/sessions/{session_id}/plan"))?;
        value
            .get("plan")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| BootstrapError::Stream(std::io::Error::other("missing plan field")))
    }

    pub fn render_session_memory_search(
        &self,
        input: SessionSearchInput,
    ) -> Result<String, BootstrapError> {
        let response: MemoryRenderResponse = self.post_json("/v1/memory/session-search", &input)?;
        Ok(response.memory)
    }

    pub fn render_session_memory_read(
        &self,
        input: SessionReadInput,
    ) -> Result<String, BootstrapError> {
        let response: MemoryRenderResponse = self.post_json("/v1/memory/session-read", &input)?;
        Ok(response.memory)
    }

    pub fn render_knowledge_search(
        &self,
        input: KnowledgeSearchInput,
    ) -> Result<String, BootstrapError> {
        let response: MemoryRenderResponse =
            self.post_json("/v1/memory/knowledge-search", &input)?;
        Ok(response.memory)
    }

    pub fn render_knowledge_read(
        &self,
        input: KnowledgeReadInput,
    ) -> Result<String, BootstrapError> {
        let response: MemoryRenderResponse = self.post_json("/v1/memory/knowledge-read", &input)?;
        Ok(response.memory)
    }

    pub fn render_context_state(&self, session_id: &str) -> Result<String, BootstrapError> {
        let value: serde_json::Value =
            self.get_json(&format!("/v1/sessions/{session_id}/context"))?;
        value
            .get("context")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| BootstrapError::Stream(std::io::Error::other("missing context field")))
    }

    pub fn render_system_blocks(&self, session_id: &str) -> Result<String, BootstrapError> {
        let response: SessionSystemResponse =
            self.get_json(&format!("/v1/sessions/{session_id}/system"))?;
        Ok(response.system)
    }

    pub fn render_session_artifacts(&self, session_id: &str) -> Result<String, BootstrapError> {
        let response: SessionArtifactsResponse =
            self.get_json(&format!("/v1/sessions/{session_id}/artifacts"))?;
        Ok(response.artifacts)
    }

    pub fn read_session_artifact(
        &self,
        session_id: &str,
        artifact_id: &str,
    ) -> Result<String, BootstrapError> {
        let response: SessionArtifactResponse = self.get_json(&format!(
            "/v1/sessions/{session_id}/artifacts/{artifact_id}"
        ))?;
        Ok(response.artifact)
    }

    pub fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        let response: SessionRunStatusResponse =
            self.get_json(&format!("/v1/sessions/{session_id}/run"))?;
        Ok(response.run)
    }

    pub fn cancel_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        let response: SessionRunControlResponse = self.post_json(
            &format!("/v1/sessions/{session_id}/cancel-run"),
            &serde_json::json!({}),
        )?;
        Ok(response.message)
    }

    pub fn cancel_all_session_work(&self, session_id: &str) -> Result<String, BootstrapError> {
        let response: SessionRunControlResponse = self.post_json(
            &format!("/v1/sessions/{session_id}/cancel-all-work"),
            &serde_json::json!({}),
        )?;
        Ok(response.message)
    }

    pub fn session_background_jobs(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionBackgroundJob>, BootstrapError> {
        let jobs: SessionBackgroundJobsResponse =
            self.get_json(&format!("/v1/sessions/{session_id}/jobs"))?;
        Ok(jobs.into_iter().map(SessionBackgroundJob::from).collect())
    }

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
            lines.push(format!("  поставлена_в_очередь: {}", job.queued_at));
            if let Some(started_at) = job.started_at {
                lines.push(format!("  запущена: {started_at}"));
            }
            if let Some(progress) = job.last_progress_message {
                lines.push(format!("  прогресс: {progress}"));
            }
        }
        Ok(lines.join("\n"))
    }

    pub fn write_debug_bundle(&self, session_id: &str) -> Result<String, BootstrapError> {
        let response: DebugBundleResponse = self.post_json(
            &format!("/v1/sessions/{session_id}/debug-bundle"),
            &serde_json::json!({}),
        )?;
        Ok(response.path)
    }

    pub fn compact_session(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        let summary: SessionSummaryResponse = self.post_json_long(
            &format!("/v1/sessions/{session_id}/compact"),
            &serde_json::json!({}),
        )?;
        Ok(summary.into())
    }
}
