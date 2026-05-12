use crate::workspace::{WorkspaceError, WorkspaceRef, WriteMode};
use std::collections::BTreeMap;
#[cfg(unix)]
use std::io;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::browser::{
    default_browser_screenshot_path, ensure_browser_output_parent, normalize_browser_workspace_path,
};
use super::*;

#[derive(Debug)]
pub struct ToolRuntime {
    workspace: WorkspaceRef,
    web: WebToolClient,
    browser: BrowserToolClient,
    processes: SharedProcessRegistry,
    limits: ToolRuntimeLimits,
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
            browser: BrowserToolClient::disabled(),
            processes,
            limits: ToolRuntimeLimits::default(),
        }
    }

    pub fn with_clients_and_process_registry(
        workspace: WorkspaceRef,
        web: WebToolClient,
        browser: BrowserToolClient,
        processes: SharedProcessRegistry,
    ) -> Self {
        Self {
            workspace,
            web,
            browser,
            processes,
            limits: ToolRuntimeLimits::default(),
        }
    }

    pub fn with_runtime_limits(mut self, limits: ToolRuntimeLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn read_file_bytes(&self, path: &str) -> Result<(String, Vec<u8>), ToolError> {
        let resolved = self.workspace.resolve(path)?;
        let metadata = std::fs::metadata(&resolved).map_err(|source| WorkspaceError::Io {
            path: resolved.clone(),
            source,
        })?;
        if !metadata.is_file() {
            return Err(ToolError::Workspace(WorkspaceError::InvalidPath {
                path: path.to_string(),
                reason: "must point to a file",
            }));
        }
        let bytes = std::fs::read(&resolved).map_err(|source| WorkspaceError::Io {
            path: resolved.clone(),
            source,
        })?;
        let relative = resolved
            .strip_prefix(&self.workspace.root)
            .unwrap_or(resolved.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        Ok((relative, bytes))
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
                let limit = normalize_fs_list_limit(input.limit, &self.limits);
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
                let limit = normalize_fs_list_limit(input.limit, &self.limits);
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
            ToolCall::BrowserOpen(input) => {
                let mut args = vec!["open".to_string(), input.url.clone()];
                let opened = self.browser.invoke("open", args.clone(), None)?;
                let mut stdout = opened.stdout;
                let mut stderr = opened.stderr;
                if let Some(wait_until) = &input.wait_until {
                    args = vec!["wait".to_string(), "--load".to_string(), wait_until.clone()];
                    let waited = self.browser.invoke("wait", args, None)?;
                    if !waited.stdout.is_empty() {
                        stdout.push_str(&waited.stdout);
                    }
                    stderr.push_str(&waited.stderr);
                }
                Ok(ToolOutput::BrowserOpen(BrowserCommandOutput {
                    action: ToolName::BrowserOpen.as_str().to_string(),
                    session: self.browser.config().session_name.clone(),
                    stdout,
                    stderr,
                    workspace_path: None,
                }))
            }
            ToolCall::BrowserSnapshot(input) => {
                let mut args = vec!["snapshot".to_string()];
                if input.interactive.unwrap_or(true) {
                    args.push("-i".to_string());
                }
                if input.compact.unwrap_or(true) {
                    args.push("-c".to_string());
                }
                if let Some(depth) = input.depth {
                    args.push("-d".to_string());
                    args.push(depth.to_string());
                }
                if let Some(selector) = &input.selector {
                    args.push("-s".to_string());
                    args.push(selector.clone());
                }
                let result =
                    self.browser
                        .invoke("snapshot", args, input.max_chars.map(|value| {
                            value.clamp(1_000, self.browser.config().max_output_chars.max(1_000))
                        }))?;
                Ok(ToolOutput::BrowserSnapshot(BrowserCommandOutput {
                    action: ToolName::BrowserSnapshot.as_str().to_string(),
                    session: self.browser.config().session_name.clone(),
                    stdout: result.stdout,
                    stderr: result.stderr,
                    workspace_path: None,
                }))
            }
            ToolCall::BrowserText(input) => {
                let selector = input.selector.as_deref().unwrap_or("body");
                let args = vec!["get".to_string(), "text".to_string(), selector.to_string()];
                let result =
                    self.browser
                        .invoke("get text", args, input.max_chars.map(|value| {
                            value.clamp(1_000, self.browser.config().max_output_chars.max(1_000))
                        }))?;
                Ok(ToolOutput::BrowserText(BrowserCommandOutput {
                    action: ToolName::BrowserText.as_str().to_string(),
                    session: self.browser.config().session_name.clone(),
                    stdout: result.stdout,
                    stderr: result.stderr,
                    workspace_path: None,
                }))
            }
            ToolCall::BrowserClick(input) => {
                let args = vec!["click".to_string(), input.selector.clone()];
                let mut result = self.browser.invoke("click", args, None)?;
                if let Some(wait_until) = &input.wait_until {
                    let waited = self.browser.invoke(
                        "wait",
                        vec!["wait".to_string(), "--load".to_string(), wait_until.clone()],
                        None,
                    )?;
                    result.stdout.push_str(&waited.stdout);
                    result.stderr.push_str(&waited.stderr);
                }
                Ok(ToolOutput::BrowserClick(BrowserCommandOutput {
                    action: ToolName::BrowserClick.as_str().to_string(),
                    session: self.browser.config().session_name.clone(),
                    stdout: result.stdout,
                    stderr: result.stderr,
                    workspace_path: None,
                }))
            }
            ToolCall::BrowserFill(input) => Ok(ToolOutput::BrowserFill(
                self.browser_command_output(
                    ToolName::BrowserFill,
                    "fill",
                    vec!["fill".to_string(), input.selector.clone(), input.text.clone()],
                    None,
                    None,
                )?,
            )),
            ToolCall::BrowserPress(input) => Ok(ToolOutput::BrowserPress(
                self.browser_command_output(
                    ToolName::BrowserPress,
                    "press",
                    vec!["press".to_string(), input.key.clone()],
                    None,
                    None,
                )?,
            )),
            ToolCall::BrowserWait(input) => {
                let args = browser_wait_args(&input)?;
                Ok(ToolOutput::BrowserWait(self.browser_command_output(
                    ToolName::BrowserWait,
                    "wait",
                    args,
                    None,
                    None,
                )?))
            }
            ToolCall::BrowserScroll(input) => {
                let mut args = vec!["scroll".to_string(), input.direction.clone()];
                if let Some(pixels) = input.pixels {
                    args.push(pixels.to_string());
                }
                Ok(ToolOutput::BrowserScroll(self.browser_command_output(
                    ToolName::BrowserScroll,
                    "scroll",
                    args,
                    None,
                    None,
                )?))
            }
            ToolCall::BrowserEval(input) => Ok(ToolOutput::BrowserEval(
                self.browser_command_output(
                    ToolName::BrowserEval,
                    "eval",
                    vec!["eval".to_string(), input.script.clone()],
                    input.max_chars.map(|value| {
                        value.clamp(1_000, self.browser.config().max_output_chars.max(1_000))
                    }),
                    None,
                )?,
            )),
            ToolCall::BrowserScreenshot(input) => {
                let default_path;
                let selected_path = if let Some(path) = input.path.as_deref() {
                    path
                } else {
                    default_path = default_browser_screenshot_path();
                    default_path.as_str()
                };
                let relative_path = normalize_browser_workspace_path(selected_path);
                let absolute_path = self.workspace.resolve(&relative_path)?;
                ensure_browser_output_parent(&absolute_path)?;
                let mut args = vec![
                    "screenshot".to_string(),
                    absolute_path.display().to_string(),
                ];
                if input.full.unwrap_or(false) {
                    args.push("--full".to_string());
                }
                if input.annotate.unwrap_or(false) {
                    args.push("--annotate".to_string());
                }
                Ok(ToolOutput::BrowserScreenshot(self.browser_command_output(
                    ToolName::BrowserScreenshot,
                    "screenshot",
                    args,
                    None,
                    Some(relative_path),
                )?))
            }
            ToolCall::BrowserPdf(input) => {
                let relative_path = normalize_browser_workspace_path(&input.path);
                let absolute_path = self.workspace.resolve(&relative_path)?;
                ensure_browser_output_parent(&absolute_path)?;
                Ok(ToolOutput::BrowserPdf(self.browser_command_output(
                    ToolName::BrowserPdf,
                    "pdf",
                    vec!["pdf".to_string(), absolute_path.display().to_string()],
                    None,
                    Some(relative_path),
                )?))
            }
            ToolCall::BrowserStatus(_) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                for (action, args) in [
                    ("session", vec!["session".to_string()]),
                    ("get url", vec!["get".to_string(), "url".to_string()]),
                    ("get title", vec!["get".to_string(), "title".to_string()]),
                ] {
                    let result = self.browser.invoke(action, args, None)?;
                    stdout.push_str(&result.stdout);
                    stderr.push_str(&result.stderr);
                }
                Ok(ToolOutput::BrowserStatus(BrowserCommandOutput {
                    action: ToolName::BrowserStatus.as_str().to_string(),
                    session: self.browser.config().session_name.clone(),
                    stdout,
                    stderr,
                    workspace_path: None,
                }))
            }
            ToolCall::BrowserClose(input) => {
                let mut args = vec!["close".to_string()];
                if input.all.unwrap_or(false) {
                    args.push("--all".to_string());
                }
                Ok(ToolOutput::BrowserClose(self.browser_command_output(
                    ToolName::BrowserClose,
                    "close",
                    args,
                    None,
                    None,
                )?))
            }
            ToolCall::ExecStart(input) => {
                let cwd = self.resolve_cwd(input.cwd.as_deref())?;
                self.start_process(ProcessKind::Exec, &input.executable, &input.args, cwd)
            }
            ToolCall::ExecReadOutput(input) => {
                let process_id = input.process_id.clone();
                self.read_process_output(&process_id, ProcessKind::Exec, input)
            }
            ToolCall::ExecWait(input) => self.wait_process(input, ProcessKind::Exec),
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
            | ToolCall::SkillDisable(_)
            | ToolCall::SkillInstall(_) => Err(ToolError::InvalidMemoryTool {
                reason: "autonomy and skill tools must execute through the canonical session path"
                    .to_string(),
            }),
            ToolCall::MemoryAdd(_)
            | ToolCall::MemorySearch(_)
            | ToolCall::MemoryList(_)
            | ToolCall::MemoryUpdate(_)
            | ToolCall::MemoryDelete(_)
            | ToolCall::KvGet(_)
            | ToolCall::KvPut(_)
            | ToolCall::KvList(_)
            | ToolCall::KvDelete(_)
            | ToolCall::KnowledgeSearch(_)
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
            | ToolCall::ArtifactUnpin(_)
            | ToolCall::DeliverFile(_) => {
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

    fn browser_command_output(
        &self,
        tool_name: ToolName,
        action: &str,
        args: Vec<String>,
        max_output_chars: Option<usize>,
        workspace_path: Option<String>,
    ) -> Result<BrowserCommandOutput, ToolError> {
        let result = self.browser.invoke(action, args, max_output_chars)?;
        Ok(BrowserCommandOutput {
            action: tool_name.as_str().to_string(),
            session: self.browser.config().session_name.clone(),
            stdout: result.stdout,
            stderr: result.stderr,
            workspace_path,
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
        let max_bytes = normalize_process_output_max_bytes(input.max_bytes, &self.limits);
        let max_lines = normalize_process_output_max_lines(input.max_lines, &self.limits);
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
        input: ProcessWaitInput,
        expected_kind: ProcessKind,
    ) -> Result<ToolOutput, ToolError> {
        let process_id = input.process_id.as_str();
        let managed = self.lookup_process(process_id, expected_kind)?;
        let timeout = normalize_process_wait_timeout(input.timeout_ms, &self.limits);
        let deadline = Instant::now() + timeout;
        let terminal_status = loop {
            if managed.try_record_exit(process_id)? {
                break ProcessResultStatus::Exited;
            }
            if Instant::now() >= deadline {
                managed.terminate(process_id, ProcessResultStatus::TimedOut, &self.limits)?;
                break ProcessResultStatus::TimedOut;
            }
            thread::sleep(
                self.limits
                    .process_wait_poll_interval
                    .min(deadline.saturating_duration_since(Instant::now())),
            );
        };

        managed.drain_readers(self.limits.process_reader_drain_grace);
        self.remove_process(process_id);
        let output = managed
            .output
            .lock()
            .expect("managed process output poisoned");

        Ok(ToolOutput::ProcessResult(ProcessResult {
            process_id: process_id.to_string(),
            status: terminal_status,
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
        managed.terminate(process_id, ProcessResultStatus::Killed, &self.limits)?;
        managed.drain_readers(self.limits.process_reader_drain_grace);
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
                    ProcessResultStatus::TimedOut => ProcessOutputStatus::TimedOut,
                });
            }
        }

        if self.try_record_exit(process_id)? {
            return Ok(ProcessOutputStatus::Exited);
        }

        Ok(ProcessOutputStatus::Running)
    }

    fn try_record_exit(&self, process_id: &str) -> Result<bool, ToolError> {
        if let Ok(mut child) = self.child.try_lock()
            && let Some(exit_status) = child.try_wait().map_err(|source| ToolError::ProcessIo {
                process_id: process_id.to_string(),
                source,
            })?
        {
            let mut output = self.output.lock().expect("managed process output poisoned");
            output.finished_status = Some(ProcessResultStatus::Exited);
            output.exit_code = exit_status.code();
            return Ok(true);
        }
        Ok(false)
    }

    fn terminate(
        &self,
        process_id: &str,
        terminal_status: ProcessResultStatus,
        limits: &ToolRuntimeLimits,
    ) -> Result<(), ToolError> {
        let mut child = self.child.lock().expect("managed child poisoned");
        if let Some(exit_status) = child.try_wait().map_err(|source| ToolError::ProcessIo {
            process_id: process_id.to_string(),
            source,
        })? {
            let mut output = self.output.lock().expect("managed process output poisoned");
            output.finished_status = Some(ProcessResultStatus::Exited);
            output.exit_code = exit_status.code();
            return Ok(());
        }

        terminate_child_process_group(&mut child, false).map_err(|source| {
            ToolError::ProcessIo {
                process_id: process_id.to_string(),
                source,
            }
        })?;
        let deadline = Instant::now() + limits.process_terminate_grace;
        loop {
            if let Some(exit_status) = child.try_wait().map_err(|source| ToolError::ProcessIo {
                process_id: process_id.to_string(),
                source,
            })? {
                let mut output = self.output.lock().expect("managed process output poisoned");
                output.finished_status = Some(terminal_status);
                output.exit_code = exit_status.code();
                return Ok(());
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(
                limits
                    .process_wait_poll_interval
                    .min(deadline.saturating_duration_since(Instant::now())),
            );
        }

        terminate_child_process_group(&mut child, true).map_err(|source| ToolError::ProcessIo {
            process_id: process_id.to_string(),
            source,
        })?;
        let exit_status = child.wait().map_err(|source| ToolError::ProcessIo {
            process_id: process_id.to_string(),
            source,
        })?;
        let mut output = self.output.lock().expect("managed process output poisoned");
        output.finished_status = Some(terminal_status);
        output.exit_code = exit_status.code();
        Ok(())
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

fn normalize_process_output_max_bytes(limit: Option<usize>, limits: &ToolRuntimeLimits) -> usize {
    limit
        .unwrap_or(limits.process_output_read_default_max_bytes)
        .clamp(1, limits.process_output_read_max_bytes)
}

fn browser_wait_args(input: &BrowserWaitInput) -> Result<Vec<String>, ToolError> {
    let mut args = vec!["wait".to_string()];
    match input.kind.as_str() {
        "selector" => {
            let value = input
                .value
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| ToolError::InvalidBrowserRequest {
                    reason: "browser_wait kind=selector requires value".to_string(),
                })?;
            args.push(value.to_string());
            if let Some(state) = &input.state {
                args.push("--state".to_string());
                args.push(state.clone());
            }
        }
        "text" => {
            let value = input
                .value
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| ToolError::InvalidBrowserRequest {
                    reason: "browser_wait kind=text requires value".to_string(),
                })?;
            args.push("--text".to_string());
            args.push(value.to_string());
        }
        "url" => {
            let value = input
                .value
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| ToolError::InvalidBrowserRequest {
                    reason: "browser_wait kind=url requires value".to_string(),
                })?;
            args.push("--url".to_string());
            args.push(value.to_string());
        }
        "load" => {
            let value = input.value.as_deref().unwrap_or("networkidle");
            args.push("--load".to_string());
            args.push(value.to_string());
        }
        "function" => {
            let value = input
                .value
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| ToolError::InvalidBrowserRequest {
                    reason: "browser_wait kind=function requires value".to_string(),
                })?;
            args.push("--fn".to_string());
            args.push(value.to_string());
        }
        "duration_ms" => {
            let value = input
                .value
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| ToolError::InvalidBrowserRequest {
                    reason: "browser_wait kind=duration_ms requires value".to_string(),
                })?;
            args.push(value.to_string());
        }
        _ => {
            return Err(ToolError::InvalidBrowserRequest {
                reason:
                    "browser_wait kind must be selector, text, url, load, function, or duration_ms"
                        .to_string(),
            });
        }
    }
    Ok(args)
}

fn normalize_process_output_max_lines(limit: Option<usize>, limits: &ToolRuntimeLimits) -> usize {
    limit
        .unwrap_or(limits.process_output_read_default_max_lines)
        .clamp(1, limits.process_output_read_max_lines)
}

fn normalize_process_wait_timeout(timeout_ms: Option<u64>, limits: &ToolRuntimeLimits) -> Duration {
    timeout_ms
        .map(Duration::from_millis)
        .unwrap_or(limits.process_wait_default_timeout)
        .clamp(Duration::from_millis(1), limits.process_wait_max_timeout)
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

#[cfg(unix)]
fn terminate_child_process_group(child: &mut Child, force: bool) -> io::Result<()> {
    let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
    let pid = child.id() as libc::pid_t;
    let rc = unsafe { libc::kill(-pid, signal) };
    if rc == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(error)
}

#[cfg(not(unix))]
fn terminate_child_process_group(child: &mut Child, _force: bool) -> io::Result<()> {
    child.kill()
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

    pub fn read_exec_output(
        &self,
        process_id: &str,
        stream: ProcessOutputStream,
        cursor: Option<usize>,
        max_bytes: Option<usize>,
        max_lines: Option<usize>,
    ) -> Result<ProcessOutputRead, ToolError> {
        self.read_exec_output_with_limits(
            process_id,
            stream,
            cursor,
            max_bytes,
            max_lines,
            &ToolRuntimeLimits::default(),
        )
    }

    pub fn read_exec_output_with_limits(
        &self,
        process_id: &str,
        stream: ProcessOutputStream,
        cursor: Option<usize>,
        max_bytes: Option<usize>,
        max_lines: Option<usize>,
        limits: &ToolRuntimeLimits,
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
            normalize_process_output_max_bytes(max_bytes, limits),
            normalize_process_output_max_lines(max_lines, limits),
        ))
    }
}
