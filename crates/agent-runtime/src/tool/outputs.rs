use crate::agent::{AgentScheduleDeliveryMode, AgentScheduleMode, AgentTemplateKind};
use crate::memory::SessionRetentionTier;
use crate::plan::{PlanItem, PlanLintIssue};
use crate::workspace::{WorkspaceEntry, WorkspaceSearchMatch};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    FileDeliveryTarget, FsWriteMode, KnowledgeReadMode, KnowledgeSourceKind, ProcessOutputStream,
    SessionReadMode,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsReadOutput {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsReadTextOutput {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsReadLinesOutput {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub total_lines: usize,
    pub eof: bool,
    pub next_start_line: Option<usize>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsWriteOutput {
    pub path: String,
    pub bytes_written: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsWriteTextOutput {
    pub path: String,
    pub mode: FsWriteMode,
    pub bytes_written: usize,
    pub created: bool,
    pub overwritten: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsPatchOutput {
    pub path: String,
    pub bytes_written: usize,
    pub edits_applied: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsPatchTextOutput {
    pub path: String,
    pub bytes_written: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsReplaceLinesOutput {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub bytes_written: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsInsertTextOutput {
    pub path: String,
    pub position: String,
    pub line: Option<usize>,
    pub bytes_written: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsListOutput {
    pub entries: Vec<WorkspaceEntry>,
    pub truncated: bool,
    pub offset: usize,
    pub limit: usize,
    pub total_entries: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsGlobOutput {
    pub entries: Vec<WorkspaceEntry>,
    pub truncated: bool,
    pub offset: usize,
    pub limit: usize,
    pub total_entries: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsSearchOutput {
    pub matches: Vec<WorkspaceSearchMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsSearchTextOutput {
    pub matches: Vec<WorkspaceSearchMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsFindInFilesOutput {
    pub matches: Vec<WorkspaceSearchMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsMkdirOutput {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsMoveOutput {
    pub src: String,
    pub dest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsTrashOutput {
    pub path: String,
    pub trashed_to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebFetchOutput {
    pub url: String,
    pub status_code: u16,
    pub content_type: Option<String>,
    pub title: Option<String>,
    pub extracted_from_html: bool,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSearchOutput {
    pub query: String,
    pub results: Vec<WebSearchResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserCommandOutput {
    pub action: String,
    pub session: String,
    pub stdout: String,
    pub stderr: String,
    pub workspace_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessKind {
    Exec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessStartOutput {
    pub process_id: String,
    pub pid_ref: String,
    pub kind: ProcessKind,
    pub command_display: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessResultStatus {
    Exited,
    Killed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutputStatus {
    Running,
    Exited,
    Killed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessResult {
    pub process_id: String,
    pub status: ProcessResultStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutputRead {
    pub process_id: String,
    pub stream: ProcessOutputStream,
    pub status: ProcessOutputStatus,
    pub exit_code: Option<i32>,
    pub cursor: usize,
    pub next_cursor: usize,
    pub truncated: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanReadOutput {
    pub goal: Option<String>,
    pub items: Vec<PlanItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanWriteOutput {
    pub goal: Option<String>,
    pub items: Vec<PlanItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitPlanOutput {
    pub goal: String,
    pub item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddTaskOutput {
    pub task: PlanItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetTaskStatusOutput {
    pub task: PlanItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddTaskNoteOutput {
    pub task: PlanItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditTaskOutput {
    pub task: PlanItem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSnapshotOutput {
    pub goal: Option<String>,
    pub items: Vec<PlanItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanLintOutput {
    pub ok: bool,
    pub issues: Vec<PlanLintIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptBudgetLayerOutput {
    pub layer: String,
    pub percent: u8,
    pub target_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptBudgetReadOutput {
    pub session_id: String,
    pub source: String,
    pub pending_next_turn_override: bool,
    pub context_window_tokens: Option<u32>,
    pub auto_compaction_trigger_basis_points: u32,
    pub usable_context_tokens: Option<u32>,
    pub total_percent: u16,
    pub layers: Vec<PromptBudgetLayerOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptBudgetUpdateOutput {
    pub session_id: String,
    pub scope: String,
    pub reset: bool,
    pub reason: Option<String>,
    pub budget: PromptBudgetReadOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillStatusOutput {
    pub name: String,
    pub description: String,
    pub mode: String,
    pub skill_dir: String,
    pub skill_md_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillListOutput {
    pub session_id: String,
    pub include_inactive: bool,
    pub offset: usize,
    pub limit: usize,
    pub total_results: usize,
    pub next_offset: Option<usize>,
    pub skills: Vec<SkillStatusOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillReadOutput {
    pub session_id: String,
    pub name: String,
    pub description: String,
    pub mode: String,
    pub skill_dir: String,
    pub skill_md_path: String,
    pub body: String,
    pub body_byte_len: usize,
    pub body_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillActivationOutput {
    pub session_id: String,
    pub name: String,
    pub mode: String,
    pub skills: Vec<SkillStatusOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyJobOutput {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub run_id: Option<String>,
    pub parent_job_id: Option<String>,
    pub last_progress_message: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyChildSessionOutput {
    pub id: String,
    pub title: String,
    pub agent_profile_id: String,
    pub parent_job_id: Option<String>,
    pub delegation_label: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyInboxEventOutput {
    pub id: String,
    pub kind: String,
    pub job_id: Option<String>,
    pub status: String,
    pub available_at: i64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyInteragentOutput {
    pub chain_id: String,
    pub origin_session_id: String,
    pub origin_agent_id: String,
    pub hop_count: u32,
    pub max_hops: u32,
    pub parent_interagent_session_id: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyMeshPeerOutput {
    pub peer_id: String,
    pub base_url: String,
    pub has_bearer_token: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomyStateReadOutput {
    pub session_id: String,
    pub title: String,
    pub agent_profile_id: String,
    pub turn_source: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_job_id: Option<String>,
    pub delegation_label: Option<String>,
    pub schedules: Vec<ScheduleViewOutput>,
    pub active_jobs: Vec<AutonomyJobOutput>,
    pub child_sessions: Vec<AutonomyChildSessionOutput>,
    pub inbox_events: Vec<AutonomyInboxEventOutput>,
    pub interagent: Option<AutonomyInteragentOutput>,
    pub mesh_peers: Vec<AutonomyMeshPeerOutput>,
    pub truncated: bool,
    pub max_items: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactReadOutput {
    pub ref_id: String,
    pub artifact_id: String,
    pub label: String,
    pub summary: String,
    pub content: String,
    pub offset: usize,
    pub content_byte_len: usize,
    pub total_byte_len: usize,
    pub content_truncated: bool,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactSearchResult {
    pub ref_id: String,
    pub artifact_id: String,
    pub label: String,
    pub summary: String,
    pub token_estimate: u32,
    pub message_count: u32,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPinOutput {
    pub ref_id: String,
    pub artifact_id: String,
    pub pinned: bool,
    pub explicit_read_count: u32,
    pub pin_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactSearchOutput {
    pub query: String,
    pub results: Vec<ArtifactSearchResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliverFileOutput {
    pub request_id: String,
    pub artifact_id: String,
    pub target: FileDeliveryTarget,
    pub file_name: String,
    pub caption: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeSearchResultOutput {
    pub path: String,
    pub kind: KnowledgeSourceKind,
    pub snippet: String,
    pub sha256: String,
    pub mtime: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeSearchOutput {
    pub query: String,
    pub results: Vec<KnowledgeSearchResultOutput>,
    pub truncated: bool,
    pub offset: usize,
    pub limit: usize,
    pub total_results: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeReadOutput {
    pub path: String,
    pub kind: KnowledgeSourceKind,
    pub sha256: String,
    pub mtime: i64,
    pub mode: KnowledgeReadMode,
    pub cursor: usize,
    pub next_cursor: Option<usize>,
    pub truncated: bool,
    pub total_lines: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSearchMatchSource {
    Title,
    Summary,
    Plan,
    SystemNote,
    Transcript,
    Artifact,
    ArchiveSummary,
    ArchiveTranscript,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSearchResultOutput {
    pub session_id: String,
    pub title: String,
    pub agent_profile_id: String,
    pub tier: SessionRetentionTier,
    pub updated_at: i64,
    pub match_source: SessionSearchMatchSource,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSearchOutput {
    pub query: String,
    pub results: Vec<SessionSearchResultOutput>,
    pub truncated: bool,
    pub offset: usize,
    pub limit: usize,
    pub total_results: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReadSummaryOutput {
    pub summary_text: String,
    pub covered_message_count: u32,
    pub summary_token_estimate: u32,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReadMessageOutput {
    pub id: String,
    pub run_id: Option<String>,
    pub role: String,
    pub created_at: i64,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReadArtifactOutput {
    pub artifact_id: String,
    pub kind: String,
    pub path: String,
    pub byte_len: u64,
    pub created_at: i64,
    pub label: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReadOutput {
    pub session_id: String,
    pub title: String,
    pub agent_profile_id: String,
    pub mode: SessionReadMode,
    pub tier: SessionRetentionTier,
    pub from_archive: bool,
    pub cursor: usize,
    pub next_cursor: Option<usize>,
    pub truncated: bool,
    pub total_items: usize,
    pub summary: Option<SessionReadSummaryOutput>,
    pub messages: Vec<SessionReadMessageOutput>,
    pub artifacts: Vec<SessionReadArtifactOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWaitOutput {
    pub session_id: String,
    pub wait_timeout_ms: u64,
    pub settled: bool,
    pub active_run_count: usize,
    pub active_job_count: usize,
    pub latest_run_id: Option<String>,
    pub latest_run_status: Option<String>,
    pub latest_job_id: Option<String>,
    pub latest_job_status: Option<String>,
    pub snapshot: SessionReadOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpDiscoveredResourceOutput {
    pub connector_id: String,
    pub uri: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSearchResourcesOutput {
    pub connector_id: Option<String>,
    pub query: Option<String>,
    pub results: Vec<McpDiscoveredResourceOutput>,
    pub truncated: bool,
    pub offset: usize,
    pub limit: usize,
    pub total_results: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpResourceContentOutput {
    pub kind: String,
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: Option<String>,
    pub blob: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpReadResourceOutput {
    pub connector_id: String,
    pub uri: String,
    pub text: String,
    pub contents: Vec<McpResourceContentOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPromptArgumentOutput {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpDiscoveredPromptOutput {
    pub connector_id: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub arguments: Vec<McpPromptArgumentOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSearchPromptsOutput {
    pub connector_id: Option<String>,
    pub query: Option<String>,
    pub results: Vec<McpDiscoveredPromptOutput>,
    pub truncated: bool,
    pub offset: usize,
    pub limit: usize,
    pub total_results: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPromptMessageOutput {
    pub role: String,
    pub content_type: String,
    pub text: Option<String>,
    pub uri: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpGetPromptOutput {
    pub connector_id: String,
    pub name: String,
    pub description: Option<String>,
    pub text: String,
    pub messages: Vec<McpPromptMessageOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCallOutput {
    pub connector_id: String,
    pub exposed_name: String,
    pub remote_name: String,
    pub content_text: String,
    pub structured_content_json: Option<String>,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSummaryOutput {
    pub id: String,
    pub name: String,
    pub template_kind: AgentTemplateKind,
    pub agent_home: String,
    pub default_workspace_root: Option<String>,
    pub allowed_tool_count: usize,
    pub created_from_template_id: Option<String>,
    pub created_by_session_id: Option<String>,
    pub created_by_agent_profile_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentListOutput {
    pub agents: Vec<AgentSummaryOutput>,
    pub truncated: bool,
    pub offset: usize,
    pub limit: usize,
    pub total_agents: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentReadOutput {
    pub agent: AgentSummaryOutput,
    pub allowed_tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCreateOutput {
    pub agent: AgentSummaryOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinueLaterOutput {
    pub schedule: ScheduleViewOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleViewOutput {
    pub id: String,
    pub agent_profile_id: String,
    pub workspace_root: String,
    pub prompt: String,
    pub mode: AgentScheduleMode,
    pub delivery_mode: AgentScheduleDeliveryMode,
    pub target_session_id: Option<String>,
    pub interval_seconds: u64,
    pub next_fire_at: i64,
    pub enabled: bool,
    pub last_triggered_at: Option<i64>,
    pub last_finished_at: Option<i64>,
    pub last_session_id: Option<String>,
    pub last_job_id: Option<String>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleListOutput {
    pub schedules: Vec<ScheduleViewOutput>,
    pub truncated: bool,
    pub offset: usize,
    pub limit: usize,
    pub total_schedules: usize,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleReadOutput {
    pub schedule: ScheduleViewOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleCreateOutput {
    pub schedule: ScheduleViewOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleUpdateOutput {
    pub schedule: ScheduleViewOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleDeleteOutput {
    pub id: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageAgentOutput {
    pub target_agent_id: String,
    pub recipient_session_id: String,
    pub recipient_job_id: String,
    pub chain_id: String,
    pub hop_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantAgentChainContinuationOutput {
    pub chain_id: String,
    pub granted_hops: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolOutput {
    FsRead(FsReadOutput),
    FsWrite(FsWriteOutput),
    FsPatch(FsPatchOutput),
    FsReadText(FsReadTextOutput),
    FsReadLines(FsReadLinesOutput),
    FsSearchText(FsSearchTextOutput),
    FsFindInFiles(FsFindInFilesOutput),
    FsWriteText(FsWriteTextOutput),
    FsPatchText(FsPatchTextOutput),
    FsReplaceLines(FsReplaceLinesOutput),
    FsInsertText(FsInsertTextOutput),
    FsMkdir(FsMkdirOutput),
    FsMove(FsMoveOutput),
    FsTrash(FsTrashOutput),
    FsList(FsListOutput),
    FsGlob(FsGlobOutput),
    FsSearch(FsSearchOutput),
    WebFetch(WebFetchOutput),
    WebSearch(WebSearchOutput),
    BrowserOpen(BrowserCommandOutput),
    BrowserSnapshot(BrowserCommandOutput),
    BrowserText(BrowserCommandOutput),
    BrowserClick(BrowserCommandOutput),
    BrowserFill(BrowserCommandOutput),
    BrowserPress(BrowserCommandOutput),
    BrowserWait(BrowserCommandOutput),
    BrowserScroll(BrowserCommandOutput),
    BrowserEval(BrowserCommandOutput),
    BrowserScreenshot(BrowserCommandOutput),
    BrowserPdf(BrowserCommandOutput),
    BrowserStatus(BrowserCommandOutput),
    BrowserClose(BrowserCommandOutput),
    ProcessStart(ProcessStartOutput),
    ProcessOutputRead(ProcessOutputRead),
    ProcessResult(ProcessResult),
    PlanRead(PlanReadOutput),
    PlanWrite(PlanWriteOutput),
    InitPlan(InitPlanOutput),
    AddTask(AddTaskOutput),
    SetTaskStatus(SetTaskStatusOutput),
    AddTaskNote(AddTaskNoteOutput),
    EditTask(EditTaskOutput),
    PlanSnapshot(PlanSnapshotOutput),
    PlanLint(PlanLintOutput),
    PromptBudgetRead(PromptBudgetReadOutput),
    PromptBudgetUpdate(PromptBudgetUpdateOutput),
    AutonomyStateRead(AutonomyStateReadOutput),
    SkillList(SkillListOutput),
    SkillRead(SkillReadOutput),
    SkillEnable(SkillActivationOutput),
    SkillDisable(SkillActivationOutput),
    ArtifactRead(ArtifactReadOutput),
    ArtifactSearch(ArtifactSearchOutput),
    ArtifactPin(ArtifactPinOutput),
    ArtifactUnpin(ArtifactPinOutput),
    DeliverFile(DeliverFileOutput),
    KnowledgeSearch(KnowledgeSearchOutput),
    KnowledgeRead(KnowledgeReadOutput),
    SessionSearch(SessionSearchOutput),
    SessionRead(SessionReadOutput),
    SessionWait(SessionWaitOutput),
    McpCall(McpCallOutput),
    McpSearchResources(McpSearchResourcesOutput),
    McpReadResource(McpReadResourceOutput),
    McpSearchPrompts(McpSearchPromptsOutput),
    McpGetPrompt(McpGetPromptOutput),
    AgentList(AgentListOutput),
    AgentRead(AgentReadOutput),
    AgentCreate(AgentCreateOutput),
    ContinueLater(ContinueLaterOutput),
    ScheduleList(ScheduleListOutput),
    ScheduleRead(ScheduleReadOutput),
    ScheduleCreate(ScheduleCreateOutput),
    ScheduleUpdate(ScheduleUpdateOutput),
    ScheduleDelete(ScheduleDeleteOutput),
    MessageAgent(MessageAgentOutput),
    GrantAgentChainContinuation(GrantAgentChainContinuationOutput),
}

impl ProcessKind {
    pub fn as_prefix(self) -> &'static str {
        match self {
            Self::Exec => "exec",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Exec => "exec",
        }
    }
}

impl ProcessOutputStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Exited => "exited",
            Self::Killed => "killed",
        }
    }
}

impl SessionSearchMatchSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Summary => "summary",
            Self::Plan => "plan",
            Self::SystemNote => "system_note",
            Self::Transcript => "transcript",
            Self::Artifact => "artifact",
            Self::ArchiveSummary => "archive_summary",
            Self::ArchiveTranscript => "archive_transcript",
        }
    }
}

impl ToolOutput {
    pub fn into_fs_read(self) -> Option<FsReadOutput> {
        match self {
            Self::FsRead(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_fs_read_text(self) -> Option<FsReadTextOutput> {
        match self {
            Self::FsReadText(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_fs_read_lines(self) -> Option<FsReadLinesOutput> {
        match self {
            Self::FsReadLines(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_fs_list(self) -> Option<FsListOutput> {
        match self {
            Self::FsList(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_fs_glob(self) -> Option<FsGlobOutput> {
        match self {
            Self::FsGlob(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_fs_search(self) -> Option<FsSearchOutput> {
        match self {
            Self::FsSearch(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_fs_search_text(self) -> Option<FsSearchTextOutput> {
        match self {
            Self::FsSearchText(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_fs_find_in_files(self) -> Option<FsFindInFilesOutput> {
        match self {
            Self::FsFindInFiles(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_web_fetch(self) -> Option<WebFetchOutput> {
        match self {
            Self::WebFetch(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_web_search(self) -> Option<WebSearchOutput> {
        match self {
            Self::WebSearch(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_process_start(self) -> Option<ProcessStartOutput> {
        match self {
            Self::ProcessStart(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_process_output_read(self) -> Option<ProcessOutputRead> {
        match self {
            Self::ProcessOutputRead(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_process_result(self) -> Option<ProcessResult> {
        match self {
            Self::ProcessResult(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_knowledge_search(self) -> Option<KnowledgeSearchOutput> {
        match self {
            Self::KnowledgeSearch(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_knowledge_read(self) -> Option<KnowledgeReadOutput> {
        match self {
            Self::KnowledgeRead(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_session_search(self) -> Option<SessionSearchOutput> {
        match self {
            Self::SessionSearch(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_session_read(self) -> Option<SessionReadOutput> {
        match self {
            Self::SessionRead(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_session_wait(self) -> Option<SessionWaitOutput> {
        match self {
            Self::SessionWait(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_plan_read(self) -> Option<PlanReadOutput> {
        match self {
            Self::PlanRead(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_plan_write(self) -> Option<PlanWriteOutput> {
        match self {
            Self::PlanWrite(output) => Some(output),
            _ => None,
        }
    }

    pub fn into_plan_snapshot(self) -> Option<PlanSnapshotOutput> {
        match self {
            Self::PlanSnapshot(output) => Some(output),
            _ => None,
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::FsRead(output) => {
                format!(
                    "fs_read path={} bytes={}",
                    output.path,
                    output.content.len()
                )
            }
            Self::FsReadText(output) => {
                format!(
                    "fs_read_text path={} bytes={}",
                    output.path,
                    output.content.len()
                )
            }
            Self::FsReadLines(output) => format!(
                "fs_read_lines path={} start_line={} end_line={} total_lines={}",
                output.path, output.start_line, output.end_line, output.total_lines
            ),
            Self::FsWrite(output) => {
                format!(
                    "fs_write path={} bytes={}",
                    output.path, output.bytes_written
                )
            }
            Self::FsWriteText(output) => format!(
                "fs_write_text path={} mode={} bytes={}",
                output.path,
                output.mode.as_str(),
                output.bytes_written
            ),
            Self::FsPatch(output) => {
                format!(
                    "fs_patch path={} edits={}",
                    output.path, output.edits_applied
                )
            }
            Self::FsPatchText(output) => {
                format!("fs_patch_text path={}", output.path)
            }
            Self::FsReplaceLines(output) => format!(
                "fs_replace_lines path={} start_line={} end_line={}",
                output.path, output.start_line, output.end_line
            ),
            Self::FsInsertText(output) => format!(
                "fs_insert_text path={} position={} line={}",
                output.path,
                output.position,
                output
                    .line
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            Self::FsMkdir(output) => format!("fs_mkdir path={}", output.path),
            Self::FsMove(output) => format!("fs_move {} -> {}", output.src, output.dest),
            Self::FsTrash(output) => {
                format!(
                    "fs_trash path={} trashed_to={}",
                    output.path, output.trashed_to
                )
            }
            Self::FsList(output) => {
                if let Some(next_offset) = output.next_offset {
                    format!(
                        "fs_list entries={} total={} truncated next_offset={}",
                        output.entries.len(),
                        output.total_entries,
                        next_offset
                    )
                } else {
                    format!("fs_list entries={}", output.entries.len())
                }
            }
            Self::FsGlob(output) => {
                if let Some(next_offset) = output.next_offset {
                    format!(
                        "fs_glob entries={} total={} truncated next_offset={}",
                        output.entries.len(),
                        output.total_entries,
                        next_offset
                    )
                } else {
                    format!("fs_glob entries={}", output.entries.len())
                }
            }
            Self::FsSearch(output) => format!("fs_search matches={}", output.matches.len()),
            Self::FsSearchText(output) => {
                format!("fs_search_text matches={}", output.matches.len())
            }
            Self::FsFindInFiles(output) => {
                format!("fs_find_in_files matches={}", output.matches.len())
            }
            Self::WebFetch(output) => {
                format!("web_fetch url={} status={}", output.url, output.status_code)
            }
            Self::WebSearch(output) => format!("web_search results={}", output.results.len()),
            Self::BrowserOpen(output)
            | Self::BrowserSnapshot(output)
            | Self::BrowserText(output)
            | Self::BrowserClick(output)
            | Self::BrowserFill(output)
            | Self::BrowserPress(output)
            | Self::BrowserWait(output)
            | Self::BrowserScroll(output)
            | Self::BrowserEval(output)
            | Self::BrowserScreenshot(output)
            | Self::BrowserPdf(output)
            | Self::BrowserStatus(output)
            | Self::BrowserClose(output) => {
                if let Some(path) = &output.workspace_path {
                    format!(
                        "{} session={} path={} stdout_bytes={} stderr_bytes={}",
                        output.action,
                        output.session,
                        path,
                        output.stdout.len(),
                        output.stderr.len()
                    )
                } else {
                    format!(
                        "{} session={} stdout_bytes={} stderr_bytes={}",
                        output.action,
                        output.session,
                        output.stdout.len(),
                        output.stderr.len()
                    )
                }
            }
            Self::ProcessStart(output) => format!(
                "{}_start process_id={} pid_ref={} cwd={} command={}",
                output.kind.as_str(),
                output.process_id,
                output.pid_ref,
                output.cwd,
                output.command_display
            ),
            Self::ProcessOutputRead(output) => format!(
                "process_output_read process_id={} stream={} status={} cursor={} next_cursor={} bytes={}",
                output.process_id,
                output.stream.as_str(),
                output.status.as_str(),
                output.cursor,
                output.next_cursor,
                output.text.len()
            ),
            Self::ProcessResult(output) => format!(
                "process_result process_id={} status={:?} exit_code={:?}",
                output.process_id, output.status, output.exit_code
            ),
            Self::PlanRead(output) => format!("plan_read items={}", output.items.len()),
            Self::PlanWrite(output) => format!("plan_write items={}", output.items.len()),
            Self::InitPlan(output) => {
                format!("init_plan goal={} items={}", output.goal, output.item_count)
            }
            Self::AddTask(output) => format!("add_task task_id={}", output.task.id),
            Self::SetTaskStatus(output) => format!(
                "set_task_status task_id={} status={}",
                output.task.id,
                output.task.status.as_str()
            ),
            Self::AddTaskNote(output) => format!("add_task_note task_id={}", output.task.id),
            Self::EditTask(output) => format!("edit_task task_id={}", output.task.id),
            Self::PlanSnapshot(output) => format!("plan_snapshot items={}", output.items.len()),
            Self::PlanLint(output) => {
                format!("plan_lint ok={} issues={}", output.ok, output.issues.len())
            }
            Self::PromptBudgetRead(output) => format!(
                "prompt_budget_read source={} pending_next_turn_override={} usable_context_tokens={}",
                output.source,
                output.pending_next_turn_override,
                output
                    .usable_context_tokens
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
            Self::PromptBudgetUpdate(output) => format!(
                "prompt_budget_update scope={} reset={} source={} usable_context_tokens={}",
                output.scope,
                output.reset,
                output.budget.source,
                output
                    .budget
                    .usable_context_tokens
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
            Self::AutonomyStateRead(output) => format!(
                "autonomy_state_read schedules={} active_jobs={} child_sessions={} inbox_events={} mesh_peers={} truncated={}",
                output.schedules.len(),
                output.active_jobs.len(),
                output.child_sessions.len(),
                output.inbox_events.len(),
                output.mesh_peers.len(),
                output.truncated
            ),
            Self::SkillList(output) => {
                if let Some(next_offset) = output.next_offset {
                    format!(
                        "skill_list skills={} total={} truncated next_offset={}",
                        output.skills.len(),
                        output.total_results,
                        next_offset
                    )
                } else {
                    format!("skill_list skills={}", output.skills.len())
                }
            }
            Self::SkillRead(output) => format!(
                "skill_read name={} mode={} bytes={} truncated={}",
                output.name, output.mode, output.body_byte_len, output.body_truncated
            ),
            Self::SkillEnable(output) => {
                format!("skill_enable name={} mode={}", output.name, output.mode)
            }
            Self::SkillDisable(output) => {
                format!("skill_disable name={} mode={}", output.name, output.mode)
            }
            Self::ArtifactRead(output) => {
                format!(
                    "artifact_read artifact_id={} offset={} bytes={}/{} truncated={}",
                    output.artifact_id,
                    output.offset,
                    output.content_byte_len,
                    output.total_byte_len,
                    output.content_truncated
                )
            }
            Self::ArtifactSearch(output) => {
                format!(
                    "artifact_search query={} results={}",
                    output.query,
                    output.results.len()
                )
            }
            Self::ArtifactPin(output) => format!(
                "artifact_pin artifact_id={} pin_status={} reads={}",
                output.artifact_id, output.pin_status, output.explicit_read_count
            ),
            Self::ArtifactUnpin(output) => format!(
                "artifact_unpin artifact_id={} pin_status={} reads={}",
                output.artifact_id, output.pin_status, output.explicit_read_count
            ),
            Self::DeliverFile(output) => format!(
                "deliver_file request_id={} file_name={} target={} status={}",
                output.request_id,
                output.file_name,
                output.target.as_str(),
                output.status
            ),
            Self::KnowledgeSearch(output) => {
                if let Some(next_offset) = output.next_offset {
                    format!(
                        "knowledge_search results={} total={} truncated next_offset={}",
                        output.results.len(),
                        output.total_results,
                        next_offset
                    )
                } else {
                    format!("knowledge_search results={}", output.results.len())
                }
            }
            Self::KnowledgeRead(output) => format!(
                "knowledge_read path={} mode={} lines={} truncated={}",
                output.path,
                output.mode.as_str(),
                output.text.lines().count(),
                output.truncated
            ),
            Self::SessionSearch(output) => {
                if let Some(next_offset) = output.next_offset {
                    format!(
                        "session_search results={} total={} truncated next_offset={}",
                        output.results.len(),
                        output.total_results,
                        next_offset
                    )
                } else {
                    format!("session_search results={}", output.results.len())
                }
            }
            Self::SessionRead(output) => format!(
                "session_read session_id={} mode={} tier={} from_archive={} messages={} artifacts={}",
                output.session_id,
                output.mode.as_str(),
                output.tier.as_str(),
                output.from_archive,
                output.messages.len(),
                output.artifacts.len()
            ),
            Self::SessionWait(output) => format!(
                "session_wait session_id={} settled={} active_runs={} active_jobs={} mode={} messages={}",
                output.session_id,
                output.settled,
                output.active_run_count,
                output.active_job_count,
                output.snapshot.mode.as_str(),
                output.snapshot.messages.len()
            ),
            Self::McpCall(output) => format!(
                "mcp_call connector_id={} exposed_name={} is_error={}",
                output.connector_id, output.exposed_name, output.is_error
            ),
            Self::McpSearchResources(output) => {
                if let Some(next_offset) = output.next_offset {
                    format!(
                        "mcp_search_resources results={} total={} truncated next_offset={}",
                        output.results.len(),
                        output.total_results,
                        next_offset
                    )
                } else {
                    format!("mcp_search_resources results={}", output.results.len())
                }
            }
            Self::McpReadResource(output) => format!(
                "mcp_read_resource connector_id={} uri={} contents={}",
                output.connector_id,
                output.uri,
                output.contents.len()
            ),
            Self::McpSearchPrompts(output) => {
                if let Some(next_offset) = output.next_offset {
                    format!(
                        "mcp_search_prompts results={} total={} truncated next_offset={}",
                        output.results.len(),
                        output.total_results,
                        next_offset
                    )
                } else {
                    format!("mcp_search_prompts results={}", output.results.len())
                }
            }
            Self::McpGetPrompt(output) => format!(
                "mcp_get_prompt connector_id={} name={} messages={}",
                output.connector_id,
                output.name,
                output.messages.len()
            ),
            Self::AgentList(output) => {
                if let Some(next_offset) = output.next_offset {
                    format!(
                        "agent_list agents={} total={} truncated next_offset={}",
                        output.agents.len(),
                        output.total_agents,
                        next_offset
                    )
                } else {
                    format!("agent_list agents={}", output.agents.len())
                }
            }
            Self::AgentRead(output) => format!("agent_read id={}", output.agent.id),
            Self::AgentCreate(output) => format!(
                "agent_create id={} template={}",
                output.agent.id,
                output.agent.template_kind.as_str()
            ),
            Self::ContinueLater(output) => format!(
                "continue_later schedule_id={} delivery_mode={}",
                output.schedule.id,
                output.schedule.delivery_mode.as_str()
            ),
            Self::ScheduleList(output) => {
                if let Some(next_offset) = output.next_offset {
                    format!(
                        "schedule_list schedules={} total={} truncated next_offset={}",
                        output.schedules.len(),
                        output.total_schedules,
                        next_offset
                    )
                } else {
                    format!("schedule_list schedules={}", output.schedules.len())
                }
            }
            Self::ScheduleRead(output) => format!("schedule_read id={}", output.schedule.id),
            Self::ScheduleCreate(output) => format!(
                "schedule_create id={} agent_profile_id={}",
                output.schedule.id, output.schedule.agent_profile_id
            ),
            Self::ScheduleUpdate(output) => format!(
                "schedule_update id={} enabled={}",
                output.schedule.id, output.schedule.enabled
            ),
            Self::ScheduleDelete(output) => {
                format!(
                    "schedule_delete id={} deleted={}",
                    output.id, output.deleted
                )
            }
            Self::MessageAgent(output) => format!(
                "message_agent target_agent_id={} recipient_session_id={} recipient_job_id={} chain_id={} hop_count={} delivery_status=queued",
                output.target_agent_id,
                output.recipient_session_id,
                output.recipient_job_id,
                output.chain_id,
                output.hop_count
            ),
            Self::GrantAgentChainContinuation(output) => format!(
                "grant_agent_chain_continuation chain_id={} granted_hops={}",
                output.chain_id, output.granted_hops
            ),
        }
    }

    pub fn model_output(&self) -> String {
        match self {
            Self::FsRead(output) => json!({
                "tool": "fs_read",
                "path": output.path,
                "content": output.content,
            })
            .to_string(),
            Self::FsReadText(output) => json!({
                "tool": "fs_read_text",
                "path": output.path,
                "content": output.content,
            })
            .to_string(),
            Self::FsReadLines(output) => json!({
                "tool": "fs_read_lines",
                "path": output.path,
                "start_line": output.start_line,
                "end_line": output.end_line,
                "total_lines": output.total_lines,
                "eof": output.eof,
                "next_start_line": output.next_start_line,
                "content": output.content,
            })
            .to_string(),
            Self::FsWrite(output) => json!({
                "tool": "fs_write",
                "path": output.path,
                "bytes_written": output.bytes_written,
            })
            .to_string(),
            Self::FsWriteText(output) => json!({
                "tool": "fs_write_text",
                "path": output.path,
                "mode": output.mode.as_str(),
                "bytes_written": output.bytes_written,
                "created": output.created,
                "overwritten": output.overwritten,
            })
            .to_string(),
            Self::FsPatch(output) => json!({
                "tool": "fs_patch",
                "path": output.path,
                "bytes_written": output.bytes_written,
                "edits_applied": output.edits_applied,
            })
            .to_string(),
            Self::FsPatchText(output) => json!({
                "tool": "fs_patch_text",
                "path": output.path,
                "bytes_written": output.bytes_written,
            })
            .to_string(),
            Self::FsReplaceLines(output) => json!({
                "tool": "fs_replace_lines",
                "path": output.path,
                "start_line": output.start_line,
                "end_line": output.end_line,
                "bytes_written": output.bytes_written,
            })
            .to_string(),
            Self::FsInsertText(output) => json!({
                "tool": "fs_insert_text",
                "path": output.path,
                "position": output.position,
                "line": output.line,
                "bytes_written": output.bytes_written,
            })
            .to_string(),
            Self::FsMkdir(output) => json!({
                "tool": "fs_mkdir",
                "path": output.path,
            })
            .to_string(),
            Self::FsMove(output) => json!({
                "tool": "fs_move",
                "src": output.src,
                "dest": output.dest,
            })
            .to_string(),
            Self::FsTrash(output) => json!({
                "tool": "fs_trash",
                "path": output.path,
                "trashed_to": output.trashed_to,
            })
            .to_string(),
            Self::FsList(output) => json!({
                "tool": "fs_list",
                "offset": output.offset,
                "limit": output.limit,
                "truncated": output.truncated,
                "total_entries": output.total_entries,
                "next_offset": output.next_offset,
                "entries": output.entries.iter().map(workspace_entry_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::FsGlob(output) => json!({
                "tool": "fs_glob",
                "offset": output.offset,
                "limit": output.limit,
                "truncated": output.truncated,
                "total_entries": output.total_entries,
                "next_offset": output.next_offset,
                "entries": output.entries.iter().map(workspace_entry_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::FsSearch(output) => json!({
                "tool": "fs_search",
                "matches": output.matches.iter().map(workspace_match_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::FsSearchText(output) => json!({
                "tool": "fs_search_text",
                "matches": output.matches.iter().map(workspace_match_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::FsFindInFiles(output) => json!({
                "tool": "fs_find_in_files",
                "matches": output.matches.iter().map(workspace_match_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::WebFetch(output) => json!({
                "tool": "web_fetch",
                "url": output.url,
                "status_code": output.status_code,
                "content_type": output.content_type,
                "title": output.title,
                "extracted_from_html": output.extracted_from_html,
                "body": output.body,
            })
            .to_string(),
            Self::WebSearch(output) => json!({
                "tool": "web_search",
                "query": output.query,
                "results": output.results.iter().map(|result| json!({
                    "title": result.title,
                    "url": result.url,
                    "snippet": result.snippet,
                })).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::BrowserOpen(output)
            | Self::BrowserSnapshot(output)
            | Self::BrowserText(output)
            | Self::BrowserClick(output)
            | Self::BrowserFill(output)
            | Self::BrowserPress(output)
            | Self::BrowserWait(output)
            | Self::BrowserScroll(output)
            | Self::BrowserEval(output)
            | Self::BrowserScreenshot(output)
            | Self::BrowserPdf(output)
            | Self::BrowserStatus(output)
            | Self::BrowserClose(output) => json!({
                "tool": output.action,
                "session": output.session,
                "stdout": output.stdout,
                "stderr": output.stderr,
                "workspace_path": output.workspace_path,
                "note": "agent-browser refs like @eN are fresh per browser_snapshot and become stale after page-changing actions"
            })
            .to_string(),
            Self::ProcessStart(output) => json!({
                "tool": "process_start",
                "process_id": output.process_id,
                "pid_ref": output.pid_ref,
                "kind": output.kind.as_str(),
                "cwd": output.cwd,
                "command": output.command_display,
            })
            .to_string(),
            Self::ProcessOutputRead(output) => json!({
                "tool": "process_output_read",
                "process_id": output.process_id,
                "stream": output.stream.as_str(),
                "status": output.status.as_str(),
                "exit_code": output.exit_code,
                "cursor": output.cursor,
                "next_cursor": output.next_cursor,
                "truncated": output.truncated,
                "text": output.text,
            })
            .to_string(),
            Self::ProcessResult(output) => json!({
                "tool": "process_result",
                "process_id": output.process_id,
                "status": format!("{:?}", output.status).to_lowercase(),
                "exit_code": output.exit_code,
                "stdout": output.stdout,
                "stderr": output.stderr,
            })
            .to_string(),
            Self::PlanRead(output) => json!({
                "tool": "plan_read",
                "goal": output.goal,
                "items": output.items.iter().map(plan_item_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::PlanWrite(output) => json!({
                "tool": "plan_write",
                "goal": output.goal,
                "items": output.items.iter().map(plan_item_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::InitPlan(output) => json!({
                "tool": "init_plan",
                "goal": output.goal,
                "item_count": output.item_count,
            })
            .to_string(),
            Self::AddTask(output) => json!({
                "tool": "add_task",
                "task": plan_item_json(&output.task),
            })
            .to_string(),
            Self::SetTaskStatus(output) => json!({
                "tool": "set_task_status",
                "task": plan_item_json(&output.task),
            })
            .to_string(),
            Self::AddTaskNote(output) => json!({
                "tool": "add_task_note",
                "task": plan_item_json(&output.task),
            })
            .to_string(),
            Self::EditTask(output) => json!({
                "tool": "edit_task",
                "task": plan_item_json(&output.task),
            })
            .to_string(),
            Self::PlanSnapshot(output) => json!({
                "tool": "plan_snapshot",
                "goal": output.goal,
                "items": output.items.iter().map(plan_item_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::PlanLint(output) => json!({
                "tool": "plan_lint",
                "ok": output.ok,
                "issues": output.issues.iter().map(|issue| json!({
                    "severity": issue.severity,
                    "task_id": issue.task_id,
                    "message": issue.message,
                })).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::PromptBudgetRead(output) => json!({
                "tool": "prompt_budget_read",
                "session_id": output.session_id,
                "source": output.source,
                "pending_next_turn_override": output.pending_next_turn_override,
                "context_window_tokens": output.context_window_tokens,
                "auto_compaction_trigger_basis_points": output.auto_compaction_trigger_basis_points,
                "usable_context_tokens": output.usable_context_tokens,
                "total_percent": output.total_percent,
                "layers": output.layers.iter().map(|layer| json!({
                    "layer": layer.layer,
                    "percent": layer.percent,
                    "target_tokens": layer.target_tokens,
                })).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::PromptBudgetUpdate(output) => json!({
                "tool": "prompt_budget_update",
                "session_id": output.session_id,
                "scope": output.scope,
                "reset": output.reset,
                "reason": output.reason,
                "source": output.budget.source,
                "context_window_tokens": output.budget.context_window_tokens,
                "auto_compaction_trigger_basis_points": output.budget.auto_compaction_trigger_basis_points,
                "usable_context_tokens": output.budget.usable_context_tokens,
                "total_percent": output.budget.total_percent,
                "layers": output.budget.layers.iter().map(|layer| json!({
                    "layer": layer.layer,
                    "percent": layer.percent,
                    "target_tokens": layer.target_tokens,
                })).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::AutonomyStateRead(output) => json!({
                "tool": "autonomy_state_read",
                "session_id": output.session_id,
                "title": output.title,
                "agent_profile_id": output.agent_profile_id,
                "turn_source": output.turn_source,
                "parent_session_id": output.parent_session_id,
                "parent_job_id": output.parent_job_id,
                "delegation_label": output.delegation_label,
                "schedules": output.schedules.iter().map(schedule_view_json).collect::<Vec<_>>(),
                "active_jobs": output.active_jobs.iter().map(autonomy_job_json).collect::<Vec<_>>(),
                "child_sessions": output.child_sessions.iter().map(autonomy_child_session_json).collect::<Vec<_>>(),
                "inbox_events": output.inbox_events.iter().map(autonomy_inbox_event_json).collect::<Vec<_>>(),
                "interagent": output.interagent.as_ref().map(autonomy_interagent_json),
                "mesh_peers": output.mesh_peers.iter().map(autonomy_mesh_peer_json).collect::<Vec<_>>(),
                "truncated": output.truncated,
                "max_items": output.max_items,
            })
            .to_string(),
            Self::SkillList(output) => json!({
                "tool": "skill_list",
                "session_id": output.session_id,
                "include_inactive": output.include_inactive,
                "offset": output.offset,
                "limit": output.limit,
                "total_results": output.total_results,
                "next_offset": output.next_offset,
                "skills": output.skills.iter().map(skill_status_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::SkillRead(output) => json!({
                "tool": "skill_read",
                "session_id": output.session_id,
                "name": output.name,
                "description": output.description,
                "mode": output.mode,
                "skill_dir": output.skill_dir,
                "skill_md_path": output.skill_md_path,
                "body": output.body,
                "body_byte_len": output.body_byte_len,
                "body_truncated": output.body_truncated,
            })
            .to_string(),
            Self::SkillEnable(output) => json!({
                "tool": "skill_enable",
                "session_id": output.session_id,
                "name": output.name,
                "mode": output.mode,
                "skills": output.skills.iter().map(skill_status_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::SkillDisable(output) => json!({
                "tool": "skill_disable",
                "session_id": output.session_id,
                "name": output.name,
                "mode": output.mode,
                "skills": output.skills.iter().map(skill_status_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::ArtifactRead(output) => json!({
                "tool": "artifact_read",
                "ref_id": output.ref_id,
                "artifact_id": output.artifact_id,
                "label": output.label,
                "summary": output.summary,
                "content": output.content,
                "offset": output.offset,
                "content_byte_len": output.content_byte_len,
                "total_byte_len": output.total_byte_len,
                "content_truncated": output.content_truncated,
                "next_offset": output.next_offset,
            })
            .to_string(),
            Self::ArtifactSearch(output) => json!({
                "tool": "artifact_search",
                "query": output.query,
                "results": output.results.iter().map(|result| json!({
                    "ref_id": result.ref_id,
                    "artifact_id": result.artifact_id,
                    "label": result.label,
                    "summary": result.summary,
                    "token_estimate": result.token_estimate,
                    "message_count": result.message_count,
                    "preview": result.preview,
                })).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::ArtifactPin(output) => json!({
                "tool": "artifact_pin",
                "ref_id": output.ref_id,
                "artifact_id": output.artifact_id,
                "pinned": output.pinned,
                "explicit_read_count": output.explicit_read_count,
                "pin_status": output.pin_status,
            })
            .to_string(),
            Self::ArtifactUnpin(output) => json!({
                "tool": "artifact_unpin",
                "ref_id": output.ref_id,
                "artifact_id": output.artifact_id,
                "pinned": output.pinned,
                "explicit_read_count": output.explicit_read_count,
                "pin_status": output.pin_status,
            })
            .to_string(),
            Self::DeliverFile(output) => json!({
                "tool": "deliver_file",
                "request_id": output.request_id,
                "target": output.target.as_str(),
                "file_name": output.file_name,
                "caption": output.caption,
                "status": output.status,
                "delivery_note": "queued for the current operator surface; queued is success, not final delivery; Telegram sends queued files as documents after the current turn and reports delivery failures to the chat; tell the user the file was queued/sent as a document, do not mention internal artifact ids, and do not invent Obsidian/vault fallback paths"
            })
            .to_string(),
            Self::KnowledgeSearch(output) => json!({
                "tool": "knowledge_search",
                "query": output.query,
                "results": output.results.iter().map(|result| json!({
                    "path": result.path,
                    "kind": result.kind.as_str(),
                    "snippet": result.snippet,
                    "sha256": result.sha256,
                    "mtime": result.mtime,
                })).collect::<Vec<_>>(),
                "truncated": output.truncated,
                "offset": output.offset,
                "limit": output.limit,
                "total_results": output.total_results,
                "next_offset": output.next_offset,
            })
            .to_string(),
            Self::KnowledgeRead(output) => json!({
                "tool": "knowledge_read",
                "path": output.path,
                "kind": output.kind.as_str(),
                "sha256": output.sha256,
                "mtime": output.mtime,
                "mode": output.mode.as_str(),
                "cursor": output.cursor,
                "next_cursor": output.next_cursor,
                "truncated": output.truncated,
                "total_lines": output.total_lines,
                "start_line": output.start_line,
                "end_line": output.end_line,
                "text": output.text,
            })
            .to_string(),
            Self::SessionSearch(output) => json!({
                "tool": "session_search",
                "query": output.query,
                "results": output.results.iter().map(|result| json!({
                    "session_id": result.session_id,
                    "title": result.title,
                    "agent_profile_id": result.agent_profile_id,
                    "tier": result.tier.as_str(),
                    "updated_at": result.updated_at,
                    "match_source": result.match_source.as_str(),
                    "snippet": result.snippet,
                })).collect::<Vec<_>>(),
                "truncated": output.truncated,
                "offset": output.offset,
                "limit": output.limit,
                "total_results": output.total_results,
                "next_offset": output.next_offset,
            })
            .to_string(),
            Self::SessionRead(output) => json!({
                "tool": "session_read",
                "session_id": output.session_id,
                "title": output.title,
                "agent_profile_id": output.agent_profile_id,
                "mode": output.mode.as_str(),
                "tier": output.tier.as_str(),
                "from_archive": output.from_archive,
                "cursor": output.cursor,
                "next_cursor": output.next_cursor,
                "truncated": output.truncated,
                "total_items": output.total_items,
                "summary": output.summary.as_ref().map(|summary| json!({
                    "summary_text": summary.summary_text,
                    "covered_message_count": summary.covered_message_count,
                    "summary_token_estimate": summary.summary_token_estimate,
                    "updated_at": summary.updated_at,
                })),
                "messages": output.messages.iter().map(|message| json!({
                    "id": message.id,
                    "run_id": message.run_id,
                    "role": message.role,
                    "created_at": message.created_at,
                    "content": message.content,
                })).collect::<Vec<_>>(),
                "artifacts": output.artifacts.iter().map(|artifact| json!({
                    "artifact_id": artifact.artifact_id,
                    "kind": artifact.kind,
                    "path": artifact.path,
                    "byte_len": artifact.byte_len,
                    "created_at": artifact.created_at,
                    "label": artifact.label,
                    "summary": artifact.summary,
                })).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::SessionWait(output) => json!({
                "tool": "session_wait",
                "session_id": output.session_id,
                "wait_timeout_ms": output.wait_timeout_ms,
                "settled": output.settled,
                "active_run_count": output.active_run_count,
                "active_job_count": output.active_job_count,
                "latest_run_id": output.latest_run_id,
                "latest_run_status": output.latest_run_status,
                "latest_job_id": output.latest_job_id,
                "latest_job_status": output.latest_job_status,
                "snapshot": {
                    "session_id": output.snapshot.session_id,
                    "title": output.snapshot.title,
                    "agent_profile_id": output.snapshot.agent_profile_id,
                    "mode": output.snapshot.mode.as_str(),
                    "tier": output.snapshot.tier.as_str(),
                    "from_archive": output.snapshot.from_archive,
                    "cursor": output.snapshot.cursor,
                    "next_cursor": output.snapshot.next_cursor,
                    "truncated": output.snapshot.truncated,
                    "total_items": output.snapshot.total_items,
                    "summary": output.snapshot.summary.as_ref().map(|summary| json!({
                        "summary_text": summary.summary_text,
                        "covered_message_count": summary.covered_message_count,
                        "summary_token_estimate": summary.summary_token_estimate,
                        "updated_at": summary.updated_at,
                    })),
                    "messages": output.snapshot.messages.iter().map(|message| json!({
                        "id": message.id,
                        "run_id": message.run_id,
                        "role": message.role,
                        "created_at": message.created_at,
                        "content": message.content,
                    })).collect::<Vec<_>>(),
                    "artifacts": output.snapshot.artifacts.iter().map(|artifact| json!({
                        "artifact_id": artifact.artifact_id,
                        "kind": artifact.kind,
                        "path": artifact.path,
                        "byte_len": artifact.byte_len,
                        "created_at": artifact.created_at,
                        "label": artifact.label,
                        "summary": artifact.summary,
                    })).collect::<Vec<_>>(),
                }
            })
            .to_string(),
            Self::McpCall(output) => json!({
                "tool": "mcp_call",
                "connector_id": output.connector_id,
                "exposed_name": output.exposed_name,
                "remote_name": output.remote_name,
                "content_text": output.content_text,
                "structured_content_json": output.structured_content_json,
                "is_error": output.is_error,
            })
            .to_string(),
            Self::McpSearchResources(output) => json!({
                "tool": "mcp_search_resources",
                "connector_id": output.connector_id,
                "query": output.query,
                "results": output.results.iter().map(|result| json!({
                    "connector_id": result.connector_id,
                    "uri": result.uri,
                    "name": result.name,
                    "title": result.title,
                    "description": result.description,
                    "mime_type": result.mime_type,
                })).collect::<Vec<_>>(),
                "truncated": output.truncated,
                "offset": output.offset,
                "limit": output.limit,
                "total_results": output.total_results,
                "next_offset": output.next_offset,
            })
            .to_string(),
            Self::McpReadResource(output) => json!({
                "tool": "mcp_read_resource",
                "connector_id": output.connector_id,
                "uri": output.uri,
                "text": output.text,
                "contents": output.contents.iter().map(|content| json!({
                    "kind": content.kind,
                    "uri": content.uri,
                    "mime_type": content.mime_type,
                    "text": content.text,
                    "blob": content.blob,
                })).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::McpSearchPrompts(output) => json!({
                "tool": "mcp_search_prompts",
                "connector_id": output.connector_id,
                "query": output.query,
                "results": output.results.iter().map(|result| json!({
                    "connector_id": result.connector_id,
                    "name": result.name,
                    "title": result.title,
                    "description": result.description,
                    "arguments": result.arguments.iter().map(|argument| json!({
                        "name": argument.name,
                        "description": argument.description,
                        "required": argument.required,
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "truncated": output.truncated,
                "offset": output.offset,
                "limit": output.limit,
                "total_results": output.total_results,
                "next_offset": output.next_offset,
            })
            .to_string(),
            Self::McpGetPrompt(output) => json!({
                "tool": "mcp_get_prompt",
                "connector_id": output.connector_id,
                "name": output.name,
                "description": output.description,
                "text": output.text,
                "messages": output.messages.iter().map(|message| json!({
                    "role": message.role,
                    "content_type": message.content_type,
                    "text": message.text,
                    "uri": message.uri,
                    "mime_type": message.mime_type,
                })).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::AgentList(output) => json!({
                "tool": "agent_list",
                "agents": output.agents.iter().map(|agent| json!({
                    "id": agent.id,
                    "name": agent.name,
                    "template_kind": agent.template_kind.as_str(),
                    "agent_home": agent.agent_home,
                    "default_workspace_root": agent.default_workspace_root,
                    "allowed_tool_count": agent.allowed_tool_count,
                    "created_from_template_id": agent.created_from_template_id,
                    "created_by_session_id": agent.created_by_session_id,
                    "created_by_agent_profile_id": agent.created_by_agent_profile_id,
                    "created_at": agent.created_at,
                    "updated_at": agent.updated_at,
                })).collect::<Vec<_>>(),
                "truncated": output.truncated,
                "offset": output.offset,
                "limit": output.limit,
                "total_agents": output.total_agents,
                "next_offset": output.next_offset,
            })
            .to_string(),
            Self::AgentRead(output) => json!({
                "tool": "agent_read",
                "agent": {
                    "id": output.agent.id,
                    "name": output.agent.name,
                    "template_kind": output.agent.template_kind.as_str(),
                    "agent_home": output.agent.agent_home,
                    "default_workspace_root": output.agent.default_workspace_root,
                    "allowed_tool_count": output.agent.allowed_tool_count,
                    "created_from_template_id": output.agent.created_from_template_id,
                    "created_by_session_id": output.agent.created_by_session_id,
                    "created_by_agent_profile_id": output.agent.created_by_agent_profile_id,
                    "created_at": output.agent.created_at,
                    "updated_at": output.agent.updated_at,
                },
                "allowed_tools": output.allowed_tools,
            })
            .to_string(),
            Self::AgentCreate(output) => json!({
                "tool": "agent_create",
                "agent": {
                    "id": output.agent.id,
                    "name": output.agent.name,
                    "template_kind": output.agent.template_kind.as_str(),
                    "agent_home": output.agent.agent_home,
                    "default_workspace_root": output.agent.default_workspace_root,
                    "allowed_tool_count": output.agent.allowed_tool_count,
                    "created_from_template_id": output.agent.created_from_template_id,
                    "created_by_session_id": output.agent.created_by_session_id,
                    "created_by_agent_profile_id": output.agent.created_by_agent_profile_id,
                    "created_at": output.agent.created_at,
                    "updated_at": output.agent.updated_at,
                }
            })
            .to_string(),
            Self::ContinueLater(output) => json!({
                "tool": "continue_later",
                "schedule": {
                    "id": output.schedule.id,
                    "agent_profile_id": output.schedule.agent_profile_id,
                    "workspace_root": output.schedule.workspace_root,
                    "prompt": output.schedule.prompt,
                    "mode": output.schedule.mode.as_str(),
                    "delivery_mode": output.schedule.delivery_mode.as_str(),
                    "target_session_id": output.schedule.target_session_id,
                    "interval_seconds": output.schedule.interval_seconds,
                    "next_fire_at": output.schedule.next_fire_at,
                    "enabled": output.schedule.enabled,
                    "last_triggered_at": output.schedule.last_triggered_at,
                    "last_finished_at": output.schedule.last_finished_at,
                    "last_session_id": output.schedule.last_session_id,
                    "last_job_id": output.schedule.last_job_id,
                    "last_result": output.schedule.last_result,
                    "last_error": output.schedule.last_error,
                    "created_at": output.schedule.created_at,
                    "updated_at": output.schedule.updated_at,
                }
            })
            .to_string(),
            Self::ScheduleList(output) => json!({
                "tool": "schedule_list",
                "schedules": output.schedules.iter().map(|schedule| json!({
                    "id": schedule.id,
                    "agent_profile_id": schedule.agent_profile_id,
                    "workspace_root": schedule.workspace_root,
                    "prompt": schedule.prompt,
                    "mode": schedule.mode.as_str(),
                    "delivery_mode": schedule.delivery_mode.as_str(),
                    "target_session_id": schedule.target_session_id,
                    "interval_seconds": schedule.interval_seconds,
                    "next_fire_at": schedule.next_fire_at,
                    "enabled": schedule.enabled,
                    "last_triggered_at": schedule.last_triggered_at,
                    "last_finished_at": schedule.last_finished_at,
                    "last_session_id": schedule.last_session_id,
                    "last_job_id": schedule.last_job_id,
                    "last_result": schedule.last_result,
                    "last_error": schedule.last_error,
                    "created_at": schedule.created_at,
                    "updated_at": schedule.updated_at,
                })).collect::<Vec<_>>(),
                "truncated": output.truncated,
                "offset": output.offset,
                "limit": output.limit,
                "total_schedules": output.total_schedules,
                "next_offset": output.next_offset,
            })
            .to_string(),
            Self::ScheduleRead(output) => json!({
                "tool": "schedule_read",
                "schedule": {
                    "id": output.schedule.id,
                    "agent_profile_id": output.schedule.agent_profile_id,
                    "workspace_root": output.schedule.workspace_root,
                    "prompt": output.schedule.prompt,
                    "mode": output.schedule.mode.as_str(),
                    "delivery_mode": output.schedule.delivery_mode.as_str(),
                    "target_session_id": output.schedule.target_session_id,
                    "interval_seconds": output.schedule.interval_seconds,
                    "next_fire_at": output.schedule.next_fire_at,
                    "enabled": output.schedule.enabled,
                    "last_triggered_at": output.schedule.last_triggered_at,
                    "last_finished_at": output.schedule.last_finished_at,
                    "last_session_id": output.schedule.last_session_id,
                    "last_job_id": output.schedule.last_job_id,
                    "last_result": output.schedule.last_result,
                    "last_error": output.schedule.last_error,
                    "created_at": output.schedule.created_at,
                    "updated_at": output.schedule.updated_at,
                }
            })
            .to_string(),
            Self::ScheduleCreate(output) => json!({
                "tool": "schedule_create",
                "schedule": {
                    "id": output.schedule.id,
                    "agent_profile_id": output.schedule.agent_profile_id,
                    "workspace_root": output.schedule.workspace_root,
                    "prompt": output.schedule.prompt,
                    "mode": output.schedule.mode.as_str(),
                    "delivery_mode": output.schedule.delivery_mode.as_str(),
                    "target_session_id": output.schedule.target_session_id,
                    "interval_seconds": output.schedule.interval_seconds,
                    "next_fire_at": output.schedule.next_fire_at,
                    "enabled": output.schedule.enabled,
                    "last_triggered_at": output.schedule.last_triggered_at,
                    "last_finished_at": output.schedule.last_finished_at,
                    "last_session_id": output.schedule.last_session_id,
                    "last_job_id": output.schedule.last_job_id,
                    "last_result": output.schedule.last_result,
                    "last_error": output.schedule.last_error,
                    "created_at": output.schedule.created_at,
                    "updated_at": output.schedule.updated_at,
                }
            })
            .to_string(),
            Self::ScheduleUpdate(output) => json!({
                "tool": "schedule_update",
                "schedule": {
                    "id": output.schedule.id,
                    "agent_profile_id": output.schedule.agent_profile_id,
                    "workspace_root": output.schedule.workspace_root,
                    "prompt": output.schedule.prompt,
                    "mode": output.schedule.mode.as_str(),
                    "delivery_mode": output.schedule.delivery_mode.as_str(),
                    "target_session_id": output.schedule.target_session_id,
                    "interval_seconds": output.schedule.interval_seconds,
                    "next_fire_at": output.schedule.next_fire_at,
                    "enabled": output.schedule.enabled,
                    "last_triggered_at": output.schedule.last_triggered_at,
                    "last_finished_at": output.schedule.last_finished_at,
                    "last_session_id": output.schedule.last_session_id,
                    "last_job_id": output.schedule.last_job_id,
                    "last_result": output.schedule.last_result,
                    "last_error": output.schedule.last_error,
                    "created_at": output.schedule.created_at,
                    "updated_at": output.schedule.updated_at,
                }
            })
            .to_string(),
            Self::ScheduleDelete(output) => json!({
                "tool": "schedule_delete",
                "id": output.id,
                "deleted": output.deleted,
            })
            .to_string(),
            Self::MessageAgent(output) => json!({
                "tool": "message_agent",
                "target_agent_id": output.target_agent_id,
                "recipient_session_id": output.recipient_session_id,
                "recipient_job_id": output.recipient_job_id,
                "chain_id": output.chain_id,
                "hop_count": output.hop_count,
                "delivery_status": "queued",
            })
            .to_string(),
            Self::GrantAgentChainContinuation(output) => json!({
                "tool": "grant_agent_chain_continuation",
                "chain_id": output.chain_id,
                "granted_hops": output.granted_hops,
            })
            .to_string(),
        }
    }
}

fn workspace_entry_json(entry: &WorkspaceEntry) -> Value {
    json!({
        "path": entry.path,
        "kind": match entry.kind {
            crate::workspace::WorkspaceEntryKind::File => "file",
            crate::workspace::WorkspaceEntryKind::Directory => "directory",
        },
        "bytes": entry.bytes,
    })
}

fn workspace_match_json(entry: &WorkspaceSearchMatch) -> Value {
    json!({
        "path": entry.path,
        "line_number": entry.line_number,
        "line": entry.line,
    })
}

fn plan_item_json(item: &PlanItem) -> Value {
    json!({
        "id": item.id,
        "content": item.content,
        "status": item.status.as_str(),
        "depends_on": item.depends_on,
        "notes": item.notes,
        "blocked_reason": item.blocked_reason,
        "parent_task_id": item.parent_task_id,
    })
}

fn schedule_view_json(schedule: &ScheduleViewOutput) -> Value {
    json!({
        "id": schedule.id,
        "agent_profile_id": schedule.agent_profile_id,
        "workspace_root": schedule.workspace_root,
        "prompt": schedule.prompt,
        "mode": schedule.mode.as_str(),
        "delivery_mode": schedule.delivery_mode.as_str(),
        "target_session_id": schedule.target_session_id,
        "interval_seconds": schedule.interval_seconds,
        "next_fire_at": schedule.next_fire_at,
        "enabled": schedule.enabled,
        "last_triggered_at": schedule.last_triggered_at,
        "last_finished_at": schedule.last_finished_at,
        "last_session_id": schedule.last_session_id,
        "last_job_id": schedule.last_job_id,
        "last_result": schedule.last_result,
        "last_error": schedule.last_error,
        "created_at": schedule.created_at,
        "updated_at": schedule.updated_at,
    })
}

fn autonomy_job_json(job: &AutonomyJobOutput) -> Value {
    json!({
        "id": job.id,
        "kind": job.kind,
        "status": job.status,
        "run_id": job.run_id,
        "parent_job_id": job.parent_job_id,
        "last_progress_message": job.last_progress_message,
        "updated_at": job.updated_at,
    })
}

fn autonomy_child_session_json(session: &AutonomyChildSessionOutput) -> Value {
    json!({
        "id": session.id,
        "title": session.title,
        "agent_profile_id": session.agent_profile_id,
        "parent_job_id": session.parent_job_id,
        "delegation_label": session.delegation_label,
        "updated_at": session.updated_at,
    })
}

fn autonomy_inbox_event_json(event: &AutonomyInboxEventOutput) -> Value {
    json!({
        "id": event.id,
        "kind": event.kind,
        "job_id": event.job_id,
        "status": event.status,
        "available_at": event.available_at,
        "error": event.error,
    })
}

fn autonomy_interagent_json(chain: &AutonomyInteragentOutput) -> Value {
    json!({
        "chain_id": chain.chain_id,
        "origin_session_id": chain.origin_session_id,
        "origin_agent_id": chain.origin_agent_id,
        "hop_count": chain.hop_count,
        "max_hops": chain.max_hops,
        "parent_interagent_session_id": chain.parent_interagent_session_id,
        "state": chain.state,
    })
}

fn autonomy_mesh_peer_json(peer: &AutonomyMeshPeerOutput) -> Value {
    json!({
        "peer_id": peer.peer_id,
        "base_url": peer.base_url,
        "has_bearer_token": peer.has_bearer_token,
    })
}

fn skill_status_json(skill: &SkillStatusOutput) -> Value {
    json!({
        "name": skill.name,
        "description": skill.description,
        "mode": skill.mode,
        "skill_dir": skill.skill_dir,
        "skill_md_path": skill.skill_md_path,
    })
}
