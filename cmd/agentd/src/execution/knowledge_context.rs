use crate::agents;
use agent_persistence::{
    ArtifactRecord, ContextSummaryRepository, KnowledgeConfig, PersistenceStore, PlanRepository,
    RuntimeLimitsConfig, ToolCallRepository,
};
use agent_runtime::context::ContextSummary;
use agent_runtime::plan::{PlanItemStatus, PlanSnapshot};
use agent_runtime::prompt::{SessionHeadJournalBlock, SessionHeadTextBlock};
use agent_runtime::session::Session;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use time::macros::format_description;
use time::{Duration, OffsetDateTime, UtcOffset};

const AREA_RELATIVE_PATH: &str = "a/teamd-agents.md";
const MANAGED_MIRRORS_START: &str = "<!-- teamd:session-mirrors:start -->";
const MANAGED_MIRRORS_END: &str = "<!-- teamd:session-mirrors:end -->";
const TEXT_ARTIFACT_EXTENSIONS: &[&str] = &[
    "bash", "css", "csv", "html", "js", "json", "lua", "md", "py", "rs", "sh", "sql", "toml", "ts",
    "txt", "xml", "yaml", "yml",
];

pub(crate) fn operator_context_block(
    data_dir: &Path,
    max_chars: usize,
) -> Option<SessionHeadTextBlock> {
    let path = agents::ensure_operator_user_md(data_dir).ok()?;
    let contents = fs::read_to_string(&path).ok()?;
    let (content, truncated) = truncate_head(contents.trim(), max_chars);
    if content.trim().is_empty() {
        return None;
    }
    Some(SessionHeadTextBlock {
        title: "USER.md".to_string(),
        path: path.display().to_string(),
        content,
        truncated,
    })
}

pub(crate) fn silverbullet_journal_context(
    knowledge: &KnowledgeConfig,
    now: i64,
    max_chars_per_day: usize,
) -> Vec<SessionHeadJournalBlock> {
    if !knowledge.silverbullet_journal_context_enabled {
        return Vec::new();
    }
    let Some(space_dir) = knowledge.silverbullet_space_dir.as_ref() else {
        return Vec::new();
    };
    let Some(today) = date_string(now, knowledge.operator_timezone.as_str(), 0) else {
        return Vec::new();
    };
    let Some(yesterday) = date_string(now, knowledge.operator_timezone.as_str(), -1) else {
        return Vec::new();
    };
    [("today", today), ("yesterday", yesterday)]
        .into_iter()
        .filter_map(|(label, date)| journal_block(space_dir, label, date, max_chars_per_day))
        .collect()
}

pub(crate) fn silverbullet_session_mirror_path(
    knowledge: &KnowledgeConfig,
    session: &Session,
) -> Option<String> {
    if !knowledge.silverbullet_mirror_enabled {
        return None;
    }
    let space_dir = knowledge.silverbullet_space_dir.as_ref()?;
    if !space_dir.exists() {
        return None;
    }
    Some(
        space_dir
            .join(session_mirror_relative_path(session))
            .display()
            .to_string(),
    )
}

