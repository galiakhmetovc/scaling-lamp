use super::*;
use crate::http::types::{
    ClearSessionRequest, CreateSessionRequest, DebugBundleResponse, SessionBackgroundJobsResponse,
    SessionDetailResponse, SkillCommandRequest,
};

impl DaemonClient {
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

    pub fn render_context_state(&self, session_id: &str) -> Result<String, BootstrapError> {
        let value: serde_json::Value =
            self.get_json(&format!("/v1/sessions/{session_id}/context"))?;
        value
            .get("context")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| BootstrapError::Stream(std::io::Error::other("missing context field")))
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
            return Ok("jobs: none active".to_string());
        }

        let mut lines = vec!["Jobs:".to_string()];
        for job in jobs {
            lines.push(format!("- [{}] {} ({})", job.status, job.id, job.kind));
            lines.push(format!("  queued_at: {}", job.queued_at));
            if let Some(started_at) = job.started_at {
                lines.push(format!("  started_at: {started_at}"));
            }
            if let Some(progress) = job.last_progress_message {
                lines.push(format!("  progress: {progress}"));
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
        let summary: SessionSummaryResponse = self.post_json(
            &format!("/v1/sessions/{session_id}/compact"),
            &serde_json::json!({}),
        )?;
        Ok(summary.into())
    }
}
