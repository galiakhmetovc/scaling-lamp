use crate::bootstrap::{
    AgentScheduleCreateOptions, AgentScheduleUpdatePatch, AgentScheduleView,
    DeliveryTargetCreateOptions, DeliveryTargetUpdatePatch, McpConnectorCreateOptions,
    McpConnectorUpdatePatch, McpConnectorView, SessionBackgroundJob, SessionDebugView,
    SessionOutputRouteCreateOptions, SessionOutputRouteUpdatePatch, SessionPendingApproval,
    SessionPreferencesPatch, SessionScheduleSummary, SessionSkillStatus, SessionSummary,
    SessionTask, SessionTranscriptView,
};
use crate::execution::{ApprovalContinuationReport, ChatExecutionEvent, ChatTurnExecutionReport};
use agent_runtime::delegation::{DelegateResultPackage, DelegateWriteScope};
use agent_runtime::prompt::MemoryRecallItem;
use agent_runtime::tool::{KvEntryOutput, MemoryItemOutput};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusResponse {
    pub ok: bool,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub tree_state: Option<String>,
    #[serde(default)]
    pub build_id: Option<String>,
    pub bind_host: String,
    pub bind_port: u16,
    pub permission_mode: String,
    pub session_count: usize,
    pub mission_count: usize,
    pub run_count: usize,
    pub job_count: usize,
    pub components: usize,
    pub data_dir: String,
    #[serde(default)]
    pub database: Option<String>,
    pub telegram_mode: String,
    pub event_bus_required: bool,
    pub event_bus_backend: String,
    pub event_bus_nats_configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskPruneRequest {
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebSnapshotResponse {
    pub generated_at: i64,
    pub status: WebRuntimeStatusResponse,
    pub event_bus: WebEventBusResponse,
    pub agents: Vec<WebAgentResponse>,
    pub sessions: Vec<SessionSummaryResponse>,
    pub recent_runs: Vec<WebRunResponse>,
    pub recent_tasks: Vec<SessionTaskResponse>,
    pub recent_tool_calls: Vec<WebToolCallResponse>,
    pub delivery_targets: Vec<WebDeliveryTargetResponse>,
    pub session_output_routes: Vec<WebSessionOutputRouteResponse>,
    pub telegram_chats: Vec<WebTelegramChatResponse>,
    pub recent_traces: Vec<WebTraceResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebRuntimeStatusResponse {
    pub ok: bool,
    pub version: Option<String>,
    pub commit: Option<String>,
    pub tree_state: Option<String>,
    pub build_id: Option<String>,
    pub data_dir: String,
    pub database: String,
    pub permission_mode: String,
    pub session_count: usize,
    pub mission_count: usize,
    pub run_count: usize,
    pub job_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebEventBusResponse {
    pub backend: String,
    pub required: bool,
    pub nats_configured: bool,
    pub input_stream: String,
    pub session_stream: String,
    pub delivery_stream: String,
    pub task_stream: String,
    pub dlq_stream: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebAgentResponse {
    pub id: String,
    pub name: String,
    pub template_kind: String,
    pub default_workspace_root: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebRunResponse {
    pub id: String,
    pub session_id: String,
    pub status: String,
    pub error: Option<String>,
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebToolCallResponse {
    pub id: String,
    pub session_id: String,
    pub run_id: String,
    pub tool_name: String,
    pub status: String,
    pub summary: String,
    pub error: Option<String>,
    pub result_summary: Option<String>,
    pub result_artifact_id: Option<String>,
    pub requested_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebDeliveryTargetResponse {
    pub target_id: String,
    pub kind: String,
    pub address: String,
    pub scope: String,
    pub owner_user_id: Option<String>,
    pub allowed_agent_ids: Vec<String>,
    pub allowed_session_ids: Vec<String>,
    pub send_policy_json: String,
    pub format_policy: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebSessionOutputRouteResponse {
    pub route_id: String,
    pub session_id: String,
    pub target_id: String,
    pub filter_json: String,
    pub format_policy: String,
    pub enabled: bool,
    pub last_delivered_transcript_created_at: Option<i64>,
    pub last_delivered_transcript_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebTelegramChatResponse {
    pub telegram_chat_id: i64,
    pub scope: String,
    pub selected_session_id: Option<String>,
    pub default_agent_profile_id: Option<String>,
    pub inbound_queue_mode: String,
    pub inbound_coalesce_window_ms: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebTraceResponse {
    pub trace_id: String,
    pub span_id: String,
    pub entity_kind: String,
    pub entity_id: String,
    pub surface: Option<String>,
    pub entrypoint: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryTargetCreateRequest {
    pub target_id: String,
    pub options: DeliveryTargetCreateOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryTargetUpdateRequest {
    pub patch: DeliveryTargetUpdatePatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOutputRouteCreateRequest {
    pub session_id: String,
    pub target_id: String,
    pub options: SessionOutputRouteCreateOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOutputRouteUpdateRequest {
    pub patch: SessionOutputRouteUpdatePatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonStopResponse {
    pub stopping: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub id: Option<String>,
    pub title: Option<String>,
    pub agent_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummaryResponse {
    pub id: String,
    pub title: String,
    pub agent_profile_id: String,
    pub agent_name: String,
    pub scheduled_by: Option<String>,
    pub schedule: Option<SessionScheduleSummaryResponse>,
    pub model: Option<String>,
    pub reasoning_visible: bool,
    pub think_level: Option<String>,
    pub compactifications: u32,
    pub completion_nudges: Option<u32>,
    pub auto_approve: bool,
    pub context_tokens: u32,
    pub usage_input_tokens: Option<u32>,
    pub usage_output_tokens: Option<u32>,
    pub usage_total_tokens: Option<u32>,
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
pub struct SessionScheduleSummaryResponse {
    pub id: String,
    pub mode: agent_runtime::agent::AgentScheduleMode,
    pub delivery_mode: agent_runtime::agent::AgentScheduleDeliveryMode,
    pub enabled: bool,
    pub next_fire_at: i64,
    pub target_session_id: Option<String>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDetailResponse {
    pub id: String,
    pub title: String,
    pub agent_profile_id: String,
    pub agent_name: String,
    pub workspace_root: String,
    pub prompt_override: Option<String>,
    pub settings_json: String,
    pub active_mission_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_job_id: Option<String>,
    pub delegation_label: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebugBundleResponse {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AboutResponse {
    pub about: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsTailRequest {
    pub max_lines: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsTailResponse {
    pub diagnostics: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRuntimeResponse {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRuntimeRequest {
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRunStatusResponse {
    pub run: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRunControlResponse {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRenderResponse {
    pub task: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskControlResponse {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAgentMessageRequest {
    pub target_agent_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionChainGrantRequest {
    pub chain_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSystemResponse {
    pub system: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionArtifactsResponse {
    pub artifacts: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionArtifactResponse {
    pub artifact: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionArtifactFileSummaryResponse {
    pub id: String,
    pub session_id: String,
    pub kind: String,
    pub metadata_json: String,
    pub path: String,
    pub byte_len: usize,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionArtifactFilesResponse {
    pub artifacts: Vec<SessionArtifactFileSummaryResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionArtifactFileResponse {
    pub id: String,
    pub session_id: String,
    pub kind: String,
    pub metadata_json: String,
    pub path: String,
    pub byte_len: usize,
    pub created_at: i64,
    pub content: Option<String>,
    pub content_truncated: bool,
    pub text: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkspaceEntryResponse {
    pub path: String,
    pub kind: String,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkspaceListResponse {
    pub workspace_root: String,
    pub path: String,
    pub entries: Vec<SessionWorkspaceEntryResponse>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkspaceFileResponse {
    pub workspace_root: String,
    pub path: String,
    pub byte_len: u64,
    pub content: Option<String>,
    pub content_truncated: bool,
    pub text: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkspaceWriteRequest {
    pub path: String,
    pub content: String,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkspacePathRequest {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkspaceWriteResponse {
    pub workspace_root: String,
    pub path: String,
    pub bytes_written: usize,
    pub created: bool,
    pub overwritten: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkspaceMkdirResponse {
    pub workspace_root: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorkspaceTrashResponse {
    pub workspace_root: String,
    pub path: String,
    pub trash_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRenderResponse {
    pub memory: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticMemoryItemResponse {
    pub id: String,
    pub memory: String,
    pub score: Option<String>,
    pub metadata: Value,
    pub user_id: Option<String>,
    pub agent_id: Option<String>,
    pub app_id: Option<String>,
    pub run_id: Option<String>,
}

impl From<MemoryItemOutput> for SemanticMemoryItemResponse {
    fn from(item: MemoryItemOutput) -> Self {
        Self {
            id: item.id,
            memory: item.memory,
            score: item.score,
            metadata: item.metadata,
            user_id: item.user_id,
            agent_id: item.agent_id,
            app_id: item.app_id,
            run_id: item.run_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticMemorySearchRequest {
    pub session_id: String,
    pub query: String,
    pub scope: Option<String>,
    pub limit: Option<usize>,
    #[serde(default)]
    pub filters: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticMemoryListResponse {
    pub results: Vec<SemanticMemoryItemResponse>,
    pub truncated: bool,
    pub offset: usize,
    pub limit: usize,
    pub total_results: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticMemorySearchResponse {
    pub query: String,
    pub results: Vec<SemanticMemoryItemResponse>,
    pub truncated: bool,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticMemoryUpdateRequest {
    pub text: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticMemoryUpdateResponse {
    pub memory_id: String,
    pub updated: bool,
    pub memory: Option<SemanticMemoryItemResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticMemoryDeleteResponse {
    pub memory_id: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KvEntryResponse {
    pub scope: String,
    pub namespace_id: String,
    pub key: String,
    pub value: Value,
    pub metadata: Value,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub expires_at: Option<i64>,
}

impl From<KvEntryOutput> for KvEntryResponse {
    fn from(entry: KvEntryOutput) -> Self {
        Self {
            scope: entry.scope,
            namespace_id: entry.namespace_id,
            key: entry.key,
            value: entry.value,
            metadata: entry.metadata,
            revision: entry.revision,
            created_at: entry.created_at,
            updated_at: entry.updated_at,
            expires_at: entry.expires_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KvListResponse {
    pub results: Vec<KvEntryResponse>,
    pub truncated: bool,
    pub offset: usize,
    pub limit: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KvPutRequest {
    pub session_id: String,
    pub key: String,
    #[serde(default)]
    pub value: Value,
    pub scope: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    pub expected_revision: Option<i64>,
    pub ttl_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KvPutResponse {
    pub entry: KvEntryResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KvDeleteRequest {
    pub session_id: String,
    pub key: String,
    pub scope: Option<String>,
    pub expected_revision: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KvDeleteResponse {
    pub key: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecallPreviewRequest {
    pub session_id: String,
    pub query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecallItemResponse {
    pub scope: String,
    pub memory_id: String,
    pub memory: String,
    pub score: Option<String>,
    pub source: Option<String>,
}

impl From<MemoryRecallItem> for MemoryRecallItemResponse {
    fn from(item: MemoryRecallItem) -> Self {
        Self {
            scope: item.scope,
            memory_id: item.memory_id,
            memory: item.memory,
            score: item.score,
            source: item.source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecallPreviewResponse {
    pub enabled: bool,
    pub query: Option<String>,
    pub items: Vec<MemoryRecallItemResponse>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRenderResponse {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSummaryResponse {
    pub id: String,
    pub name: String,
    pub template_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDetailResponse {
    pub id: String,
    pub name: String,
    pub template_kind: String,
    pub agent_home: String,
    pub allowed_tools: Vec<String>,
    pub default_workspace_root: Option<String>,
    pub created_from_template_id: Option<String>,
    pub created_by_session_id: Option<String>,
    pub created_by_agent_profile_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSelectRequest {
    pub identifier: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCreateRequest {
    pub name: String,
    pub template_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentUpdateRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub default_workspace_root: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDeleteResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFileEntryResponse {
    pub path: String,
    pub kind: String,
    pub byte_len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFilesResponse {
    pub agent_id: String,
    pub agent_name: String,
    pub agent_home: String,
    pub files: Vec<AgentFileEntryResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFileReadResponse {
    pub agent_id: String,
    pub agent_home: String,
    pub path: String,
    pub kind: String,
    pub byte_len: u64,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFileWriteRequest {
    pub path: String,
    pub content: String,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFileWriteResponse {
    pub agent_id: String,
    pub agent_home: String,
    pub path: String,
    pub kind: String,
    pub bytes_written: usize,
    pub created: bool,
    pub overwritten: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentResolveRequest {
    pub identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentScheduleCreateRequest {
    pub id: String,
    pub options: AgentScheduleCreateOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentScheduleResolveRequest {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentScheduleDetailResponse {
    pub schedule: AgentScheduleView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentScheduleListResponse {
    pub schedules: Vec<AgentScheduleView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentScheduleUpdateRequest {
    pub patch: AgentScheduleUpdatePatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConnectorCreateRequest {
    pub id: String,
    pub options: McpConnectorCreateOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConnectorUpdateRequest {
    pub patch: McpConnectorUpdatePatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConnectorDetailResponse {
    pub connector: McpConnectorView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceReadRequest {
    pub connector_id: String,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPromptGetRequest {
    pub connector_id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: Option<std::collections::BTreeMap<String, String>>,
}

impl From<SessionSummary> for SessionSummaryResponse {
    fn from(value: SessionSummary) -> Self {
        Self {
            id: value.id,
            title: value.title,
            agent_profile_id: value.agent_profile_id,
            agent_name: value.agent_name,
            scheduled_by: value.scheduled_by,
            schedule: value.schedule.map(SessionScheduleSummaryResponse::from),
            model: value.model,
            reasoning_visible: value.reasoning_visible,
            think_level: value.think_level,
            compactifications: value.compactifications,
            completion_nudges: value.completion_nudges,
            auto_approve: value.auto_approve,
            context_tokens: value.context_tokens,
            usage_input_tokens: value.usage_input_tokens,
            usage_output_tokens: value.usage_output_tokens,
            usage_total_tokens: value.usage_total_tokens,
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

impl From<SessionScheduleSummary> for SessionScheduleSummaryResponse {
    fn from(value: SessionScheduleSummary) -> Self {
        Self {
            id: value.id,
            mode: value.mode,
            delivery_mode: value.delivery_mode,
            enabled: value.enabled,
            next_fire_at: value.next_fire_at,
            target_session_id: value.target_session_id,
            last_result: value.last_result,
            last_error: value.last_error,
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
pub struct SessionTaskResponse {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub source_session_id: Option<String>,
    pub owner_agent_id: Option<String>,
    pub executor_agent_id: Option<String>,
    pub parent_task_id: Option<String>,
    pub dependency_json: String,
    pub context_ref_json: String,
    pub result_ref_json: Option<String>,
    pub retry_policy_json: String,
    pub attempt_count: i64,
    pub max_attempts: i64,
    pub timeout_at: Option<i64>,
    pub chain_id: Option<String>,
    pub hop_count: Option<i64>,
    pub max_hops: Option<i64>,
    pub trace_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
}

impl From<SessionTask> for SessionTaskResponse {
    fn from(value: SessionTask) -> Self {
        Self {
            id: value.id,
            kind: value.kind,
            status: value.status,
            source_session_id: value.source_session_id,
            owner_agent_id: value.owner_agent_id,
            executor_agent_id: value.executor_agent_id,
            parent_task_id: value.parent_task_id,
            dependency_json: value.dependency_json,
            context_ref_json: value.context_ref_json,
            result_ref_json: value.result_ref_json,
            retry_policy_json: value.retry_policy_json,
            attempt_count: value.attempt_count,
            max_attempts: value.max_attempts,
            timeout_at: value.timeout_at,
            chain_id: value.chain_id,
            hop_count: value.hop_count,
            max_hops: value.max_hops,
            trace_id: value.trace_id,
            created_at: value.created_at,
            updated_at: value.updated_at,
            started_at: value.started_at,
            finished_at: value.finished_at,
            error: value.error,
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
    #[serde(default)]
    pub surface: Option<String>,
    #[serde(default)]
    pub entrypoint: Option<String>,
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
pub type SessionDebugResponse = SessionDebugView;
pub type SessionPendingApprovalsResponse = Vec<SessionPendingApproval>;
pub type SessionPreferencesRequest = SessionPreferencesPatch;
pub type SessionSkillsResponse = Vec<SessionSkillStatus>;
pub type SessionBackgroundJobsResponse = Vec<SessionBackgroundJobResponse>;
pub type SessionTasksResponse = Vec<SessionTaskResponse>;
