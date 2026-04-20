#![cfg_attr(not(test), allow(dead_code))]

mod chat;
mod mission;
mod provider_loop;
mod supervisor;
mod tools;

use agent_persistence::{
    ContextSummaryRepository, JobRecord, JobRepository, MissionRecord, MissionRepository,
    PersistenceStore, PlanRecord, PlanRepository, RecordConversionError, RunRecord, RunRepository,
    SessionRepository, StoreError, TranscriptRecord, TranscriptRepository,
};
use agent_runtime::mission::{
    JobExecutionInput, JobResult, JobSpec, JobStatus, MissionSpec, MissionStatus,
};
use agent_runtime::permission::{PermissionAction, PermissionConfig};
use agent_runtime::plan::PlanSnapshot;
use agent_runtime::provider::{ProviderDriver, ProviderError};
use agent_runtime::run::{RunEngine, RunSnapshot, RunStatus, RunTransitionError};
use agent_runtime::scheduler::{MissionVerificationSummary, SupervisorAction, SupervisorLoop};
use agent_runtime::session::Session;
use agent_runtime::tool::{ToolCall, ToolError};
use agent_runtime::verification::EvidenceBundle;
use agent_runtime::workspace::WorkspaceRef;
use std::error::Error;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorTickReport {
    pub actions: Vec<SupervisorAction>,
    pub queued_jobs: usize,
    pub dispatched_jobs: usize,
    pub blocked_jobs: usize,
    pub deferred_missions: usize,
    pub completed_missions: usize,
    pub budget_remaining: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionTurnExecutionReport {
    pub job_id: String,
    pub run_id: String,
    pub response_id: String,
    pub output_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTurnExecutionReport {
    pub session_id: String,
    pub run_id: String,
    pub response_id: String,
    pub output_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalContinuationReport {
    pub run_id: String,
    pub run_status: RunStatus,
    pub response_id: Option<String>,
    pub output_text: Option<String>,
    pub approval_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExecutionReport {
    pub job_id: String,
    pub run_id: String,
    pub run_status: RunStatus,
    pub approval_id: Option<String>,
    pub output_summary: Option<String>,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatExecutionEvent {
    ReasoningDelta(String),
    AssistantTextDelta(String),
    ToolStatus {
        tool_name: String,
        status: ToolExecutionStatus,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolExecutionStatus {
    Requested,
    WaitingApproval,
    Approved,
    Running,
    Completed,
    Failed,
}

impl ToolExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::WaitingApproval => "waiting_approval",
            Self::Approved => "approved",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ToolResumeRequest<'a> {
    pub job_id: &'a str,
    pub run_id: &'a str,
    pub approval_id: &'a str,
    pub tool_call: &'a ToolCall,
    pub workspace_root: &'a Path,
    pub evidence: Option<&'a EvidenceBundle>,
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionService {
    permissions: PermissionConfig,
    provider_max_output_tokens: Option<u32>,
    supervisor: SupervisorLoop,
    workspace: WorkspaceRef,
}

#[derive(Debug)]
pub enum ExecutionError {
    MissingJob {
        id: String,
    },
    MissingMission {
        id: String,
    },
    MissingRun {
        id: String,
    },
    MissingSession {
        id: String,
    },
    UnsupportedJobInput {
        id: String,
        kind: String,
    },
    PermissionDenied {
        tool: String,
        reason: String,
    },
    ApprovalRequired {
        tool: String,
        approval_id: String,
        reason: String,
    },
    InterruptedByQueuedInput,
    Provider(ProviderError),
    ProviderLoop {
        reason: String,
    },
    RecordConversion(RecordConversionError),
    RunTransition(RunTransitionError),
    Store(StoreError),
    ToolCallParse {
        name: String,
        reason: String,
    },
    Tool(ToolError),
}

impl Default for ExecutionService {
    fn default() -> Self {
        Self::new(PermissionConfig::default(), WorkspaceRef::default(), None)
    }
}

impl ExecutionService {
    pub fn new(
        permissions: PermissionConfig,
        workspace: WorkspaceRef,
        provider_max_output_tokens: Option<u32>,
    ) -> Self {
        Self {
            permissions,
            provider_max_output_tokens,
            supervisor: SupervisorLoop::default(),
            workspace,
        }
    }
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingJob { id } => write!(formatter, "execution job {id} was not found"),
            Self::MissingMission { id } => {
                write!(formatter, "execution mission {id} was not found")
            }
            Self::MissingRun { id } => write!(formatter, "execution run {id} was not found"),
            Self::MissingSession { id } => {
                write!(formatter, "execution session {id} was not found")
            }
            Self::UnsupportedJobInput { id, kind } => {
                write!(
                    formatter,
                    "execution job {id} has unsupported input for kind {kind}"
                )
            }
            Self::PermissionDenied { tool, reason } => {
                write!(
                    formatter,
                    "execution permission denied for {tool}: {reason}"
                )
            }
            Self::ApprovalRequired {
                tool,
                approval_id,
                reason,
            } => write!(
                formatter,
                "execution approval required for {tool} ({approval_id}): {reason}"
            ),
            Self::InterruptedByQueuedInput => {
                write!(formatter, "execution interrupted by queued user input")
            }
            Self::Provider(source) => write!(formatter, "execution provider error: {source}"),
            Self::ProviderLoop { reason } => {
                write!(formatter, "execution provider loop error: {reason}")
            }
            Self::RecordConversion(source) => {
                write!(formatter, "execution record conversion error: {source}")
            }
            Self::RunTransition(source) => {
                write!(formatter, "execution run transition error: {source}")
            }
            Self::Store(source) => write!(formatter, "execution store error: {source}"),
            Self::ToolCallParse { name, reason } => {
                write!(
                    formatter,
                    "execution failed to parse tool call {name}: {reason}"
                )
            }
            Self::Tool(source) => write!(formatter, "execution tool error: {source}"),
        }
    }
}

impl Error for ExecutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Provider(source) => Some(source),
            Self::RecordConversion(source) => Some(source),
            Self::RunTransition(source) => Some(source),
            Self::Store(source) => Some(source),
            Self::Tool(source) => Some(source),
            Self::MissingJob { .. }
            | Self::MissingMission { .. }
            | Self::MissingRun { .. }
            | Self::MissingSession { .. }
            | Self::PermissionDenied { .. }
            | Self::ApprovalRequired { .. }
            | Self::InterruptedByQueuedInput
            | Self::ProviderLoop { .. }
            | Self::ToolCallParse { .. }
            | Self::UnsupportedJobInput { .. } => None,
        }
    }
}
