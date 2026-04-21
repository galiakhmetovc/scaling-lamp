use crate::tui::app::{DialogState, TuiAppState, TuiScreen};
use crate::tui::timeline::{TimelineEntry, TimelineEntryKind};
use crate::tui::worker::{ActiveRunKind, ActiveRunPhase};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use std::time::{SystemTime, UNIX_EPOCH};
use time::macros::format_description;
use time::{Date, OffsetDateTime};

pub fn render(frame: &mut Frame<'_>, state: &TuiAppState) {
    match state.active_screen() {
        TuiScreen::Sessions => render_session_screen(frame, state),
        TuiScreen::Chat => render_chat_screen(frame, state),
    }

    if let Some(dialog) = state.dialog_state() {
        render_dialog(frame, dialog);
    }
}

fn render_session_screen(frame: &mut Frame<'_>, state: &TuiAppState) {
    let area = frame.area();
    let now = unix_timestamp();
    let items = state
        .sessions()
        .iter()
        .map(|session| {
            let selected = state.selected_session().map(|current| current.id.as_str())
                == Some(session.id.as_str());
            let prefix = if selected { "> " } else { "  " };
            let approval = if session.has_pending_approval {
                " | approval"
            } else {
                ""
            };
            let preview = session.last_message_preview.as_deref().unwrap_or("<empty>");
            let label = format!(
                "{prefix}{} | updated={} | messages={}{}",
                session.title,
                format_timestamp(session.updated_at, now),
                session.message_count,
                approval
            );
            let preview_line = format!("    {preview}");
            let mut item = ListItem::new(vec![Line::from(label), Line::from(preview_line)]);
            if selected {
                item = item.style(Style::default().add_modifier(Modifier::BOLD));
            }
            item
        })
        .collect::<Vec<_>>();

    let list = List::new(items).block(
        Block::default()
            .title("Sessions | Enter open | N new | D delete | Esc back")
            .borders(Borders::ALL),
    );
    frame.render_widget(list, area);
}

fn render_chat_screen(frame: &mut Frame<'_>, state: &TuiAppState) {
    let area = frame.area();
    let now = unix_timestamp();
    let composer_height = composer_height(state);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(composer_height),
        ])
        .split(area);

    let top_lines = if let Some(summary) = state.current_session_summary() {
        vec![
            Line::from(format!(
                "{} | model={} | reasoning={} | think={} | finish={} | ctx={} | compact={} | messages={} | bg={} (run={} queued={})",
                summary.title,
                summary.model.as_deref().unwrap_or("<default>"),
                if summary.reasoning_visible {
                    "on"
                } else {
                    "off"
                },
                summary.think_level.as_deref().unwrap_or("<default>"),
                summary
                    .completion_nudges
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "off".to_string()),
                summary.context_tokens,
                summary.compactifications,
                summary.message_count.max(state.timeline().message_count()),
                summary.background_job_count,
                summary.running_background_job_count,
                summary.queued_background_job_count,
            )),
            Line::from(format!(
                "run={}{} | queued={} (asap={} deferred={})",
                describe_run_status(state, now),
                format_provider_loop_progress(state),
                state.queued_draft_count(),
                state.queued_priority_count(),
                state.queued_deferred_count()
            )),
        ]
    } else {
        vec![Line::from("No active session")]
    };
    let top =
        Paragraph::new(top_lines).block(Block::default().borders(Borders::ALL).title("Session"));

    let timeline_lines = state
        .timeline()
        .entries(
            state
                .current_session_summary()
                .map(|summary| summary.reasoning_visible)
                .unwrap_or(true),
        )
        .into_iter()
        .flat_map(|entry| render_timeline_entry(entry, now))
        .collect::<Vec<_>>();
    let timeline_viewport_height = usize::from(chunks[1].height.saturating_sub(2));
    let timeline_scroll_top = chat_scroll_top(
        timeline_lines.len(),
        timeline_viewport_height,
        state.scroll_offset(),
    );
    let timeline = Paragraph::new(timeline_lines)
        .block(Block::default().title("Chat").borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((timeline_scroll_top, 0));

    let mut composer_lines = render_composer_lines(state);
    composer_lines.push(Line::from(format!(
        "Enter=send after tool-step | Tab=queue after full run | Shift+Tab=cycle /commands | {}",
        describe_run_status(state, now)
    )));
    let input = Paragraph::new(composer_lines)
        .block(Block::default().title("Composer").borders(Borders::ALL));

    frame.render_widget(top, chunks[0]);
    frame.render_widget(timeline, chunks[1]);
    frame.render_widget(input, chunks[2]);
}

