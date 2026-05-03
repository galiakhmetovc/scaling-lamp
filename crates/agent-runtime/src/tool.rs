use crate::plan::{PlanItem, PlanItemStatus, PlanItemStatusParseError};
use crate::workspace::{WorkspaceError, WriteMode};
use std::error::Error;
use std::fmt;
use std::time::Duration;

mod browser;
mod catalog;
mod inputs;
mod names;
mod outputs;
mod parse;
mod parse_repair;
mod runtime;
mod schema;
mod web;

pub use browser::{BrowserCommandResult, BrowserToolClient, BrowserToolConfig};
pub use catalog::{ToolCatalog, ToolDefinition, ToolPolicy};
pub use inputs::*;
pub use names::{ToolFamily, ToolName};
pub use outputs::*;
pub use parse::ToolCallParseError;
pub use runtime::{SharedProcessRegistry, ToolRuntime};
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
    InvalidBrowserRequest {
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
    BrowserIo {
        command: String,
        source: std::io::Error,
    },
    BrowserFailed {
        command: String,
        status_code: Option<i32>,
        stderr: String,
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
            Self::InvalidBrowserRequest { reason } => {
                write!(formatter, "invalid browser request: {reason}")
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
            Self::BrowserIo { command, source } => {
                write!(formatter, "browser command `{command}` failed: {source}")
            }
            Self::BrowserFailed {
                command,
                status_code,
                stderr,
            } => write!(
                formatter,
                "browser command `{command}` exited with status {:?}: {}",
                status_code, stderr
            ),
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
            Self::BrowserIo { source, .. } => Some(source),
            Self::ProcessIo { source, .. } => Some(source),
            Self::Workspace(source) => Some(source),
            Self::InvalidExec { .. }
            | Self::InvalidPatch { .. }
            | Self::InvalidWebRequest { .. }
            | Self::InvalidBrowserRequest { .. }
            | Self::WebHttpStatus { .. }
            | Self::WebParse { .. }
            | Self::BrowserFailed { .. }
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