pub(crate) fn mirror_session_snapshot(
    knowledge: &KnowledgeConfig,
    store: &PersistenceStore,
    session: &Session,
    reason: &str,
    now: i64,
    limits: &RuntimeLimitsConfig,
) -> Result<Option<PathBuf>, String> {
    if !knowledge.silverbullet_mirror_enabled {
        return Ok(None);
    }
    let Some(space_dir) = knowledge.silverbullet_space_dir.as_ref() else {
        return Ok(None);
    };
    if !space_dir.exists() {
        return Ok(None);
    }

    let relative_path = session_mirror_relative_path(session);
    let absolute_path = space_dir.join(&relative_path);
    let plan = store
        .get_plan(&session.id)
        .map_err(|source| source.to_string())?
        .map(PlanSnapshot::try_from)
        .transpose()
        .map_err(|source| source.to_string())?;
    let context_summary = store
        .get_context_summary(&session.id)
        .map_err(|source| source.to_string())?
        .map(ContextSummary::try_from)
        .transpose()
        .map_err(|source| source.to_string())?;
    let tool_calls = store
        .list_tool_calls_for_session(&session.id)
        .map_err(|source| source.to_string())?;
    let artifacts = store
        .list_artifacts_for_session(&session.id)
        .map_err(|source| source.to_string())?;

    if let Some(parent) = absolute_path.parent() {
        fs::create_dir_all(parent).map_err(|source| source.to_string())?;
    }
    let markdown = render_session_mirror(
        session,
        &relative_path,
        reason,
        now,
        knowledge.operator_timezone.as_str(),
        plan.as_ref(),
        context_summary.as_ref(),
        &tool_calls,
        &artifacts,
        limits,
    );
    fs::write(&absolute_path, markdown).map_err(|source| source.to_string())?;
    update_area_index(space_dir, now, knowledge.operator_timezone.as_str())
        .map_err(|source| source.to_string())?;
    Ok(Some(absolute_path))
}

pub(crate) fn session_mirror_relative_path(session: &Session) -> PathBuf {
    PathBuf::from("p").join(format!("teamd-session-{}.md", session.id))
}

fn journal_block(
    space_dir: &Path,
    label: &str,
    date: String,
    max_chars: usize,
) -> Option<SessionHeadJournalBlock> {
    let path = space_dir.join("journals").join(format!("{date}.md"));
    let contents = fs::read_to_string(&path).ok()?;
    let (content, truncated) = truncate_tail(contents.trim(), max_chars);
    if content.trim().is_empty() {
        return None;
    }
    Some(SessionHeadJournalBlock {
        label: label.to_string(),
        date,
        path: path.display().to_string(),
        content,
        truncated,
    })
}

fn date_string(now: i64, timezone: &str, day_offset: i64) -> Option<String> {
    let offset = timezone_offset(timezone);
    let date = OffsetDateTime::from_unix_timestamp(now)
        .ok()?
        .to_offset(offset)
        .date()
        .saturating_add(Duration::days(day_offset));
    date.format(format_description!("[year]-[month]-[day]"))
        .ok()
}

fn datetime_string(now: i64, timezone: &str) -> String {
    OffsetDateTime::from_unix_timestamp(now)
        .map(|value| value.to_offset(timezone_offset(timezone)))
        .ok()
        .and_then(|value| {
            value
                .format(format_description!(
                    "[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory]:[offset_minute]"
                ))
                .ok()
        })
        .unwrap_or_else(|| now.to_string())
}

#[allow(clippy::too_many_arguments)]
fn render_session_mirror(
    session: &Session,
    relative_path: &Path,
    reason: &str,
    now: i64,
    timezone: &str,
    plan: Option<&PlanSnapshot>,
    context_summary: Option<&ContextSummary>,
    tool_calls: &[agent_persistence::ToolCallRecord],
    artifacts: &[ArtifactRecord],
    limits: &RuntimeLimitsConfig,
) -> String {
    let updated = datetime_string(now, timezone);
    let mut lines = vec![
        "---".to_string(),
        "tags: [project, teamd-session, teamd-agent]".to_string(),
        format!("teamd_session_id: {}", session.id),
        format!("teamd_agent_profile_id: {}", session.agent_profile_id),
        format!("teamd_workspace_root: {}", yaml_quote(&session.workspace_root.display().to_string())),
        format!("teamd_mirror_path: {}", yaml_quote(&relative_path.display().to_string())),
        format!("updated: {}", yaml_quote(&updated)),
        "---".to_string(),
        String::new(),
        format!("# TeamD Session: {}", markdown_inline(&session.title)),
        String::new(),
        "> This page is a human-readable mirror. The canonical runtime state remains in agentd SQLite, transcripts, artifacts and tool-call ledgers.".to_string(),
        String::new(),
        format!("- Session: `{}`", session.id),
        format!("- Agent Profile: `{}`", session.agent_profile_id),
        format!("- Workspace: `{}`", session.workspace_root.display()),
        format!("- Last Mirror Reason: {}", markdown_inline(reason)),
        format!("- Updated: `{updated}`"),
        String::new(),
    ];

    lines.extend(render_plan_section(plan));
    lines.extend(render_context_summary_section(context_summary));
    lines.extend(render_tool_activity_section(tool_calls));
    lines.extend(render_artifacts_section(artifacts, limits));

    lines.join("\n")
}

