use crate::agent::{AgentScheduleDeliveryMode, AgentScheduleMode, AgentTemplateKind};
use crate::memory::SessionRetentionTier;
use crate::plan::{PlanItem, PlanItemStatus, PlanItemStatusParseError, PlanLintIssue};
use crate::session::PromptBudgetPolicy;
use crate::workspace::{
    WorkspaceEntry, WorkspaceError, WorkspaceRef, WorkspaceSearchMatch, WriteMode,
};
use html_to_markdown_rs::convert as convert_html_to_markdown;
use reqwest::Url;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
#[cfg(unix)]
use std::io;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_FS_LIST_LIMIT: usize = 200;
const MAX_FS_LIST_LIMIT: usize = 1_000;
const DEFAULT_PROCESS_OUTPUT_READ_MAX_BYTES: usize = 4 * 1024;
const MAX_PROCESS_OUTPUT_READ_MAX_BYTES: usize = 32 * 1024;
const DEFAULT_PROCESS_OUTPUT_READ_MAX_LINES: usize = 20;
const MAX_PROCESS_OUTPUT_READ_MAX_LINES: usize = 200;
const PROCESS_READER_DRAIN_GRACE: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy)]
struct EnumLikeFieldRepair {
    field: &'static str,
    allowed_values: &'static [&'static str],
}

const KNOWLEDGE_READ_ENUM_REPAIRS: &[EnumLikeFieldRepair] = &[EnumLikeFieldRepair {
    field: "mode",
    allowed_values: &["excerpt", "full"],
}];

const SESSION_READ_ENUM_REPAIRS: &[EnumLikeFieldRepair] = &[EnumLikeFieldRepair {
    field: "mode",
    allowed_values: &["summary", "timeline", "transcript", "artifacts"],
}];

const SESSION_WAIT_ENUM_REPAIRS: &[EnumLikeFieldRepair] = &[EnumLikeFieldRepair {
    field: "mode",
    allowed_values: &["summary", "timeline", "transcript", "artifacts"],
}];

const CONTINUE_LATER_ENUM_REPAIRS: &[EnumLikeFieldRepair] = &[EnumLikeFieldRepair {
    field: "delivery_mode",
    allowed_values: &["fresh_session", "existing_session"],
}];

