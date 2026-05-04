use crate::agent::{AgentScheduleDeliveryMode, AgentScheduleMode};
use crate::memory::SessionRetentionTier;
use crate::session::PromptBudgetPolicy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

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
pub struct BrowserOpenInput {
    pub url: String,
    #[serde(default)]
    pub wait_until: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserSnapshotInput {
    #[serde(default)]
    pub interactive: Option<bool>,
    #[serde(default)]
    pub compact: Option<bool>,
    #[serde(default)]
    pub depth: Option<usize>,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub max_chars: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserTextInput {
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub max_chars: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserClickInput {
    pub selector: String,
    #[serde(default)]
    pub wait_until: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserFillInput {
    pub selector: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserPressInput {
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserWaitInput {
    pub kind: String,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserScrollInput {
    pub direction: String,
    #[serde(default)]
    pub pixels: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserEvalInput {
    pub script: String,
    #[serde(default)]
    pub max_chars: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserScreenshotInput {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub full: Option<bool>,
    #[serde(default)]
    pub annotate: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserPdfInput {
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserStatusInput {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserCloseInput {
    #[serde(default)]
    pub all: Option<bool>,
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
    #[serde(default)]
    pub timeout_ms: Option<u64>,
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
    pub memory_recall: Option<u8>,
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
        if let Some(value) = self.memory_recall {
            policy.memory_recall = value;
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
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
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
pub enum FileDeliveryTarget {
    CurrentChat,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeliverFileInput {
    pub artifact_id: Option<String>,
    pub workspace_path: Option<String>,
    pub file_name: Option<String>,
    pub caption: Option<String>,
    pub target: Option<FileDeliveryTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryMessageInput {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryAddInput {
    pub text: String,
    pub messages: Vec<MemoryMessageInput>,
    pub scope: Option<String>,
    pub infer: Option<bool>,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemorySearchInput {
    pub query: String,
    pub scope: Option<String>,
    pub limit: Option<usize>,
    pub filters: Value,
}

impl Default for MemorySearchInput {
    fn default() -> Self {
        Self {
            query: String::new(),
            scope: None,
            limit: None,
            filters: Value::Null,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryListInput {
    pub scope: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub filters: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryUpdateInput {
    pub memory_id: String,
    pub text: String,
    pub metadata: Value,
}

impl Default for MemoryUpdateInput {
    fn default() -> Self {
        Self {
            memory_id: String::new(),
            text: String::new(),
            metadata: Value::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDeleteInput {
    pub memory_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KvGetInput {
    pub key: String,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KvPutInput {
    pub key: String,
    pub value: Value,
    pub scope: Option<String>,
    pub metadata: Value,
    pub expected_revision: Option<i64>,
    pub ttl_seconds: Option<i64>,
}

impl Default for KvPutInput {
    fn default() -> Self {
        Self {
            key: String::new(),
            value: Value::Null,
            scope: None,
            metadata: Value::Null,
            expected_revision: None,
            ttl_seconds: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KvListInput {
    pub scope: Option<String>,
    pub prefix: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KvDeleteInput {
    pub key: String,
    pub scope: Option<String>,
    pub expected_revision: Option<i64>,
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
    BrowserOpen(BrowserOpenInput),
    BrowserSnapshot(BrowserSnapshotInput),
    BrowserText(BrowserTextInput),
    BrowserClick(BrowserClickInput),
    BrowserFill(BrowserFillInput),
    BrowserPress(BrowserPressInput),
    BrowserWait(BrowserWaitInput),
    BrowserScroll(BrowserScrollInput),
    BrowserEval(BrowserEvalInput),
    BrowserScreenshot(BrowserScreenshotInput),
    BrowserPdf(BrowserPdfInput),
    BrowserStatus(BrowserStatusInput),
    BrowserClose(BrowserCloseInput),
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
    DeliverFile(DeliverFileInput),
    MemoryAdd(MemoryAddInput),
    MemorySearch(MemorySearchInput),
    MemoryList(MemoryListInput),
    MemoryUpdate(MemoryUpdateInput),
    MemoryDelete(MemoryDeleteInput),
    KvGet(KvGetInput),
    KvPut(KvPutInput),
    KvList(KvListInput),
    KvDelete(KvDeleteInput),
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

impl FileDeliveryTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CurrentChat => "current_chat",
        }
    }
}