fn render_plan_section(plan: Option<&PlanSnapshot>) -> Vec<String> {
    let mut lines = vec!["## Plan Snapshot".to_string(), String::new()];
    let Some(plan) = plan else {
        lines.push("_No plan has been initialized for this session._".to_string());
        lines.push(String::new());
        return lines;
    };
    if let Some(goal) = plan.goal.as_deref() {
        lines.push(format!("- Goal: {}", markdown_inline(goal)));
    }
    if plan.items.is_empty() {
        lines.push("- Items: none".to_string());
    } else {
        for item in &plan.items {
            let checkbox = match item.status {
                PlanItemStatus::Completed => "x",
                _ => " ",
            };
            lines.push(format!(
                "- [{checkbox}] `{}` {} ({})",
                item.id,
                markdown_inline(&item.content),
                item.status.as_str()
            ));
            if !item.depends_on.is_empty() {
                lines.push(format!(
                    "  - depends_on: `{}`",
                    item.depends_on.join("`, `")
                ));
            }
            if let Some(blocked_reason) = item.blocked_reason.as_deref() {
                lines.push(format!(
                    "  - blocked_reason: {}",
                    markdown_inline(blocked_reason)
                ));
            }
            for note in &item.notes {
                lines.push(format!("  - note: {}", markdown_inline(note)));
            }
        }
    }
    lines.push(String::new());
    lines
}

fn render_context_summary_section(summary: Option<&ContextSummary>) -> Vec<String> {
    let mut lines = vec!["## Context Summary".to_string(), String::new()];
    let Some(summary) = summary else {
        lines.push("_No compaction summary yet._".to_string());
        lines.push(String::new());
        return lines;
    };
    lines.push(format!(
        "- Covers: `{}` transcript messages",
        summary.covered_message_count
    ));
    lines.push(format!(
        "- Token Estimate: `{}`",
        summary.summary_token_estimate
    ));
    lines.push(String::new());
    lines.push("```text".to_string());
    lines.push(summary.summary_text.clone());
    lines.push("```".to_string());
    lines.push(String::new());
    lines
}

fn render_tool_activity_section(tool_calls: &[agent_persistence::ToolCallRecord]) -> Vec<String> {
    let mut lines = vec!["## Recent Tool Activity".to_string(), String::new()];
    if tool_calls.is_empty() {
        lines.push("_No tool calls recorded for this session._".to_string());
        lines.push(String::new());
        return lines;
    }
    lines.push("| Time | Tool | Status | Summary | Error |".to_string());
    lines.push("| --- | --- | --- | --- | --- |".to_string());
    for call in tool_calls.iter().rev().take(30).rev() {
        lines.push(format!(
            "| `{}` | `{}` | `{}` | {} | {} |",
            call.updated_at,
            markdown_table_cell(&call.tool_name),
            markdown_table_cell(&call.status),
            markdown_table_cell(&call.summary),
            markdown_table_cell(call.error.as_deref().unwrap_or(""))
        ));
    }
    lines.push(String::new());
    lines
}