fn render_composer_lines(state: &TuiAppState) -> Vec<Line<'static>> {
    let cursor = state.input_cursor().min(state.input_buffer().len());
    let (before, rest) = state.input_buffer().split_at(cursor);
    let mut rest_chars = rest.chars();
    let cursor_char = rest_chars.next();
    let after = rest_chars.as_str();
    let mut lines = Vec::new();
    let mut spans = vec![Span::styled("> ", Style::default().fg(Color::Cyan))];

    let push_segment = |segments: &mut Vec<Line<'static>>,
                        current_spans: &mut Vec<Span<'static>>,
                        text: &str,
                        style: Option<Style>| {
        let mut parts = text.split('\n').peekable();
        while let Some(part) = parts.next() {
            if !part.is_empty() {
                let span = match style {
                    Some(style) => Span::styled(part.to_string(), style),
                    None => Span::raw(part.to_string()),
                };
                current_spans.push(span);
            }
            if parts.peek().is_some() {
                segments.push(Line::from(std::mem::take(current_spans)));
                current_spans.push(Span::styled("  ", Style::default().fg(Color::Cyan)));
            }
        }
    };

    push_segment(&mut lines, &mut spans, before, None);
    match cursor_char {
        Some(current) => {
            let highlighted = current.to_string();
            push_segment(
                &mut lines,
                &mut spans,
                &highlighted,
                Some(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            );
        }
        None => spans.push(Span::styled("█", Style::default().fg(Color::Yellow))),
    }
    push_segment(&mut lines, &mut spans, after, None);
    lines.push(Line::from(spans));
    lines
}

fn render_timeline_entry(entry: &TimelineEntry, now: i64) -> Vec<Line<'static>> {
    let timestamp = format_timestamp(entry.timestamp, now);
    let label = match &entry.kind {
        TimelineEntryKind::User => "user".to_string(),
        TimelineEntryKind::Assistant => "assistant".to_string(),
        TimelineEntryKind::Reasoning => "reasoning".to_string(),
        TimelineEntryKind::Tool {
            tool_name, status, ..
        } => format!("tool: {tool_name} | {status}"),
        TimelineEntryKind::Approval { approval_id } => format!("approval:{approval_id}"),
        TimelineEntryKind::System => "system".to_string(),
    };
    let prefix = format!("[{timestamp}] {label}: ");
    let continuation_prefix = " ".repeat(prefix.len());
    match entry.kind {
        TimelineEntryKind::Assistant => render_markdown_entry(
            prefix.as_str(),
            continuation_prefix.as_str(),
            &entry.content,
        ),
        TimelineEntryKind::Tool { .. } => render_tool_entry(
            prefix.as_str(),
            continuation_prefix.as_str(),
            &entry.content,
        ),
        _ => render_plain_entry(
            prefix.as_str(),
            continuation_prefix.as_str(),
            &entry.content,
        ),
    }
}

fn render_plain_entry(
    prefix: &str,
    continuation_prefix: &str,
    content: &str,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (index, raw_line) in content.lines().enumerate() {
        let current_prefix = if index == 0 {
            prefix
        } else {
            continuation_prefix
        };
        lines.push(Line::from(format!("{current_prefix}{raw_line}")));
    }
    if lines.is_empty() {
        lines.push(Line::from(prefix.to_string()));
    }
    lines
}

