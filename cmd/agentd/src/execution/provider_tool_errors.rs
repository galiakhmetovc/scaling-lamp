use super::ExecutionError;
use agent_runtime::tool::{ProcessKind, SharedProcessRegistry, ToolCall, ToolError};
use agent_runtime::workspace::{WorkspaceError, WorkspaceRef};

pub(super) fn provider_tool_output(
    tool_name: &str,
    reason: &str,
    retryable: bool,
    details: serde_json::Value,
) -> String {
    serde_json::json!({
        "tool": tool_name,
        "error": reason,
        "retryable": retryable,
        "details": details,
    })
    .to_string()
}

pub(super) fn invalid_provider_tool_output(tool_name: &str, reason: &str) -> String {
    serde_json::json!({
        "tool": tool_name,
        "error": format!("invalid tool call: {reason}"),
        "retryable": true,
    })
    .to_string()
}

pub(super) fn retryable_provider_tool_output(
    tool_name: &str,
    reason: &str,
    details: serde_json::Value,
) -> String {
    provider_tool_output(tool_name, reason, true, details)
}

pub(super) fn non_retryable_provider_tool_output(
    tool_name: &str,
    reason: &str,
    details: serde_json::Value,
) -> String {
    provider_tool_output(tool_name, reason, false, details)
}

pub(super) fn recoverable_tool_error_output(
    processes: &SharedProcessRegistry,
    workspace: &WorkspaceRef,
    parsed: &ToolCall,
    error: &ToolError,
) -> Option<String> {
    match error {
        ToolError::UnknownProcess { process_id } => Some(retryable_provider_tool_output(
            parsed.name().as_str(),
            &format!("unknown process {process_id}"),
            serde_json::json!({
                "requested_process_id": process_id,
                "active_process_ids": processes.active_process_ids(Some(ProcessKind::Exec)),
            }),
        )),
        ToolError::ProcessFamilyMismatch {
            process_id,
            expected,
            actual,
        } => Some(retryable_provider_tool_output(
            parsed.name().as_str(),
            &format!(
                "process {process_id} has family mismatch: expected {} but found {}",
                expected.as_prefix(),
                actual.as_prefix()
            ),
            serde_json::json!({
                "requested_process_id": process_id,
                "expected_kind": expected.as_prefix(),
                "actual_kind": actual.as_prefix(),
                "active_process_ids": processes.active_process_ids(None),
            }),
        )),
        ToolError::ProcessIo { process_id, source } => Some(retryable_provider_tool_output(
            parsed.name().as_str(),
            &format!("process io error for {process_id}: {source}"),
            serde_json::json!({
                "process_or_executable": process_id,
                "active_process_ids": processes.active_process_ids(None),
            }),
        )),
        ToolError::Workspace(WorkspaceError::InvalidPath { path, reason }) => {
            Some(retryable_provider_tool_output(
                parsed.name().as_str(),
                &format!("invalid workspace path {path}: {reason}"),
                serde_json::json!({
                    "requested_path": path,
                    "constraint": "workspace_relative_only",
                    "workspace_root": workspace.root.display().to_string(),
                }),
            ))
        }
        ToolError::Workspace(WorkspaceError::Io { path, source })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            Some(retryable_provider_tool_output(
                parsed.name().as_str(),
                &format!("workspace path not found: {}", path.display()),
                serde_json::json!({
                    "requested_path": path.display().to_string(),
                    "hint": "check the exact relative path and list nearby files before retrying",
                }),
            ))
        }
        ToolError::Workspace(WorkspaceError::Io { path, source })
            if matches!(
                source.kind(),
                std::io::ErrorKind::IsADirectory | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Some(retryable_provider_tool_output(
                parsed.name().as_str(),
                &format!("workspace path is not a regular file: {}", path.display()),
                serde_json::json!({
                    "requested_path": path.display().to_string(),
                    "io_error": source.to_string(),
                    "hint": "re-check whether the path should target a file or use a list/read-directory style tool instead",
                }),
            ))
        }
        ToolError::InvalidPatch { path, reason } => Some(retryable_provider_tool_output(
            parsed.name().as_str(),
            &format!("invalid patch for {path}: {reason}"),
            serde_json::json!({
                "requested_path": path,
                "patch_error": reason,
                "hint": "re-read the file and construct the patch from the current content",
            }),
        )),
        ToolError::InvalidPlanWrite { reason } if is_retryable_plan_write_reason(reason) => {
            Some(retryable_provider_tool_output(
                parsed.name().as_str(),
                &format!("invalid plan reference: {reason}"),
                serde_json::json!({
                    "plan_error": reason,
                    "hint": "use canonical task_id values returned by add_task or plan_snapshot",
                }),
            ))
        }
        _ => Some(non_retryable_provider_tool_output(
            parsed.name().as_str(),
            &error.to_string(),
            serde_json::json!({
                "requested_tool": parsed.name().as_str(),
                "request_summary": parsed.summary(),
                "error_kind": format!("{error:?}"),
                "hint": "inspect the error details and adjust the tool arguments or choose a different tool before retrying",
            }),
        )),
    }
}

pub(super) fn recoverable_execution_error_output(
    processes: &SharedProcessRegistry,
    workspace: &WorkspaceRef,
    parsed: &ToolCall,
    error: &ExecutionError,
) -> Option<String> {
    match error {
        ExecutionError::Tool(tool_error) => {
            recoverable_tool_error_output(processes, workspace, parsed, tool_error)
        }
        ExecutionError::PermissionDenied { tool, reason } => Some(
            serde_json::json!({
                "tool": tool,
                "error": reason,
                "retryable": false,
                "details": {
                    "requested_tool": tool,
                    "constraint": "agent_allowed_tools",
                },
            })
            .to_string(),
        ),
        _ => None,
    }
}

pub(super) fn is_retryable_plan_write_reason(reason: &str) -> bool {
    reason.starts_with("unknown dependency ")
        || reason.starts_with("unknown task ")
        || reason.starts_with("unknown parent task ")
}