fn render_artifacts_section(
    artifacts: &[ArtifactRecord],
    limits: &RuntimeLimitsConfig,
) -> Vec<String> {
    let mut lines = vec!["## Artifacts".to_string(), String::new()];
    if artifacts.is_empty() {
        lines.push("_No artifacts recorded for this session._".to_string());
        lines.push(String::new());
        return lines;
    }
    for artifact in artifacts.iter().rev().take(24).rev() {
        lines.push(format!(
            "### `{}` ({})",
            artifact.id,
            markdown_inline(&artifact.kind)
        ));
        lines.push(format!("- Path: `{}`", artifact.path.display()));
        lines.push(format!("- Bytes: `{}`", artifact.bytes.len()));
        if should_inline_artifact(artifact) {
            let max_chars = if is_script_artifact(artifact) {
                limits.silverbullet_mirror_script_max_chars
            } else {
                limits.silverbullet_mirror_text_artifact_max_chars
            };
            let (content, truncated) = truncate_head_lossy(&artifact.bytes, max_chars);
            if !content.trim().is_empty() {
                if truncated {
                    lines.push("- Content: truncated".to_string());
                } else {
                    lines.push("- Content: full preview".to_string());
                }
                lines.push("```text".to_string());
                lines.push(content);
                lines.push("```".to_string());
            }
        }
        lines.push(String::new());
    }
    lines
}

fn update_area_index(space_dir: &Path, now: i64, timezone: &str) -> io::Result<()> {
    let area_path = space_dir.join(AREA_RELATIVE_PATH);
    if let Some(parent) = area_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = fs::read_to_string(&area_path).unwrap_or_else(|_| {
        [
            "---",
            "tags: [area, teamd, agents]",
            "---",
            "",
            "# TeamD Agents",
            "",
            "This page indexes TeamD session mirrors. Runtime remains the canonical source of truth.",
            "",
        ]
        .join("\n")
    });
    let mirrors = mirror_index_lines(space_dir, now, timezone);
    let updated = replace_managed_section(&existing, &mirrors);
    fs::write(area_path, updated)
}

fn mirror_index_lines(space_dir: &Path, now: i64, timezone: &str) -> String {
    let sessions_dir = space_dir.join("p");
    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(&sessions_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with("teamd-session-") || !name.ends_with(".md") {
                continue;
            }
            entries.push(format!("- [[p/{}]]", name.trim_end_matches(".md")));
        }
    }
    entries.sort();
    let mut lines = vec![
        "## Session Mirrors".to_string(),
        String::new(),
        format!("Last refreshed: `{}`", datetime_string(now, timezone)),
        String::new(),
    ];
    if entries.is_empty() {
        lines.push("_No session mirrors yet._".to_string());
    } else {
        lines.extend(entries);
    }
    lines.join("\n")
}

fn replace_managed_section(existing: &str, replacement: &str) -> String {
    let section = format!("{MANAGED_MIRRORS_START}\n{replacement}\n{MANAGED_MIRRORS_END}");
    let Some(start) = existing.find(MANAGED_MIRRORS_START) else {
        let separator = if existing.ends_with('\n') { "" } else { "\n" };
        return format!("{existing}{separator}\n{section}\n");
    };
    let Some(end_relative) = existing[start..].find(MANAGED_MIRRORS_END) else {
        let separator = if existing.ends_with('\n') { "" } else { "\n" };
        return format!("{existing}{separator}\n{section}\n");
    };
    let end = start + end_relative + MANAGED_MIRRORS_END.len();
    format!("{}{}{}", &existing[..start], section, &existing[end..])
}

fn should_inline_artifact(artifact: &ArtifactRecord) -> bool {
    if artifact.kind.contains("text") || artifact.kind.contains("script") {
        return true;
    }
    artifact
        .path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            TEXT_ARTIFACT_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
        .unwrap_or(false)
}

fn is_script_artifact(artifact: &ArtifactRecord) -> bool {
    if artifact.kind.contains("script") {
        return true;
    }
    artifact
        .path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            ["bash", "js", "lua", "py", "rs", "sh", "ts"]
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
        .unwrap_or(false)
}

fn truncate_head_lossy(bytes: &[u8], max_chars: usize) -> (String, bool) {
    let value = String::from_utf8_lossy(bytes);
    truncate_head(value.trim(), max_chars)
}

fn timezone_offset(timezone: &str) -> UtcOffset {
    match timezone.trim() {
        "Europe/Moscow" | "MSK" | "UTC+3" | "UTC+03:00" | "+03:00" => {
            UtcOffset::from_hms(3, 0, 0).unwrap_or(UtcOffset::UTC)
        }
        "Etc/UTC" | "UTC" | "Z" | "+00:00" => UtcOffset::UTC,
        value => parse_numeric_offset(value).unwrap_or(UtcOffset::UTC),
    }
}