const SCHEDULE_ENUM_REPAIRS: &[EnumLikeFieldRepair] = &[
    EnumLikeFieldRepair {
        field: "mode",
        allowed_values: &["interval", "after_completion", "once"],
    },
    EnumLikeFieldRepair {
        field: "delivery_mode",
        allowed_values: &["fresh_session", "existing_session"],
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolFamily {
    Filesystem,
    Web,
    Exec,
    Planning,
    Offload,
    Memory,
    Mcp,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolName {
    FsRead,
    FsWrite,
    FsPatch,
    FsReadText,
    FsReadLines,
    FsSearchText,
    FsFindInFiles,
    FsWriteText,
    FsPatchText,
    FsReplaceLines,
    FsInsertText,
    FsMkdir,
    FsMove,
    FsTrash,
    FsList,
    FsGlob,
    FsSearch,
    WebFetch,
    WebSearch,
    ExecStart,
    ExecReadOutput,
    ExecWait,
    ExecKill,
    PlanRead,
    PlanWrite,
    InitPlan,
    AddTask,
    SetTaskStatus,
    AddTaskNote,
    EditTask,
    PlanSnapshot,
    PlanLint,
    PromptBudgetRead,
    PromptBudgetUpdate,
    AutonomyStateRead,
    SkillList,
    SkillRead,
    SkillEnable,
    SkillDisable,
    ArtifactRead,
    ArtifactSearch,
    ArtifactPin,
    ArtifactUnpin,
    KnowledgeSearch,
    KnowledgeRead,
    SessionSearch,
    SessionRead,
    SessionWait,
    McpCall,
    McpSearchResources,
    McpReadResource,
    McpSearchPrompts,
    McpGetPrompt,
    AgentList,
    AgentRead,
    AgentCreate,
    ContinueLater,
    ScheduleList,
    ScheduleRead,
    ScheduleCreate,
    ScheduleUpdate,
    ScheduleDelete,
    MessageAgent,
    GrantAgentChainContinuation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPolicy {
    pub read_only: bool,
    pub destructive: bool,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDefinition {
    pub name: ToolName,
    pub family: ToolFamily,
    pub description: &'static str,
    pub policy: ToolPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCatalog {
    pub families: Vec<&'static str>,
    definitions: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsReadInput {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsWriteInput {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsWriteMode {
    Create,
    Overwrite,
    Upsert,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsReadTextInput {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsReadLinesInput {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsSearchTextInput {
    pub path: String,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsFindInFilesInput {
    pub query: String,
    pub glob: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsWriteTextInput {
    pub path: String,
    pub content: String,
    pub mode: FsWriteMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsPatchEdit {
    pub old: String,
    pub new: String,
    pub replace_all: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsPatchInput {
    pub path: String,
    pub edits: Vec<FsPatchEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsPatchTextInput {
    pub path: String,
    pub search: String,
    pub replace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsReplaceLinesInput {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsInsertTextInput {
    pub path: String,
    pub line: Option<usize>,
    pub position: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsListInput {
    pub path: String,
    pub recursive: bool,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsGlobInput {
    pub path: String,
    pub pattern: String,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsSearchInput {
    pub path: String,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsMkdirInput {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsMoveInput {
    pub src: String,
    pub dest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsTrashInput {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebFetchInput {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebSearchInput {
    pub query: String,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecStartInput {
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessOutputStream {
    Merged,
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessReadOutputInput {
    pub process_id: String,
    #[serde(default)]
    pub stream: Option<ProcessOutputStream>,
    #[serde(default)]
    pub cursor: Option<usize>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
    #[serde(default)]
    pub max_lines: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessWaitInput {
    pub process_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessKillInput {
    pub process_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanReadInput {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanWriteItemInput {
    pub id: String,
    pub content: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanWriteInput {
    pub items: Vec<PlanWriteItemInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitPlanInput {
    pub goal: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddTaskInput {
    pub description: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub parent_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetTaskStatusInput {
    pub task_id: String,
    pub new_status: String,
    #[serde(default)]
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddTaskNoteInput {
    pub task_id: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditTaskInput {
    pub task_id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub depends_on: Option<Vec<String>>,
    #[serde(default)]
    pub parent_task_id: Option<String>,
    #[serde(default)]
    pub clear_parent_task: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanSnapshotInput {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanLintInput {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptBudgetReadInput {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptBudgetUpdateScope {
    #[default]
    Session,
    NextTurn,
}

impl PromptBudgetUpdateScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::NextTurn => "next_turn",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptBudgetLayerPercentagesInput {
    pub system: Option<u8>,
    pub agents: Option<u8>,
    pub active_skills: Option<u8>,
    pub session_head: Option<u8>,
    pub autonomy_state: Option<u8>,
    pub plan: Option<u8>,
    pub context_summary: Option<u8>,
    pub offload_refs: Option<u8>,
    pub recent_tool_activity: Option<u8>,
    pub transcript_tail: Option<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptBudgetUpdateInput {
    pub scope: PromptBudgetUpdateScope,
    pub reset: bool,
    pub percentages: Option<PromptBudgetLayerPercentagesInput>,
    pub reason: Option<String>,
}

impl PromptBudgetLayerPercentagesInput {
    pub fn apply_to(&self, policy: &mut PromptBudgetPolicy) {
        if let Some(value) = self.system {
            policy.system = value;
        }
        if let Some(value) = self.agents {
            policy.agents = value;
        }
        if let Some(value) = self.active_skills {
            policy.active_skills = value;
        }
        if let Some(value) = self.session_head {
            policy.session_head = value;
        }
        if let Some(value) = self.autonomy_state {
            policy.autonomy_state = value;
        }
        if let Some(value) = self.plan {
            policy.plan = value;
        }
        if let Some(value) = self.context_summary {
            policy.context_summary = value;
        }
        if let Some(value) = self.offload_refs {
            policy.offload_refs = value;
        }
        if let Some(value) = self.recent_tool_activity {
            policy.recent_tool_activity = value;
        }
        if let Some(value) = self.transcript_tail {
            policy.transcript_tail = value;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillListInput {
    pub include_inactive: Option<bool>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutonomyStateReadInput {
    pub max_items: Option<usize>,
    pub include_inactive_schedules: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillReadInput {
    pub name: String,
    #[serde(default)]
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillActivationInput {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactReadInput {
    pub artifact_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactSearchInput {
    pub query: String,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactPinInput {
    pub artifact_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceKind {
    RootDoc,
    ProjectDoc,
    ProjectNote,
    ExtraDoc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRoot {
    RootDocs,
    Docs,
    Projects,
    Notes,
    Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeSearchInput {
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub kinds: Option<Vec<KnowledgeSourceKind>>,
    #[serde(default)]
    pub roots: Option<Vec<KnowledgeRoot>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeReadMode {
    Excerpt,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeReadInput {
    pub path: String,
    #[serde(default)]
    pub mode: Option<KnowledgeReadMode>,
    #[serde(default)]
    pub cursor: Option<usize>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
    #[serde(default)]
    pub max_lines: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSearchInput {
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub tiers: Option<Vec<SessionRetentionTier>>,
    #[serde(default)]
    pub agent_identifier: Option<String>,
    #[serde(default)]
    pub updated_after: Option<i64>,
    #[serde(default)]
    pub updated_before: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionReadMode {
    Summary,
    Timeline,
    Transcript,
    Artifacts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionReadInput {
    pub session_id: String,
    #[serde(default)]
    pub mode: Option<SessionReadMode>,
    #[serde(default)]
    pub cursor: Option<usize>,
    #[serde(default)]
    pub max_items: Option<usize>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
    #[serde(default)]
    pub include_tools: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWaitInput {
    pub session_id: String,
    #[serde(default)]
    pub wait_timeout_ms: Option<u64>,
    #[serde(default)]
    pub mode: Option<SessionReadMode>,
    #[serde(default)]
    pub cursor: Option<usize>,
    #[serde(default)]
    pub max_items: Option<usize>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
    #[serde(default)]
    pub include_tools: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpSearchResourcesInput {
    #[serde(default)]
    pub connector_id: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpReadResourceInput {
    pub connector_id: String,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpSearchPromptsInput {
    #[serde(default)]
    pub connector_id: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpGetPromptInput {
    pub connector_id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpCallInput {
    pub exposed_name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageAgentInput {
    pub target_agent_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantAgentChainContinuationInput {
    pub chain_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentListInput {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentReadInput {
    pub identifier: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCreateInput {
    pub name: String,
    #[serde(default)]
    pub template_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinueLaterInput {
    pub delay_seconds: u64,
    pub handoff_payload: String,
    #[serde(default)]
    pub delivery_mode: Option<AgentScheduleDeliveryMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleListInput {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub agent_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleReadInput {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleCreateInput {
    pub id: String,
    #[serde(default)]
    pub agent_identifier: Option<String>,
    pub prompt: String,
    #[serde(default)]
    pub mode: Option<AgentScheduleMode>,
    #[serde(default)]
    pub delivery_mode: Option<AgentScheduleDeliveryMode>,
    #[serde(default)]
    pub target_session_id: Option<String>,
    pub interval_seconds: u64,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleUpdateInput {
    pub id: String,
    #[serde(default)]
    pub agent_identifier: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub mode: Option<AgentScheduleMode>,
    #[serde(default)]
    pub delivery_mode: Option<AgentScheduleDeliveryMode>,
    #[serde(default)]
    pub target_session_id: Option<String>,
    #[serde(default)]
    pub interval_seconds: Option<u64>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleDeleteInput {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCall {
    FsRead(FsReadInput),
    FsWrite(FsWriteInput),
    FsPatch(FsPatchInput),
    FsReadText(FsReadTextInput),
    FsReadLines(FsReadLinesInput),
    FsSearchText(FsSearchTextInput),
    FsFindInFiles(FsFindInFilesInput),
    FsWriteText(FsWriteTextInput),
    FsPatchText(FsPatchTextInput),
    FsReplaceLines(FsReplaceLinesInput),
    FsInsertText(FsInsertTextInput),
    FsMkdir(FsMkdirInput),
    FsMove(FsMoveInput),
    FsTrash(FsTrashInput),
    FsList(FsListInput),
    FsGlob(FsGlobInput),
    FsSearch(FsSearchInput),
    WebFetch(WebFetchInput),
    WebSearch(WebSearchInput),
    ExecStart(ExecStartInput),
    ExecReadOutput(ProcessReadOutputInput),
    ExecWait(ProcessWaitInput),
    ExecKill(ProcessKillInput),
    PlanRead(PlanReadInput),
    PlanWrite(PlanWriteInput),
    InitPlan(InitPlanInput),
    AddTask(AddTaskInput),
    SetTaskStatus(SetTaskStatusInput),
    AddTaskNote(AddTaskNoteInput),
    EditTask(EditTaskInput),
    PlanSnapshot(PlanSnapshotInput),
    PlanLint(PlanLintInput),
    PromptBudgetRead(PromptBudgetReadInput),
    PromptBudgetUpdate(PromptBudgetUpdateInput),
    AutonomyStateRead(AutonomyStateReadInput),
    SkillList(SkillListInput),
    SkillRead(SkillReadInput),
    SkillEnable(SkillActivationInput),
    SkillDisable(SkillActivationInput),
    ArtifactRead(ArtifactReadInput),
    ArtifactSearch(ArtifactSearchInput),
    ArtifactPin(ArtifactPinInput),
    ArtifactUnpin(ArtifactPinInput),
    KnowledgeSearch(KnowledgeSearchInput),
    KnowledgeRead(KnowledgeReadInput),
    SessionSearch(SessionSearchInput),
    SessionRead(SessionReadInput),
    SessionWait(SessionWaitInput),
    McpCall(McpCallInput),
    McpSearchResources(McpSearchResourcesInput),
    McpReadResource(McpReadResourceInput),
    McpSearchPrompts(McpSearchPromptsInput),
    McpGetPrompt(McpGetPromptInput),
    AgentList(AgentListInput),
    AgentRead(AgentReadInput),
    AgentCreate(AgentCreateInput),
    ContinueLater(ContinueLaterInput),
    ScheduleList(ScheduleListInput),
    ScheduleRead(ScheduleReadInput),
    ScheduleCreate(ScheduleCreateInput),
    ScheduleUpdate(ScheduleUpdateInput),
    ScheduleDelete(ScheduleDeleteInput),
    MessageAgent(MessageAgentInput),
    GrantAgentChainContinuation(GrantAgentChainContinuationInput),
}

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

#[derive(Debug)]
pub enum ToolError {
    InvalidExec {
        reason: &'static str,
    },
    InvalidPatch {
        path: String,
        reason: String,
    },
    InvalidWebRequest {
        reason: String,
    },
    WebHttp(reqwest::Error),
    WebHttpStatus {
        url: String,
        status_code: u16,
    },
    WebParse {
        url: String,
        reason: String,
    },
    ProcessFamilyMismatch {
        process_id: String,
        expected: ProcessKind,
        actual: ProcessKind,
    },
    ProcessIo {
        process_id: String,
        source: std::io::Error,
    },
    InvalidPlanWrite {
        reason: String,
    },
    InvalidMemoryTool {
        reason: String,
    },
    InvalidMcpTool {
        reason: String,
    },
    InvalidAgentTool {
        reason: String,
    },
    InvalidArtifactTool {
        reason: String,
    },
    UnknownProcess {
        process_id: String,
    },
    Workspace(WorkspaceError),
}

#[derive(Debug)]
pub enum ToolCallParseError {
    UnknownTool {
        name: String,
    },
    InvalidArguments {
        name: String,
        source: serde_json::Error,
    },
}

#[derive(Debug)]
pub struct ToolRuntime {
    workspace: WorkspaceRef,
    web: WebToolClient,
    processes: SharedProcessRegistry,
}

#[derive(Debug, Clone)]
pub struct WebToolClient {
    client: Client,
    search_backend: WebSearchBackend,
    search_url: String,
}

fn repair_bare_enum_like_values(input: &str, repairs: &[EnumLikeFieldRepair]) -> Option<String> {
    fn allowed_values_for_field<'a>(
        repairs: &'a [EnumLikeFieldRepair],
        field: &str,
    ) -> Option<&'a [&'static str]> {
        repairs
            .iter()
            .find(|repair| repair.field == field)
            .map(|repair| repair.allowed_values)
    }

    fn is_enum_token_byte(byte: u8) -> bool {
        byte.is_ascii_lowercase() || byte == b'_'
    }

    let bytes = input.as_bytes();
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] != b'"' {
            index += 1;
            continue;
        }

        let key_start = index + 1;
        index += 1;
        let mut escaped = false;
        while index < bytes.len() {
            let byte = bytes[index];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                break;
            }
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let key = &input[key_start..index];
        index += 1;

        let mut cursor = index;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b':' {
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let Some(allowed_values) = allowed_values_for_field(repairs, key) else {
            continue;
        };
        if cursor >= bytes.len() || bytes[cursor] == b'"' {
            continue;
        }

        let value_start = cursor;
        while cursor < bytes.len() && is_enum_token_byte(bytes[cursor]) {
            cursor += 1;
        }
        if cursor == value_start {
            continue;
        }

        let token = &input[value_start..cursor];
        if !allowed_values.contains(&token) {
            continue;
        }

        let mut delimiter = cursor;
        while delimiter < bytes.len() && bytes[delimiter].is_ascii_whitespace() {
            delimiter += 1;
        }
        if delimiter < bytes.len() && !matches!(bytes[delimiter], b',' | b'}' | b']') {
            continue;
        }

        replacements.push((value_start, cursor, format!("\"{token}\"")));
    }

    if replacements.is_empty() {
        return None;
    }

    let mut repaired = String::with_capacity(input.len() + replacements.len() * 2);
    let mut cursor = 0usize;
    for (start, end, replacement) in replacements {
        repaired.push_str(&input[cursor..start]);
        repaired.push_str(&replacement);
        cursor = end;
    }
    repaired.push_str(&input[cursor..]);
    Some(repaired)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebSearchBackend {
    #[serde(rename = "duckduckgo_html")]
    #[default]
    DuckDuckGoHtml,
    #[serde(rename = "searxng_json")]
    SearxngJson,
}

#[derive(Debug)]
struct ManagedProcess {
    kind: ProcessKind,
    child: Mutex<Child>,
    output: Arc<Mutex<ManagedProcessOutput>>,
    stdout_reader: Mutex<Option<thread::JoinHandle<()>>>,
    stderr_reader: Mutex<Option<thread::JoinHandle<()>>>,
}

#[derive(Debug, Clone, Default)]
pub struct SharedProcessRegistry {
    inner: Arc<Mutex<ProcessRegistryState>>,
}

#[derive(Debug)]
struct ProcessRegistryState {
    next_process_id: usize,
    processes: BTreeMap<String, Arc<ManagedProcess>>,
}

#[derive(Debug, Default)]
struct ManagedProcessOutput {
    stdout: String,
    stderr: String,
    merged: String,
    finished_status: Option<ProcessResultStatus>,
    exit_code: Option<i32>,
}

impl Default for ProcessRegistryState {
    fn default() -> Self {
        Self {
            next_process_id: 1,
            processes: BTreeMap::new(),
        }
    }
}

impl ToolFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Filesystem => "fs",
            Self::Web => "web",
            Self::Exec => "exec",
            Self::Planning => "plan",
            Self::Offload => "offload",
            Self::Memory => "memory",
            Self::Mcp => "mcp",
            Self::Agent => "agent",
        }
    }
}

impl ToolName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FsRead => "fs_read",
            Self::FsWrite => "fs_write",
            Self::FsPatch => "fs_patch",
            Self::FsReadText => "fs_read_text",
            Self::FsReadLines => "fs_read_lines",
            Self::FsSearchText => "fs_search_text",
            Self::FsFindInFiles => "fs_find_in_files",
            Self::FsWriteText => "fs_write_text",
            Self::FsPatchText => "fs_patch_text",
            Self::FsReplaceLines => "fs_replace_lines",
            Self::FsInsertText => "fs_insert_text",
            Self::FsMkdir => "fs_mkdir",
            Self::FsMove => "fs_move",
            Self::FsTrash => "fs_trash",
            Self::FsList => "fs_list",
            Self::FsGlob => "fs_glob",
            Self::FsSearch => "fs_search",
            Self::WebFetch => "web_fetch",
            Self::WebSearch => "web_search",
            Self::ExecStart => "exec_start",
            Self::ExecReadOutput => "exec_read_output",
            Self::ExecWait => "exec_wait",
            Self::ExecKill => "exec_kill",
            Self::PlanRead => "plan_read",
            Self::PlanWrite => "plan_write",
            Self::InitPlan => "init_plan",
            Self::AddTask => "add_task",
            Self::SetTaskStatus => "set_task_status",
            Self::AddTaskNote => "add_task_note",
            Self::EditTask => "edit_task",
            Self::PlanSnapshot => "plan_snapshot",
            Self::PlanLint => "plan_lint",
            Self::PromptBudgetRead => "prompt_budget_read",
            Self::PromptBudgetUpdate => "prompt_budget_update",
            Self::AutonomyStateRead => "autonomy_state_read",
            Self::SkillList => "skill_list",
            Self::SkillRead => "skill_read",
            Self::SkillEnable => "skill_enable",
            Self::SkillDisable => "skill_disable",
            Self::ArtifactRead => "artifact_read",
            Self::ArtifactSearch => "artifact_search",
            Self::ArtifactPin => "artifact_pin",
            Self::ArtifactUnpin => "artifact_unpin",
            Self::KnowledgeSearch => "knowledge_search",
            Self::KnowledgeRead => "knowledge_read",
            Self::SessionSearch => "session_search",
            Self::SessionRead => "session_read",
            Self::SessionWait => "session_wait",
            Self::McpCall => "mcp_call",
            Self::McpSearchResources => "mcp_search_resources",
            Self::McpReadResource => "mcp_read_resource",
            Self::McpSearchPrompts => "mcp_search_prompts",
            Self::McpGetPrompt => "mcp_get_prompt",
            Self::AgentList => "agent_list",
            Self::AgentRead => "agent_read",
            Self::AgentCreate => "agent_create",
            Self::ContinueLater => "continue_later",
            Self::ScheduleList => "schedule_list",
            Self::ScheduleRead => "schedule_read",
            Self::ScheduleCreate => "schedule_create",
            Self::ScheduleUpdate => "schedule_update",
            Self::ScheduleDelete => "schedule_delete",
            Self::MessageAgent => "message_agent",
            Self::GrantAgentChainContinuation => "grant_agent_chain_continuation",
        }
    }
}

impl ToolCatalog {
    pub fn definition(&self, name: ToolName) -> Option<&ToolDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.name == name)
    }

    pub fn definition_for_call(&self, call: &ToolCall) -> Option<&ToolDefinition> {
        self.definition(call.name())
    }

    pub fn all_definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    pub fn automatic_model_definitions(&self) -> Vec<&ToolDefinition> {
        self.definitions
            .iter()
            .filter(|definition| {
                matches!(
                    definition.name,
                    ToolName::FsReadText
                        | ToolName::FsReadLines
                        | ToolName::FsList
                        | ToolName::FsGlob
                        | ToolName::FsSearchText
                        | ToolName::FsFindInFiles
                        | ToolName::FsWriteText
                        | ToolName::FsPatchText
                        | ToolName::FsReplaceLines
                        | ToolName::FsInsertText
                        | ToolName::FsMkdir
                        | ToolName::FsMove
                        | ToolName::FsTrash
                        | ToolName::ExecStart
                        | ToolName::ExecReadOutput
                        | ToolName::ExecWait
                        | ToolName::ExecKill
                        | ToolName::WebFetch
                        | ToolName::WebSearch
                        | ToolName::InitPlan
                        | ToolName::AddTask
                        | ToolName::SetTaskStatus
                        | ToolName::AddTaskNote
                        | ToolName::EditTask
                        | ToolName::PlanSnapshot
                        | ToolName::PlanLint
                        | ToolName::PromptBudgetRead
                        | ToolName::PromptBudgetUpdate
                        | ToolName::AutonomyStateRead
                        | ToolName::SkillList
                        | ToolName::SkillRead
                        | ToolName::SkillEnable
                        | ToolName::SkillDisable
                        | ToolName::ArtifactRead
                        | ToolName::ArtifactSearch
                        | ToolName::ArtifactPin
                        | ToolName::ArtifactUnpin
                        | ToolName::KnowledgeSearch
                        | ToolName::KnowledgeRead
                        | ToolName::SessionSearch
                        | ToolName::SessionRead
                        | ToolName::SessionWait
                        | ToolName::McpSearchResources
                        | ToolName::McpReadResource
                        | ToolName::McpSearchPrompts
                        | ToolName::McpGetPrompt
                        | ToolName::AgentList
                        | ToolName::AgentRead
                        | ToolName::AgentCreate
                        | ToolName::ContinueLater
                        | ToolName::ScheduleList
                        | ToolName::ScheduleRead
                        | ToolName::ScheduleCreate
                        | ToolName::ScheduleUpdate
                        | ToolName::ScheduleDelete
                        | ToolName::MessageAgent
                        | ToolName::GrantAgentChainContinuation
                )
            })
            .collect()
    }

    fn definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: ToolName::FsRead,
                family: ToolFamily::Filesystem,
                description: "Read a UTF-8 text file from the workspace",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsWrite,
                family: ToolFamily::Filesystem,
                description: "Write a UTF-8 text file inside the workspace",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsPatch,
                family: ToolFamily::Filesystem,
                description: "Apply exact text edits to a UTF-8 text file inside the workspace",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsReadText,
                family: ToolFamily::Filesystem,
                description: "Read a UTF-8 text file from the workspace",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsReadLines,
                family: ToolFamily::Filesystem,
                description: "Read an inclusive line range from a UTF-8 text file and report file bounds",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsSearchText,
                family: ToolFamily::Filesystem,
                description: "Search for literal text within a single UTF-8 text file",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsFindInFiles,
                family: ToolFamily::Filesystem,
                description: "Search for literal text across regular workspace files, optionally constrained by glob; special files such as sockets are ignored",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsWriteText,
                family: ToolFamily::Filesystem,
                description: "Write full UTF-8 text to a file with explicit create, overwrite, or upsert semantics",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsPatchText,
                family: ToolFamily::Filesystem,
                description: "Replace one exact text fragment inside a UTF-8 text file. Use JSON fields `search` and `replace`; do not send `old`/`new` patch arrays",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsReplaceLines,
                family: ToolFamily::Filesystem,
                description: "Replace an explicit inclusive line range inside a UTF-8 text file",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsInsertText,
                family: ToolFamily::Filesystem,
                description: "Insert text before or after a line, or prepend or append to a file",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsMkdir,
                family: ToolFamily::Filesystem,
                description: "Create a directory inside the workspace",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsMove,
                family: ToolFamily::Filesystem,
                description: "Move or rename a file or directory inside the workspace",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsTrash,
                family: ToolFamily::Filesystem,
                description: "Move a file or directory into workspace trash instead of deleting it permanently",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::FsList,
                family: ToolFamily::Filesystem,
                description: "List files and directories inside the workspace with bounded pagination",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsGlob,
                family: ToolFamily::Filesystem,
                description: "Match workspace paths with glob-style patterns",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::FsSearch,
                family: ToolFamily::Filesystem,
                description: "Search text content inside the workspace",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::WebFetch,
                family: ToolFamily::Web,
                description: "Fetch an exact URL and return readable response text; prefer web_search first unless the user supplied the URL, web_search returned it, or it is a known canonical source",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::WebSearch,
                family: ToolFamily::Web,
                description: "Search first for current or external information via the configured backend; deployments may use SearXNG",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ExecStart,
                family: ToolFamily::Exec,
                description: "Start a structured executable plus args process",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::ExecReadOutput,
                family: ToolFamily::Exec,
                description: "Read bounded live output from a structured exec process",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ExecWait,
                family: ToolFamily::Exec,
                description: "Wait for a structured exec process to finish",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ExecKill,
                family: ToolFamily::Exec,
                description: "Kill a structured exec process",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: true,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::PlanRead,
                family: ToolFamily::Planning,
                description: "Read the structured plan snapshot for the current session",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::PlanWrite,
                family: ToolFamily::Planning,
                description: "Replace the structured plan snapshot for the current session",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::InitPlan,
                family: ToolFamily::Planning,
                description: "Initialize a structured session plan with a top-level goal",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::AddTask,
                family: ToolFamily::Planning,
                description: "Add a task to the structured session plan",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SetTaskStatus,
                family: ToolFamily::Planning,
                description: "Update the status of a task in the structured session plan",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::AddTaskNote,
                family: ToolFamily::Planning,
                description: "Append a note to a task in the structured session plan",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::EditTask,
                family: ToolFamily::Planning,
                description: "Edit a task description, dependencies, or parent relationship in the structured session plan",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::PlanSnapshot,
                family: ToolFamily::Planning,
                description: "Read the current structured session plan including goal and tasks",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::PlanLint,
                family: ToolFamily::Planning,
                description: "Validate the current structured session plan and report issues",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::PromptBudgetRead,
                family: ToolFamily::Planning,
                description: "Read the effective prompt budget policy for the next full prompt assembly, including usable context basis, layer percentages, target tokens, and pending one-shot override state",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::PromptBudgetUpdate,
                family: ToolFamily::Planning,
                description: "Update or reset prompt budget policy. scope=session persists a session policy; scope=next_turn queues a one-shot override for the next full prompt assembly. Percentages must sum to 100 after merging",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::AutonomyStateRead,
                family: ToolFamily::Memory,
                description: "Read a bounded aggregate autonomy state for the current session: related schedules, active jobs, delegated child sessions, inbox events, inter-agent chain metadata, and configured mesh/A2A peers.",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SkillList,
                family: ToolFamily::Memory,
                description: "List skills available to the current session, including activation mode and source paths. Use before enabling, disabling, or reading a skill.",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SkillRead,
                family: ToolFamily::Memory,
                description: "Read one skill's SKILL.md body by name with an optional max_bytes bound. Use this instead of guessing skill instructions.",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SkillEnable,
                family: ToolFamily::Memory,
                description: "Manually enable one skill for the current session. This changes session settings only; use skill_list or skill_read first if unsure.",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SkillDisable,
                family: ToolFamily::Memory,
                description: "Manually disable one skill for the current session. This changes session settings only; use when an automatic skill is distracting or wrong.",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ArtifactRead,
                family: ToolFamily::Offload,
                description: "Read the full content of an offloaded context artifact by artifact_id",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ArtifactSearch,
                family: ToolFamily::Offload,
                description: "Search across the current session's offloaded context references and payloads",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ArtifactPin,
                family: ToolFamily::Offload,
                description: "Manually pin an offloaded context artifact so its ref is prioritized in future prompt OffloadRefs",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ArtifactUnpin,
                family: ToolFamily::Offload,
                description: "Remove a manual pin from an offloaded context artifact; frequently read artifacts may still be auto-pinned",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::KnowledgeSearch,
                family: ToolFamily::Memory,
                description: "Search project knowledge roots with bounded pagination and canonical source metadata",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::KnowledgeRead,
                family: ToolFamily::Memory,
                description: "Read one project knowledge source in a bounded excerpt or full-text view; enum-like arguments such as mode must be quoted JSON strings",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SessionSearch,
                family: ToolFamily::Memory,
                description: "Search historical sessions with bounded pagination so you can find the exact session_id before reading it",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SessionRead,
                family: ToolFamily::Memory,
                description: "Read one session snapshot in bounded summary, timeline, transcript, or artifact views; this inspects current data and does not wait for new work",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::SessionWait,
                family: ToolFamily::Agent,
                description: "Wait for queued or running work in a session to settle, then return a bounded session snapshot; use this after message_agent when you need the other agent's reply before concluding. If you set mode, send it as a quoted JSON string, for example {\"session_id\":\"...\",\"mode\":\"transcript\"}",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::McpCall,
                family: ToolFamily::Mcp,
                description: "Call one dynamically discovered MCP tool through the canonical runtime path",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::McpSearchResources,
                family: ToolFamily::Mcp,
                description: "Search discovered MCP resources with bounded pagination",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::McpReadResource,
                family: ToolFamily::Mcp,
                description: "Read one MCP resource by connector id and URI",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::McpSearchPrompts,
                family: ToolFamily::Mcp,
                description: "Search discovered MCP prompts with bounded pagination",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::McpGetPrompt,
                family: ToolFamily::Mcp,
                description: "Fetch one MCP prompt by connector id and prompt name",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::AgentList,
                family: ToolFamily::Agent,
                description: "List available agent profiles with bounded pagination",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::AgentRead,
                family: ToolFamily::Agent,
                description: "Read one agent profile by id or name",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::AgentCreate,
                family: ToolFamily::Agent,
                description: "Create a new agent profile from a built-in or existing template",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: true,
                },
            },
            ToolDefinition {
                name: ToolName::ContinueLater,
                family: ToolFamily::Agent,
                description: "Create a self-addressed one-shot timer in the same session; use this when the user asks you to remind or message them later. If you set delivery_mode, send it as a quoted JSON string, for example {\"delay_seconds\":300,\"handoff_payload\":\"...\",\"delivery_mode\":\"existing_session\"}",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ScheduleList,
                family: ToolFamily::Agent,
                description: "List agent schedules for the current workspace with bounded pagination",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ScheduleRead,
                family: ToolFamily::Agent,
                description: "Read one agent schedule by id",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ScheduleCreate,
                family: ToolFamily::Agent,
                description: "Create an advanced or recurring agent schedule in the current workspace. For simple one-shot reminders, prefer continue_later. If you set mode or delivery_mode, send them as quoted JSON strings, for example {\"id\":\"nightly\",\"prompt\":\"...\",\"interval_seconds\":3600,\"mode\":\"once\",\"delivery_mode\":\"fresh_session\"}",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ScheduleUpdate,
                family: ToolFamily::Agent,
                description: "Update an existing agent schedule in the current workspace. If you set mode or delivery_mode, send them as quoted JSON strings, for example {\"id\":\"nightly\",\"mode\":\"once\",\"delivery_mode\":\"existing_session\"}",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::ScheduleDelete,
                family: ToolFamily::Agent,
                description: "Delete an existing agent schedule in the current workspace",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::MessageAgent,
                family: ToolFamily::Agent,
                description: "Queue an asynchronous message to another agent by creating a fresh recipient session and background job; this returns recipient ids and does not wait for the reply",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::GrantAgentChainContinuation,
                family: ToolFamily::Agent,
                description: "Grant exactly one additional hop to a blocked inter-agent chain after you have confirmed the chain hit max_hops",
                policy: ToolPolicy {
                    read_only: false,
                    destructive: false,
                    requires_approval: false,
                },
            },
        ]
    }
}

impl Default for ToolCatalog {
    fn default() -> Self {
        Self {
            families: vec![
                "fs", "web", "exec", "plan", "offload", "memory", "mcp", "agent",
            ],
            definitions: Self::definitions(),
        }
    }
}

impl ToolRuntime {
    pub fn new(workspace: WorkspaceRef) -> Self {
        Self::with_shared_process_registry(workspace, SharedProcessRegistry::default())
    }

    pub fn with_web_client(workspace: WorkspaceRef, web: WebToolClient) -> Self {
        Self::with_web_client_and_process_registry(workspace, web, SharedProcessRegistry::default())
    }

    pub fn with_shared_process_registry(
        workspace: WorkspaceRef,
        processes: SharedProcessRegistry,
    ) -> Self {
        Self::with_web_client_and_process_registry(workspace, WebToolClient::default(), processes)
    }

    pub fn with_web_client_and_process_registry(
        workspace: WorkspaceRef,
        web: WebToolClient,
        processes: SharedProcessRegistry,
    ) -> Self {
        Self {
            workspace,
            web,
            processes,
        }
    }

    pub fn invoke(&mut self, call: ToolCall) -> Result<ToolOutput, ToolError> {
        match call {
            ToolCall::FsRead(input) => Ok(ToolOutput::FsRead(FsReadOutput {
                path: normalize_tool_path(&input.path),
                content: self.workspace.read_text(&input.path)?,
            })),
            ToolCall::FsWrite(input) => {
                let result = self.workspace.write_text_with_mode(
                    &input.path,
                    &input.content,
                    WriteMode::Upsert,
                )?;
                Ok(ToolOutput::FsWrite(FsWriteOutput {
                    path: normalize_tool_path(&input.path),
                    bytes_written: result.bytes_written,
                }))
            }
            ToolCall::FsPatch(input) => {
                let path = normalize_tool_path(&input.path);
                let content = self.workspace.read_text(&input.path)?;
                let patched = apply_patch_edits(&path, content, &input.edits)?;
                let bytes_written = self
                    .workspace
                    .write_text_with_mode(&input.path, &patched.content, WriteMode::Upsert)?
                    .bytes_written;
                Ok(ToolOutput::FsPatch(FsPatchOutput {
                    path,
                    bytes_written,
                    edits_applied: input.edits.len(),
                }))
            }
            ToolCall::FsReadText(input) => Ok(ToolOutput::FsReadText(FsReadTextOutput {
                path: normalize_tool_path(&input.path),
                content: self.workspace.read_text(&input.path)?,
            })),
            ToolCall::FsReadLines(input) => {
                let chunk =
                    self.workspace
                        .read_lines(&input.path, input.start_line, input.end_line)?;
                Ok(ToolOutput::FsReadLines(FsReadLinesOutput {
                    path: chunk.path,
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    total_lines: chunk.total_lines,
                    eof: chunk.eof,
                    next_start_line: chunk.next_start_line,
                    content: chunk.content,
                }))
            }
            ToolCall::FsSearchText(input) => Ok(ToolOutput::FsSearchText(FsSearchTextOutput {
                matches: self.workspace.search_text(&input.path, &input.query)?,
            })),
            ToolCall::FsFindInFiles(input) => {
                let mut matches = self.workspace.find_in_files("", &input.query)?;
                if let Some(glob) = &input.glob {
                    matches.retain(|entry| glob_matches(glob, &entry.path));
                }
                if let Some(limit) = input.limit {
                    matches.truncate(limit);
                }
                Ok(ToolOutput::FsFindInFiles(FsFindInFilesOutput { matches }))
            }
            ToolCall::FsWriteText(input) => {
                let result = self.workspace.write_text_with_mode(
                    &input.path,
                    &input.content,
                    input.mode.into(),
                )?;
                Ok(ToolOutput::FsWriteText(FsWriteTextOutput {
                    path: normalize_tool_path(&input.path),
                    mode: input.mode,
                    bytes_written: result.bytes_written,
                    created: result.created,
                    overwritten: result.overwritten,
                }))
            }
            ToolCall::FsPatchText(input) => {
                let path = normalize_tool_path(&input.path);
                let content = self.workspace.read_text(&input.path)?;
                let updated = content.replacen(&input.search, &input.replace, 1);
                if updated == content {
                    return Err(ToolError::InvalidPatch {
                        path,
                        reason: "search text not found in file".to_string(),
                    });
                }
                let bytes_written = self
                    .workspace
                    .write_text_with_mode(&input.path, &updated, WriteMode::Upsert)?
                    .bytes_written;
                Ok(ToolOutput::FsPatchText(FsPatchTextOutput {
                    path: normalize_tool_path(&input.path),
                    bytes_written,
                }))
            }
            ToolCall::FsReplaceLines(input) => {
                let path = normalize_tool_path(&input.path);
                let content = self.workspace.read_text(&input.path)?;
                let updated = replace_lines_range(
                    path.as_str(),
                    content.as_str(),
                    input.start_line,
                    input.end_line,
                    input.content.as_str(),
                )?;
                let bytes_written = self
                    .workspace
                    .write_text_with_mode(&input.path, updated.as_str(), WriteMode::Upsert)?
                    .bytes_written;
                Ok(ToolOutput::FsReplaceLines(FsReplaceLinesOutput {
                    path,
                    start_line: input.start_line,
                    end_line: input.end_line,
                    bytes_written,
                }))
            }
            ToolCall::FsInsertText(input) => {
                let path = normalize_tool_path(&input.path);
                let content = self.workspace.read_text(&input.path).or_else(|error| {
                    if matches!(
                        &error,
                        WorkspaceError::Io {
                            source,
                            ..
                        } if source.kind() == std::io::ErrorKind::NotFound
                    ) {
                        Ok(String::new())
                    } else {
                        Err(error)
                    }
                })?;
                let updated = insert_text(
                    path.as_str(),
                    content.as_str(),
                    input.line,
                    input.position.as_str(),
                    input.content.as_str(),
                )?;
                let bytes_written = self
                    .workspace
                    .write_text_with_mode(&input.path, updated.as_str(), WriteMode::Upsert)?
                    .bytes_written;
                Ok(ToolOutput::FsInsertText(FsInsertTextOutput {
                    path,
                    position: input.position,
                    line: input.line,
                    bytes_written,
                }))
            }
            ToolCall::FsMkdir(input) => Ok(ToolOutput::FsMkdir(FsMkdirOutput {
                path: self.workspace.mkdir(&input.path)?,
            })),
            ToolCall::FsMove(input) => {
                let (src, dest) = self.workspace.move_path(&input.src, &input.dest)?;
                Ok(ToolOutput::FsMove(FsMoveOutput { src, dest }))
            }
            ToolCall::FsTrash(input) => {
                let (path, trashed_to) = self.workspace.trash_path(&input.path)?;
                Ok(ToolOutput::FsTrash(FsTrashOutput { path, trashed_to }))
            }
            ToolCall::FsList(input) => {
                let all_entries = self.workspace.list(&input.path, input.recursive)?;
                let total_entries = all_entries.len();
                let offset = input.offset.unwrap_or(0).min(total_entries);
                let limit = normalize_fs_list_limit(input.limit);
                let entries = all_entries
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .collect::<Vec<_>>();
                let next_offset =
                    (offset + entries.len() < total_entries).then_some(offset + entries.len());
                Ok(ToolOutput::FsList(FsListOutput {
                    truncated: next_offset.is_some(),
                    offset,
                    limit,
                    total_entries,
                    next_offset,
                    entries,
                }))
            }
            ToolCall::FsGlob(input) => {
                let mut entries = self.workspace.list(&input.path, true)?;
                entries.retain(|entry| glob_matches(&input.pattern, &entry.path));
                let total_entries = entries.len();
                let offset = input.offset.unwrap_or(0).min(total_entries);
                let limit = normalize_fs_list_limit(input.limit);
                let entries = entries
                    .into_iter()
                    .skip(offset)
                    .take(limit)
                    .collect::<Vec<_>>();
                let next_offset =
                    (offset + entries.len() < total_entries).then_some(offset + entries.len());
                Ok(ToolOutput::FsGlob(FsGlobOutput {
                    truncated: next_offset.is_some(),
                    offset,
                    limit,
                    total_entries,
                    next_offset,
                    entries,
                }))
            }
            ToolCall::FsSearch(input) => Ok(ToolOutput::FsSearch(FsSearchOutput {
                matches: self.workspace.search(&input.path, &input.query)?,
            })),
            ToolCall::WebFetch(input) => Ok(ToolOutput::WebFetch(self.web.fetch(&input.url)?)),
            ToolCall::WebSearch(input) => Ok(ToolOutput::WebSearch(
                self.web.search(&input.query, input.limit)?,
            )),
            ToolCall::ExecStart(input) => {
                let cwd = self.resolve_cwd(input.cwd.as_deref())?;
                self.start_process(ProcessKind::Exec, &input.executable, &input.args, cwd)
            }
            ToolCall::ExecReadOutput(input) => {
                let process_id = input.process_id.clone();
                self.read_process_output(&process_id, ProcessKind::Exec, input)
            }
            ToolCall::ExecWait(input) => self.wait_process(&input.process_id, ProcessKind::Exec),
            ToolCall::ExecKill(input) => self.kill_process(&input.process_id, ProcessKind::Exec),
            ToolCall::PlanRead(_)
            | ToolCall::PlanWrite(_)
            | ToolCall::InitPlan(_)
            | ToolCall::AddTask(_)
            | ToolCall::SetTaskStatus(_)
            | ToolCall::AddTaskNote(_)
            | ToolCall::EditTask(_)
            | ToolCall::PlanSnapshot(_)
            | ToolCall::PlanLint(_)
            | ToolCall::PromptBudgetRead(_)
            | ToolCall::PromptBudgetUpdate(_) => Err(ToolError::InvalidPlanWrite {
                reason: "planning tools must execute through the canonical session path"
                    .to_string(),
            }),
            ToolCall::AutonomyStateRead(_)
            | ToolCall::SkillList(_)
            | ToolCall::SkillRead(_)
            | ToolCall::SkillEnable(_)
            | ToolCall::SkillDisable(_) => Err(ToolError::InvalidMemoryTool {
                reason: "autonomy and skill tools must execute through the canonical session path"
                    .to_string(),
            }),
            ToolCall::KnowledgeSearch(_)
            | ToolCall::KnowledgeRead(_)
            | ToolCall::SessionSearch(_)
            | ToolCall::SessionRead(_) => Err(ToolError::InvalidMemoryTool {
                reason: "memory tools must execute through the canonical session path".to_string(),
            }),
            ToolCall::McpCall(_)
            | ToolCall::McpSearchResources(_)
            | ToolCall::McpReadResource(_)
            | ToolCall::McpSearchPrompts(_)
            | ToolCall::McpGetPrompt(_) => Err(ToolError::InvalidMcpTool {
                reason: "MCP tools must execute through the canonical session path".to_string(),
            }),
            ToolCall::AgentList(_)
            | ToolCall::AgentRead(_)
            | ToolCall::AgentCreate(_)
            | ToolCall::ContinueLater(_)
            | ToolCall::ScheduleList(_)
            | ToolCall::ScheduleRead(_)
            | ToolCall::ScheduleCreate(_)
            | ToolCall::ScheduleUpdate(_)
            | ToolCall::ScheduleDelete(_)
            | ToolCall::MessageAgent(_)
            | ToolCall::SessionWait(_)
            | ToolCall::GrantAgentChainContinuation(_) => Err(ToolError::InvalidAgentTool {
                reason: "agent, schedule, and inter-agent tools must execute through the canonical session path"
                    .to_string(),
            }),
            ToolCall::ArtifactRead(_)
            | ToolCall::ArtifactSearch(_)
            | ToolCall::ArtifactPin(_)
            | ToolCall::ArtifactUnpin(_) => {
                Err(ToolError::InvalidArtifactTool {
                    reason:
                        "offload retrieval tools must execute through the canonical session path"
                            .to_string(),
                })
            }
        }
    }

    fn resolve_cwd(&self, cwd: Option<&str>) -> Result<PathBuf, ToolError> {
        cwd.map(|path| self.workspace.resolve(path))
            .transpose()?
            .or_else(|| Some(self.workspace.root.clone()))
            .ok_or(ToolError::InvalidExec {
                reason: "working directory resolution failed",
            })
    }

    fn start_process(
        &mut self,
        kind: ProcessKind,
        executable: &str,
        args: &[String],
        cwd: PathBuf,
    ) -> Result<ToolOutput, ToolError> {
        if executable.trim().is_empty() {
            return Err(ToolError::InvalidExec {
                reason: "executable must not be empty",
            });
        }

        let mut command = Command::new(executable);
        command
            .args(args)
            .current_dir(&cwd)
            // Structured exec is intentionally non-interactive: tools must not steal
            // the operator's TTY or block on terminal input while the TUI is running.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        detach_command_from_terminal(&mut command);

        let command_display = format_exec_command_display(executable, args);
        let cwd_display = cwd.display().to_string();
        let mut child = command.spawn().map_err(|source| ToolError::ProcessIo {
            process_id: executable.to_string(),
            source,
        })?;
        let pid_ref = format!("pid:{}", child.id());
        let output = Arc::new(Mutex::new(ManagedProcessOutput::default()));
        let stdout_reader = child.stdout.take().map(|stdout| {
            spawn_process_reader(stdout, ProcessOutputStream::Stdout, output.clone())
        });
        let stderr_reader = child.stderr.take().map(|stderr| {
            spawn_process_reader(stderr, ProcessOutputStream::Stderr, output.clone())
        });

        let process_id = {
            let mut registry = self.processes.lock();
            let process_id = format!("{}-{}", kind.as_prefix(), registry.next_process_id);
            registry.next_process_id += 1;
            registry.processes.insert(
                process_id.clone(),
                Arc::new(ManagedProcess {
                    kind,
                    child: Mutex::new(child),
                    output,
                    stdout_reader: Mutex::new(stdout_reader),
                    stderr_reader: Mutex::new(stderr_reader),
                }),
            );
            process_id
        };

        Ok(ToolOutput::ProcessStart(ProcessStartOutput {
            process_id,
            pid_ref,
            kind,
            command_display,
            cwd: cwd_display,
        }))
    }

    fn read_process_output(
        &mut self,
        process_id: &str,
        expected_kind: ProcessKind,
        input: ProcessReadOutputInput,
    ) -> Result<ToolOutput, ToolError> {
        let managed = self.lookup_process(process_id, expected_kind)?;
        let status = managed.poll_status(process_id)?;
        let output = managed
            .output
            .lock()
            .expect("managed process output poisoned");
        let stream = input.stream.unwrap_or(ProcessOutputStream::Merged);
        let source = match stream {
            ProcessOutputStream::Merged => output.merged.as_str(),
            ProcessOutputStream::Stdout => output.stdout.as_str(),
            ProcessOutputStream::Stderr => output.stderr.as_str(),
        };
        let max_bytes = normalize_process_output_max_bytes(input.max_bytes);
        let max_lines = normalize_process_output_max_lines(input.max_lines);
        let read = build_process_output_read(
            ProcessOutputView {
                process_id,
                stream,
                status,
                exit_code: output.exit_code,
                source,
            },
            input.cursor,
            max_bytes,
            max_lines,
        );
        Ok(ToolOutput::ProcessOutputRead(read))
    }

    fn wait_process(
        &mut self,
        process_id: &str,
        expected_kind: ProcessKind,
    ) -> Result<ToolOutput, ToolError> {
        let managed = self.lookup_process(process_id, expected_kind)?;
        let exit_status = {
            let mut child = managed.child.lock().expect("managed child poisoned");
            child.wait().map_err(|source| ToolError::ProcessIo {
                process_id: process_id.to_string(),
                source,
            })?
        };
        {
            let mut output = managed
                .output
                .lock()
                .expect("managed process output poisoned");
            output.finished_status = Some(ProcessResultStatus::Exited);
            output.exit_code = exit_status.code();
        }
        managed.drain_readers(PROCESS_READER_DRAIN_GRACE);
        self.remove_process(process_id);
        let output = managed
            .output
            .lock()
            .expect("managed process output poisoned");

        Ok(ToolOutput::ProcessResult(ProcessResult {
            process_id: process_id.to_string(),
            status: ProcessResultStatus::Exited,
            exit_code: output.exit_code,
            stdout: output.stdout.clone(),
            stderr: output.stderr.clone(),
        }))
    }

    fn kill_process(
        &mut self,
        process_id: &str,
        expected_kind: ProcessKind,
    ) -> Result<ToolOutput, ToolError> {
        let managed = self.lookup_process(process_id, expected_kind)?;
        {
            let mut child = managed.child.lock().expect("managed child poisoned");
            child.kill().map_err(|source| ToolError::ProcessIo {
                process_id: process_id.to_string(),
                source,
            })?;
            let exit_status = child.wait().map_err(|source| ToolError::ProcessIo {
                process_id: process_id.to_string(),
                source,
            })?;
            let mut output = managed
                .output
                .lock()
                .expect("managed process output poisoned");
            output.finished_status = Some(ProcessResultStatus::Killed);
            output.exit_code = exit_status.code();
        }
        managed.drain_readers(PROCESS_READER_DRAIN_GRACE);
        self.remove_process(process_id);
        let output = managed
            .output
            .lock()
            .expect("managed process output poisoned");

        Ok(ToolOutput::ProcessResult(ProcessResult {
            process_id: process_id.to_string(),
            status: ProcessResultStatus::Killed,
            exit_code: output.exit_code,
            stdout: output.stdout.clone(),
            stderr: output.stderr.clone(),
        }))
    }

    fn lookup_process(
        &self,
        process_id: &str,
        expected_kind: ProcessKind,
    ) -> Result<Arc<ManagedProcess>, ToolError> {
        let managed = self
            .processes
            .lock()
            .processes
            .get(process_id)
            .cloned()
            .ok_or_else(|| ToolError::UnknownProcess {
                process_id: process_id.to_string(),
            })?;

        if managed.kind != expected_kind {
            return Err(ToolError::ProcessFamilyMismatch {
                process_id: process_id.to_string(),
                expected: expected_kind,
                actual: managed.kind,
            });
        }

        Ok(managed)
    }

    fn remove_process(&self, process_id: &str) {
        self.processes.lock().processes.remove(process_id);
    }
}

impl ManagedProcess {
    fn poll_status(&self, process_id: &str) -> Result<ProcessOutputStatus, ToolError> {
        {
            let output = self.output.lock().expect("managed process output poisoned");
            if let Some(status) = output.finished_status {
                return Ok(match status {
                    ProcessResultStatus::Exited => ProcessOutputStatus::Exited,
                    ProcessResultStatus::Killed => ProcessOutputStatus::Killed,
                });
            }
        }

        if let Ok(mut child) = self.child.try_lock()
            && let Some(exit_status) = child.try_wait().map_err(|source| ToolError::ProcessIo {
                process_id: process_id.to_string(),
                source,
            })?
        {
            let mut output = self.output.lock().expect("managed process output poisoned");
            output.finished_status = Some(ProcessResultStatus::Exited);
            output.exit_code = exit_status.code();
            return Ok(ProcessOutputStatus::Exited);
        }

        Ok(ProcessOutputStatus::Running)
    }

    fn drain_readers(&self, max_wait: Duration) {
        let deadline = Instant::now() + max_wait;
        loop {
            let stdout_done = Self::join_reader_if_finished(&self.stdout_reader);
            let stderr_done = Self::join_reader_if_finished(&self.stderr_reader);
            if stdout_done && stderr_done {
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn join_reader_if_finished(slot: &Mutex<Option<thread::JoinHandle<()>>>) -> bool {
        let mut guard = slot.lock().expect("reader mutex poisoned");
        let Some(handle) = guard.as_ref() else {
            return true;
        };
        if !handle.is_finished() {
            return false;
        }
        let Some(handle) = guard.take() else {
            return true;
        };
        let _ = handle.join();
        true
    }
}

fn spawn_process_reader<R>(
    mut reader: R,
    stream: ProcessOutputStream,
    output: Arc<Mutex<ManagedProcessOutput>>,
) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            let bytes = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(bytes) => bytes,
                Err(_) => break,
            };
            let text = String::from_utf8_lossy(&buffer[..bytes]).to_string();
            let mut guard = output.lock().expect("managed process output poisoned");
            match stream {
                ProcessOutputStream::Merged => {}
                ProcessOutputStream::Stdout => guard.stdout.push_str(text.as_str()),
                ProcessOutputStream::Stderr => guard.stderr.push_str(text.as_str()),
            }
            guard.merged.push_str(text.as_str());
        }
    })
}

fn normalize_process_output_max_bytes(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_PROCESS_OUTPUT_READ_MAX_BYTES)
        .clamp(1, MAX_PROCESS_OUTPUT_READ_MAX_BYTES)
}

fn normalize_process_output_max_lines(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_PROCESS_OUTPUT_READ_MAX_LINES)
        .clamp(1, MAX_PROCESS_OUTPUT_READ_MAX_LINES)
}

fn clamp_utf8_boundary(text: &str, offset: usize) -> usize {
    let mut clamped = offset.min(text.len());
    while clamped > 0 && !text.is_char_boundary(clamped) {
        clamped -= 1;
    }
    clamped
}

fn prefix_boundary_for_max_bytes(text: &str, max_bytes: usize) -> usize {
    let mut end = text.len().min(max_bytes);
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

fn tail_start_for_max_bytes(text: &str, max_bytes: usize) -> usize {
    if text.len() <= max_bytes {
        return 0;
    }
    let mut start = text.len() - max_bytes;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    start.min(text.len())
}

fn tail_start_for_max_lines(text: &str, max_lines: usize) -> usize {
    let segments = text.split_inclusive('\n').collect::<Vec<_>>();
    if segments.len() <= max_lines {
        return 0;
    }
    segments[..segments.len() - max_lines]
        .iter()
        .map(|segment| segment.len())
        .sum()
}

fn prefix_end_for_max_lines_and_bytes(text: &str, max_lines: usize, max_bytes: usize) -> usize {
    let mut end = 0_usize;
    for (lines, segment) in text.split_inclusive('\n').enumerate() {
        if lines >= max_lines || end + segment.len() > max_bytes {
            break;
        }
        end += segment.len();
    }

    if end == 0 && !text.is_empty() {
        prefix_boundary_for_max_bytes(text, max_bytes)
    } else {
        end
    }
}

struct ProcessOutputView<'a> {
    process_id: &'a str,
    stream: ProcessOutputStream,
    status: ProcessOutputStatus,
    exit_code: Option<i32>,
    source: &'a str,
}

fn build_process_output_read(
    view: ProcessOutputView<'_>,
    cursor: Option<usize>,
    max_bytes: usize,
    max_lines: usize,
) -> ProcessOutputRead {
    match cursor {
        Some(cursor) => {
            let cursor = clamp_utf8_boundary(view.source, cursor);
            let remaining = &view.source[cursor..];
            let end = cursor + prefix_end_for_max_lines_and_bytes(remaining, max_lines, max_bytes);
            ProcessOutputRead {
                process_id: view.process_id.to_string(),
                stream: view.stream,
                status: view.status,
                exit_code: view.exit_code,
                cursor,
                next_cursor: end,
                truncated: end < view.source.len(),
                text: view.source[cursor..end].to_string(),
            }
        }
        None => {
            let byte_start = tail_start_for_max_bytes(view.source, max_bytes);
            let line_start =
                byte_start + tail_start_for_max_lines(&view.source[byte_start..], max_lines);
            ProcessOutputRead {
                process_id: view.process_id.to_string(),
                stream: view.stream,
                status: view.status,
                exit_code: view.exit_code,
                cursor: line_start,
                next_cursor: view.source.len(),
                truncated: line_start > 0,
                text: view.source[line_start..].to_string(),
            }
        }
    }
}

#[cfg(unix)]
fn detach_command_from_terminal(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn detach_command_from_terminal(_command: &mut Command) {}

impl SharedProcessRegistry {
    fn lock(&self) -> std::sync::MutexGuard<'_, ProcessRegistryState> {
        self.inner.lock().expect("shared process registry poisoned")
    }

    pub fn active_process_ids(&self, kind: Option<ProcessKind>) -> Vec<String> {
        let registry = self.lock();
        registry
            .processes
            .iter()
            .filter_map(|(process_id, managed)| {
                if kind.is_none_or(|expected| expected == managed.kind) {
                    Some(process_id.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn read_exec_output(
        &self,
        process_id: &str,
        stream: ProcessOutputStream,
        cursor: Option<usize>,
        max_bytes: Option<usize>,
        max_lines: Option<usize>,
    ) -> Result<ProcessOutputRead, ToolError> {
        let managed = self
            .lock()
            .processes
            .get(process_id)
            .cloned()
            .ok_or_else(|| ToolError::UnknownProcess {
                process_id: process_id.to_string(),
            })?;
        if managed.kind != ProcessKind::Exec {
            return Err(ToolError::ProcessFamilyMismatch {
                process_id: process_id.to_string(),
                expected: ProcessKind::Exec,
                actual: managed.kind,
            });
        }
        let status = managed.poll_status(process_id)?;
        let output = managed
            .output
            .lock()
            .expect("managed process output poisoned");
        let source = match stream {
            ProcessOutputStream::Merged => output.merged.as_str(),
            ProcessOutputStream::Stdout => output.stdout.as_str(),
            ProcessOutputStream::Stderr => output.stderr.as_str(),
        };
        Ok(build_process_output_read(
            ProcessOutputView {
                process_id,
                stream,
                status,
                exit_code: output.exit_code,
                source,
            },
            cursor,
            normalize_process_output_max_bytes(max_bytes),
            normalize_process_output_max_lines(max_lines),
        ))
    }
}

impl ToolCall {
    fn invalid_arguments_error(name: &str, source: serde_json::Error) -> ToolCallParseError {
        ToolCallParseError::InvalidArguments {
            name: name.to_string(),
            source,
        }
    }

    fn parse_arguments_with_enum_repair<T: DeserializeOwned>(
        name: &str,
        arguments: &str,
        repairs: &[EnumLikeFieldRepair],
    ) -> Result<T, ToolCallParseError> {
        match serde_json::from_str(arguments) {
            Ok(parsed) => Ok(parsed),
            Err(source) => {
                if let Some(repaired) = repair_bare_enum_like_values(arguments, repairs)
                    && let Ok(parsed) = serde_json::from_str(&repaired)
                {
                    return Ok(parsed);
                }
                Err(Self::invalid_arguments_error(name, source))
            }
        }
    }

    pub fn name(&self) -> ToolName {
        match self {
            Self::FsRead(_) => ToolName::FsRead,
            Self::FsWrite(_) => ToolName::FsWrite,
            Self::FsPatch(_) => ToolName::FsPatch,
            Self::FsReadText(_) => ToolName::FsReadText,
            Self::FsReadLines(_) => ToolName::FsReadLines,
            Self::FsSearchText(_) => ToolName::FsSearchText,
            Self::FsFindInFiles(_) => ToolName::FsFindInFiles,
            Self::FsWriteText(_) => ToolName::FsWriteText,
            Self::FsPatchText(_) => ToolName::FsPatchText,
            Self::FsReplaceLines(_) => ToolName::FsReplaceLines,
            Self::FsInsertText(_) => ToolName::FsInsertText,
            Self::FsMkdir(_) => ToolName::FsMkdir,
            Self::FsMove(_) => ToolName::FsMove,
            Self::FsTrash(_) => ToolName::FsTrash,
            Self::FsList(_) => ToolName::FsList,
            Self::FsGlob(_) => ToolName::FsGlob,
            Self::FsSearch(_) => ToolName::FsSearch,
            Self::WebFetch(_) => ToolName::WebFetch,
            Self::WebSearch(_) => ToolName::WebSearch,
            Self::ExecStart(_) => ToolName::ExecStart,
            Self::ExecReadOutput(_) => ToolName::ExecReadOutput,
            Self::ExecWait(_) => ToolName::ExecWait,
            Self::ExecKill(_) => ToolName::ExecKill,
            Self::PlanRead(_) => ToolName::PlanRead,
            Self::PlanWrite(_) => ToolName::PlanWrite,
            Self::InitPlan(_) => ToolName::InitPlan,
            Self::AddTask(_) => ToolName::AddTask,
            Self::SetTaskStatus(_) => ToolName::SetTaskStatus,
            Self::AddTaskNote(_) => ToolName::AddTaskNote,
            Self::EditTask(_) => ToolName::EditTask,
            Self::PlanSnapshot(_) => ToolName::PlanSnapshot,
            Self::PlanLint(_) => ToolName::PlanLint,
            Self::PromptBudgetRead(_) => ToolName::PromptBudgetRead,
            Self::PromptBudgetUpdate(_) => ToolName::PromptBudgetUpdate,
            Self::AutonomyStateRead(_) => ToolName::AutonomyStateRead,
            Self::SkillList(_) => ToolName::SkillList,
            Self::SkillRead(_) => ToolName::SkillRead,
            Self::SkillEnable(_) => ToolName::SkillEnable,
            Self::SkillDisable(_) => ToolName::SkillDisable,
            Self::ArtifactRead(_) => ToolName::ArtifactRead,
            Self::ArtifactSearch(_) => ToolName::ArtifactSearch,
            Self::ArtifactPin(_) => ToolName::ArtifactPin,
            Self::ArtifactUnpin(_) => ToolName::ArtifactUnpin,
            Self::KnowledgeSearch(_) => ToolName::KnowledgeSearch,
            Self::KnowledgeRead(_) => ToolName::KnowledgeRead,
            Self::SessionSearch(_) => ToolName::SessionSearch,
            Self::SessionRead(_) => ToolName::SessionRead,
            Self::SessionWait(_) => ToolName::SessionWait,
            Self::McpCall(_) => ToolName::McpCall,
            Self::McpSearchResources(_) => ToolName::McpSearchResources,
            Self::McpReadResource(_) => ToolName::McpReadResource,
            Self::McpSearchPrompts(_) => ToolName::McpSearchPrompts,
            Self::McpGetPrompt(_) => ToolName::McpGetPrompt,
            Self::AgentList(_) => ToolName::AgentList,
            Self::AgentRead(_) => ToolName::AgentRead,
            Self::AgentCreate(_) => ToolName::AgentCreate,
            Self::ContinueLater(_) => ToolName::ContinueLater,
            Self::ScheduleList(_) => ToolName::ScheduleList,
            Self::ScheduleRead(_) => ToolName::ScheduleRead,
            Self::ScheduleCreate(_) => ToolName::ScheduleCreate,
            Self::ScheduleUpdate(_) => ToolName::ScheduleUpdate,
            Self::ScheduleDelete(_) => ToolName::ScheduleDelete,
            Self::MessageAgent(_) => ToolName::MessageAgent,
            Self::GrantAgentChainContinuation(_) => ToolName::GrantAgentChainContinuation,
        }
    }

    pub fn scope_target(&self) -> Option<String> {
        match self {
            Self::FsRead(input) => Some(normalize_tool_path(&input.path)),
            Self::FsWrite(input) => Some(normalize_tool_path(&input.path)),
            Self::FsPatch(input) => Some(normalize_tool_path(&input.path)),
            Self::FsReadText(input) => Some(normalize_tool_path(&input.path)),
            Self::FsReadLines(input) => Some(normalize_tool_path(&input.path)),
            Self::FsSearchText(input) => Some(normalize_tool_path(&input.path)),
            Self::FsFindInFiles(_) => None,
            Self::FsWriteText(input) => Some(normalize_tool_path(&input.path)),
            Self::FsPatchText(input) => Some(normalize_tool_path(&input.path)),
            Self::FsReplaceLines(input) => Some(normalize_tool_path(&input.path)),
            Self::FsInsertText(input) => Some(normalize_tool_path(&input.path)),
            Self::FsMkdir(input) => Some(normalize_tool_path(&input.path)),
            Self::FsMove(input) => Some(normalize_tool_path(&input.dest)),
            Self::FsTrash(input) => Some(normalize_tool_path(&input.path)),
            Self::FsList(input) => Some(normalize_tool_path(&input.path)),
            Self::FsGlob(input) => Some(normalize_tool_path(&input.path)),
            Self::FsSearch(input) => Some(normalize_tool_path(&input.path)),
            Self::WebFetch(input) => Some(input.url.clone()),
            Self::WebSearch(_) => None,
            Self::ExecStart(input) => input.cwd.clone(),
            Self::ExecReadOutput(_) | Self::ExecWait(_) | Self::ExecKill(_) => None,
            Self::PlanRead(_)
            | Self::PlanWrite(_)
            | Self::InitPlan(_)
            | Self::AddTask(_)
            | Self::SetTaskStatus(_)
            | Self::AddTaskNote(_)
            | Self::EditTask(_)
            | Self::PlanSnapshot(_)
            | Self::PlanLint(_)
            | Self::PromptBudgetRead(_)
            | Self::PromptBudgetUpdate(_)
            | Self::AutonomyStateRead(_)
            | Self::SkillList(_)
            | Self::SkillRead(_)
            | Self::SkillEnable(_)
            | Self::SkillDisable(_)
            | Self::KnowledgeSearch(_)
            | Self::KnowledgeRead(_)
            | Self::SessionSearch(_)
            | Self::SessionRead(_)
            | Self::SessionWait(_)
            | Self::McpSearchResources(_)
            | Self::McpSearchPrompts(_)
            | Self::AgentList(_)
            | Self::AgentRead(_)
            | Self::AgentCreate(_)
            | Self::ContinueLater(_)
            | Self::ScheduleList(_)
            | Self::ScheduleRead(_)
            | Self::ScheduleCreate(_)
            | Self::ScheduleUpdate(_)
            | Self::ScheduleDelete(_)
            | Self::MessageAgent(_)
            | Self::GrantAgentChainContinuation(_) => None,
            Self::McpCall(input) => Some(input.exposed_name.clone()),
            Self::McpReadResource(input) => Some(format!("{}:{}", input.connector_id, input.uri)),
            Self::McpGetPrompt(input) => Some(format!("{}:{}", input.connector_id, input.name)),
            Self::ArtifactRead(input) => Some(input.artifact_id.clone()),
            Self::ArtifactPin(input) | Self::ArtifactUnpin(input) => {
                Some(input.artifact_id.clone())
            }
            Self::ArtifactSearch(_) => None,
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::FsRead(input) => format!("fs_read path={}", normalize_tool_path(&input.path)),
            Self::FsWrite(input) => format!(
                "fs_write path={} bytes={}",
                normalize_tool_path(&input.path),
                input.content.len()
            ),
            Self::FsPatch(input) => format!(
                "fs_patch path={} edits={}",
                normalize_tool_path(&input.path),
                input.edits.len()
            ),
            Self::FsReadText(input) => {
                format!("fs_read_text path={}", normalize_tool_path(&input.path))
            }
            Self::FsReadLines(input) => format!(
                "fs_read_lines path={} start_line={} end_line={}",
                normalize_tool_path(&input.path),
                input.start_line,
                input.end_line
            ),
            Self::FsSearchText(input) => format!(
                "fs_search_text path={} query={}",
                normalize_tool_path(&input.path),
                input.query
            ),
            Self::FsFindInFiles(input) => format!(
                "fs_find_in_files query={} glob={} limit={}",
                input.query,
                input.glob.as_deref().unwrap_or("*"),
                input.limit.unwrap_or(0)
            ),
            Self::FsWriteText(input) => format!(
                "fs_write_text path={} mode={} bytes={}",
                normalize_tool_path(&input.path),
                input.mode.as_str(),
                input.content.len()
            ),
            Self::FsPatchText(input) => {
                format!("fs_patch_text path={}", normalize_tool_path(&input.path))
            }
            Self::FsReplaceLines(input) => format!(
                "fs_replace_lines path={} start_line={} end_line={}",
                normalize_tool_path(&input.path),
                input.start_line,
                input.end_line
            ),
            Self::FsInsertText(input) => format!(
                "fs_insert_text path={} position={} line={}",
                normalize_tool_path(&input.path),
                input.position,
                input
                    .line
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            Self::FsMkdir(input) => format!("fs_mkdir path={}", normalize_tool_path(&input.path)),
            Self::FsMove(input) => format!(
                "fs_move src={} dest={}",
                normalize_tool_path(&input.src),
                normalize_tool_path(&input.dest)
            ),
            Self::FsTrash(input) => format!("fs_trash path={}", normalize_tool_path(&input.path)),
            Self::FsList(input) => format!(
                "fs_list path={} recursive={}",
                normalize_tool_path(&input.path),
                input.recursive
            ),
            Self::FsGlob(input) => format!(
                "fs_glob path={} pattern={}",
                normalize_tool_path(&input.path),
                input.pattern
            ),
            Self::FsSearch(input) => format!(
                "fs_search path={} query={}",
                normalize_tool_path(&input.path),
                input.query
            ),
            Self::WebFetch(input) => format!("web_fetch url={}", input.url),
            Self::WebSearch(input) => {
                format!("web_search query={} limit={}", input.query, input.limit)
            }
            Self::ExecStart(input) => {
                format!(
                    "exec_start cwd={} command={}",
                    input.cwd.as_deref().unwrap_or("."),
                    format_exec_command_display(input.executable.as_str(), &input.args)
                )
            }
            Self::ExecReadOutput(input) => format!(
                "exec_read_output process_id={} stream={} cursor={} max_bytes={} max_lines={}",
                input.process_id,
                input.stream.unwrap_or(ProcessOutputStream::Merged).as_str(),
                input
                    .cursor
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                input
                    .max_bytes
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                input
                    .max_lines
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            Self::ExecWait(input) => format!("exec_wait process_id={}", input.process_id),
            Self::ExecKill(input) => format!("exec_kill process_id={}", input.process_id),
            Self::PlanRead(_) => "plan_read".to_string(),
            Self::PlanWrite(input) => format!("plan_write items={}", input.items.len()),
            Self::InitPlan(input) => format!("init_plan goal={}", input.goal),
            Self::AddTask(input) => format!(
                "add_task description={} depends_on={}",
                input.description,
                input.depends_on.len()
            ),
            Self::SetTaskStatus(input) => format!(
                "set_task_status task_id={} status={}",
                input.task_id, input.new_status
            ),
            Self::AddTaskNote(input) => format!("add_task_note task_id={}", input.task_id),
            Self::EditTask(input) => format!("edit_task task_id={}", input.task_id),
            Self::PlanSnapshot(_) => "plan_snapshot".to_string(),
            Self::PlanLint(_) => "plan_lint".to_string(),
            Self::PromptBudgetRead(_) => "prompt_budget_read".to_string(),
            Self::PromptBudgetUpdate(input) => format!(
                "prompt_budget_update scope={} reset={} percentages={}",
                input.scope.as_str(),
                input.reset,
                input.percentages.is_some()
            ),
            Self::AutonomyStateRead(input) => format!(
                "autonomy_state_read max_items={} include_inactive_schedules={}",
                input
                    .max_items
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "default".to_string()),
                input.include_inactive_schedules.unwrap_or(false)
            ),
            Self::SkillList(input) => format!(
                "skill_list include_inactive={} offset={} limit={}",
                input.include_inactive.unwrap_or(true),
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(20)
            ),
            Self::SkillRead(input) => format!(
                "skill_read name={} max_bytes={}",
                input.name,
                input
                    .max_bytes
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "default".to_string())
            ),
            Self::SkillEnable(input) => format!("skill_enable name={}", input.name),
            Self::SkillDisable(input) => format!("skill_disable name={}", input.name),
            Self::ArtifactRead(input) => format!("artifact_read artifact_id={}", input.artifact_id),
            Self::ArtifactSearch(input) => {
                format!(
                    "artifact_search query={} limit={}",
                    input.query, input.limit
                )
            }
            Self::ArtifactPin(input) => format!("artifact_pin artifact_id={}", input.artifact_id),
            Self::ArtifactUnpin(input) => {
                format!("artifact_unpin artifact_id={}", input.artifact_id)
            }
            Self::KnowledgeSearch(input) => format!(
                "knowledge_search query={} offset={} limit={}",
                input.query,
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(0)
            ),
            Self::KnowledgeRead(input) => format!(
                "knowledge_read path={} mode={}",
                normalize_tool_path(&input.path),
                input.mode.unwrap_or(KnowledgeReadMode::Excerpt).as_str()
            ),
            Self::SessionSearch(input) => format!(
                "session_search query={} offset={} limit={}",
                input.query,
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(0)
            ),
            Self::SessionRead(input) => format!(
                "session_read session_id={} mode={}",
                input.session_id,
                input.mode.unwrap_or(SessionReadMode::Summary).as_str()
            ),
            Self::SessionWait(input) => format!(
                "session_wait session_id={} timeout_ms={} mode={}",
                input.session_id,
                input.wait_timeout_ms.unwrap_or(0),
                input.mode.unwrap_or(SessionReadMode::Transcript).as_str()
            ),
            Self::McpCall(input) => format!("mcp_call exposed_name={}", input.exposed_name),
            Self::McpSearchResources(input) => format!(
                "mcp_search_resources connector={} query={} offset={} limit={}",
                input.connector_id.as_deref().unwrap_or("*"),
                input.query.as_deref().unwrap_or("*"),
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(0)
            ),
            Self::McpReadResource(input) => format!(
                "mcp_read_resource connector_id={} uri={}",
                input.connector_id, input.uri
            ),
            Self::McpSearchPrompts(input) => format!(
                "mcp_search_prompts connector={} query={} offset={} limit={}",
                input.connector_id.as_deref().unwrap_or("*"),
                input.query.as_deref().unwrap_or("*"),
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(0)
            ),
            Self::McpGetPrompt(input) => format!(
                "mcp_get_prompt connector_id={} name={}",
                input.connector_id, input.name
            ),
            Self::AgentList(input) => format!(
                "agent_list offset={} limit={}",
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(0)
            ),
            Self::AgentRead(input) => format!("agent_read identifier={}", input.identifier),
            Self::AgentCreate(input) => format!(
                "agent_create name={} template={}",
                input.name,
                input.template_identifier.as_deref().unwrap_or("current")
            ),
            Self::ContinueLater(input) => format!(
                "continue_later delay_seconds={} delivery_mode={}",
                input.delay_seconds,
                input
                    .delivery_mode
                    .unwrap_or(AgentScheduleDeliveryMode::ExistingSession)
                    .as_str()
            ),
            Self::ScheduleList(input) => format!(
                "schedule_list agent={} offset={} limit={}",
                input.agent_identifier.as_deref().unwrap_or("*"),
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(0)
            ),
            Self::ScheduleRead(input) => format!("schedule_read id={}", input.id),
            Self::ScheduleCreate(input) => format!(
                "schedule_create id={} interval_seconds={}",
                input.id, input.interval_seconds
            ),
            Self::ScheduleUpdate(input) => format!("schedule_update id={}", input.id),
            Self::ScheduleDelete(input) => format!("schedule_delete id={}", input.id),
            Self::MessageAgent(input) => format!(
                "message_agent target_agent_id={} message_bytes={}",
                input.target_agent_id,
                input.message.len()
            ),
            Self::GrantAgentChainContinuation(input) => {
                format!("grant_agent_chain_continuation chain_id={}", input.chain_id)
            }
        }
    }

    pub fn from_openai_function(name: &str, arguments: &str) -> Result<Self, ToolCallParseError> {
        match name {
            "fs_read" => serde_json::from_str(arguments)
                .map(Self::FsRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_write" => serde_json::from_str(arguments)
                .map(Self::FsWrite)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_patch" => serde_json::from_str(arguments)
                .map(Self::FsPatch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_read_text" => serde_json::from_str(arguments)
                .map(Self::FsReadText)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_read_lines" => serde_json::from_str(arguments)
                .map(Self::FsReadLines)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_search_text" => serde_json::from_str(arguments)
                .map(Self::FsSearchText)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_find_in_files" => serde_json::from_str(arguments)
                .map(Self::FsFindInFiles)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_write_text" => serde_json::from_str(arguments)
                .map(Self::FsWriteText)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_patch_text" => serde_json::from_str(arguments)
                .map(Self::FsPatchText)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_replace_lines" => serde_json::from_str(arguments)
                .map(Self::FsReplaceLines)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_insert_text" => serde_json::from_str(arguments)
                .map(Self::FsInsertText)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_mkdir" => serde_json::from_str(arguments)
                .map(Self::FsMkdir)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_move" => serde_json::from_str(arguments)
                .map(Self::FsMove)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_trash" => serde_json::from_str(arguments)
                .map(Self::FsTrash)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_list" => serde_json::from_str(arguments)
                .map(Self::FsList)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_glob" => serde_json::from_str(arguments)
                .map(Self::FsGlob)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_search" => serde_json::from_str(arguments)
                .map(Self::FsSearch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "web_fetch" => serde_json::from_str(arguments)
                .map(Self::WebFetch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "web_search" => serde_json::from_str(arguments)
                .map(Self::WebSearch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "exec_start" => serde_json::from_str(arguments)
                .map(Self::ExecStart)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "exec_read_output" => serde_json::from_str(arguments)
                .map(Self::ExecReadOutput)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "exec_wait" => serde_json::from_str(arguments)
                .map(Self::ExecWait)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "exec_kill" => serde_json::from_str(arguments)
                .map(Self::ExecKill)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "plan_read" => serde_json::from_str(arguments)
                .map(Self::PlanRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "plan_write" => serde_json::from_str(arguments)
                .map(Self::PlanWrite)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "init_plan" => serde_json::from_str(arguments)
                .map(Self::InitPlan)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "add_task" => serde_json::from_str(arguments)
                .map(Self::AddTask)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "set_task_status" => serde_json::from_str(arguments)
                .map(Self::SetTaskStatus)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "add_task_note" => serde_json::from_str(arguments)
                .map(Self::AddTaskNote)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "edit_task" => serde_json::from_str(arguments)
                .map(Self::EditTask)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "plan_snapshot" => serde_json::from_str(arguments)
                .map(Self::PlanSnapshot)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "plan_lint" => serde_json::from_str(arguments)
                .map(Self::PlanLint)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "prompt_budget_read" => serde_json::from_str(arguments)
                .map(Self::PromptBudgetRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "prompt_budget_update" => serde_json::from_str(arguments)
                .map(Self::PromptBudgetUpdate)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "autonomy_state_read" => serde_json::from_str(arguments)
                .map(Self::AutonomyStateRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "skill_list" => serde_json::from_str(arguments)
                .map(Self::SkillList)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "skill_read" => serde_json::from_str(arguments)
                .map(Self::SkillRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "skill_enable" => serde_json::from_str(arguments)
                .map(Self::SkillEnable)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "skill_disable" => serde_json::from_str(arguments)
                .map(Self::SkillDisable)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "artifact_read" => serde_json::from_str(arguments)
                .map(Self::ArtifactRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "artifact_search" => serde_json::from_str(arguments)
                .map(Self::ArtifactSearch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "artifact_pin" => serde_json::from_str(arguments)
                .map(Self::ArtifactPin)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "artifact_unpin" => serde_json::from_str(arguments)
                .map(Self::ArtifactUnpin)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "knowledge_search" => serde_json::from_str(arguments)
                .map(Self::KnowledgeSearch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "knowledge_read" => {
                Self::parse_arguments_with_enum_repair(name, arguments, KNOWLEDGE_READ_ENUM_REPAIRS)
                    .map(Self::KnowledgeRead)
            }
            "session_search" => serde_json::from_str(arguments)
                .map(Self::SessionSearch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "session_read" => {
                Self::parse_arguments_with_enum_repair(name, arguments, SESSION_READ_ENUM_REPAIRS)
                    .map(Self::SessionRead)
            }
            "session_wait" => {
                Self::parse_arguments_with_enum_repair(name, arguments, SESSION_WAIT_ENUM_REPAIRS)
                    .map(Self::SessionWait)
            }
            "mcp_search_resources" => serde_json::from_str(arguments)
                .map(Self::McpSearchResources)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "mcp_read_resource" => serde_json::from_str(arguments)
                .map(Self::McpReadResource)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "mcp_search_prompts" => serde_json::from_str(arguments)
                .map(Self::McpSearchPrompts)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "mcp_get_prompt" => serde_json::from_str(arguments)
                .map(Self::McpGetPrompt)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "agent_list" => serde_json::from_str(arguments)
                .map(Self::AgentList)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "agent_read" => serde_json::from_str(arguments)
                .map(Self::AgentRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "agent_create" => serde_json::from_str(arguments)
                .map(Self::AgentCreate)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "continue_later" => {
                Self::parse_arguments_with_enum_repair(name, arguments, CONTINUE_LATER_ENUM_REPAIRS)
                    .map(Self::ContinueLater)
            }
            "schedule_list" => serde_json::from_str(arguments)
                .map(Self::ScheduleList)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "schedule_read" => serde_json::from_str(arguments)
                .map(Self::ScheduleRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "schedule_create" => {
                Self::parse_arguments_with_enum_repair(name, arguments, SCHEDULE_ENUM_REPAIRS)
                    .map(Self::ScheduleCreate)
            }
            "schedule_update" => {
                Self::parse_arguments_with_enum_repair(name, arguments, SCHEDULE_ENUM_REPAIRS)
                    .map(Self::ScheduleUpdate)
            }
            "schedule_delete" => serde_json::from_str(arguments)
                .map(Self::ScheduleDelete)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "message_agent" => serde_json::from_str(arguments)
                .map(Self::MessageAgent)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "grant_agent_chain_continuation" => serde_json::from_str(arguments)
                .map(Self::GrantAgentChainContinuation)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            _ if name.starts_with("mcp__") => {
                let parsed = if arguments.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str::<Value>(arguments).map_err(|source| {
                        ToolCallParseError::InvalidArguments {
                            name: name.to_string(),
                            source,
                        }
                    })?
                };
                if !parsed.is_object() {
                    return Err(ToolCallParseError::InvalidArguments {
                        name: name.to_string(),
                        source: serde_json::Error::io(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "dynamic MCP tool arguments must be a JSON object",
                        )),
                    });
                }
                Ok(Self::McpCall(McpCallInput {
                    exposed_name: name.to_string(),
                    arguments_json: parsed.to_string(),
                }))
            }
            _ => Err(ToolCallParseError::UnknownTool {
                name: name.to_string(),
            }),
        }
    }
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

impl ProcessOutputStream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merged => "merged",
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
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

impl KnowledgeSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RootDoc => "root_doc",
            Self::ProjectDoc => "project_doc",
            Self::ProjectNote => "project_note",
            Self::ExtraDoc => "extra_doc",
        }
    }
}

impl KnowledgeRoot {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RootDocs => "root_docs",
            Self::Docs => "docs",
            Self::Projects => "projects",
            Self::Notes => "notes",
            Self::Extra => "extra",
        }
    }
}

impl KnowledgeReadMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Excerpt => "excerpt",
            Self::Full => "full",
        }
    }
}

impl SessionReadMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Timeline => "timeline",
            Self::Transcript => "transcript",
            Self::Artifacts => "artifacts",
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
                    "artifact_read artifact_id={} bytes={}",
                    output.artifact_id,
                    output.content.len()
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

impl ToolDefinition {
    pub fn openai_function_schema(&self) -> Value {
        json!({
            "type": "function",
            "name": self.name.as_str(),
            "description": self.description,
            "parameters": self.name.input_schema(),
        })
    }
}

impl ToolName {
    pub fn input_schema(self) -> Value {
        match self {
            Self::FsRead => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to read" }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            Self::FsReadText => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to read" }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            Self::FsReadLines => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to read" },
                    "start_line": { "type": "integer", "minimum": 1 },
                    "end_line": { "type": "integer", "minimum": 1 }
                },
                "required": ["path", "start_line", "end_line"],
                "additionalProperties": false,
            }),
            Self::FsWrite => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to write" },
                    "content": { "type": "string", "description": "UTF-8 file content to write" }
                },
                "required": ["path", "content"],
                "additionalProperties": false,
            }),
            Self::FsWriteText => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to write" },
                    "content": { "type": "string", "description": "UTF-8 file content to write" },
                    "mode": { "type": "string", "enum": ["create", "overwrite", "upsert"] }
                },
                "required": ["path", "content", "mode"],
                "additionalProperties": false,
            }),
            Self::FsPatch => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to patch" },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old": { "type": "string" },
                                "new": { "type": "string" },
                                "replace_all": { "type": "boolean" }
                            },
                            "required": ["old", "new", "replace_all"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["path", "edits"],
                "additionalProperties": false,
            }),
            Self::FsPatchText => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to patch" },
                    "search": { "type": "string", "description": "Exact existing text fragment to replace. Do not send old/new patch arrays; use this `search` field directly." },
                    "replace": { "type": "string", "description": "Replacement text for the first matching `search` fragment." }
                },
                "required": ["path", "search", "replace"],
                "additionalProperties": false,
            }),
            Self::FsReplaceLines => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to patch" },
                    "start_line": { "type": "integer", "minimum": 1 },
                    "end_line": { "type": "integer", "minimum": 1 },
                    "content": { "type": "string" }
                },
                "required": ["path", "start_line", "end_line", "content"],
                "additionalProperties": false,
            }),
            Self::FsInsertText => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to modify" },
                    "line": { "type": ["integer", "null"], "minimum": 1 },
                    "position": { "type": "string", "enum": ["before", "after", "prepend", "append"] },
                    "content": { "type": "string" }
                },
                "required": ["path", "position", "content"],
                "additionalProperties": false,
            }),
            Self::FsList => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to list" },
                    "recursive": { "type": "boolean", "description": "Whether to recurse into subdirectories" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of entries to return; defaults to a safe bounded page size" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional zero-based offset for continuing a previous paginated listing" }
                },
                "required": ["path", "recursive"],
                "additionalProperties": false,
            }),
            Self::FsGlob => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace root to search from" },
                    "pattern": { "type": "string", "description": "Glob-style path pattern" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of matches to return; defaults to a safe bounded page size" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional zero-based offset for continuing a previous paginated glob result" }
                },
                "required": ["path", "pattern"],
                "additionalProperties": false,
            }),
            Self::FsSearch => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace root to search from" },
                    "query": { "type": "string", "description": "Literal text to search for" }
                },
                "required": ["path", "query"],
                "additionalProperties": false,
            }),
            Self::FsSearchText => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace file to search" },
                    "query": { "type": "string", "description": "Literal text to search for" }
                },
                "required": ["path", "query"],
                "additionalProperties": false,
            }),
            Self::FsFindInFiles => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Literal text to search for" },
                    "glob": { "type": ["string", "null"], "description": "Optional glob used to filter matching paths" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of matches" }
                },
                "required": ["query"],
                "additionalProperties": false,
            }),
            Self::FsMkdir => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative directory path to create" }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            Self::FsMove => json!({
                "type": "object",
                "properties": {
                    "src": { "type": "string", "description": "Relative source path" },
                    "dest": { "type": "string", "description": "Relative destination path" }
                },
                "required": ["src", "dest"],
                "additionalProperties": false,
            }),
            Self::FsTrash => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to move into trash" }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            Self::WebFetch => json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Absolute exact URL to fetch and convert into readable text. Prefer web_search first unless the user supplied this URL, web_search returned it, or it is a known canonical source." }
                },
                "required": ["url"],
                "additionalProperties": false,
            }),
            Self::WebSearch => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query text. Use this first for current or external information; configured deployments may use SearXNG." },
                    "limit": { "type": "integer", "minimum": 1, "description": "Maximum number of results" }
                },
                "required": ["query", "limit"],
                "additionalProperties": false,
            }),
            Self::ExecStart => json!({
                "type": "object",
                "properties": {
                    "executable": { "type": "string" },
                    "args": { "type": "array", "items": { "type": "string" } },
                    "cwd": { "type": ["string", "null"] }
                },
                "required": ["executable", "args"],
                "additionalProperties": false,
            }),
            Self::ExecReadOutput => json!({
                "type": "object",
                "properties": {
                    "process_id": { "type": "string", "description": "Process id returned by exec_start" },
                    "stream": { "type": ["string", "null"], "enum": ["merged", "stdout", "stderr", null], "description": "Which stream view to read; defaults to merged" },
                    "cursor": { "type": ["integer", "null"], "minimum": 0, "description": "Optional cursor returned by a previous exec_read_output call" },
                    "max_bytes": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of UTF-8 bytes to return" },
                    "max_lines": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of lines to return" }
                },
                "required": ["process_id"],
                "additionalProperties": false,
            }),
            Self::ExecWait | Self::ExecKill => json!({
                "type": "object",
                "properties": {
                    "process_id": { "type": "string" }
                },
                "required": ["process_id"],
                "additionalProperties": false,
            }),
            Self::PlanRead => json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
            Self::PlanWrite => json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "content": { "type": "string" },
                                "status": {
                                    "type": "string",
                                    "enum": ["pending", "in_progress", "completed"]
                                }
                            },
                            "required": ["id", "content", "status"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["items"],
                "additionalProperties": false,
            }),
            Self::InitPlan => json!({
                "type": "object",
                "properties": {
                    "goal": { "type": "string", "description": "Top-level goal for the structured session plan" }
                },
                "required": ["goal"],
                "additionalProperties": false,
            }),
            Self::AddTask => json!({
                "type": "object",
                "properties": {
                    "description": { "type": "string", "description": "Human-readable task description" },
                    "depends_on": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional task ids that must complete first"
                    },
                    "parent_task_id": { "type": ["string", "null"], "description": "Optional parent task id" }
                },
                "required": ["description"],
                "additionalProperties": false,
            }),
            Self::SetTaskStatus => json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "new_status": {
                        "type": "string",
                        "enum": ["pending", "in_progress", "completed", "blocked", "cancelled"]
                    },
                    "blocked_reason": { "type": ["string", "null"] }
                },
                "required": ["task_id", "new_status"],
                "additionalProperties": false,
            }),
            Self::AddTaskNote => json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "note": { "type": "string" }
                },
                "required": ["task_id", "note"],
                "additionalProperties": false,
            }),
            Self::EditTask => json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string" },
                    "description": { "type": ["string", "null"] },
                    "depends_on": {
                        "type": ["array", "null"],
                        "items": { "type": "string" }
                    },
                    "parent_task_id": { "type": ["string", "null"] },
                    "clear_parent_task": { "type": "boolean" }
                },
                "required": ["task_id"],
                "additionalProperties": false,
            }),
            Self::PlanSnapshot | Self::PlanLint => json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
            Self::PromptBudgetRead => json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
            Self::PromptBudgetUpdate => json!({
                "type": "object",
                "properties": {
                    "scope": {
                        "type": "string",
                        "enum": ["session", "next_turn"],
                        "description": "Update scope. Use session for persistent session policy. Use next_turn for a one-shot override applied to the next full prompt assembly only; provider continuation rounds do not rebuild the current prompt."
                    },
                    "reset": { "type": "boolean", "description": "Reset the selected prompt budget scope back to runtime defaults before applying any supplied percentages. With scope=next_turn and no percentages, this clears the queued one-shot override." },
                    "percentages": {
                        "type": ["object", "null"],
                        "description": "Optional layer percentage overrides. Supplied values merge into the selected scope's base policy; after merging, all percentages must sum to 100.",
                        "properties": {
                            "system": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "agents": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "active_skills": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "session_head": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "autonomy_state": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "plan": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "context_summary": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "offload_refs": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "recent_tool_activity": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 },
                            "transcript_tail": { "type": ["integer", "null"], "minimum": 0, "maximum": 100 }
                        },
                        "additionalProperties": false
                    },
                    "reason": { "type": ["string", "null"], "description": "Short explanation for audit/debug views" }
                },
                "additionalProperties": false,
            }),
            Self::SkillList => json!({
                "type": "object",
                "properties": {
                    "include_inactive": { "type": ["boolean", "null"], "description": "Whether to include inactive skills. Defaults to true so the model can discover the full skill catalog before activation." },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of skills to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" }
                },
                "additionalProperties": false,
            }),
            Self::AutonomyStateRead => json!({
                "type": "object",
                "properties": {
                    "max_items": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of schedules, jobs, child sessions, inbox events, and mesh peers per section" },
                    "include_inactive_schedules": { "type": ["boolean", "null"], "description": "Whether to include disabled schedules in the related schedules section. Defaults to false." }
                },
                "additionalProperties": false,
            }),
            Self::SkillRead => json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Skill name from skill_list" },
                    "max_bytes": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum UTF-8 bytes from SKILL.md body to return" }
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
            Self::SkillEnable => json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Skill name from skill_list to manually enable for the current session" }
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
            Self::SkillDisable => json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Skill name from skill_list to manually disable for the current session" }
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
            Self::ArtifactRead => json!({
                "type": "object",
                "properties": {
                    "artifact_id": { "type": "string", "description": "Artifact id from the offloaded context references block" }
                },
                "required": ["artifact_id"],
                "additionalProperties": false,
            }),
            Self::ArtifactSearch => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query against offloaded context labels, summaries, and payloads" },
                    "limit": { "type": "integer", "minimum": 1, "description": "Maximum number of matching artifacts to return" }
                },
                "required": ["query", "limit"],
                "additionalProperties": false,
            }),
            Self::ArtifactPin | Self::ArtifactUnpin => json!({
                "type": "object",
                "properties": {
                    "artifact_id": { "type": "string", "description": "Artifact id from the offloaded context references block" }
                },
                "required": ["artifact_id"],
                "additionalProperties": false,
            }),
            Self::KnowledgeSearch => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query across canonical project knowledge roots" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of search results to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" },
                    "kinds": {
                        "type": ["array", "null"],
                        "items": { "type": "string", "enum": ["root_doc", "project_doc", "project_note", "extra_doc"] },
                        "description": "Optional knowledge source kind filters"
                    },
                    "roots": {
                        "type": ["array", "null"],
                        "items": { "type": "string", "enum": ["root_docs", "docs", "projects", "notes", "extra"] },
                        "description": "Optional canonical knowledge root filters"
                    }
                },
                "required": ["query"],
                "additionalProperties": false,
            }),
            Self::KnowledgeRead => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative knowledge file path to read" },
                    "mode": { "type": ["string", "null"], "enum": ["excerpt", "full", null], "description": "Optional view mode as a quoted JSON string; use \"excerpt\" or \"full\". Defaults to excerpt" },
                    "cursor": { "type": ["integer", "null"], "minimum": 0, "description": "Optional zero-based line cursor returned by a previous knowledge_read call" },
                    "max_bytes": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum UTF-8 bytes to return" },
                    "max_lines": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of lines to return" }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            Self::SessionSearch => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query across historical sessions so you can find the exact session_id before reading or waiting on a session" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of search results to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" },
                    "tiers": {
                        "type": ["array", "null"],
                        "items": { "type": "string", "enum": ["active", "warm", "cold"] },
                        "description": "Optional retention tier filters"
                    },
                    "agent_identifier": { "type": ["string", "null"], "description": "Optional agent id or name filter" },
                    "updated_after": { "type": ["integer", "null"], "description": "Optional inclusive lower updated_at bound" },
                    "updated_before": { "type": ["integer", "null"], "description": "Optional inclusive upper updated_at bound" }
                },
                "required": ["query"],
                "additionalProperties": false,
            }),
            Self::SessionRead => json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session id to inspect without waiting for new work" },
                    "mode": { "type": ["string", "null"], "enum": ["summary", "timeline", "transcript", "artifacts", null], "description": "Optional bounded view mode as a quoted JSON string; use \"summary\", \"timeline\", \"transcript\", or \"artifacts\". Defaults to summary" },
                    "cursor": { "type": ["integer", "null"], "minimum": 0, "description": "Optional item cursor returned by a previous session_read call" },
                    "max_items": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of messages or artifacts to return" },
                    "max_bytes": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum content bytes to return across message bodies" },
                    "include_tools": { "type": ["boolean", "null"], "description": "Whether transcript and timeline modes should include tool-role entries; defaults to true" }
                },
                "required": ["session_id"],
                "additionalProperties": false,
            }),
            Self::SessionWait => json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session id to wait on; after message_agent this should normally be the returned recipient_session_id" },
                    "wait_timeout_ms": { "type": ["integer", "null"], "minimum": 0, "description": "Optional wait timeout in milliseconds; defaults to the runtime request timeout and 0 means read immediately without waiting" },
                    "mode": { "type": ["string", "null"], "enum": ["summary", "timeline", "transcript", "artifacts", null], "description": "Optional bounded view mode for the returned snapshot as a quoted JSON string; use \"summary\", \"timeline\", \"transcript\", or \"artifacts\". Defaults to transcript" },
                    "cursor": { "type": ["integer", "null"], "minimum": 0, "description": "Optional item cursor returned by a previous session_wait or session_read call" },
                    "max_items": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of messages or artifacts to return" },
                    "max_bytes": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum content bytes to return across message bodies" },
                    "include_tools": { "type": ["boolean", "null"], "description": "Whether transcript and timeline modes should include tool-role entries; defaults to true" }
                },
                "required": ["session_id"],
                "additionalProperties": false,
            }),
            Self::McpCall => json!({
                "type": "object",
                "properties": {
                    "arguments": {
                        "type": "object",
                        "description": "Dynamic MCP tool arguments; this schema is replaced at runtime with the discovered MCP tool schema"
                    }
                },
                "additionalProperties": true,
            }),
            Self::McpSearchResources => json!({
                "type": "object",
                "properties": {
                    "connector_id": { "type": ["string", "null"], "description": "Optional MCP connector id filter" },
                    "query": { "type": ["string", "null"], "description": "Optional search query against resource metadata" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of results to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" }
                },
                "additionalProperties": false,
            }),
            Self::McpReadResource => json!({
                "type": "object",
                "properties": {
                    "connector_id": { "type": "string", "description": "MCP connector id" },
                    "uri": { "type": "string", "description": "Resource URI to read" }
                },
                "required": ["connector_id", "uri"],
                "additionalProperties": false,
            }),
            Self::McpSearchPrompts => json!({
                "type": "object",
                "properties": {
                    "connector_id": { "type": ["string", "null"], "description": "Optional MCP connector id filter" },
                    "query": { "type": ["string", "null"], "description": "Optional search query against prompt metadata" },
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of results to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" }
                },
                "additionalProperties": false,
            }),
            Self::McpGetPrompt => json!({
                "type": "object",
                "properties": {
                    "connector_id": { "type": "string", "description": "MCP connector id" },
                    "name": { "type": "string", "description": "Prompt name to retrieve" },
                    "arguments": {
                        "type": ["object", "null"],
                        "description": "Optional prompt arguments",
                        "additionalProperties": { "type": "string" }
                    }
                },
                "required": ["connector_id", "name"],
                "additionalProperties": false,
            }),
            Self::AgentList => json!({
                "type": "object",
                "properties": {
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of agents to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" }
                },
                "additionalProperties": false,
            }),
            Self::AgentRead => json!({
                "type": "object",
                "properties": {
                    "identifier": { "type": "string", "description": "Agent id or name to resolve" }
                },
                "required": ["identifier"],
                "additionalProperties": false,
            }),
            Self::AgentCreate => json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Display name for the new agent" },
                    "template_identifier": { "type": ["string", "null"], "description": "Optional template agent id or name; defaults to the current session agent" }
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
            Self::ContinueLater => json!({
                "type": "object",
                "properties": {
                    "delay_seconds": { "type": "integer", "minimum": 1, "description": "How many seconds to wait before the same session wakes up" },
                    "handoff_payload": { "type": "string", "description": "what to say or do when the timer fires; include the user's requested reminder text and any relevant context" },
                    "delivery_mode": { "type": ["string", "null"], "enum": ["fresh_session", "existing_session", null], "description": "Optional delivery mode as a quoted JSON string; use \"fresh_session\" or \"existing_session\". Defaults to existing_session, which resumes the same session and is the right default for reminders" }
                },
                "required": ["delay_seconds", "handoff_payload"],
                "additionalProperties": false,
            }),
            Self::ScheduleList => json!({
                "type": "object",
                "properties": {
                    "limit": { "type": ["integer", "null"], "minimum": 1, "description": "Optional maximum number of schedules to return" },
                    "offset": { "type": ["integer", "null"], "minimum": 0, "description": "Optional pagination offset" },
                    "agent_identifier": { "type": ["string", "null"], "description": "Optional agent id or name filter" }
                },
                "additionalProperties": false,
            }),
            Self::ScheduleRead => json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Schedule id to read" }
                },
                "required": ["id"],
                "additionalProperties": false,
            }),
            Self::ScheduleCreate => json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Stable schedule id" },
                    "agent_identifier": { "type": ["string", "null"], "description": "Optional agent id or name; defaults to the current session agent" },
                    "prompt": { "type": "string", "description": "Prompt delivered when the advanced or recurring schedule fires" },
                    "mode": { "type": ["string", "null"], "enum": ["interval", "after_completion", "once", null], "description": "Optional schedule mode as a quoted JSON string; use \"interval\", \"after_completion\", or \"once\". Defaults to interval. Use once only for explicit one-shot schedules; for simple reminders use continue_later instead" },
                    "delivery_mode": { "type": ["string", "null"], "enum": ["fresh_session", "existing_session", null], "description": "Optional delivery mode as a quoted JSON string; use \"fresh_session\" or \"existing_session\". Defaults to fresh_session. Use existing_session when the result must appear in the current chat/session" },
                    "target_session_id": { "type": ["string", "null"], "description": "Optional target session id; for existing_session defaults to the current session" },
                    "interval_seconds": { "type": "integer", "minimum": 1, "description": "Positive schedule cadence in seconds; this is not the preferred field for a simple user reminder" },
                    "enabled": { "type": ["boolean", "null"], "description": "Optional enabled state; defaults to true" }
                },
                "required": ["id", "prompt", "interval_seconds"],
                "additionalProperties": false,
            }),
            Self::ScheduleUpdate => json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Schedule id to update" },
                    "agent_identifier": { "type": ["string", "null"], "description": "Optional agent id or name override" },
                    "prompt": { "type": ["string", "null"], "description": "Optional replacement prompt" },
                    "mode": { "type": ["string", "null"], "enum": ["interval", "after_completion", "once", null], "description": "Optional replacement mode as a quoted JSON string; use \"interval\", \"after_completion\", or \"once\"" },
                    "delivery_mode": { "type": ["string", "null"], "enum": ["fresh_session", "existing_session", null], "description": "Optional replacement delivery mode as a quoted JSON string; use \"fresh_session\" or \"existing_session\"" },
                    "target_session_id": { "type": ["string", "null"], "description": "Optional replacement target session id" },
                    "interval_seconds": { "type": ["integer", "null"], "minimum": 1, "description": "Optional replacement interval in seconds" },
                    "enabled": { "type": ["boolean", "null"], "description": "Optional replacement enabled state" }
                },
                "required": ["id"],
                "additionalProperties": false,
            }),
            Self::ScheduleDelete => json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Schedule id to delete" }
                },
                "required": ["id"],
                "additionalProperties": false,
            }),
            Self::MessageAgent => json!({
                "type": "object",
                "properties": {
                    "target_agent_id": { "type": "string", "description": "Global agent profile id to message" },
                    "message": { "type": "string", "description": "The user-like message to deliver into a fresh recipient session; this tool only queues the work and does not wait for the reply" }
                },
                "required": ["target_agent_id", "message"],
                "additionalProperties": false,
            }),
            Self::GrantAgentChainContinuation => json!({
                "type": "object",
                "properties": {
                    "chain_id": { "type": "string", "description": "Blocked inter-agent chain id to extend once after it hit max_hops" },
                    "reason": { "type": "string", "description": "Why one more hop should be allowed" }
                },
                "required": ["chain_id", "reason"],
                "additionalProperties": false,
            }),
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidExec { reason } => write!(formatter, "invalid exec request: {reason}"),
            Self::InvalidPatch { path, reason } => {
                write!(formatter, "invalid patch for {path}: {reason}")
            }
            Self::InvalidWebRequest { reason } => {
                write!(formatter, "invalid web request: {reason}")
            }
            Self::WebHttp(source) => write!(formatter, "web http error: {source}"),
            Self::WebHttpStatus { url, status_code } => {
                write!(
                    formatter,
                    "web request to {url} failed with status {status_code}"
                )
            }
            Self::WebParse { url, reason } => {
                write!(
                    formatter,
                    "failed to parse web response from {url}: {reason}"
                )
            }
            Self::ProcessFamilyMismatch {
                process_id,
                expected,
                actual,
            } => write!(
                formatter,
                "process {process_id} belongs to {:?}, not {:?}",
                actual, expected
            ),
            Self::ProcessIo { process_id, source } => {
                write!(formatter, "process io error for {process_id}: {source}")
            }
            Self::InvalidPlanWrite { reason } => {
                write!(formatter, "invalid plan write request: {reason}")
            }
            Self::InvalidMemoryTool { reason } => {
                write!(formatter, "invalid memory request: {reason}")
            }
            Self::InvalidMcpTool { reason } => {
                write!(formatter, "invalid MCP request: {reason}")
            }
            Self::InvalidAgentTool { reason } => {
                write!(formatter, "invalid agent tool request: {reason}")
            }
            Self::InvalidArtifactTool { reason } => {
                write!(formatter, "invalid offload retrieval request: {reason}")
            }
            Self::UnknownProcess { process_id } => {
                write!(formatter, "unknown process {process_id}")
            }
            Self::Workspace(source) => write!(formatter, "{source}"),
        }
    }
}

