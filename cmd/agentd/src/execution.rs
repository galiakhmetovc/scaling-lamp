#![cfg_attr(not(test), allow(dead_code))]

mod autonomy;
mod background;
mod chat;
mod delegate_jobs;
mod delegation;
mod interagent;
mod mcp;
mod memory;
mod mission;
mod provider_loop;
mod supervisor;
mod tools;
mod wakeup;

use crate::a2a::A2AClient;
use crate::mcp::SharedMcpRegistry;
use agent_persistence::{
    A2APeerConfig, AgentRepository, ContextOffloadRepository, ContextSummaryRepository, JobRecord,
    JobRepository, McpRepository, MissionRecord, MissionRepository, PersistenceStore, PlanRecord,
    PlanRepository, RecordConversionError, RunRecord, RunRepository, RuntimeLimitsConfig,
    RuntimeTimingConfig, SessionInboxRepository, SessionRepository, StoreError, TranscriptRecord,
    TranscriptRepository,
};
use agent_runtime::agent::AgentProfile;
use agent_runtime::inbox::SessionInboxEvent;
use agent_runtime::mission::{
    JobExecutionInput, JobResult, JobSpec, JobStatus, MissionSpec, MissionStatus,
};
use agent_runtime::permission::{PermissionAction, PermissionConfig};
use agent_runtime::provider::{ProviderDriver, ProviderError};
use agent_runtime::run::{RunEngine, RunSnapshot, RunStatus, RunTransitionError};
use agent_runtime::scheduler::{MissionVerificationSummary, SupervisorAction, SupervisorLoop};
use agent_runtime::session::{Session, SessionSettings};
use agent_runtime::tool::{SharedProcessRegistry, ToolCall, ToolError, ToolName, ToolRuntime};
use agent_runtime::verification::EvidenceBundle;
use agent_runtime::workspace::WorkspaceRef;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static EXECUTION_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionTurnExecutionReport {
    pub job_id: String,
    pub run_id: String,
    pub response_id: String,
    pub output_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatTurnExecutionReport {
    pub session_id: String,
    pub run_id: String,
    pub response_id: String,
    pub output_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalContinuationReport {
    pub run_id: String,
    pub run_status: RunStatus,
    pub response_id: Option<String>,
    pub output_text: Option<String>,
    pub approval_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolExecutionReport {
    pub job_id: String,
    pub run_id: String,
    pub run_status: RunStatus,
    pub approval_id: Option<String>,
    pub output_summary: Option<String>,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BackgroundWorkerTickReport {
    pub queued_jobs: usize,
    pub dispatched_jobs: usize,
    pub executed_jobs: usize,
    pub emitted_inbox_events: usize,
    pub woken_sessions: usize,
    pub failed_jobs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SessionWorkCancellationReport {
    pub session_count: usize,
    pub run_count: usize,
    pub job_count: usize,
    pub mission_count: usize,
    pub inbox_event_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatExecutionEvent {
    ReasoningDelta(String),
    AssistantTextDelta(String),
    ProviderLoopProgress {
        current_round: usize,
        max_rounds: usize,
    },
    ToolStatus {
        tool_name: String,
        summary: String,
        status: ToolExecutionStatus,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone)]
pub struct ExecutionServiceConfig {
    pub data_dir: PathBuf,
    pub provider_max_tool_rounds: usize,
    pub provider_max_output_tokens: Option<u32>,
    pub session_defaults: SessionSettings,
    pub skills_dir: PathBuf,
    pub a2a_public_base_url: Option<String>,
    pub a2a_callback_bearer_token: Option<String>,
    pub a2a_peers: BTreeMap<String, A2APeerConfig>,
    pub runtime_timing: RuntimeTimingConfig,
    pub runtime_limits: RuntimeLimitsConfig,
}

impl Default for ExecutionServiceConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::new(),
            provider_max_tool_rounds: agent_runtime::provider::DEFAULT_PROVIDER_MAX_TOOL_ROUNDS
                as usize,
            provider_max_output_tokens: None,
            session_defaults: SessionSettings::default(),
            skills_dir: PathBuf::new(),
            a2a_public_base_url: None,
            a2a_callback_bearer_token: None,
            a2a_peers: BTreeMap::new(),
            runtime_timing: RuntimeTimingConfig::default(),
            runtime_limits: RuntimeLimitsConfig::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionService {
    permissions: PermissionConfig,
    config: ExecutionServiceConfig,
    supervisor: SupervisorLoop,
    workspace: WorkspaceRef,
    processes: SharedProcessRegistry,
    mcp: SharedMcpRegistry,
    a2a: A2AClient,
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
    MissingAgentProfile {
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
    CancelledByOperator,
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
        Self::new(
            PermissionConfig::default(),
            WorkspaceRef::default(),
            SharedProcessRegistry::default(),
            SharedMcpRegistry::default(),
            ExecutionServiceConfig {
                skills_dir: PathBuf::from("skills"),
                ..ExecutionServiceConfig::default()
            },
        )
    }
}

impl ExecutionService {
    pub fn new(
        permissions: PermissionConfig,
        workspace: WorkspaceRef,
        processes: SharedProcessRegistry,
        mcp: SharedMcpRegistry,
        config: ExecutionServiceConfig,
    ) -> Self {
        let a2a = A2AClient::new(config.runtime_timing.a2a_http_connect_timeout());
        Self {
            permissions,
            config,
            supervisor: SupervisorLoop::default(),
            workspace,
            processes,
            mcp,
            a2a,
        }
    }

    fn tool_runtime(&self) -> ToolRuntime {
        ToolRuntime::with_shared_process_registry(self.workspace.clone(), self.processes.clone())
    }

    fn load_session(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<Session, ExecutionError> {
        Session::try_from(
            store
                .get_session(session_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingSession {
                    id: session_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)
    }

    fn load_agent_profile(
        &self,
        store: &PersistenceStore,
        agent_profile_id: &str,
    ) -> Result<AgentProfile, ExecutionError> {
        AgentProfile::try_from(
            store
                .get_agent_profile(agent_profile_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingAgentProfile {
                    id: agent_profile_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)
    }

    fn load_agent_profile_for_session(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<AgentProfile, ExecutionError> {
        let session = self.load_session(store, session_id)?;
        self.load_agent_profile(store, &session.agent_profile_id)
    }

    fn ensure_agent_tool_allowed(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        tool_name: ToolName,
    ) -> Result<AgentProfile, ExecutionError> {
        let profile = self.load_agent_profile_for_session(store, session_id)?;
        if profile.allows_tool_id(tool_name.as_str()) {
            Ok(profile)
        } else {
            Err(ExecutionError::PermissionDenied {
                tool: tool_name.as_str().to_string(),
                reason: format!(
                    "tool {} is not allowed by agent profile {} ({})",
                    tool_name.as_str(),
                    profile.name,
                    profile.id
                ),
            })
        }
    }
}

fn unique_execution_token() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let seq = EXECUTION_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{millis}-{seq}")
}

fn ensure_unique_run_id(
    store: &PersistenceStore,
    base_id: String,
) -> Result<String, ExecutionError> {
    if store
        .get_run(&base_id)
        .map_err(ExecutionError::Store)?
        .is_none()
    {
        return Ok(base_id);
    }
    Ok(format!("{base_id}-{}", unique_execution_token()))
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
            Self::MissingAgentProfile { id } => {
                write!(formatter, "execution agent profile {id} was not found")
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
            Self::CancelledByOperator => {
                write!(formatter, "execution cancelled by operator")
            }
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
            | Self::MissingAgentProfile { .. }
            | Self::PermissionDenied { .. }
            | Self::ApprovalRequired { .. }
            | Self::CancelledByOperator
            | Self::InterruptedByQueuedInput
            | Self::ProviderLoop { .. }
            | Self::ToolCallParse { .. }
            | Self::UnsupportedJobInput { .. } => None,
        }
    }
}
