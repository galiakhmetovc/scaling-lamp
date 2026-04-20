use agent_persistence::TranscriptRecord;
use agent_runtime::context::{ContextSummary, approximate_token_count};
use agent_runtime::prompt::{
    SessionHead, SessionHeadFsActivity, SessionHeadWorkspaceEntry, SessionHeadWorkspaceEntryKind,
};
use agent_runtime::run::{RunSnapshot, RunStatus, RunStepKind};
use agent_runtime::session::Session;
use agent_runtime::workspace::{WorkspaceEntryKind, WorkspaceRef};
use std::io::ErrorKind;

const RECENT_FILESYSTEM_ACTIVITY_LIMIT: usize = 6;
const WORKSPACE_TREE_LIMIT: usize = 12;
const DEFAULT_SYSTEM_PROMPT: &str = "You are a useful AI assistant.";

pub(crate) fn build_session_head(
    session: &Session,
    transcripts: &[TranscriptRecord],
    context_summary: Option<&ContextSummary>,
    runs: &[RunSnapshot],
    workspace: &WorkspaceRef,
) -> SessionHead {
    let message_count = transcripts.len();
    let covered_message_count = context_summary
        .map(|summary| summary.covered_message_count as usize)
        .unwrap_or(0)
        .min(message_count);
    let uncovered_transcript_tokens = transcripts
        .iter()
        .skip(covered_message_count)
        .map(|record| approximate_token_count(record.content.as_str()))
        .sum::<u32>();
    let summary_tokens = context_summary
        .map(|summary| summary.summary_token_estimate)
        .unwrap_or(0);
    let pending_approval_count = runs
        .iter()
        .filter(|run| {
            run.session_id == session.id
                && run.status == RunStatus::WaitingApproval
                && !run.pending_approvals.is_empty()
        })
        .map(|run| run.pending_approvals.len())
        .sum::<usize>();

    let (workspace_tree, workspace_tree_truncated) = build_workspace_tree(workspace);

    SessionHead {
        session_id: session.id.clone(),
        title: session.title.clone(),
        message_count,
        context_tokens: uncovered_transcript_tokens + summary_tokens,
        compactifications: session.settings.compactifications,
        summary_covered_message_count: covered_message_count as u32,
        pending_approval_count,
        last_user_preview: transcripts
            .iter()
            .rev()
            .find(|record| record.kind == "user")
            .map(|record| preview_text(record.content.as_str(), 96)),
        last_assistant_preview: transcripts
            .iter()
            .rev()
            .find(|record| record.kind == "assistant")
            .map(|record| preview_text(record.content.as_str(), 96)),
        recent_filesystem_activity: build_recent_filesystem_activity(session, runs),
        workspace_tree,
        workspace_tree_truncated,
    }
}

pub(crate) fn preview_text(content: &str, limit: usize) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= limit {
        return collapsed;
    }
    let mut preview = collapsed
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    preview.push('…');
    preview
}

pub(crate) fn load_system_prompt(workspace: &WorkspaceRef) -> String {
    read_prompt_file(workspace, "SYSTEM.md")
        .filter(|content| !content.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string())
}

pub(crate) fn load_agents_prompt(workspace: &WorkspaceRef) -> Option<String> {
    read_prompt_file(workspace, "AGENTS.md").filter(|content| !content.trim().is_empty())
}

fn read_prompt_file(workspace: &WorkspaceRef, path: &str) -> Option<String> {
    match workspace.read_text(path) {
        Ok(content) => Some(content.trim().to_string()),
        Err(agent_runtime::workspace::WorkspaceError::Io { source, .. })
            if source.kind() == ErrorKind::NotFound =>
        {
            None
        }
        Err(_) => None,
    }
}

fn build_recent_filesystem_activity(
    session: &Session,
    runs: &[RunSnapshot],
) -> Vec<SessionHeadFsActivity> {
    let mut activity = runs
        .iter()
        .filter(|run| run.session_id == session.id)
        .flat_map(|run| run.recent_steps.iter())
        .filter(|step| step.kind == RunStepKind::ToolCompleted)
        .filter_map(|step| parse_filesystem_activity(step.detail.as_str(), step.recorded_at))
        .collect::<Vec<_>>();

    activity.sort_by(|left, right| {
        right
            .recorded_at
            .cmp(&left.recorded_at)
            .then_with(|| left.detail.cmp(&right.detail))
    });
    activity.truncate(RECENT_FILESYSTEM_ACTIVITY_LIMIT);
    activity
}

fn build_workspace_tree(workspace: &WorkspaceRef) -> (Vec<SessionHeadWorkspaceEntry>, bool) {
    let Ok(entries) = workspace.list("", false) else {
        return (Vec::new(), false);
    };
    let truncated = entries.len() > WORKSPACE_TREE_LIMIT;
    let rendered = entries
        .into_iter()
        .take(WORKSPACE_TREE_LIMIT)
        .map(|entry| SessionHeadWorkspaceEntry {
            path: entry.path,
            kind: match entry.kind {
                WorkspaceEntryKind::File => SessionHeadWorkspaceEntryKind::File,
                WorkspaceEntryKind::Directory => SessionHeadWorkspaceEntryKind::Directory,
            },
        })
        .collect();
    (rendered, truncated)
}

fn parse_filesystem_activity(detail: &str, recorded_at: i64) -> Option<SessionHeadFsActivity> {
    let (tool_summary, _) = detail.split_once(" -> ")?;
    let (action, target) = if tool_summary.starts_with("fs_read ")
        || tool_summary.starts_with("fs_read_text ")
        || tool_summary.starts_with("fs_read_lines ")
    {
        ("read", extract_tool_field(tool_summary, "path")?)
    } else if tool_summary.starts_with("fs_write ") || tool_summary.starts_with("fs_write_text ") {
        ("write", extract_tool_field(tool_summary, "path")?)
    } else if tool_summary.starts_with("fs_patch ")
        || tool_summary.starts_with("fs_patch_text ")
        || tool_summary.starts_with("fs_replace_lines ")
        || tool_summary.starts_with("fs_insert_text ")
    {
        ("patch", extract_tool_field(tool_summary, "path")?)
    } else if tool_summary.starts_with("fs_list ") {
        ("list", extract_tool_field(tool_summary, "path")?)
    } else if tool_summary.starts_with("fs_glob ") {
        ("glob", extract_tool_field(tool_summary, "path")?)
    } else if tool_summary.starts_with("fs_search ") || tool_summary.starts_with("fs_search_text ")
    {
        ("search", extract_tool_field(tool_summary, "path")?)
    } else if tool_summary.starts_with("fs_find_in_files ") {
        ("search", "<workspace>".to_string())
    } else if tool_summary.starts_with("fs_mkdir ") {
        ("mkdir", extract_tool_field(tool_summary, "path")?)
    } else if tool_summary.starts_with("fs_move ") {
        ("move", extract_tool_field(tool_summary, "dest")?)
    } else if tool_summary.starts_with("fs_trash ") {
        ("trash", extract_tool_field(tool_summary, "path")?)
    } else {
        return None;
    };

    Some(SessionHeadFsActivity {
        action: action.to_string(),
        target,
        detail: detail.to_string(),
        recorded_at,
    })
}

fn extract_tool_field(summary: &str, field: &str) -> Option<String> {
    let marker = format!("{field}=");
    let value = summary.split_once(&marker)?.1;
    Some(
        value
            .split_once(' ')
            .map(|(target, _)| target)
            .unwrap_or(value)
            .to_string(),
    )
}