impl Error for ToolError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WebHttp(source) => Some(source),
            Self::ProcessIo { source, .. } => Some(source),
            Self::Workspace(source) => Some(source),
            Self::InvalidExec { .. }
            | Self::InvalidPatch { .. }
            | Self::InvalidWebRequest { .. }
            | Self::WebHttpStatus { .. }
            | Self::WebParse { .. }
            | Self::ProcessFamilyMismatch { .. }
            | Self::InvalidPlanWrite { .. }
            | Self::InvalidMemoryTool { .. }
            | Self::InvalidMcpTool { .. }
            | Self::InvalidAgentTool { .. }
            | Self::InvalidArtifactTool { .. }
            | Self::UnknownProcess { .. } => None,
        }
    }
}

impl fmt::Display for ToolCallParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTool { name } => write!(formatter, "unknown tool call {name}"),
            Self::InvalidArguments { name, source } => {
                write!(formatter, "invalid arguments for {name}: {source}")
            }
        }
    }
}

impl Error for ToolCallParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidArguments { source, .. } => Some(source),
            Self::UnknownTool { .. } => None,
        }
    }
}

impl From<WorkspaceError> for ToolError {
    fn from(source: WorkspaceError) -> Self {
        Self::Workspace(source)
    }
}

impl FsWriteMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Overwrite => "overwrite",
            Self::Upsert => "upsert",
        }
    }
}

