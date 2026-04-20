use crate::bootstrap::{
    BootstrapError, SessionPendingApproval, SessionPreferencesPatch, SessionSkillStatus,
    SessionSummary, SessionTranscriptView,
};
use crate::execution::{
    ApprovalContinuationReport, ChatExecutionEvent, ChatTurnExecutionReport, ExecutionError,
};
use crate::http::types::{
    ApproveRunRequest, ChatTurnRequest, ClearSessionRequest, CreateSessionRequest, ErrorResponse,
    SessionDetailResponse, SessionSummaryResponse, SkillCommandRequest, StatusResponse,
    WorkerOutcomeResponse,
};
use agent_persistence::AppConfig;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DaemonConnectOptions {
    pub host: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct DaemonClient {
    http: Client,
    base_url: String,
    bearer_token: Option<String>,
}

impl DaemonClient {
    pub fn new(config: &AppConfig, options: &DaemonConnectOptions) -> Self {
        let host = options
            .host
            .clone()
            .unwrap_or_else(|| config.daemon.bind_host.clone());
        let port = options.port.unwrap_or(config.daemon.bind_port);
        Self {
            http: Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(5))
                .build()
                .expect("build daemon http client"),
            base_url: format!("http://{host}:{port}"),
            bearer_token: config.daemon.bearer_token.clone(),
        }
    }

    pub fn status(&self) -> Result<StatusResponse, BootstrapError> {
        self.get_json("/v1/status")
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

    pub fn compact_session(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        let summary: SessionSummaryResponse = self.post_json(
            &format!("/v1/sessions/{session_id}/compact"),
            &serde_json::json!({}),
        )?;
        Ok(summary.into())
    }

    pub fn execute_chat_turn_with_control_and_observer(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        _interrupt_after_tool_step: Option<&AtomicBool>,
        _observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatTurnExecutionReport, BootstrapError> {
        match self.post_json::<WorkerOutcomeResponse, _>(
            "/v1/chat/turn",
            &ChatTurnRequest {
                session_id: session_id.to_string(),
                message: message.to_string(),
                now,
            },
        )? {
            WorkerOutcomeResponse::ChatCompleted { report } => Ok(report),
            WorkerOutcomeResponse::ApprovalRequired {
                approval_id,
                reason,
            } => Err(BootstrapError::Execution(
                ExecutionError::ApprovalRequired {
                    tool: "remote_tool".to_string(),
                    approval_id,
                    reason,
                },
            )),
            WorkerOutcomeResponse::InterruptedByQueuedInput => Err(BootstrapError::Execution(
                ExecutionError::InterruptedByQueuedInput,
            )),
            WorkerOutcomeResponse::Failed { reason } => Err(BootstrapError::Usage { reason }),
            WorkerOutcomeResponse::ApprovalCompleted { .. } => Err(BootstrapError::Usage {
                reason: "unexpected approval response for chat turn".to_string(),
            }),
        }
    }

    pub fn approve_run_with_control_and_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        now: i64,
        _interrupt_after_tool_step: Option<&AtomicBool>,
        _observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ApprovalContinuationReport, BootstrapError> {
        match self.post_json::<WorkerOutcomeResponse, _>(
            "/v1/runs/approve",
            &ApproveRunRequest {
                run_id: run_id.to_string(),
                approval_id: approval_id.to_string(),
                now,
            },
        )? {
            WorkerOutcomeResponse::ApprovalCompleted { report } => Ok(report),
            WorkerOutcomeResponse::ApprovalRequired {
                approval_id,
                reason: _,
            } => Ok(ApprovalContinuationReport {
                run_id: run_id.to_string(),
                run_status: agent_runtime::run::RunStatus::WaitingApproval,
                response_id: None,
                output_text: None,
                approval_id: Some(approval_id),
            }),
            WorkerOutcomeResponse::InterruptedByQueuedInput => Err(BootstrapError::Execution(
                ExecutionError::InterruptedByQueuedInput,
            )),
            WorkerOutcomeResponse::Failed { reason } => Err(BootstrapError::Usage { reason }),
            WorkerOutcomeResponse::ChatCompleted { .. } => Err(BootstrapError::Usage {
                reason: "unexpected chat response for approval continuation".to_string(),
            }),
        }
    }

    fn get_json<T>(&self, path: &str) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
    {
        let mut request = self.http.get(format!("{}{}", self.base_url, path));
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .map_err(|error| BootstrapError::Stream(std::io::Error::other(error.to_string())))?;
        decode_response(response)
    }

    fn post_json<T, B>(&self, path: &str, body: &B) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        let mut request = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .json(body);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .map_err(|error| BootstrapError::Stream(std::io::Error::other(error.to_string())))?;
        decode_response(response)
    }

    fn patch_json<T, B>(&self, path: &str, body: &B) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        let mut request = self
            .http
            .patch(format!("{}{}", self.base_url, path))
            .json(body);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .map_err(|error| BootstrapError::Stream(std::io::Error::other(error.to_string())))?;
        decode_response(response)
    }

    fn delete_json<T>(&self, path: &str) -> Result<T, BootstrapError>
    where
        T: DeserializeOwned,
    {
        let mut request = self.http.delete(format!("{}{}", self.base_url, path));
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .map_err(|error| BootstrapError::Stream(std::io::Error::other(error.to_string())))?;
        decode_response(response)
    }
}

pub fn connect_or_autospawn<F>(
    config: &AppConfig,
    options: &DaemonConnectOptions,
    spawn_local: F,
) -> Result<DaemonClient, BootstrapError>
where
    F: FnOnce() -> Result<(), BootstrapError>,
{
    let client = DaemonClient::new(config, options);
    if client.status().is_ok() {
        return Ok(client);
    }

    if options.host.is_some() || options.port.is_some() {
        return Err(BootstrapError::Usage {
            reason: format!("daemon {} is unavailable", client.base_url),
        });
    }

    spawn_local()?;
    for _ in 0..50 {
        if client.status().is_ok() {
            return Ok(client);
        }
        thread::sleep(Duration::from_millis(100));
    }
    client.status()?;
    Ok(client)
}

fn decode_response<T>(response: reqwest::blocking::Response) -> Result<T, BootstrapError>
where
    T: DeserializeOwned,
{
    let status = response.status();
    if status.is_success() {
        return response.json::<T>().map_err(|error| {
            BootstrapError::Stream(std::io::Error::other(format!(
                "invalid daemon response: {error}"
            )))
        });
    }

    let error = response
        .json::<ErrorResponse>()
        .ok()
        .map(|error| error.error);
    let reason = error.unwrap_or_else(|| {
        status
            .canonical_reason()
            .unwrap_or("daemon error")
            .to_string()
    });
    let kind = if status == StatusCode::UNAUTHORIZED {
        "daemon authorization failed"
    } else {
        "daemon request failed"
    };
    Err(BootstrapError::Usage {
        reason: format!("{kind}: {reason}"),
    })
}

impl From<SessionSummaryResponse> for SessionSummary {
    fn from(value: SessionSummaryResponse) -> Self {
        Self {
            id: value.id,
            title: value.title,
            model: value.model,
            reasoning_visible: value.reasoning_visible,
            think_level: value.think_level,
            compactifications: value.compactifications,
            context_tokens: value.context_tokens,
            has_pending_approval: value.has_pending_approval,
            last_message_preview: value.last_message_preview,
            message_count: value.message_count,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}
