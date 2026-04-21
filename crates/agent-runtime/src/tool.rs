use crate::plan::{PlanItem, PlanItemStatus, PlanItemStatusParseError, PlanLintIssue};
use crate::workspace::{
    WorkspaceEntry, WorkspaceError, WorkspaceRef, WorkspaceSearchMatch, WriteMode,
};
use reqwest::Url;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolFamily {
    Filesystem,
    Web,
    Exec,
    Planning,
    Offload,
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
    ArtifactRead,
    ArtifactSearch,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsGlobInput {
    pub path: String,
    pub pattern: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactReadInput {
    pub artifact_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactSearchInput {
    pub query: String,
    pub limit: usize,
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
    ArtifactRead(ArtifactReadInput),
    ArtifactSearch(ArtifactSearchInput),
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsGlobOutput {
    pub entries: Vec<WorkspaceEntry>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessResultStatus {
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
pub struct ArtifactSearchOutput {
    pub query: String,
    pub results: Vec<ArtifactSearchResult>,
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
    ArtifactRead(ArtifactReadOutput),
    ArtifactSearch(ArtifactSearchOutput),
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
    search_url: String,
}

#[derive(Debug)]
struct ManagedProcess {
    kind: ProcessKind,
    child: Child,
}

#[derive(Debug, Clone, Default)]
pub struct SharedProcessRegistry {
    inner: Arc<Mutex<ProcessRegistryState>>,
}

#[derive(Debug)]
struct ProcessRegistryState {
    next_process_id: usize,
    processes: BTreeMap<String, ManagedProcess>,
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
            Self::ArtifactRead => "artifact_read",
            Self::ArtifactSearch => "artifact_search",
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
                        | ToolName::ArtifactRead
                        | ToolName::ArtifactSearch
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
                description: "Search for literal text across workspace files, optionally constrained by glob",
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
                description: "Replace one exact text fragment inside a UTF-8 text file",
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
                description: "List files and directories inside the workspace",
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
                description: "Fetch a URL and return its response body",
                policy: ToolPolicy {
                    read_only: true,
                    destructive: false,
                    requires_approval: false,
                },
            },
            ToolDefinition {
                name: ToolName::WebSearch,
                family: ToolFamily::Web,
                description: "Run a search query against the configured web search backend",
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
        ]
    }
}

impl Default for ToolCatalog {
    fn default() -> Self {
        Self {
            families: vec!["fs", "web", "exec", "plan", "offload"],
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
            ToolCall::FsList(input) => Ok(ToolOutput::FsList(FsListOutput {
                entries: self.workspace.list(&input.path, input.recursive)?,
            })),
            ToolCall::FsGlob(input) => {
                let mut entries = self.workspace.list(&input.path, true)?;
                entries.retain(|entry| glob_matches(&input.pattern, &entry.path));
                Ok(ToolOutput::FsGlob(FsGlobOutput { entries }))
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
            | ToolCall::PlanLint(_) => Err(ToolError::InvalidPlanWrite {
                reason: "planning tools must execute through the canonical session path"
                    .to_string(),
            }),
            ToolCall::ArtifactRead(_) | ToolCall::ArtifactSearch(_) => {
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
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = command.spawn().map_err(|source| ToolError::ProcessIo {
            process_id: executable.to_string(),
            source,
        })?;
        let pid_ref = format!("pid:{}", child.id());

        let process_id = {
            let mut registry = self.processes.lock();
            let process_id = format!("{}-{}", kind.as_prefix(), registry.next_process_id);
            registry.next_process_id += 1;
            registry
                .processes
                .insert(process_id.clone(), ManagedProcess { kind, child });
            process_id
        };

        Ok(ToolOutput::ProcessStart(ProcessStartOutput {
            process_id,
            pid_ref,
            kind,
        }))
    }

    fn wait_process(
        &mut self,
        process_id: &str,
        expected_kind: ProcessKind,
    ) -> Result<ToolOutput, ToolError> {
        let managed = self.take_process(process_id, expected_kind)?;
        let output = managed
            .child
            .wait_with_output()
            .map_err(|source| ToolError::ProcessIo {
                process_id: process_id.to_string(),
                source,
            })?;

        Ok(ToolOutput::ProcessResult(ProcessResult {
            process_id: process_id.to_string(),
            status: ProcessResultStatus::Exited,
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }))
    }

    fn kill_process(
        &mut self,
        process_id: &str,
        expected_kind: ProcessKind,
    ) -> Result<ToolOutput, ToolError> {
        let mut managed = self.take_process(process_id, expected_kind)?;
        managed
            .child
            .kill()
            .map_err(|source| ToolError::ProcessIo {
                process_id: process_id.to_string(),
                source,
            })?;
        let output = managed
            .child
            .wait_with_output()
            .map_err(|source| ToolError::ProcessIo {
                process_id: process_id.to_string(),
                source,
            })?;

        Ok(ToolOutput::ProcessResult(ProcessResult {
            process_id: process_id.to_string(),
            status: ProcessResultStatus::Killed,
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }))
    }

    fn take_process(
        &mut self,
        process_id: &str,
        expected_kind: ProcessKind,
    ) -> Result<ManagedProcess, ToolError> {
        let managed = self
            .processes
            .lock()
            .processes
            .remove(process_id)
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
}

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
}

impl ToolCall {
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
            Self::ArtifactRead(_) => ToolName::ArtifactRead,
            Self::ArtifactSearch(_) => ToolName::ArtifactSearch,
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
            Self::ExecWait(_) | Self::ExecKill(_) => None,
            Self::PlanRead(_)
            | Self::PlanWrite(_)
            | Self::InitPlan(_)
            | Self::AddTask(_)
            | Self::SetTaskStatus(_)
            | Self::AddTaskNote(_)
            | Self::EditTask(_)
            | Self::PlanSnapshot(_)
            | Self::PlanLint(_) => None,
            Self::ArtifactRead(input) => Some(input.artifact_id.clone()),
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
                    "exec_start executable={} argc={}",
                    input.executable,
                    input.args.len()
                )
            }
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
            Self::ArtifactRead(input) => format!("artifact_read artifact_id={}", input.artifact_id),
            Self::ArtifactSearch(input) => {
                format!(
                    "artifact_search query={} limit={}",
                    input.query, input.limit
                )
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

    pub fn into_process_result(self) -> Option<ProcessResult> {
        match self {
            Self::ProcessResult(output) => Some(output),
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
            Self::FsList(output) => format!("fs_list entries={}", output.entries.len()),
            Self::FsGlob(output) => format!("fs_glob entries={}", output.entries.len()),
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
                "{}_start process_id={} pid_ref={}",
                output.kind.as_str(),
                output.process_id,
                output.pid_ref
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
                "entries": output.entries.iter().map(workspace_entry_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::FsGlob(output) => json!({
                "tool": "fs_glob",
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
                    "search": { "type": "string" },
                    "replace": { "type": "string" }
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
                    "recursive": { "type": "boolean", "description": "Whether to recurse into subdirectories" }
                },
                "required": ["path", "recursive"],
                "additionalProperties": false,
            }),
            Self::FsGlob => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace root to search from" },
                    "pattern": { "type": "string", "description": "Glob-style path pattern" }
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
                    "url": { "type": "string", "description": "Absolute URL to fetch" }
                },
                "required": ["url"],
                "additionalProperties": false,
            }),
            Self::WebSearch => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query text" },
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
            search_url: "https://duckduckgo.com/html/".to_string(),
        }
    }
}

impl WebToolClient {
    pub fn for_tests(_base_url: impl Into<String>, search_url: impl Into<String>) -> Self {
        Self {
            client: Client::builder()
                .user_agent("teamd-agent-test/0.1")
                .build()
                .expect("test web tool client"),
            search_url: search_url.into(),
        }
    }

    fn fetch(&self, url: &str) -> Result<WebFetchOutput, ToolError> {
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

        Ok(WebFetchOutput {
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

        let mut url = Url::parse(&self.search_url).map_err(|_| ToolError::InvalidWebRequest {
            reason: format!("invalid search URL: {}", self.search_url),
        })?;
        url.query_pairs_mut().append_pair("q", query);

        let fetch = self.fetch(url.as_str())?;
        let mut results = parse_search_results(&fetch.body, fetch.url.as_str())?;
        if limit > 0 && results.len() > limit {
            results.truncate(limit);
        }

        Ok(WebSearchOutput {
            query: query.to_string(),
            results,
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
    let link_prefix = "<a class=\"result__a\" href=\"";
    let snippet_prefix = "<a class=\"result__snippet\">";

    while let Some(index) = cursor.find(link_prefix) {
        cursor = &cursor[index + link_prefix.len()..];
        let Some(url_end) = cursor.find('"') else {
            return Err(ToolError::WebParse {
                url: source_url.to_string(),
                reason: "result href was not terminated".to_string(),
            });
        };
        let url = decode_html_entities(&cursor[..url_end]);
        cursor = &cursor[url_end + 1..];

        let Some(tag_close) = cursor.find('>') else {
            return Err(ToolError::WebParse {
                url: source_url.to_string(),
                reason: "result anchor was malformed".to_string(),
            });
        };
        cursor = &cursor[tag_close + 1..];

        let Some(title_end) = cursor.find("</a>") else {
            return Err(ToolError::WebParse {
                url: source_url.to_string(),
                reason: "result title was not terminated".to_string(),
            });
        };
        let title = strip_html_tags(&decode_html_entities(&cursor[..title_end]));
        cursor = &cursor[title_end + 4..];

        let snippet = cursor.find(snippet_prefix).and_then(|snippet_index| {
            let after_prefix = &cursor[snippet_index + snippet_prefix.len()..];
            after_prefix.find("</a>").map(|snippet_end| {
                strip_html_tags(&decode_html_entities(&after_prefix[..snippet_end]))
            })
        });

        results.push(WebSearchResult {
            title,
            url,
            snippet,
        });
    }

    Ok(results)
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