impl From<FsWriteMode> for WriteMode {
    fn from(value: FsWriteMode) -> Self {
        match value {
            FsWriteMode::Create => WriteMode::Create,
            FsWriteMode::Overwrite => WriteMode::Overwrite,
            FsWriteMode::Upsert => WriteMode::Upsert,
        }
    }
}

fn normalize_tool_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn format_exec_command_display(executable: &str, args: &[String]) -> String {
    std::iter::once(executable.to_string())
        .chain(args.iter().cloned())
        .map(|part| shell_quote_exec_part(part.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote_exec_part(part: &str) -> String {
    if part.is_empty() {
        return "''".to_string();
    }
    if part
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':' | '='))
    {
        return part.to_string();
    }
    format!("'{}'", part.replace('\'', "'\"'\"'"))
}

fn replace_lines_range(
    path: &str,
    content: &str,
    start_line: usize,
    end_line: usize,
    replacement: &str,
) -> Result<String, ToolError> {
    let mut lines = split_preserving_empty(content);
    if start_line == 0 || end_line == 0 || start_line > end_line {
        return Err(ToolError::InvalidPatch {
            path: path.to_string(),
            reason: "line range must be 1-based and inclusive".to_string(),
        });
    }
    if start_line > lines.len() {
        return Err(ToolError::InvalidPatch {
            path: path.to_string(),
            reason: "start_line exceeds file length".to_string(),
        });
    }
    let bounded_end = end_line.min(lines.len());
    let replacement_lines = split_preserving_empty(replacement);
    lines.splice(start_line - 1..bounded_end, replacement_lines);
    Ok(join_lines(lines))
}

fn insert_text(
    path: &str,
    content: &str,
    line: Option<usize>,
    position: &str,
    inserted: &str,
) -> Result<String, ToolError> {
    let mut lines = split_preserving_empty(content);
    let insert_lines = split_preserving_empty(inserted);
    let index = match position {
        "prepend" => 0,
        "append" => lines.len(),
        "before" => {
            let line = line.ok_or_else(|| ToolError::InvalidPatch {
                path: path.to_string(),
                reason: "line is required for before insertion".to_string(),
            })?;
            if line == 0 || line > lines.len() {
                return Err(ToolError::InvalidPatch {
                    path: path.to_string(),
                    reason: "line exceeds file length".to_string(),
                });
            }
            line - 1
        }
        "after" => {
            let line = line.ok_or_else(|| ToolError::InvalidPatch {
                path: path.to_string(),
                reason: "line is required for after insertion".to_string(),
            })?;
            if line == 0 || line > lines.len() {
                return Err(ToolError::InvalidPatch {
                    path: path.to_string(),
                    reason: "line exceeds file length".to_string(),
                });
            }
            line
        }
        other => {
            return Err(ToolError::InvalidPatch {
                path: path.to_string(),
                reason: format!(
                    "position must be one of before, after, prepend, append, got {other}"
                ),
            });
        }
    };
    lines.splice(index..index, insert_lines);
    Ok(join_lines(lines))
}

fn split_preserving_empty(content: &str) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }
    let normalized = content.replace("\r\n", "\n");
    normalized
        .trim_end_matches('\n')
        .split('\n')
        .map(str::to_string)
        .collect()
}

