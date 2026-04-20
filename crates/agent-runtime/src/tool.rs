use crate::plan::{PlanItem, PlanItemStatus, PlanItemStatusParseError};
use crate::workspace::{WorkspaceEntry, WorkspaceError, WorkspaceRef, WorkspaceSearchMatch};
use reqwest::Url;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolFamily {
    Filesystem,
    Web,
    Exec,
    Planning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolName {
    FsRead,
    FsWrite,
    FsPatch,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCall {
    FsRead(FsReadInput),
    FsWrite(FsWriteInput),
    FsPatch(FsPatchInput),
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsReadOutput {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsWriteOutput {
    pub path: String,
    pub bytes_written: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsPatchOutput {
    pub path: String,
    pub bytes_written: usize,
    pub edits_applied: usize,
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
    pub items: Vec<PlanItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanWriteOutput {
    pub items: Vec<PlanItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolOutput {
    FsRead(FsReadOutput),
    FsWrite(FsWriteOutput),
    FsPatch(FsPatchOutput),
    FsList(FsListOutput),
    FsGlob(FsGlobOutput),
    FsSearch(FsSearchOutput),
    WebFetch(WebFetchOutput),
    WebSearch(WebSearchOutput),
    ProcessStart(ProcessStartOutput),
    ProcessResult(ProcessResult),
    PlanRead(PlanReadOutput),
    PlanWrite(PlanWriteOutput),
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
    next_process_id: usize,
    processes: BTreeMap<String, ManagedProcess>,
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

impl ToolFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Filesystem => "fs",
            Self::Web => "web",
            Self::Exec => "exec",
            Self::Planning => "plan",
        }
    }
}

impl ToolName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FsRead => "fs_read",
            Self::FsWrite => "fs_write",
            Self::FsPatch => "fs_patch",
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
                    ToolName::FsRead
                        | ToolName::FsList
                        | ToolName::FsGlob
                        | ToolName::FsSearch
                        | ToolName::WebFetch
                        | ToolName::WebSearch
                        | ToolName::PlanRead
                        | ToolName::PlanWrite
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
        ]
    }
}

impl Default for ToolCatalog {
    fn default() -> Self {
        Self {
            families: vec!["fs", "web", "exec", "plan"],
            definitions: Self::definitions(),
        }
    }
}

impl ToolRuntime {
    pub fn new(workspace: WorkspaceRef) -> Self {
        Self::with_web_client(workspace, WebToolClient::default())
    }

    pub fn with_web_client(workspace: WorkspaceRef, web: WebToolClient) -> Self {
        Self {
            workspace,
            web,
            next_process_id: 1,
            processes: BTreeMap::new(),
        }
    }

    pub fn invoke(&mut self, call: ToolCall) -> Result<ToolOutput, ToolError> {
        match call {
            ToolCall::FsRead(input) => Ok(ToolOutput::FsRead(FsReadOutput {
                path: normalize_tool_path(&input.path),
                content: self.workspace.read_text(&input.path)?,
            })),
            ToolCall::FsWrite(input) => Ok(ToolOutput::FsWrite(FsWriteOutput {
                path: normalize_tool_path(&input.path),
                bytes_written: self.workspace.write_text(&input.path, &input.content)?,
            })),
            ToolCall::FsPatch(input) => {
                let path = normalize_tool_path(&input.path);
                let content = self.workspace.read_text(&input.path)?;
                let patched = apply_patch_edits(&path, content, &input.edits)?;
                let bytes_written = self.workspace.write_text(&input.path, &patched.content)?;
                Ok(ToolOutput::FsPatch(FsPatchOutput {
                    path,
                    bytes_written,
                    edits_applied: input.edits.len(),
                }))
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
            ToolCall::PlanRead(_) | ToolCall::PlanWrite(_) => Err(ToolError::InvalidPlanWrite {
                reason: "planning tools must execute through the canonical session path"
                    .to_string(),
            }),
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
        let process_id = format!("{}-{}", kind.as_prefix(), self.next_process_id);
        self.next_process_id += 1;

        let pid_ref = format!("pid:{}", child.id());
        self.processes
            .insert(process_id.clone(), ManagedProcess { kind, child });

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
        let managed =
            self.processes
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

impl ToolCall {
    pub fn name(&self) -> ToolName {
        match self {
            Self::FsRead(_) => ToolName::FsRead,
            Self::FsWrite(_) => ToolName::FsWrite,
            Self::FsPatch(_) => ToolName::FsPatch,
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
        }
    }

    pub fn scope_target(&self) -> Option<String> {
        match self {
            Self::FsRead(input) => Some(normalize_tool_path(&input.path)),
            Self::FsWrite(input) => Some(normalize_tool_path(&input.path)),
            Self::FsPatch(input) => Some(normalize_tool_path(&input.path)),
            Self::FsList(input) => Some(normalize_tool_path(&input.path)),
            Self::FsGlob(input) => Some(normalize_tool_path(&input.path)),
            Self::FsSearch(input) => Some(normalize_tool_path(&input.path)),
            Self::WebFetch(input) => Some(input.url.clone()),
            Self::WebSearch(_) => None,
            Self::ExecStart(input) => input.cwd.clone(),
            Self::ExecWait(_) | Self::ExecKill(_) => None,
            Self::PlanRead(_) | Self::PlanWrite(_) => None,
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
            _ => Err(ToolCallParseError::UnknownTool {
                name: name.to_string(),
            }),
        }
    }
}

impl ProcessKind {
    fn as_prefix(self) -> &'static str {
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

    pub fn summary(&self) -> String {
        match self {
            Self::FsRead(output) => {
                format!(
                    "fs_read path={} bytes={}",
                    output.path,
                    output.content.len()
                )
            }
            Self::FsWrite(output) => {
                format!(
                    "fs_write path={} bytes={}",
                    output.path, output.bytes_written
                )
            }
            Self::FsPatch(output) => {
                format!(
                    "fs_patch path={} edits={}",
                    output.path, output.edits_applied
                )
            }
            Self::FsList(output) => format!("fs_list entries={}", output.entries.len()),
            Self::FsGlob(output) => format!("fs_glob entries={}", output.entries.len()),
            Self::FsSearch(output) => format!("fs_search matches={}", output.matches.len()),
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
            Self::FsWrite(output) => json!({
                "tool": "fs_write",
                "path": output.path,
                "bytes_written": output.bytes_written,
            })
            .to_string(),
            Self::FsPatch(output) => json!({
                "tool": "fs_patch",
                "path": output.path,
                "bytes_written": output.bytes_written,
                "edits_applied": output.edits_applied,
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
                "items": output.items.iter().map(plan_item_json).collect::<Vec<_>>(),
            })
            .to_string(),
            Self::PlanWrite(output) => json!({
                "tool": "plan_write",
                "items": output.items.iter().map(plan_item_json).collect::<Vec<_>>(),
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
            Self::FsWrite => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative workspace path to write" },
                    "content": { "type": "string", "description": "UTF-8 file content to write" }
                },
                "required": ["path", "content"],
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

fn normalize_tool_path(path: &str) -> String {
    path.replace('\\', "/")
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
    })
}

impl TryFrom<PlanWriteItemInput> for PlanItem {
    type Error = PlanItemStatusParseError;

    fn try_from(value: PlanWriteItemInput) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            content: value.content,
            status: PlanItemStatus::try_from(value.status.as_str())?,
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
mod tests {
    use super::{
        ExecStartInput, FsGlobInput, FsListInput, FsPatchEdit, FsPatchInput, FsReadInput,
        FsSearchInput, FsWriteInput, ProcessKillInput, ProcessResultStatus, ProcessWaitInput,
        ToolCall, ToolCatalog, ToolFamily, ToolName, ToolRuntime, WebFetchInput, WebSearchInput,
        WebToolClient,
    };
    use crate::workspace::WorkspaceRef;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn catalog_exposes_distinct_families_and_policy_flags() {
        let catalog = ToolCatalog::default();
        let exec_start = catalog.definition(ToolName::ExecStart).expect("exec_start");
        let fs_glob = catalog.definition(ToolName::FsGlob).expect("fs_glob");
        let fs_patch = catalog.definition(ToolName::FsPatch).expect("fs_patch");
        let plan_read = catalog.definition(ToolName::PlanRead).expect("plan_read");
        let plan_write = catalog.definition(ToolName::PlanWrite).expect("plan_write");
        let web_fetch = catalog.definition(ToolName::WebFetch).expect("web_fetch");
        let web_search = catalog.definition(ToolName::WebSearch).expect("web_search");
        let fs_read = catalog.definition(ToolName::FsRead).expect("fs_read");
        let fs_write = catalog.definition(ToolName::FsWrite).expect("fs_write");

        assert_eq!(catalog.families, ["fs", "web", "exec", "plan"]);
        assert_eq!(exec_start.family, ToolFamily::Exec);
        assert_eq!(fs_glob.family, ToolFamily::Filesystem);
        assert_eq!(fs_patch.family, ToolFamily::Filesystem);
        assert_eq!(plan_read.family, ToolFamily::Planning);
        assert_eq!(plan_write.family, ToolFamily::Planning);
        assert_eq!(web_fetch.family, ToolFamily::Web);
        assert_eq!(web_search.family, ToolFamily::Web);
        assert!(exec_start.policy.requires_approval);
        assert!(fs_glob.policy.read_only);
        assert!(fs_patch.policy.destructive);
        assert!(plan_read.policy.read_only);
        assert!(!plan_write.policy.read_only);
        assert!(!plan_write.policy.requires_approval);
        assert!(web_fetch.policy.read_only);
        assert!(web_search.policy.read_only);
        assert!(fs_read.policy.read_only);
        assert!(fs_write.policy.destructive);
    }

    #[test]
    fn filesystem_tools_read_write_list_and_search_within_workspace() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = WorkspaceRef::new(temp.path());
        let mut runtime = ToolRuntime::new(workspace.clone());

        runtime
            .invoke(ToolCall::FsWrite(FsWriteInput {
                path: "docs/notes.txt".to_string(),
                content: "alpha\nbeta\n".to_string(),
            }))
            .expect("fs_write");
        runtime
            .invoke(ToolCall::FsWrite(FsWriteInput {
                path: "docs/summary.txt".to_string(),
                content: "beta\ngamma\n".to_string(),
            }))
            .expect("fs_write summary");

        let read = runtime
            .invoke(ToolCall::FsRead(FsReadInput {
                path: "docs/notes.txt".to_string(),
            }))
            .expect("fs_read")
            .into_fs_read()
            .expect("fs_read output");
        let list = runtime
            .invoke(ToolCall::FsList(FsListInput {
                path: "docs".to_string(),
                recursive: true,
            }))
            .expect("fs_list")
            .into_fs_list()
            .expect("fs_list output");
        let search = runtime
            .invoke(ToolCall::FsSearch(FsSearchInput {
                path: "docs".to_string(),
                query: "beta".to_string(),
            }))
            .expect("fs_search")
            .into_fs_search()
            .expect("fs_search output");

        assert_eq!(read.path, "docs/notes.txt");
        assert_eq!(read.content, "alpha\nbeta\n");
        assert_eq!(
            list.entries
                .iter()
                .filter(|entry| entry.kind == crate::workspace::WorkspaceEntryKind::File)
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            vec!["docs/notes.txt", "docs/summary.txt"]
        );
        assert_eq!(search.matches.len(), 2);
        assert_eq!(search.matches[0].path, "docs/notes.txt");
        assert_eq!(search.matches[1].path, "docs/summary.txt");
    }

    #[test]
    fn filesystem_tools_reject_paths_that_escape_workspace() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = WorkspaceRef::new(temp.path());
        let mut runtime = ToolRuntime::new(workspace);

        assert!(
            runtime
                .invoke(ToolCall::FsRead(FsReadInput {
                    path: "../secret.txt".to_string(),
                }))
                .is_err()
        );
    }

    #[test]
    fn filesystem_tools_glob_and_patch_files_with_exact_edits() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = WorkspaceRef::new(temp.path());
        let mut runtime = ToolRuntime::new(workspace.clone());

        runtime
            .invoke(ToolCall::FsWrite(FsWriteInput {
                path: "src/main.rs".to_string(),
                content: "fn main() {\n    println!(\"old\");\n}\n".to_string(),
            }))
            .expect("fs_write main");
        runtime
            .invoke(ToolCall::FsWrite(FsWriteInput {
                path: "src/lib.rs".to_string(),
                content: "pub fn helper() {}\n".to_string(),
            }))
            .expect("fs_write lib");

        let globbed = runtime
            .invoke(ToolCall::FsGlob(FsGlobInput {
                path: "src".to_string(),
                pattern: "**/*.rs".to_string(),
            }))
            .expect("fs_glob")
            .into_fs_glob()
            .expect("fs_glob output");
        let patched = runtime
            .invoke(ToolCall::FsPatch(FsPatchInput {
                path: "src/main.rs".to_string(),
                edits: vec![FsPatchEdit {
                    old: "println!(\"old\");".to_string(),
                    new: "println!(\"new\");".to_string(),
                    replace_all: false,
                }],
            }))
            .expect("fs_patch");
        let read = runtime
            .invoke(ToolCall::FsRead(FsReadInput {
                path: "src/main.rs".to_string(),
            }))
            .expect("fs_read patched")
            .into_fs_read()
            .expect("fs_read output");

        assert_eq!(
            globbed
                .entries
                .iter()
                .filter(|entry| entry.kind == crate::workspace::WorkspaceEntryKind::File)
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            vec!["src/lib.rs", "src/main.rs"]
        );
        assert_eq!(patched.summary(), "fs_patch path=src/main.rs edits=1");
        assert!(read.content.contains("println!(\"new\");"));
    }

    #[test]
    fn fs_patch_rejects_ambiguous_single_replace_edits() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = WorkspaceRef::new(temp.path());
        let mut runtime = ToolRuntime::new(workspace);

        runtime
            .invoke(ToolCall::FsWrite(FsWriteInput {
                path: "docs/repeated.txt".to_string(),
                content: "same\nsame\n".to_string(),
            }))
            .expect("fs_write repeated");

        assert!(
            runtime
                .invoke(ToolCall::FsPatch(FsPatchInput {
                    path: "docs/repeated.txt".to_string(),
                    edits: vec![FsPatchEdit {
                        old: "same".to_string(),
                        new: "updated".to_string(),
                        replace_all: false,
                    }],
                }))
                .is_err()
        );
    }

    #[test]
    fn structured_exec_treats_shell_tokens_as_literal_args() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = WorkspaceRef::new(temp.path());
        let mut runtime = ToolRuntime::new(workspace);

        let started = runtime
            .invoke(ToolCall::ExecStart(ExecStartInput {
                executable: "/bin/echo".to_string(),
                args: vec!["left|right".to_string()],
                cwd: None,
            }))
            .expect("exec_start")
            .into_process_start()
            .expect("process start");
        let waited = runtime
            .invoke(ToolCall::ExecWait(ProcessWaitInput {
                process_id: started.process_id.clone(),
            }))
            .expect("exec_wait")
            .into_process_result()
            .expect("process result");

        assert_eq!(waited.status, ProcessResultStatus::Exited);
        assert_eq!(waited.exit_code, Some(0));
        assert_eq!(waited.stdout, "left|right\n");
    }

    #[test]
    fn exec_kill_terminates_structured_processes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = WorkspaceRef::new(temp.path());
        let mut runtime = ToolRuntime::new(workspace);

        let exec_started = runtime
            .invoke(ToolCall::ExecStart(ExecStartInput {
                executable: "/bin/sleep".to_string(),
                args: vec!["5".to_string()],
                cwd: None,
            }))
            .expect("exec_start sleep")
            .into_process_start()
            .expect("sleep start");
        let killed = runtime
            .invoke(ToolCall::ExecKill(ProcessKillInput {
                process_id: exec_started.process_id,
            }))
            .expect("exec_kill")
            .into_process_result()
            .expect("killed process result");

        assert_eq!(killed.status, ProcessResultStatus::Killed);
    }

    #[test]
    fn web_tools_fetch_pages_and_return_search_results() {
        let server = TestHttpServer::spawn();
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = WorkspaceRef::new(temp.path());
        let mut runtime = ToolRuntime::with_web_client(
            workspace,
            WebToolClient::for_tests(server.base_url(), server.search_url()),
        );

        let fetched = runtime
            .invoke(ToolCall::WebFetch(WebFetchInput {
                url: server.page_url(),
            }))
            .expect("web_fetch")
            .into_web_fetch()
            .expect("web_fetch output");
        let searched = runtime
            .invoke(ToolCall::WebSearch(WebSearchInput {
                query: "agent runtime".to_string(),
                limit: 5,
            }))
            .expect("web_search")
            .into_web_search()
            .expect("web_search output");

        assert_eq!(fetched.url, server.page_url());
        assert_eq!(fetched.status_code, 200);
        assert!(fetched.body.contains("Agent runtime page"));
        assert_eq!(searched.results.len(), 2);
        assert_eq!(searched.results[0].title, "Agent runtime docs");
        assert_eq!(searched.results[0].url, "https://example.test/docs");
    }

    struct TestHttpServer {
        base_url: String,
        search_url: String,
    }

    impl TestHttpServer {
        fn spawn() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let address = listener.local_addr().expect("local addr");
            let base_url = format!("http://{}", address);
            let search_url = format!("{}/search", base_url);

            thread::spawn(move || {
                for _ in 0..2 {
                    let (mut stream, _) = listener.accept().expect("accept");
                    let mut buffer = [0_u8; 4096];
                    let bytes = stream.read(&mut buffer).expect("read request");
                    let request = String::from_utf8_lossy(&buffer[..bytes]);
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/");

                    let body = if path.starts_with("/search") {
                        "<html><body>\
                         <a class=\"result__a\" href=\"https://example.test/docs\">Agent runtime docs</a>\
                         <a class=\"result__snippet\">Typed tools and run engine</a>\
                         <a class=\"result__a\" href=\"https://example.test/blog\">Blog post</a>\
                         <a class=\"result__snippet\">Web tool coverage</a>\
                         </body></html>"
                    } else {
                        "<html><head><title>Agent runtime page</title></head>\
                         <body>Agent runtime page body</body></html>"
                    };

                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .expect("write response");
                }
            });

            Self {
                base_url,
                search_url,
            }
        }

        fn base_url(&self) -> &str {
            &self.base_url
        }

        fn search_url(&self) -> &str {
            &self.search_url
        }

        fn page_url(&self) -> String {
            format!("{}/page", self.base_url)
        }
    }
}