fn render_markdown_entry(
    prefix: &str,
    continuation_prefix: &str,
    content: &str,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut first_line = true;

    for raw_line in content.lines() {
        if raw_line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        let line_prefix = if first_line {
            prefix
        } else {
            continuation_prefix
        };
        first_line = false;
        if in_code_block {
            lines.push(Line::from(vec![
                Span::raw(line_prefix.to_string()),
                Span::styled(
                    format!("    {raw_line}"),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::DIM),
                ),
            ]));
            continue;
        }

        let trimmed = raw_line.trim_start();
        if let Some(heading) = trimmed.strip_prefix("### ") {
            lines.push(Line::from(vec![
                Span::raw(line_prefix.to_string()),
                Span::styled(
                    heading.to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]));
            continue;
        }
        if let Some(heading) = trimmed
            .strip_prefix("## ")
            .or_else(|| trimmed.strip_prefix("# "))
        {
            lines.push(Line::from(vec![
                Span::raw(line_prefix.to_string()),
                Span::styled(
                    heading.to_string(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            continue;
        }
        if let Some(quote) = trimmed.strip_prefix("> ") {
            lines.push(Line::from(vec![
                Span::raw(line_prefix.to_string()),
                Span::styled("> ", Style::default().fg(Color::Blue)),
                Span::styled(
                    quote.to_string(),
                    Style::default().add_modifier(Modifier::ITALIC),
                ),
            ]));
            continue;
        }
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            lines.push(Line::from(vec![
                Span::raw(line_prefix.to_string()),
                Span::styled("• ", Style::default().fg(Color::Cyan)),
                Span::raw(trimmed[2..].to_string()),
            ]));
            continue;
        }

        let mut spans = vec![Span::raw(line_prefix.to_string())];
        spans.extend(render_inline_code(trimmed));
        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        lines.push(Line::from(prefix.to_string()));
    }
    lines
}

fn render_tool_entry(prefix: &str, continuation_prefix: &str, content: &str) -> Vec<Line<'static>> {
    if content.trim().is_empty() {
        return vec![Line::from(prefix.to_string())];
    }

    let mut lines = vec![Line::from(prefix.to_string())];
    for raw_line in content.lines() {
        lines.push(Line::from(vec![
            Span::raw(continuation_prefix.to_string()),
            Span::styled("  -> ", Style::default().fg(Color::Cyan)),
            Span::raw(raw_line.to_string()),
        ]));
    }
    lines
}

fn render_inline_code(content: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (index, segment) in content.split('`').enumerate() {
        if segment.is_empty() {
            continue;
        }
        let span = if index % 2 == 1 {
            Span::styled(
                segment.to_string(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw(segment.to_string())
        };
        spans.push(span);
    }
    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}

fn describe_run_status(state: &TuiAppState, now: i64) -> String {
    if let Some(active_run) = state.active_run() {
        let kind = match active_run.kind() {
            ActiveRunKind::Chat => "chat",
            ActiveRunKind::Approval => "approval",
        };
        let phase = match active_run.phase() {
            ActiveRunPhase::Sending => "sending".to_string(),
            ActiveRunPhase::Streaming => "streaming".to_string(),
            ActiveRunPhase::WaitingApproval => "waiting_approval".to_string(),
            ActiveRunPhase::ToolRequested { tool_name } => format!("tool requested ({tool_name})"),
            ActiveRunPhase::ToolRunning { tool_name } => format!("tool running ({tool_name})"),
            ActiveRunPhase::ToolCompleted { tool_name } => {
                format!("tool completed ({tool_name})")
            }
            ActiveRunPhase::Failed => "failed".to_string(),
        };
        return format!(
            "{kind} {phase} {}",
            format_elapsed(active_run.started_at(), now)
        );
    }
    if state
        .current_session_summary()
        .is_some_and(|summary| summary.has_pending_approval)
    {
        return "waiting_approval".to_string();
    }
    "idle".to_string()
}

fn format_provider_loop_progress(state: &TuiAppState) -> String {
    state
        .provider_loop_progress()
        .map(|(current_round, max_rounds)| format!(" | tools={current_round}/{max_rounds}"))
        .unwrap_or_default()
}

fn composer_height(state: &TuiAppState) -> u16 {
    let input_lines = state
        .input_buffer()
        .chars()
        .filter(|ch| *ch == '\n')
        .count()
        + 1;
    (input_lines as u16 + 2).clamp(4, 8)
}

fn render_dialog(frame: &mut Frame<'_>, dialog: DialogState) {
    let area = centered_rect(frame.area(), 60, 20);
    frame.render_widget(Clear, area);
    let content = match dialog {
        DialogState::CreateSession { value } => {
            format!("Create Session\n\n{value}\n\nEnter confirm, Esc cancel")
        }
        DialogState::RenameSession { value, .. } => {
            format!("Rename Session\n\n{value}\n\nEnter confirm, Esc cancel")
        }
        DialogState::ConfirmDelete { session_id } => {
            format!("Delete session {session_id}?\n\nEnter confirm, Esc cancel")
        }
        DialogState::ConfirmClear { session_id } => {
            format!("Clear session {session_id}?\n\nEnter confirm, Esc cancel")
        }
    };
    let paragraph = Paragraph::new(content)
        .block(Block::default().title("Dialog").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn format_timestamp(timestamp: i64, now: i64) -> String {
    let Ok(current) = OffsetDateTime::from_unix_timestamp(now) else {
        return timestamp.to_string();
    };
    let Ok(value) = OffsetDateTime::from_unix_timestamp(timestamp) else {
        return timestamp.to_string();
    };

    let same_day = Date::from_calendar_date(current.year(), current.month(), current.day()).ok()
        == Date::from_calendar_date(value.year(), value.month(), value.day()).ok();

    if same_day {
        value
            .format(&format_description!("[hour repr:24]:[minute]:[second]"))
            .unwrap_or_else(|_| timestamp.to_string())
    } else {
        value
            .format(&format_description!(
                "[year]-[month]-[day] [hour repr:24]:[minute]"
            ))
            .unwrap_or_else(|_| timestamp.to_string())
    }
}

fn format_elapsed(started_at: i64, now: i64) -> String {
    let elapsed = now.saturating_sub(started_at);
    let minutes = elapsed / 60;
    let seconds = elapsed % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn chat_scroll_top(total_lines: usize, viewport_lines: usize, offset_from_bottom: u16) -> u16 {
    if viewport_lines == 0 || total_lines <= viewport_lines {
        return 0;
    }

    let max_top = total_lines.saturating_sub(viewport_lines);
    let offset_from_bottom = usize::from(offset_from_bottom).min(max_top);
    max_top.saturating_sub(offset_from_bottom) as u16
}

#[cfg(test)]
mod tests {
    use super::{chat_scroll_top, format_timestamp, render_markdown_entry, render_timeline_entry};
    use crate::tui::timeline::{TimelineEntry, TimelineEntryKind};

    #[test]
    fn timestamps_render_in_human_readable_form_for_same_day_entries() {
        let formatted = format_timestamp(1_775_200_010, 1_775_200_099);
        assert_eq!(formatted.len(), 8);
        assert!(formatted.contains(':'));
    }

    #[test]
    fn markdown_renderer_formats_headings_lists_and_code_blocks() {
        let lines = render_markdown_entry(
            "[12:00:00] assistant: ",
            "                     ",
            "# Heading\n- item one\n```rust\nfn main() {}\n```",
        );
        let rendered = lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("Heading"));
        assert!(rendered.contains("• item one"));
        assert!(rendered.contains("fn main() {}"));
    }

    #[test]
    fn chat_scroll_top_follows_the_tail_and_respects_manual_offset() {
        assert_eq!(chat_scroll_top(3, 8, 0), 0);
        assert_eq!(chat_scroll_top(20, 5, 0), 15);
        assert_eq!(chat_scroll_top(20, 5, 3), 12);
        assert_eq!(chat_scroll_top(20, 5, 99), 0);
    }

    #[test]
    fn tool_entries_render_with_a_clear_status_line_and_summary_detail() {
        let lines = render_timeline_entry(
            &TimelineEntry {
                timestamp: 1_775_200_010,
                kind: TimelineEntryKind::Tool {
                    tool_name: "web_fetch".to_string(),
                    status: "completed".to_string(),
                    summary: "web_fetch url=https://example.com/doc".to_string(),
                },
                content: "web_fetch url=https://example.com/doc".to_string(),
            },
            1_775_200_099,
        );
        let rendered = lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("tool: web_fetch | completed:"));
        assert!(rendered.contains("-> web_fetch url=https://example.com/doc"));
    }
}