fn join_lines(lines: Vec<String>) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn normalize_fs_list_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_FS_LIST_LIMIT)
        .clamp(1, MAX_FS_LIST_LIMIT)
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

impl TryFrom<PlanWriteItemInput> for PlanItem {
    type Error = PlanItemStatusParseError;

    fn try_from(value: PlanWriteItemInput) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            content: value.content,
            status: PlanItemStatus::try_from(value.status.as_str())?,
            depends_on: Vec::new(),
            notes: Vec::new(),
            blocked_reason: None,
            parent_task_id: None,
        })
    }
}

impl Default for WebToolClient {
    fn default() -> Self {
        Self {
            client: Client::builder()
                .user_agent("teamd-agent/0.1")
                .build()
                .expect("web tool client"),
            search_backend: WebSearchBackend::default(),
            search_url: "https://duckduckgo.com/html/".to_string(),
        }
    }
}

impl WebToolClient {
    pub fn new(search_backend: WebSearchBackend, search_url: impl Into<String>) -> Self {
        Self {
            client: Client::builder()
                .user_agent("teamd-agent/0.1")
                .build()
                .expect("web tool client"),
            search_backend,
            search_url: search_url.into(),
        }
    }

    pub fn for_tests(_base_url: impl Into<String>, search_url: impl Into<String>) -> Self {
        Self::for_tests_with_search_backend(WebSearchBackend::DuckDuckGoHtml, _base_url, search_url)
    }

