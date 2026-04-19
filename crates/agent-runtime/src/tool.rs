use crate::workspace::{WorkspaceEntry, WorkspaceError, WorkspaceRef, WorkspaceSearchMatch};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolFamily {
    Filesystem,
    Exec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolName {
    FsRead,
    FsWrite,
    FsList,
    FsSearch,
    ExecStart,
    ExecWait,
    ExecKill,
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
pub struct FsListInput {
    pub path: String,
    pub recursive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsSearchInput {
    pub path: String,
    pub query: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCall {
    FsRead(FsReadInput),
    FsWrite(FsWriteInput),
    FsList(FsListInput),
    FsSearch(FsSearchInput),
    ExecStart(ExecStartInput),
    ExecWait(ProcessWaitInput),
    ExecKill(ProcessKillInput),
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
pub struct FsListOutput {
    pub entries: Vec<WorkspaceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsSearchOutput {
    pub matches: Vec<WorkspaceSearchMatch>,
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
pub enum ToolOutput {
    FsRead(FsReadOutput),
    FsWrite(FsWriteOutput),
    FsList(FsListOutput),
    FsSearch(FsSearchOutput),
    ProcessStart(ProcessStartOutput),
    ProcessResult(ProcessResult),
}

#[derive(Debug)]
pub enum ToolError {
    InvalidExec {
        reason: &'static str,
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
    UnknownProcess {
        process_id: String,
    },
    Workspace(WorkspaceError),
}

#[derive(Debug)]
pub struct ToolRuntime {
    workspace: WorkspaceRef,
    next_process_id: usize,
    processes: BTreeMap<String, ManagedProcess>,
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
            Self::Exec => "exec",
        }
    }
}

impl ToolName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FsRead => "fs_read",
            Self::FsWrite => "fs_write",
            Self::FsList => "fs_list",
            Self::FsSearch => "fs_search",
            Self::ExecStart => "exec_start",
            Self::ExecWait => "exec_wait",
            Self::ExecKill => "exec_kill",
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
        ]
    }
}

impl Default for ToolCatalog {
    fn default() -> Self {
        Self {
            families: vec!["fs", "exec"],
            definitions: Self::definitions(),
        }
    }
}

impl ToolRuntime {
    pub fn new(workspace: WorkspaceRef) -> Self {
        Self {
            workspace,
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
            ToolCall::FsList(input) => Ok(ToolOutput::FsList(FsListOutput {
                entries: self.workspace.list(&input.path, input.recursive)?,
            })),
            ToolCall::FsSearch(input) => Ok(ToolOutput::FsSearch(FsSearchOutput {
                matches: self.workspace.search(&input.path, &input.query)?,
            })),
            ToolCall::ExecStart(input) => {
                let cwd = self.resolve_cwd(input.cwd.as_deref())?;
                self.start_process(ProcessKind::Exec, &input.executable, &input.args, cwd)
            }
            ToolCall::ExecWait(input) => self.wait_process(&input.process_id, ProcessKind::Exec),
            ToolCall::ExecKill(input) => self.kill_process(&input.process_id, ProcessKind::Exec),
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
            Self::FsList(_) => ToolName::FsList,
            Self::FsSearch(_) => ToolName::FsSearch,
            Self::ExecStart(_) => ToolName::ExecStart,
            Self::ExecWait(_) => ToolName::ExecWait,
            Self::ExecKill(_) => ToolName::ExecKill,
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
            Self::FsList(input) => format!(
                "fs_list path={} recursive={}",
                normalize_tool_path(&input.path),
                input.recursive
            ),
            Self::FsSearch(input) => format!(
                "fs_search path={} query={}",
                normalize_tool_path(&input.path),
                input.query
            ),
            Self::ExecStart(input) => {
                format!(
                    "exec_start executable={} argc={}",
                    input.executable,
                    input.args.len()
                )
            }
            Self::ExecWait(input) => format!("exec_wait process_id={}", input.process_id),
            Self::ExecKill(input) => format!("exec_kill process_id={}", input.process_id),
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

    pub fn into_fs_search(self) -> Option<FsSearchOutput> {
        match self {
            Self::FsSearch(output) => Some(output),
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
            Self::FsList(output) => format!("fs_list entries={}", output.entries.len()),
            Self::FsSearch(output) => format!("fs_search matches={}", output.matches.len()),
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
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidExec { reason } => write!(formatter, "invalid exec request: {reason}"),
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
            Self::ProcessIo { source, .. } => Some(source),
            Self::Workspace(source) => Some(source),
            Self::InvalidExec { .. }
            | Self::ProcessFamilyMismatch { .. }
            | Self::UnknownProcess { .. } => None,
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

#[cfg(test)]
mod tests {
    use super::{
        ExecStartInput, FsListInput, FsReadInput, FsSearchInput, FsWriteInput, ProcessKillInput,
        ProcessResultStatus, ProcessWaitInput, ToolCall, ToolCatalog, ToolFamily, ToolName,
        ToolRuntime,
    };
    use crate::workspace::WorkspaceRef;

    #[test]
    fn catalog_exposes_distinct_families_and_policy_flags() {
        let catalog = ToolCatalog::default();
        let exec_start = catalog.definition(ToolName::ExecStart).expect("exec_start");
        let fs_read = catalog.definition(ToolName::FsRead).expect("fs_read");
        let fs_write = catalog.definition(ToolName::FsWrite).expect("fs_write");

        assert_eq!(catalog.families, ["fs", "exec"]);
        assert_eq!(exec_start.family, ToolFamily::Exec);
        assert!(exec_start.policy.requires_approval);
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
}
