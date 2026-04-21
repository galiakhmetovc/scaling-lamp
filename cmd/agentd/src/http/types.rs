use crate::bootstrap::{
    SessionBackgroundJob, SessionPendingApproval, SessionPreferencesPatch, SessionSkillStatus,
    SessionSummary, SessionTranscriptView,
};
use crate::execution::{ApprovalContinuationReport, ChatExecutionEvent, ChatTurnExecutionReport};
use agent_runtime::delegation::{DelegateResultPackage, DelegateWriteScope};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusResponse {
    pub ok: bool,
    pub bind_host: String,
    pub bind_port: u16,
    pub permission_mode: String,
    pub session_count: usize,
    pub mission_count: usize,
    pub run_count: usize,
    pub job_count: usize,
    pub components: usize,
    pub data_dir: String,
    pub state_db: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonStopResponse {
    pub stopping: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub id: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummaryResponse {
    pub id: String,
    pub title: String,
    pub model: Option<String>,
    pub reasoning_visible: bool,
    pub think_level: Option<String>,
    pub compactifications: u32,
    pub completion_nudges: Option<u32>,
    pub context_tokens: u32,
    pub has_pending_approval: bool,
    pub last_message_preview: Option<String>,
    pub message_count: usize,
    pub background_job_count: usize,
    pub running_background_job_count: usize,
    pub queued_background_job_count: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDetailResponse {
    pub id: String,
    pub title: String,
    pub prompt_override: Option<String>,
    pub settings_json: String,
    pub active_mission_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_job_id: Option<String>,
    pub delegation_label: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<SessionSummary> for SessionSummaryResponse {
    fn from(value: SessionSummary) -> Self {
        Self {
            id: value.id,
            title: value.title,
            model: value.model,
            reasoning_visible: value.reasoning_visible,
            think_level: value.think_level,
            compactifications: value.compactifications,
            completion_nudges: value.completion_nudges,
            context_tokens: value.context_tokens,
            has_pending_approval: value.has_pending_approval,
            last_message_preview: value.last_message_preview,
            message_count: value.message_count,
            background_job_count: value.background_job_count,
            running_background_job_count: value.running_background_job_count,
            queued_background_job_count: value.queued_background_job_count,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionBackgroundJobResponse {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub last_progress_message: Option<String>,
}

impl From<SessionBackgroundJob> for SessionBackgroundJobResponse {
    fn from(value: SessionBackgroundJob) -> Self {
        Self {
            id: value.id,
            kind: value.kind,
            status: value.status,
            queued_at: value.queued_at,
            started_at: value.started_at,
            last_progress_message: value.last_progress_message,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearSessionRequest {
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatTurnRequest {
    pub session_id: String,
    pub message: String,
    pub now: i64,
    #[serde(default)]
    pub interrupt_after_tool_step: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApproveRunRequest {
    pub run_id: String,
    pub approval_id: String,
    pub now: i64,
    #[serde(default)]
    pub interrupt_after_tool_step: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillCommandRequest {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2ACallbackTargetRequest {
    pub url: String,
    pub bearer_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2ADelegationCreateRequest {
    pub parent_session_id: String,
    pub parent_job_id: String,
    pub label: String,
    pub goal: String,
    pub bounded_context: Vec<String>,
    pub write_scope: DelegateWriteScope,
    pub expected_output: String,
    pub owner: String,
    pub callback: A2ACallbackTargetRequest,
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2ADelegationAcceptedResponse {
    pub accepted: bool,
    pub remote_session_id: String,
    pub remote_job_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum A2ADelegationCompletionOutcomeRequest {
    Completed {
        remote_session_id: String,
        remote_job_id: String,
        package: DelegateResultPackage,
    },
    Failed {
        remote_session_id: String,
        remote_job_id: String,
        reason: String,
    },
    Blocked {
        remote_session_id: String,
        remote_job_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2ADelegationCompletionRequest {
    pub outcome: A2ADelegationCompletionOutcomeRequest,
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkerOutcomeResponse {
    ChatCompleted { report: ChatTurnExecutionReport },
    ApprovalCompleted { report: ApprovalContinuationReport },
    ApprovalRequired { approval_id: String, reason: String },
    InterruptedByQueuedInput,
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkerStreamEventResponse {
    ChatEvent { event: ChatExecutionEvent },
    Finished { outcome: WorkerOutcomeResponse },
}

pub type SessionTranscriptResponse = SessionTranscriptView;
pub type SessionPendingApprovalsResponse = Vec<SessionPendingApproval>;
pub type SessionPreferencesRequest = SessionPreferencesPatch;
pub type SessionSkillsResponse = Vec<SessionSkillStatus>;
pub type SessionBackgroundJobsResponse = Vec<SessionBackgroundJobResponse>;