    pub fn for_tests_with_search_backend(
        search_backend: WebSearchBackend,
        _base_url: impl Into<String>,
        search_url: impl Into<String>,
    ) -> Self {
        Self {
            client: Client::builder()
                .user_agent("teamd-agent-test/0.1")
                .build()
                .expect("test web tool client"),
            search_backend,
            search_url: search_url.into(),
        }
    }

    fn fetch(&self, url: &str) -> Result<WebFetchOutput, ToolError> {
        let RawWebResponse {
            url,
            status_code,
            content_type,
            body,
        } = self.fetch_raw(url)?;
        let extracted_from_html = is_html_response(content_type.as_deref(), body.as_str());
        let title = extracted_from_html
            .then(|| extract_html_title(body.as_str()))
            .flatten();
        let body = if extracted_from_html {
            render_html_fetch_body(body.as_str())
        } else {
            body
        };

        Ok(WebFetchOutput {
            url,
            status_code,
            content_type,
            title,
            extracted_from_html,
            body,
        })
    }

    fn fetch_raw(&self, url: &str) -> Result<RawWebResponse, ToolError> {
        let response = self.client.get(url).send().map_err(ToolError::WebHttp)?;
        let status_code = response.status().as_u16();
        if !response.status().is_success() {
            return Err(ToolError::WebHttpStatus {
                url: url.to_string(),
                status_code,
            });
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let body = response.text().map_err(ToolError::WebHttp)?;

        Ok(RawWebResponse {
            url: url.to_string(),
            status_code,
            content_type,
            body,
        })
    }

    fn search(&self, query: &str, limit: usize) -> Result<WebSearchOutput, ToolError> {
        if query.trim().is_empty() {
            return Err(ToolError::InvalidWebRequest {
                reason: "query must not be empty".to_string(),
            });
        }

        let mut url = self.search_url()?;
        {
            let mut query_pairs = url.query_pairs_mut();
            query_pairs.append_pair("q", query);
            if self.search_backend == WebSearchBackend::SearxngJson {
                query_pairs.append_pair("format", "json");
            }
        }

        let fetch = self.fetch_raw(url.as_str())?;
        let mut results = match self.search_backend {
            WebSearchBackend::DuckDuckGoHtml => {
                parse_search_results(&fetch.body, fetch.url.as_str())?
            }
            WebSearchBackend::SearxngJson => {
                parse_searxng_json_results(&fetch.body, fetch.url.as_str())?
            }
        };
        if limit > 0 && results.len() > limit {
            results.truncate(limit);
        }

        Ok(WebSearchOutput {
            query: query.to_string(),
            results,
        })
    }

    fn search_url(&self) -> Result<Url, ToolError> {
        Url::parse(&self.search_url).map_err(|_| ToolError::InvalidWebRequest {
            reason: format!("invalid search URL: {}", self.search_url),
        })
    }
}

struct AppliedPatch {
    content: String,
}

fn apply_patch_edits(
    path: &str,
    mut content: String,
    edits: &[FsPatchEdit],
) -> Result<AppliedPatch, ToolError> {
    if edits.is_empty() {
        return Err(ToolError::InvalidPatch {
            path: path.to_string(),
            reason: "at least one edit is required".to_string(),
        });
    }

    for edit in edits {
        if edit.old.is_empty() {
            return Err(ToolError::InvalidPatch {
                path: path.to_string(),
                reason: "edit.old must not be empty".to_string(),
            });
        }

        let occurrences = content.matches(&edit.old).count();
        if occurrences == 0 {
            return Err(ToolError::InvalidPatch {
                path: path.to_string(),
                reason: format!("edit target was not found: {}", edit.old),
            });
        }

        if edit.replace_all {
            content = content.replace(&edit.old, &edit.new);
            continue;
        }

        if occurrences > 1 {
            return Err(ToolError::InvalidPatch {
                path: path.to_string(),
                reason: format!("edit target is ambiguous: {}", edit.old),
            });
        }

        content = content.replacen(&edit.old, &edit.new, 1);
    }

    Ok(AppliedPatch { content })
}

fn glob_matches(pattern: &str, candidate: &str) -> bool {
    let normalized_pattern = normalize_tool_path(pattern);
    let normalized_candidate = normalize_tool_path(candidate);
    let pattern_segments = normalized_pattern.split('/').collect::<Vec<_>>();
    let candidate_segments = normalized_candidate.split('/').collect::<Vec<_>>();
    glob_match_segments(&pattern_segments, &candidate_segments)
}

fn glob_match_segments(pattern: &[&str], candidate: &[&str]) -> bool {
    match pattern.split_first() {
        None => candidate.is_empty(),
        Some((&"**", rest)) => {
            glob_match_segments(rest, candidate)
                || (!candidate.is_empty() && glob_match_segments(pattern, &candidate[1..]))
        }
        Some((segment, rest)) => {
            !candidate.is_empty()
                && glob_match_segment(segment, candidate[0])
                && glob_match_segments(rest, &candidate[1..])
        }
    }
}

fn glob_match_segment(pattern: &str, candidate: &str) -> bool {
    let pattern_chars = pattern.chars().collect::<Vec<_>>();
    let candidate_chars = candidate.chars().collect::<Vec<_>>();
    glob_match_segment_chars(&pattern_chars, &candidate_chars)
}

fn glob_match_segment_chars(pattern: &[char], candidate: &[char]) -> bool {
    match pattern.split_first() {
        None => candidate.is_empty(),
        Some((&'*', rest)) => {
            glob_match_segment_chars(rest, candidate)
                || (!candidate.is_empty() && glob_match_segment_chars(pattern, &candidate[1..]))
        }
        Some((&'?', rest)) => {
            !candidate.is_empty() && glob_match_segment_chars(rest, &candidate[1..])
        }
        Some((&expected, rest)) => {
            !candidate.is_empty()
                && candidate[0] == expected
                && glob_match_segment_chars(rest, &candidate[1..])
        }
    }
}

fn parse_search_results(html: &str, source_url: &str) -> Result<Vec<WebSearchResult>, ToolError> {
    let mut results = Vec::new();
    let mut cursor = html;

    while let Some((_, tag_end, tag)) = find_anchor_tag_with_class(cursor, "result__a") {
        let Some(raw_url) = extract_html_attr(tag, "href") else {
            return Err(ToolError::WebParse {
                url: source_url.to_string(),
                reason: "result href was missing".to_string(),
            });
        };
        let url = normalize_duckduckgo_result_url(&decode_html_entities(raw_url));
        cursor = &cursor[tag_end + 1..];

        let Some(title_end) = cursor.find("</a>") else {
            return Err(ToolError::WebParse {
                url: source_url.to_string(),
                reason: "result title was not terminated".to_string(),
            });
        };
        let title = strip_html_tags(&decode_html_entities(&cursor[..title_end]));
        cursor = &cursor[title_end + 4..];

        let next_result = find_anchor_tag_with_class(cursor, "result__a")
            .map(|(index, _, _)| index)
            .unwrap_or(cursor.len());
        let snippet_region = &cursor[..next_result];
        let snippet = find_anchor_tag_with_class(snippet_region, "result__snippet").and_then(
            |(_, snippet_tag_end, _)| {
                let after_tag = &snippet_region[snippet_tag_end + 1..];
                after_tag.find("</a>").map(|snippet_end| {
                    strip_html_tags(&decode_html_entities(&after_tag[..snippet_end]))
                })
            },
        );

        results.push(WebSearchResult {
            title,
            url,
            snippet,
        });
    }

    Ok(results)
}

#[derive(Debug, Deserialize)]
struct SearxngSearchResponse {
    #[serde(default)]
    results: Vec<SearxngSearchResult>,
}

#[derive(Debug, Deserialize)]
struct SearxngSearchResult {
    title: Option<String>,
    url: Option<String>,
    content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawWebResponse {
    url: String,
    status_code: u16,
    content_type: Option<String>,
    body: String,
}

fn parse_searxng_json_results(
    body: &str,
    source_url: &str,
) -> Result<Vec<WebSearchResult>, ToolError> {
    let response: SearxngSearchResponse =
        serde_json::from_str(body).map_err(|error| ToolError::WebParse {
            url: source_url.to_string(),
            reason: format!("invalid SearXNG JSON response: {error}"),
        })?;

    Ok(response
        .results
        .into_iter()
        .filter_map(|result| {
            let title = result.title?.trim().to_string();
            let url = result.url?.trim().to_string();
            if title.is_empty() || url.is_empty() {
                return None;
            }
            Some(WebSearchResult {
                title,
                url,
                snippet: result
                    .content
                    .map(|content| content.trim().to_string())
                    .filter(|content| !content.is_empty()),
            })
        })
        .collect())
}

fn find_anchor_tag_with_class<'a>(
    haystack: &'a str,
    class_name: &str,
) -> Option<(usize, usize, &'a str)> {
    let mut search_from = 0;
    while let Some(relative_start) = haystack[search_from..].find("<a") {
        let start = search_from + relative_start;
        let relative_end = haystack[start..].find('>')?;
        let end = start + relative_end;
        let tag = &haystack[start..=end];
        if html_tag_has_class(tag, class_name) {
            return Some((start, end, tag));
        }
        search_from = end + 1;
    }
    None
}

fn html_tag_has_class(tag: &str, expected: &str) -> bool {
    extract_html_attr(tag, "class")
        .map(|classes| classes.split_whitespace().any(|class| class == expected))
        .unwrap_or(false)
}

fn extract_html_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    for quote in ['"', '\''] {
        let prefix = format!("{name}={quote}");
        if let Some(start) = tag.find(prefix.as_str()) {
            let value_start = start + prefix.len();
            let value_end = tag[value_start..].find(quote)?;
            return Some(&tag[value_start..value_start + value_end]);
        }
    }
    None
}

