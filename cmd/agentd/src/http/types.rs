use crate::bootstrap::{
    SessionPendingApproval, SessionPreferencesPatch, SessionSkillStatus, SessionSummary,
    SessionTranscriptView,
};
use crate::execution::{ApprovalContinuationReport, ChatTurnExecutionReport};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusResponse {
    pub ok: bool,
    pub bind_host: String,
    pub bind_port: u16,
    pub session_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateSessionRequest {
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
    pub context_tokens: u32,
    pub has_pending_approval: bool,
    pub last_message_preview: Option<String>,
    pub message_count: usize,
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
            context_tokens: value.context_tokens,
            has_pending_approval: value.has_pending_approval,
            last_message_preview: value.last_message_preview,
            message_count: value.message_count,
            created_at: value.created_at,
            updated_at: value.updated_at,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApproveRunRequest {
    pub run_id: String,
    pub approval_id: String,
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillCommandRequest {
    pub name: String,
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

pub type SessionTranscriptResponse = SessionTranscriptView;
pub type SessionPendingApprovalsResponse = Vec<SessionPendingApproval>;
pub type SessionPreferencesRequest = SessionPreferencesPatch;
pub type SessionSkillsResponse = Vec<SessionSkillStatus>;