fn parse_numeric_offset(value: &str) -> Option<UtcOffset> {
    let raw = value.strip_prefix("UTC").unwrap_or(value).trim();
    let sign = if raw.starts_with('-') { -1 } else { 1 };
    let raw = raw.strip_prefix(['+', '-']).unwrap_or(raw);
    let (hours, minutes) = raw.split_once(':').unwrap_or((raw, "0"));
    let hours = hours.parse::<i8>().ok()?.saturating_mul(sign);
    let minutes = minutes.parse::<i8>().ok()?.saturating_mul(sign);
    UtcOffset::from_hms(hours, minutes, 0).ok()
}

fn truncate_head(contents: &str, max_chars: usize) -> (String, bool) {
    truncate_chars(contents, max_chars, false)
}

fn truncate_tail(contents: &str, max_chars: usize) -> (String, bool) {
    truncate_chars(contents, max_chars, true)
}

fn truncate_chars(contents: &str, max_chars: usize, tail: bool) -> (String, bool) {
    let char_count = contents.chars().count();
    if char_count <= max_chars {
        return (contents.to_string(), false);
    }
    if tail {
        let kept = contents
            .chars()
            .skip(char_count.saturating_sub(max_chars))
            .collect::<String>();
        return (format!("[truncated: showing tail]\n{kept}"), true);
    }
    let kept = contents.chars().take(max_chars).collect::<String>();
    (format!("{kept}\n[truncated]"), true)
}

fn markdown_inline(value: &str) -> String {
    value.replace('\n', " ")
}

fn markdown_table_cell(value: &str) -> String {
    markdown_inline(value)
        .replace('|', "\\|")
        .replace('\r', " ")
}

fn yaml_quote(value: &str) -> String {
    format!("{value:?}")
}

pub(crate) fn compaction_preview(summary: &ContextSummary) -> String {
    let single_line = summary
        .summary_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let (preview, truncated) = truncate_head(single_line.as_str(), 240);
    if truncated {
        preview.replace('\n', " ")
    } else {
        preview
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_runtime::session::Session;
    use tempfile::TempDir;
    use time::macros::datetime;

    #[test]
    fn operator_context_block_seeds_user_md() {
        let temp = TempDir::new().expect("temp dir");

        let block = operator_context_block(temp.path(), 128).expect("operator context");

        assert_eq!(block.title, "USER.md");
        assert!(block.path.ends_with("USER.md"));
        assert!(block.content.contains("Operator timezone"));
        assert!(temp.path().join("USER.md").exists());
    }

    #[test]
    fn silverbullet_journal_context_uses_operator_timezone_and_tail_limit() {
        let temp = TempDir::new().expect("temp dir");
        let journals = temp.path().join("journals");
        fs::create_dir_all(&journals).expect("journals dir");
        fs::write(
            journals.join("2026-05-05.md"),
            "old line\nimportant today line",
        )
        .expect("today journal");
        fs::write(journals.join("2026-05-04.md"), "yesterday line").expect("yesterday journal");
        let knowledge = KnowledgeConfig {
            silverbullet_space_dir: Some(temp.path().to_path_buf()),
            ..KnowledgeConfig::default()
        };
        let now = datetime!(2026-05-04 21:30 UTC).unix_timestamp();

        let blocks = silverbullet_journal_context(&knowledge, now, 20);

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].date, "2026-05-05");
        assert_eq!(blocks[0].label, "today");
        assert!(blocks[0].content.contains("important today line"));
        assert!(blocks[0].truncated);
        assert_eq!(blocks[1].date, "2026-05-04");
    }

    #[test]
    fn session_mirror_relative_path_is_stable() {
        let session = Session {
            id: "session-123".to_string(),
            ..Session::default()
        };

        assert_eq!(
            session_mirror_relative_path(&session),
            PathBuf::from("p/teamd-session-session-123.md")
        );
    }
}