fn normalize_duckduckgo_result_url(url: &str) -> String {
    let absolute = if url.starts_with("//") {
        format!("https:{url}")
    } else {
        url.to_string()
    };
    let Ok(parsed) = Url::parse(absolute.as_str()) else {
        return url.to_string();
    };
    let is_duckduckgo_redirect = parsed
        .host_str()
        .is_some_and(|host| host.ends_with("duckduckgo.com"))
        && parsed.path().starts_with("/l/");
    if !is_duckduckgo_redirect {
        return absolute;
    }
    parsed
        .query_pairs()
        .find(|(key, _)| key == "uddg")
        .map(|(_, value)| value.into_owned())
        .unwrap_or(absolute)
}

fn is_html_response(content_type: Option<&str>, body: &str) -> bool {
    if let Some(content_type) = content_type {
        let normalized = content_type.to_ascii_lowercase();
        if normalized.contains("html") || normalized.contains("xhtml") {
            return true;
        }
    }

    let trimmed = body.trim_start().to_ascii_lowercase();
    trimmed.starts_with("<!doctype html")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<head")
        || trimmed.starts_with("<body")
}

fn extract_html_title(input: &str) -> Option<String> {
    extract_html_tag_text(input, "title")
}

fn extract_html_tag_text(input: &str, tag_name: &str) -> Option<String> {
    let lower = input.to_ascii_lowercase();
    let open_pattern = format!("<{tag_name}");
    let close_pattern = format!("</{tag_name}>");
    let start = lower.find(open_pattern.as_str())?;
    let open_end = input[start..].find('>')? + start + 1;
    let close_start = lower[open_end..].find(close_pattern.as_str())? + open_end;
    let text = strip_html_tags(&decode_html_entities(&input[open_end..close_start]));
    (!text.is_empty()).then_some(text)
}

fn render_html_fetch_body(input: &str) -> String {
    match convert_html_to_markdown(input, None) {
        Ok(markdown) => {
            let normalized = normalize_markdown_output(markdown.content.as_deref().unwrap_or(""));
            if normalized.is_empty() {
                fallback_extract_html_text(input)
            } else {
                normalized
            }
        }
        Err(_) => fallback_extract_html_text(input),
    }
}

fn normalize_markdown_output(input: &str) -> String {
    input.replace("\r\n", "\n").trim().to_string()
}

fn fallback_extract_html_text(input: &str) -> String {
    collapse_inline_whitespace(&strip_html_tags(&decode_html_entities(input)))
}

fn collapse_inline_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_html_tags(input: &str) -> String {
    let mut output = String::new();
    let mut inside_tag = false;
    for character in input.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => output.push(character),
            _ => {}
        }
    }
    output.trim().to_string()
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests;
