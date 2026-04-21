use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionInboxEventKind {
    JobCompleted,
    JobFailed,
    JobProgressed,
    JobBlocked,
    ApprovalNeeded,
    ExternalInputReceived,
    DelegationResultReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionInboxEventStatus {
    Queued,
    Claimed,
    Processed,
    Failed,
}

impl SessionInboxEventStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Claimed => "claimed",
            Self::Processed => "processed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInboxEventParseError {
    field: &'static str,
    value: String,
}

impl SessionInboxEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::JobCompleted => "job_completed",
            Self::JobFailed => "job_failed",
            Self::JobProgressed => "job_progressed",
            Self::JobBlocked => "job_blocked",
            Self::ApprovalNeeded => "approval_needed",
            Self::ExternalInputReceived => "external_input_received",
            Self::DelegationResultReady => "delegation_result_ready",
        }
    }
}

impl TryFrom<&str> for SessionInboxEventKind {
    type Error = SessionInboxEventParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "job_completed" => Ok(Self::JobCompleted),
            "job_failed" => Ok(Self::JobFailed),
            "job_progressed" => Ok(Self::JobProgressed),
            "job_blocked" => Ok(Self::JobBlocked),
            "approval_needed" => Ok(Self::ApprovalNeeded),
            "external_input_received" => Ok(Self::ExternalInputReceived),
            "delegation_result_ready" => Ok(Self::DelegationResultReady),
            other => Err(SessionInboxEventParseError {
                field: "kind",
                value: other.to_string(),
            }),
        }
    }
}

impl TryFrom<&str> for SessionInboxEventStatus {
    type Error = SessionInboxEventParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "queued" => Ok(Self::Queued),
            "claimed" => Ok(Self::Claimed),
            "processed" => Ok(Self::Processed),
            "failed" => Ok(Self::Failed),
            other => Err(SessionInboxEventParseError {
                field: "status",
                value: other.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionInboxEventPayload {
    JobCompleted {
        summary: String,
    },
    JobFailed {
        error: String,
    },
    JobProgressed {
        message: String,
    },
    JobBlocked {
        reason: String,
    },
    ApprovalNeeded {
        run_id: String,
        approval_id: String,
        reason: String,
    },
    ExternalInputReceived {
        source: String,
        summary: String,
    },
    DelegationResultReady {
        summary: String,
        artifact_refs: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInboxEvent {
    pub id: String,
    pub session_id: String,
    pub job_id: Option<String>,
    pub kind: SessionInboxEventKind,
    pub payload: SessionInboxEventPayload,
    pub status: SessionInboxEventStatus,
    pub created_at: i64,
    pub available_at: i64,
    pub claimed_at: Option<i64>,
    pub processed_at: Option<i64>,
    pub error: Option<String>,
}

impl SessionInboxEvent {
    pub fn new(
        id: impl Into<String>,
        session_id: impl Into<String>,
        job_id: Option<&str>,
        kind: SessionInboxEventKind,
        payload: SessionInboxEventPayload,
        created_at: i64,
    ) -> Self {
        Self {
            id: id.into(),
            session_id: session_id.into(),
            job_id: job_id.map(str::to_owned),
            kind,
            payload,
            status: SessionInboxEventStatus::Queued,
            created_at,
            available_at: created_at,
            claimed_at: None,
            processed_at: None,
            error: None,
        }
    }

    pub fn job_completed(
        id: impl Into<String>,
        session_id: impl Into<String>,
        job_id: Option<&str>,
        summary: impl Into<String>,
        created_at: i64,
    ) -> Self {
        Self::new(
            id,
            session_id,
            job_id,
            SessionInboxEventKind::JobCompleted,
            SessionInboxEventPayload::JobCompleted {
                summary: summary.into(),
            },
            created_at,
        )
    }

    pub fn job_failed(
        id: impl Into<String>,
        session_id: impl Into<String>,
        job_id: Option<&str>,
        error: impl Into<String>,
        created_at: i64,
    ) -> Self {
        Self::new(
            id,
            session_id,
            job_id,
            SessionInboxEventKind::JobFailed,
            SessionInboxEventPayload::JobFailed {
                error: error.into(),
            },
            created_at,
        )
    }

    pub fn job_blocked(
        id: impl Into<String>,
        session_id: impl Into<String>,
        job_id: Option<&str>,
        reason: impl Into<String>,
        created_at: i64,
    ) -> Self {
        Self::new(
            id,
            session_id,
            job_id,
            SessionInboxEventKind::JobBlocked,
            SessionInboxEventPayload::JobBlocked {
                reason: reason.into(),
            },
            created_at,
        )
    }

    pub fn delegation_result_ready(
        id: impl Into<String>,
        session_id: impl Into<String>,
        job_id: Option<&str>,
        summary: impl Into<String>,
        artifact_refs: Vec<String>,
        created_at: i64,
    ) -> Self {
        Self::new(
            id,
            session_id,
            job_id,
            SessionInboxEventKind::DelegationResultReady,
            SessionInboxEventPayload::DelegationResultReady {
                summary: summary.into(),
                artifact_refs,
            },
            created_at,
        )
    }

    pub fn mark_claimed(mut self, claimed_at: i64) -> Self {
        self.status = SessionInboxEventStatus::Claimed;
        self.claimed_at = Some(claimed_at);
        self
    }

    pub fn mark_processed(mut self, processed_at: i64) -> Self {
        self.status = SessionInboxEventStatus::Processed;
        self.processed_at = Some(processed_at);
        self
    }

    pub fn requeue(mut self, available_at: i64, error: impl Into<String>) -> Self {
        self.status = SessionInboxEventStatus::Queued;
        self.available_at = available_at;
        self.claimed_at = None;
        self.processed_at = None;
        self.error = Some(error.into());
        self
    }

    pub fn transcript_summary(&self) -> String {
        match &self.payload {
            SessionInboxEventPayload::JobCompleted { summary } => {
                format!("background job completed: {summary}")
            }
            SessionInboxEventPayload::JobFailed { error } => {
                format!("background job failed: {error}")
            }
            SessionInboxEventPayload::JobProgressed { message } => {
                format!("background job progressed: {message}")
            }
            SessionInboxEventPayload::JobBlocked { reason } => {
                format!("background job blocked: {reason}")
            }
            SessionInboxEventPayload::ApprovalNeeded {
                approval_id,
                reason,
                ..
            } => {
                format!("approval needed ({approval_id}): {reason}")
            }
            SessionInboxEventPayload::ExternalInputReceived { source, summary } => {
                format!("external input received from {source}: {summary}")
            }
            SessionInboxEventPayload::DelegationResultReady {
                summary,
                artifact_refs,
            } => {
                if artifact_refs.is_empty() {
                    format!("delegation result ready: {summary}")
                } else {
                    format!(
                        "delegation result ready: {summary} (artifacts: {})",
                        artifact_refs.join(", ")
                    )
                }
            }
        }
    }
}

impl std::fmt::Display for SessionInboxEventParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid session inbox event {}: {}",
            self.field, self.value
        )
    }
}

impl std::error::Error for SessionInboxEventParseError {}
