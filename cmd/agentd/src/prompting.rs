use crate::agents;
use agent_persistence::TranscriptRecord;
use agent_runtime::context::{ContextSummary, approximate_token_count};
use agent_runtime::prompt::{
    SessionHead, SessionHeadFsActivity, SessionHeadProcessActivity, SessionHeadScheduleSummary,
    SessionHeadWorkspaceEntry, SessionHeadWorkspaceEntryKind,
};
use agent_runtime::run::{RunSnapshot, RunStatus, RunStepKind};
use agent_runtime::session::Session;
use agent_runtime::skills::{
    SessionSkillStatus, SkillActivationMode, SkillCatalog, parse_skill_document,
};
use agent_runtime::workspace::{WorkspaceEntryKind, WorkspaceRef};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

const RECENT_FILESYSTEM_ACTIVITY_LIMIT: usize = 6;
const RECENT_PROCESS_ACTIVITY_LIMIT: usize = 6;
const WORKSPACE_TREE_LIMIT: usize = 12;

pub(crate) fn build_session_head(
    session: &Session,
    agent_name: &str,
    schedule: Option<SessionHeadScheduleSummary>,
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
    let provider_input_tokens = runs
        .iter()
        .filter(|run| run.session_id == session.id)
        .max_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.started_at.cmp(&right.started_at))
                .then_with(|| left.id.cmp(&right.id))
        })
        .and_then(|run| run.latest_provider_usage.as_ref())
        .map(|usage| usage.input_tokens);
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
        agent_profile_id: session.agent_profile_id.clone(),
        agent_name: agent_name.to_string(),
        schedule,
        message_count,
        context_tokens: provider_input_tokens
            .unwrap_or(uncovered_transcript_tokens + summary_tokens),
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
        recent_process_activity: build_recent_process_activity(session, runs),
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

pub(crate) fn load_system_prompt(data_dir: &Path, agent_profile_id: &str) -> String {
    read_prompt_file(&agents::agent_home(data_dir, agent_profile_id).join("SYSTEM.md"))
        .filter(|content| !content.trim().is_empty())
        .unwrap_or_else(|| agents::fallback_system_md(agent_profile_id).to_string())
}

pub(crate) fn load_agents_prompt(data_dir: &Path, agent_profile_id: &str) -> Option<String> {
    Some(
        read_prompt_file(&agents::agent_home(data_dir, agent_profile_id).join("AGENTS.md"))
            .filter(|content| !content.trim().is_empty())
            .unwrap_or_else(|| agents::fallback_agents_md(agent_profile_id).to_string()),
    )
}

pub(crate) fn load_active_skill_prompts(
    catalog: &SkillCatalog,
    active_skills: &[SessionSkillStatus],
) -> Vec<String> {
    let active_names = active_skills
        .iter()
        .filter(|skill| {
            matches!(
                skill.mode,
                SkillActivationMode::Automatic | SkillActivationMode::Manual
            )
        })
        .map(|skill| skill.name.as_str())
        .collect::<Vec<_>>();

    let mut prompts = Vec::new();
    for skill in catalog.entries.iter().filter(|entry| {
        active_names
            .iter()
            .any(|candidate| entry.name.eq_ignore_ascii_case(candidate))
    }) {
        let Ok(contents) = std::fs::read_to_string(&skill.skill_md_path) else {
            continue;
        };
        let Ok(document) = parse_skill_document(&skill.skill_md_path, &contents) else {
            continue;
        };
        if document.body.trim().is_empty() {
            continue;
        }
        prompts.push(document.body);
    }
    prompts
}

fn read_prompt_file(path: &Path) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(content) => Some(content.trim().to_string()),
        Err(source) if source.kind() == ErrorKind::NotFound => None,
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

fn build_recent_process_activity(
    session: &Session,
    runs: &[RunSnapshot],
) -> Vec<SessionHeadProcessActivity> {
    let mut activity = runs
        .iter()
        .filter(|run| run.session_id == session.id)
        .flat_map(|run| run.recent_steps.iter())
        .filter(|step| step.kind == RunStepKind::ToolCompleted)
        .filter_map(|step| parse_process_activity(step.detail.as_str(), step.recorded_at))
        .collect::<Vec<_>>();

    activity.sort_by(|left, right| {
        right
            .recorded_at
            .cmp(&left.recorded_at)
            .then_with(|| left.detail.cmp(&right.detail))
    });
    activity.truncate(RECENT_PROCESS_ACTIVITY_LIMIT);
    activity
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

fn parse_process_activity(detail: &str, recorded_at: i64) -> Option<SessionHeadProcessActivity> {
    let (tool_summary, output_summary) = detail.split_once(" -> ")?;

    let (action, target) = if let Some(command) = parse_exec_start_command(tool_summary) {
        ("start", preview_text(command, 72))
    } else if tool_summary.starts_with("exec_wait ") {
        let process_id = extract_tool_field(tool_summary, "process_id")?;
        (
            "finish",
            format_process_result_target(process_id, output_summary),
        )
    } else if tool_summary.starts_with("exec_kill ") {
        let process_id = extract_tool_field(tool_summary, "process_id")?;
        (
            "kill",
            format_process_result_target(process_id, output_summary),
        )
    } else {
        return None;
    };

    Some(SessionHeadProcessActivity {
        action: action.to_string(),
        target,
        detail: detail.to_string(),
        recorded_at,
    })
}

fn parse_exec_start_command(summary: &str) -> Option<&str> {
    summary
        .strip_prefix("exec_start ")?
        .split_once(" command=")
        .map(|(_, command)| command)
}

fn format_process_result_target(process_id: String, output_summary: &str) -> String {
    let Some(result_summary) = output_summary
        .strip_prefix("process_result ")
        .or_else(|| output_summary.strip_prefix("process_output_read "))
    else {
        return process_id;
    };
    let status = extract_tool_field(result_summary, "status");
    let exit_code =
        extract_tool_field(result_summary, "exit_code").and_then(|value| parse_exit_code(&value));

    match (status, exit_code) {
        (Some(status), Some(exit_code)) => {
            format!("{process_id} status={status} exit={exit_code}")
        }
        (Some(status), None) => format!("{process_id} status={status}"),
        (None, Some(exit_code)) => format!("{process_id} exit={exit_code}"),
        (None, None) => process_id,
    }
}

fn parse_exit_code(raw: &str) -> Option<String> {
    Some(raw.strip_prefix("Some(")?.strip_suffix(')')?.to_string())
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
