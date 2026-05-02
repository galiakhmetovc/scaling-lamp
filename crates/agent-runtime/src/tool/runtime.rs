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

use super::*;

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
