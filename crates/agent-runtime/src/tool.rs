use crate::agent::AgentScheduleDeliveryMode;
use crate::plan::{PlanItem, PlanItemStatus, PlanItemStatusParseError};
use crate::workspace::{WorkspaceError, WorkspaceRef, WriteMode};
use serde::de::DeserializeOwned;
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

mod catalog;
mod inputs;
mod names;
mod outputs;
mod parse_repair;
mod schema;
mod web;

pub use catalog::{ToolCatalog, ToolDefinition, ToolPolicy};
pub use inputs::*;
pub use names::{ToolFamily, ToolName};
pub use outputs::*;
use parse_repair::{
    CONTINUE_LATER_ENUM_REPAIRS, EnumLikeFieldRepair, KNOWLEDGE_READ_ENUM_REPAIRS,
    SCHEDULE_ENUM_REPAIRS, SESSION_READ_ENUM_REPAIRS, SESSION_WAIT_ENUM_REPAIRS,
    repair_bare_enum_like_values,
};
#[cfg(test)]
use web::parse_search_results;
pub use web::{WebSearchBackend, WebToolClient};

const DEFAULT_FS_LIST_LIMIT: usize = 200;
const MAX_FS_LIST_LIMIT: usize = 1_000;
const DEFAULT_PROCESS_OUTPUT_READ_MAX_BYTES: usize = 4 * 1024;
const MAX_PROCESS_OUTPUT_READ_MAX_BYTES: usize = 32 * 1024;
const DEFAULT_PROCESS_OUTPUT_READ_MAX_LINES: usize = 20;
const MAX_PROCESS_OUTPUT_READ_MAX_LINES: usize = 200;
const PROCESS_READER_DRAIN_GRACE: Duration = Duration::from_millis(200);

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
            processes,
        }
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
            Self::DeliverFile(_) => ToolName::DeliverFile,
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
            | Self::DeliverFile(_)
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
            Self::DeliverFile(input) => {
                if let Some(artifact_id) = &input.artifact_id {
                    format!("deliver_file artifact_id={artifact_id}")
                } else {
                    format!(
                        "deliver_file workspace_path={}",
                        input.workspace_path.as_deref().unwrap_or("<missing>")
                    )
                }
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
            "deliver_file" => serde_json::from_str(arguments)
                .map(Self::DeliverFile)
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

impl ProcessOutputStream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merged => "merged",
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
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

#[cfg(test)]
mod tests;
