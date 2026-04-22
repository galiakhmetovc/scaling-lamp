use crate::about::{APP_VERSION, short_version_label};
use crate::bootstrap::SessionSummary;
use crate::help::QUICK_HELP_LINE;
use crate::tui::app::{DialogState, TuiAppState, TuiScreen};
use crate::tui::timeline::{TimelineEntry, TimelineEntryKind};
use crate::tui::worker::{ActiveRunKind, ActiveRunPhase};
use agent_runtime::interagent::parse_agent_input_message;
use pulldown_cmark::{
    Event as MarkdownEvent, Options as MarkdownOptions, Parser as MarkdownParser, Tag, TagEnd,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use std::time::{SystemTime, UNIX_EPOCH};
use time::macros::format_description;
use time::{Date, OffsetDateTime};
use unicode_width::UnicodeWidthStr;

const MAX_SESSION_HEADER_HEIGHT: u16 = 7;
const MAX_COMPOSER_INPUT_LINES: u16 = 6;
const MAX_ACTIVE_DETAIL_CHARS: usize = 180;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatViewportDebug {
    pub terminal_width: u16,
    pub terminal_height: u16,
    pub composer_height: u16,
    pub timeline_viewport_width: usize,
    pub timeline_viewport_height: usize,
    pub timeline_total_lines: usize,
    pub timeline_scroll_top: u16,
    pub scroll_offset: u16,
    pub reasoning_visible: bool,
    pub visible_entry_count: usize,
    pub total_entry_count: usize,
}

pub fn render(frame: &mut Frame<'_>, state: &TuiAppState) {
    match state.active_screen() {
        TuiScreen::Sessions => render_session_screen(frame, state),
        TuiScreen::Chat => render_chat_screen(frame, state),
        TuiScreen::Agents | TuiScreen::Schedules | TuiScreen::Artifacts => {
            render_inspector_screen(frame, state)
        }
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
                " | апрув"
            } else {
                ""
            };
            let preview = session.last_message_preview.as_deref().unwrap_or("<пусто>");
            let label = format!(
                "{prefix}{} | агент={} | обновлено={} | сообщений={}{}",
                session.title,
                session.agent_name,
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
            .title(format!(
                "Сессии | {} | Enter открыть | Н новая | У удалить | П переименовать | А агенты | Р расписания | Esc назад",
                short_version_label()
            ))
            .borders(Borders::ALL),
    );
    frame.render_widget(list, area);
}

fn render_inspector_screen(frame: &mut Frame<'_>, state: &TuiAppState) {
    let area = frame.area();
    let title = state
        .active_inspector_title()
        .unwrap_or(match state.active_screen() {
            TuiScreen::Agents => "Агенты",
            TuiScreen::Schedules => "Расписания",
            TuiScreen::Artifacts => "Артефакты",
            _ => "Просмотр",
        });
    let content = state.active_inspector_content().unwrap_or("<пусто>");
    let widget = Paragraph::new(content.to_string())
        .block(
            Block::default()
                .title(format!("{title} | {} | Esc назад", short_version_label()))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_chat_screen(frame: &mut Frame<'_>, state: &TuiAppState) {
    let area = frame.area();
    let now = unix_timestamp();
    let top_lines = session_header_lines(state, now);
    let session_header_height = wrapped_block_height(
        &top_lines,
        inner_block_width(area.width),
        MAX_SESSION_HEADER_HEIGHT,
    );
    let composer_height = composer_height(state, area.width);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(session_header_height),
            Constraint::Min(1),
            Constraint::Length(composer_height),
        ])
        .split(area);

    let top = Paragraph::new(top_lines)
        .block(Block::default().borders(Borders::ALL).title("Сессия"))
        .wrap(Wrap { trim: false });

    let reasoning_visible = state
        .current_session_summary()
        .map(|summary| summary.reasoning_visible)
        .unwrap_or(true);
    let timeline_entries = state.timeline().entries(reasoning_visible);
    let timeline_lines = if timeline_entries.is_empty() {
        render_empty_chat_lines()
    } else {
        timeline_entries
            .into_iter()
            .flat_map(|entry| render_timeline_entry(entry, now))
            .collect::<Vec<_>>()
    };
    let timeline_viewport_height = usize::from(chunks[1].height.saturating_sub(2));
    let timeline_viewport_width = usize::from(chunks[1].width.saturating_sub(2)).max(1);
    let timeline_total_lines = paragraph_line_count(&timeline_lines, timeline_viewport_width);
    let timeline_scroll_top = chat_scroll_top(
        timeline_total_lines,
        timeline_viewport_height,
        state.scroll_offset(),
    );
    let timeline = Paragraph::new(timeline_lines)
        .block(Block::default().title("Чат").borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((timeline_scroll_top, 0));

    let composer_block = Block::default().title("Ввод").borders(Borders::ALL);
    let composer_inner = composer_block.inner(chunks[2]);
    let composer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(composer_inner);
    let composer_lines = render_composer_lines(state);
    let composer_scroll_top = composer_scroll_top(
        &composer_lines,
        composer_chunks[0].width,
        composer_chunks[0].height,
        state,
    );
    let input = Paragraph::new(composer_lines)
        .wrap(Wrap { trim: false })
        .scroll((composer_scroll_top, 0));
    let hint = Paragraph::new(Line::from(format!(
        "Enter=отправить после шага инструмента | Tab=в очередь после полного хода | Shift+Tab=перебор команд | \\процессы | \\пауза | \\стоп | {}",
        describe_run_status(state, now)
    )));

    frame.render_widget(top, chunks[0]);
    frame.render_widget(timeline, chunks[1]);
    frame.render_widget(composer_block, chunks[2]);
    frame.render_widget(input, composer_chunks[0]);
    frame.render_widget(hint, composer_chunks[1]);
}

pub fn chat_viewport_debug(state: &TuiAppState, area: Rect) -> Option<ChatViewportDebug> {
    if state.active_screen() != TuiScreen::Chat {
        return None;
    }

    let top_lines = session_header_lines(state, unix_timestamp());
    let session_header_height = wrapped_block_height(
        &top_lines,
        inner_block_width(area.width),
        MAX_SESSION_HEADER_HEIGHT,
    );
    let composer_height = composer_height(state, area.width);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(session_header_height),
            Constraint::Min(1),
            Constraint::Length(composer_height),
        ])
        .split(area);
    let reasoning_visible = state
        .current_session_summary()
        .map(|summary| summary.reasoning_visible)
        .unwrap_or(true);
    let timeline_entries = state.timeline().entries(reasoning_visible);
    let timeline_lines = timeline_entries
        .iter()
        .flat_map(|entry| render_timeline_entry(entry, unix_timestamp()))
        .collect::<Vec<_>>();
    let timeline_viewport_height = usize::from(chunks[1].height.saturating_sub(2));
    let timeline_viewport_width = usize::from(chunks[1].width.saturating_sub(2)).max(1);
    let timeline_total_lines = paragraph_line_count(&timeline_lines, timeline_viewport_width);
    let timeline_scroll_top = chat_scroll_top(
        timeline_total_lines,
        timeline_viewport_height,
        state.scroll_offset(),
    );

    Some(ChatViewportDebug {
        terminal_width: area.width,
        terminal_height: area.height,
        composer_height,
        timeline_viewport_width,
        timeline_viewport_height,
        timeline_total_lines,
        timeline_scroll_top,
        scroll_offset: state.scroll_offset(),
        reasoning_visible,
        visible_entry_count: timeline_entries.len(),
        total_entry_count: state.timeline().entries(true).len(),
    })
}

fn render_empty_chat_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(format!("{}.", short_version_label())),
        Line::from("Новая сессия. Введите задачу обычным текстом или используйте команды ниже."),
        Line::from(""),
        Line::from(QUICK_HELP_LINE),
    ]
}

fn session_header_lines(state: &TuiAppState, now: i64) -> Vec<Line<'static>> {
    if let Some(summary) = state.current_session_summary() {
        let mut lines = vec![
            Line::from(format!(
                "{} | версия={} | агент={} | модель={} | размышления={} | думай={} | доводка={} | апрув={} | usage={} | сжатия={} | сообщений={} | фон={} (выполняется={} в очереди={})",
                summary.title,
                APP_VERSION,
                summary.agent_name,
                summary.model.as_deref().unwrap_or("<по умолчанию>"),
                if summary.reasoning_visible {
                    "вкл"
                } else {
                    "выкл"
                },
                summary.think_level.as_deref().unwrap_or("<по умолчанию>"),
                summary
                    .completion_nudges
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "выкл".to_string()),
                if summary.auto_approve {
                    "авто"
                } else {
                    "ручной"
                },
                format_usage(summary),
                summary.compactifications,
                summary.message_count.max(state.timeline().message_count()),
                summary.background_job_count,
                summary.running_background_job_count,
                summary.queued_background_job_count,
            )),
            Line::from(format!(
                "выполнение={}{} | очередь={} (срочно={} отложено={})",
                describe_run_status(state, now),
                format_provider_loop_progress(state),
                state.queued_draft_count(),
                state.queued_priority_count(),
                state.queued_deferred_count()
            )),
        ];
        if let Some(detail) = active_run_detail_line(state) {
            lines.push(Line::from(detail));
        }
        lines
    } else {
        vec![Line::from("Нет активной сессии")]
    }
}

fn render_composer_lines(state: &TuiAppState) -> Vec<Line<'static>> {
    build_composer_lines(state.input_buffer(), state.input_cursor(), true)
}

fn build_composer_lines(
    input: &str,
    cursor: usize,
    include_after_cursor: bool,
) -> Vec<Line<'static>> {
    let cursor = cursor.min(input.len());
    let (before, rest) = input.split_at(cursor);
    let mut rest_chars = rest.chars();
    let cursor_char = rest_chars.next();
    let after = if include_after_cursor {
        rest_chars.as_str()
    } else {
        ""
    };
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
    let rendered_content = match &entry.kind {
        TimelineEntryKind::User => parse_agent_input_message(&entry.content)
            .map(|(_, body)| body.to_string())
            .unwrap_or_else(|| entry.content.clone()),
        _ => entry.content.clone(),
    };
    let label = match &entry.kind {
        TimelineEntryKind::User => parse_agent_input_message(&entry.content)
            .map(|(agent_name, _)| format!("агент: {agent_name}"))
            .unwrap_or_else(|| "пользователь".to_string()),
        TimelineEntryKind::Assistant => "ассистент".to_string(),
        TimelineEntryKind::Reasoning => "размышления".to_string(),
        TimelineEntryKind::Tool {
            tool_name, status, ..
        } => format!(
            "инструмент: {tool_name} | {}",
            translate_tool_status(status)
        ),
        TimelineEntryKind::Approval { .. } => "апрув".to_string(),
        TimelineEntryKind::System => "система".to_string(),
    };
    let prefix = format!("[{timestamp}] {label}: ");
    let continuation_prefix = " ".repeat(prefix.len());
    match &entry.kind {
        TimelineEntryKind::Assistant | TimelineEntryKind::Reasoning => {
            render_markdown_entry(prefix.as_str(), &entry.content)
        }
        TimelineEntryKind::Tool { .. } => render_tool_entry(
            prefix.as_str(),
            continuation_prefix.as_str(),
            &entry.content,
        ),
        TimelineEntryKind::Approval { approval_id } => render_approval_entry(
            prefix.as_str(),
            continuation_prefix.as_str(),
            approval_id.as_str(),
            &entry.content,
        ),
        _ => render_plain_entry(
            prefix.as_str(),
            continuation_prefix.as_str(),
            &rendered_content,
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

fn render_markdown_entry(prefix: &str, content: &str) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(prefix.to_string())];
    if content.trim().is_empty() {
        return lines;
    }

    let raw_lines = content.lines().collect::<Vec<_>>();
    let mut index = 0;
    while index < raw_lines.len() {
        let raw_line = raw_lines[index];
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            lines.push(Line::default());
            index += 1;
            continue;
        }

        if let Some((code_lines, next_index)) = collect_code_block(&raw_lines, index) {
            lines.extend(render_code_block(&code_lines));
            index = next_index;
            continue;
        }

        if let Some((table_rows, next_index)) = collect_table_block(&raw_lines, index) {
            lines.extend(render_table_block(&table_rows));
            index = next_index;
            continue;
        }

        if let Some(heading) = trimmed.strip_prefix("### ") {
            lines.push(render_inline_markdown_line(
                heading,
                Style::default().add_modifier(Modifier::BOLD),
            ));
            index += 1;
            continue;
        }
        if let Some(heading) = trimmed
            .strip_prefix("## ")
            .or_else(|| trimmed.strip_prefix("# "))
        {
            lines.push(render_inline_markdown_line(
                heading,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
            index += 1;
            continue;
        }
        if let Some(quote) = trimmed.strip_prefix("> ") {
            let mut spans = vec![Span::styled("> ", Style::default().fg(Color::Blue))];
            spans.extend(render_inline_markdown_spans(
                quote,
                Style::default().add_modifier(Modifier::ITALIC),
            ));
            lines.push(Line::from(spans));
            index += 1;
            continue;
        }
        if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            let mut spans = vec![Span::styled("• ", Style::default().fg(Color::Cyan))];
            spans.extend(render_inline_markdown_spans(item, Style::default()));
            lines.push(Line::from(spans));
            index += 1;
            continue;
        }
        if let Some((ordinal, item)) = parse_ordered_list_item(trimmed) {
            let mut spans = vec![Span::styled(
                format!("{ordinal}. "),
                Style::default().fg(Color::Cyan),
            )];
            spans.extend(render_inline_markdown_spans(item, Style::default()));
            lines.push(Line::from(spans));
            index += 1;
            continue;
        }

        lines.push(render_inline_markdown_line(trimmed, Style::default()));
        index += 1;
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

fn render_approval_entry(
    prefix: &str,
    continuation_prefix: &str,
    approval_id: &str,
    reason: &str,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(format!("{prefix}ожидает решения"))];
    lines.push(Line::from(vec![
        Span::raw(continuation_prefix.to_string()),
        Span::styled("  -> ", Style::default().fg(Color::Yellow)),
        Span::styled(
            format!("id={approval_id}"),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ]));
    for raw_line in reason.lines() {
        lines.push(Line::from(vec![
            Span::raw(continuation_prefix.to_string()),
            Span::styled("  -> ", Style::default().fg(Color::Yellow)),
            Span::raw(raw_line.to_string()),
        ]));
    }
    lines.push(Line::from(vec![
        Span::raw(continuation_prefix.to_string()),
        Span::styled("  -> ", Style::default().fg(Color::Yellow)),
        Span::raw("\\апрув чтобы продолжить, \\автоапрув вкл для автоапрува".to_string()),
    ]));
    lines
}

fn translate_tool_status(status: &str) -> &str {
    match status {
        "completed" => "завершён",
        "failed" => "ошибка",
        "running" => "выполняется",
        "approved" => "подтверждён",
        "requested" => "запрошен",
        "waiting" => "ожидание",
        other => other,
    }
}

fn paragraph_line_count(lines: &[Line<'_>], width: usize) -> usize {
    if width == 0 {
        return lines.len();
    }

    Paragraph::new(lines.to_vec())
        .wrap(Wrap { trim: false })
        .line_count(width as u16)
}

fn collect_code_block(lines: &[&str], start: usize) -> Option<(Vec<String>, usize)> {
    if !lines.get(start)?.trim_start().starts_with("```") {
        return None;
    }

    let mut code_lines = Vec::new();
    let mut index = start + 1;
    while index < lines.len() {
        let raw_line = lines[index];
        if raw_line.trim_start().starts_with("```") {
            return Some((code_lines, index + 1));
        }
        code_lines.push(raw_line.to_string());
        index += 1;
    }

    Some((code_lines, index))
}

fn collect_table_block(lines: &[&str], start: usize) -> Option<(Vec<Vec<String>>, usize)> {
    let header = lines.get(start)?.trim();
    let separator = lines.get(start + 1)?.trim();
    if !looks_like_table_row(header) || !looks_like_table_separator(separator) {
        return None;
    }

    let mut rows = vec![parse_table_cells(header)];
    let mut index = start + 2;
    while index < lines.len() {
        let row = lines[index].trim();
        if !looks_like_table_row(row) {
            break;
        }
        rows.push(parse_table_cells(row));
        index += 1;
    }

    Some((rows, index))
}

fn looks_like_table_row(line: &str) -> bool {
    line.starts_with('|') && line.ends_with('|') && line.matches('|').count() >= 2
}

fn looks_like_table_separator(line: &str) -> bool {
    if !looks_like_table_row(line) {
        return false;
    }
    line.trim_matches('|')
        .split('|')
        .map(str::trim)
        .all(|cell| !cell.is_empty() && cell.chars().all(|ch| matches!(ch, '-' | ':' | ' ')))
}

fn parse_table_cells(line: &str) -> Vec<String> {
    line.trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn render_code_block(code_lines: &[String]) -> Vec<Line<'static>> {
    if code_lines.is_empty() {
        return vec![Line::from(Span::styled(
            "    ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::DIM),
        ))];
    }

    code_lines
        .iter()
        .map(|line| {
            Line::from(vec![Span::styled(
                format!("    {line}"),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::DIM),
            )])
        })
        .collect()
}

fn render_table_block(rows: &[Vec<String>]) -> Vec<Line<'static>> {
    if rows.is_empty() {
        return Vec::new();
    }

    let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let normalized = rows
        .iter()
        .map(|row| {
            (0..column_count)
                .map(|column| {
                    row.get(column)
                        .map(|cell| inline_markdown_plain_text(cell))
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let mut widths = vec![0; column_count];
    for row in &normalized {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }

    let mut lines = Vec::new();
    for (index, row) in normalized.iter().enumerate() {
        let style = if index == 0 {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![Span::styled(
            format_table_row(row, &widths),
            style,
        )]));
        if index == 0 {
            lines.push(Line::from(vec![Span::styled(
                format_table_separator(&widths),
                Style::default().fg(Color::DarkGray),
            )]));
        }
    }
    lines
}

fn format_table_row(row: &[String], widths: &[usize]) -> String {
    let cells = row
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let width = widths.get(index).copied().unwrap_or_default();
            format!(" {}{} ", cell, pad_display_width(cell, width))
        })
        .collect::<Vec<_>>();
    format!("|{}|", cells.join("|"))
}

fn format_table_separator(widths: &[usize]) -> String {
    let cells = widths
        .iter()
        .map(|width| format!(" {} ", "-".repeat((*width).max(1))))
        .collect::<Vec<_>>();
    format!("|{}|", cells.join("|"))
}

fn pad_display_width(text: &str, width: usize) -> String {
    let current = UnicodeWidthStr::width(text);
    " ".repeat(width.saturating_sub(current))
}

fn parse_ordered_list_item(line: &str) -> Option<(usize, &str)> {
    let digit_count = line.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digit_count == 0 {
        return None;
    }
    let (digits, rest) = line.split_at(digit_count);
    let item = rest.strip_prefix(". ")?;
    Some((digits.parse().ok()?, item))
}

fn render_inline_markdown_line(content: &str, base_style: Style) -> Line<'static> {
    Line::from(render_inline_markdown_spans(content, base_style))
}

fn render_inline_markdown_spans(content: &str, base_style: Style) -> Vec<Span<'static>> {
    #[derive(Debug)]
    struct LinkState {
        destination: String,
        rendered_text: String,
    }

    let mut spans = Vec::new();
    let mut style_stack = vec![base_style];
    let mut link_stack: Vec<LinkState> = Vec::new();
    let parser = MarkdownParser::new_ext(content, MarkdownOptions::ENABLE_STRIKETHROUGH);

    for event in parser {
        match event {
            MarkdownEvent::Start(tag) => match tag {
                Tag::Emphasis => style_stack.push(Style::default().add_modifier(Modifier::ITALIC)),
                Tag::Strong => style_stack.push(Style::default().add_modifier(Modifier::BOLD)),
                Tag::Strikethrough => {
                    style_stack.push(Style::default().add_modifier(Modifier::CROSSED_OUT))
                }
                Tag::Link { dest_url, .. } => link_stack.push(LinkState {
                    destination: dest_url.to_string(),
                    rendered_text: String::new(),
                }),
                _ => {}
            },
            MarkdownEvent::End(tag_end) => match tag_end {
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough
                    if style_stack.len() > 1 =>
                {
                    style_stack.pop();
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {}
                TagEnd::Link => {
                    if let Some(link) = link_stack.pop()
                        && !link.destination.is_empty()
                        && link.rendered_text != link.destination
                    {
                        append_span(
                            &mut spans,
                            format!(" <{}>", link.destination),
                            merged_markdown_style(&style_stack)
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::UNDERLINED),
                        );
                    }
                }
                _ => {}
            },
            MarkdownEvent::Text(text) => {
                let style = markdown_text_style(&style_stack, !link_stack.is_empty());
                append_span(&mut spans, text.to_string(), style);
                if let Some(link) = link_stack.last_mut() {
                    link.rendered_text.push_str(text.as_ref());
                }
            }
            MarkdownEvent::Code(text) => {
                let rendered = text.to_string();
                append_span(
                    &mut spans,
                    rendered.clone(),
                    markdown_text_style(&style_stack, false)
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                );
                if let Some(link) = link_stack.last_mut() {
                    link.rendered_text.push_str(rendered.as_str());
                }
            }
            MarkdownEvent::SoftBreak | MarkdownEvent::HardBreak => {
                append_span(
                    &mut spans,
                    " ".to_string(),
                    markdown_text_style(&style_stack, !link_stack.is_empty()),
                );
                if let Some(link) = link_stack.last_mut() {
                    link.rendered_text.push(' ');
                }
            }
            MarkdownEvent::InlineHtml(text) | MarkdownEvent::Html(text) => {
                append_span(
                    &mut spans,
                    text.to_string(),
                    markdown_text_style(&style_stack, !link_stack.is_empty())
                        .add_modifier(Modifier::DIM),
                );
                if let Some(link) = link_stack.last_mut() {
                    link.rendered_text.push_str(text.as_ref());
                }
            }
            MarkdownEvent::Rule => append_span(
                &mut spans,
                "────────".to_string(),
                markdown_text_style(&style_stack, !link_stack.is_empty()).fg(Color::DarkGray),
            ),
            MarkdownEvent::TaskListMarker(checked) => {
                let marker = if checked { "☑ " } else { "☐ " };
                append_span(
                    &mut spans,
                    marker.to_string(),
                    markdown_text_style(&style_stack, !link_stack.is_empty()).fg(Color::Cyan),
                );
                if let Some(link) = link_stack.last_mut() {
                    link.rendered_text.push_str(marker);
                }
            }
            MarkdownEvent::FootnoteReference(name) => {
                append_span(
                    &mut spans,
                    format!("[{name}]"),
                    markdown_text_style(&style_stack, !link_stack.is_empty()).fg(Color::Yellow),
                );
                if let Some(link) = link_stack.last_mut() {
                    link.rendered_text.push('[');
                    link.rendered_text.push_str(name.as_ref());
                    link.rendered_text.push(']');
                }
            }
            MarkdownEvent::InlineMath(text) | MarkdownEvent::DisplayMath(text) => {
                let rendered = format!("${text}$");
                append_span(
                    &mut spans,
                    rendered.clone(),
                    markdown_text_style(&style_stack, !link_stack.is_empty())
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::ITALIC),
                );
                if let Some(link) = link_stack.last_mut() {
                    link.rendered_text.push_str(rendered.as_str());
                }
            }
        }
    }

    if spans.is_empty() {
        spans.push(Span::styled(String::new(), base_style));
    }
    spans
}

fn merged_markdown_style(style_stack: &[Style]) -> Style {
    style_stack
        .iter()
        .copied()
        .fold(Style::default(), Style::patch)
}

fn markdown_text_style(style_stack: &[Style], in_link: bool) -> Style {
    let style = merged_markdown_style(style_stack);
    if in_link {
        style.fg(Color::Cyan).add_modifier(Modifier::UNDERLINED)
    } else {
        style
    }
}

fn append_span(spans: &mut Vec<Span<'static>>, text: String, style: Style) {
    if text.is_empty() {
        return;
    }
    spans.push(Span::styled(text, style));
}

fn inline_markdown_plain_text(content: &str) -> String {
    render_inline_markdown_spans(content, Style::default())
        .into_iter()
        .map(|span| span.content.into_owned())
        .collect::<String>()
}

fn describe_run_status(state: &TuiAppState, now: i64) -> String {
    if let Some(active_run) = state.active_run() {
        let kind = match active_run.kind() {
            ActiveRunKind::Chat => "чат",
            ActiveRunKind::Approval => "апрув",
        };
        let phase = match active_run.phase() {
            ActiveRunPhase::Sending => "отправка".to_string(),
            ActiveRunPhase::Streaming => "поток".to_string(),
            ActiveRunPhase::WaitingApproval => "ожидает апрув".to_string(),
            ActiveRunPhase::ToolRequested { tool_name, summary } => {
                format!("запрошен инструмент ({tool_name}: {summary})")
            }
            ActiveRunPhase::ToolRunning { tool_name, summary } => {
                format!("инструмент выполняется ({tool_name}: {summary})")
            }
            ActiveRunPhase::ToolCompleted { tool_name, summary } => {
                format!("инструмент завершён ({tool_name}: {summary})")
            }
            ActiveRunPhase::Failed => "ошибка".to_string(),
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
        return "ожидает апрув".to_string();
    }
    "простой".to_string()
}

fn format_provider_loop_progress(state: &TuiAppState) -> String {
    state
        .provider_loop_progress()
        .map(|(current_round, max_rounds)| format!(" | инструменты={current_round}/{max_rounds}"))
        .unwrap_or_default()
}

fn active_run_detail_line(state: &TuiAppState) -> Option<String> {
    let active_run = state.active_run()?;
    let mut details = Vec::new();
    if let Some(summary) = active_run.current_tool_summary()
        && !summary.trim().is_empty()
    {
        details.push(format!(
            "текущий_exec={}",
            truncate_display_text(summary, MAX_ACTIVE_DETAIL_CHARS)
        ));
    }
    if active_run.interrupt_after_tool_step_requested() {
        details.push("остановка_после_шага=запрошена".to_string());
    }
    if details.is_empty() {
        None
    } else {
        Some(details.join(" | "))
    }
}

fn composer_height(state: &TuiAppState, width: u16) -> u16 {
    let input_lines = render_composer_lines(state);
    let input_width = inner_block_width(width);
    let wrapped_input_lines = paragraph_line_count(&input_lines, input_width);
    let input_visible_lines = (wrapped_input_lines as u16).clamp(1, MAX_COMPOSER_INPUT_LINES);
    (input_visible_lines + 3).clamp(4, MAX_COMPOSER_INPUT_LINES + 3)
}

fn composer_scroll_top(lines: &[Line<'_>], width: u16, height: u16, state: &TuiAppState) -> u16 {
    if height == 0 {
        return 0;
    }
    let viewport_lines = usize::from(height);
    let width = usize::from(width).max(1);
    let total_lines = paragraph_line_count(lines, width);
    let cursor_lines = paragraph_line_count(
        &build_composer_lines(state.input_buffer(), state.input_cursor(), false),
        width,
    )
    .max(1);
    let cursor_line = cursor_lines.saturating_sub(1);
    let max_top = total_lines.saturating_sub(viewport_lines);
    cursor_line
        .saturating_add(1)
        .saturating_sub(viewport_lines)
        .min(max_top) as u16
}

fn wrapped_block_height(lines: &[Line<'_>], inner_width: usize, max_height: u16) -> u16 {
    (paragraph_line_count(lines, inner_width) as u16 + 2).clamp(3, max_height)
}

fn inner_block_width(width: u16) -> usize {
    usize::from(width.saturating_sub(2)).max(1)
}

fn truncate_display_text(text: &str, max_chars: usize) -> String {
    let normalized = text.replace('\n', " ↩ ");
    let mut chars = normalized.chars();
    let preview = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn render_dialog(frame: &mut Frame<'_>, dialog: DialogState) {
    let area = centered_rect(frame.area(), 60, 20);
    frame.render_widget(Clear, area);
    let content = match dialog {
        DialogState::CreateSession { value } => {
            format!("Новая сессия\n\n{value}\n\nEnter подтвердить, Esc отмена")
        }
        DialogState::RenameSession { value, .. } => {
            format!("Переименовать сессию\n\n{value}\n\nEnter подтвердить, Esc отмена")
        }
        DialogState::ConfirmDelete { session_id } => {
            format!("Удалить сессию {session_id}?\n\nEnter подтвердить, Esc отмена")
        }
        DialogState::ConfirmClear { session_id } => {
            format!("Очистить сессию {session_id}?\n\nEnter подтвердить, Esc отмена")
        }
    };
    let paragraph = Paragraph::new(content)
        .block(Block::default().title("Диалог").borders(Borders::ALL))
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

fn format_usage(summary: &SessionSummary) -> String {
    match (
        summary.usage_input_tokens,
        summary.usage_output_tokens,
        summary.usage_total_tokens,
    ) {
        (Some(input), Some(output), Some(total)) => format!("{input}/{output}/{total}"),
        _ => format!("~{}", summary.context_tokens),
    }
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
    use crate::bootstrap::SessionSummary;
    use crate::tui::app::TuiAppState;
    use crate::tui::timeline::Timeline;
    use crate::tui::timeline::{TimelineEntry, TimelineEntryKind};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Modifier;

    #[test]
    fn timestamps_render_in_human_readable_form_for_same_day_entries() {
        let formatted = format_timestamp(1_775_200_010, 1_775_200_099);
        assert_eq!(formatted.len(), 8);
        assert!(formatted.contains(':'));
    }

    #[test]
    fn markdown_renderer_formats_headings_lists_code_links_and_tables() {
        let lines = render_markdown_entry(
            "[12:00:00] assistant: ",
            "# Heading\n- item one\nSee **docs** at [site](https://example.com)\n| A | B |\n| --- | --- |\n| x | y |\n```rust\nfn main() {}\n```",
        );
        let rendered = lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("Heading"));
        assert!(rendered.contains("• item one"));
        assert!(rendered.contains("site <https://example.com>"));
        assert!(rendered.contains("| A | B |"));
        assert!(rendered.contains("fn main() {}"));
    }

    #[test]
    fn markdown_renderer_keeps_assistant_content_left_aligned() {
        let lines = render_markdown_entry("[12:00:00] ассистент: ", "Первая строка\nВторая строка");
        assert_eq!(lines[0].to_string(), "[12:00:00] ассистент: ");
        assert_eq!(lines[1].to_string(), "Первая строка");
        assert_eq!(lines[2].to_string(), "Вторая строка");
    }

    #[test]
    fn markdown_renderer_applies_emphasis_styles() {
        let lines = render_markdown_entry("[12:00:00] ассистент: ", "**жирный** и *курсив*");
        let content_line = &lines[1];
        assert!(
            content_line
                .spans
                .iter()
                .any(|span| span.style.add_modifier.contains(Modifier::BOLD))
        );
        assert!(
            content_line
                .spans
                .iter()
                .any(|span| span.style.add_modifier.contains(Modifier::ITALIC))
        );
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
        assert!(rendered.contains("инструмент: web_fetch | завершён:"));
        assert!(rendered.contains("-> web_fetch url=https://example.com/doc"));
    }

    #[test]
    fn agent_origin_user_messages_render_with_agent_label() {
        let lines = render_timeline_entry(
            &TimelineEntry {
                timestamp: 1_775_200_010,
                kind: TimelineEntryKind::User,
                content: "[agent:judge]\nКритических замечаний нет.".to_string(),
            },
            1_775_200_099,
        );
        let rendered = lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("агент: judge:"));
        assert!(rendered.contains("Критических замечаний нет."));
        assert!(!rendered.contains("[agent:judge]"));
    }

    #[test]
    fn session_header_wraps_instead_of_truncating_on_narrow_viewports() {
        let summary = SessionSummary {
            id: "session-wrap".to_string(),
            title: "очень длинная сессия".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: true,
            context_tokens: 5990,
            usage_input_tokens: Some(5990),
            usage_output_tokens: Some(15),
            usage_total_tokens: Some(6005),
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 22,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 1,
        };
        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, Timeline::default());

        let backend = TestBackend::new(78, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, &state))
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                rendered.push_str(buffer[(x, y)].symbol());
            }
            rendered.push('\n');
        }

        assert!(rendered.contains("агент=Default"));
        assert!(rendered.contains("usage=5990/15/6005"));
        assert!(rendered.contains("фон=0"));
        assert!(rendered.contains("выполняется=0"));
    }

    #[test]
    fn empty_chat_renders_quick_help_for_a_new_session() {
        let summary = SessionSummary {
            id: "session-empty".to_string(),
            title: "новая".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: true,
            context_tokens: 0,
            usage_input_tokens: None,
            usage_output_tokens: None,
            usage_total_tokens: None,
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 0,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 1,
        };
        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, Timeline::default());

        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, &state))
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                rendered.push_str(buffer[(x, y)].symbol());
            }
            rendered.push('\n');
        }

        assert!(rendered.contains("Новая сессия."));
        assert!(rendered.contains("\\процессы"));
    }

    #[test]
    fn composer_wraps_long_input_and_keeps_the_tail_visible() {
        let summary = SessionSummary {
            id: "session-compose".to_string(),
            title: "compose".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: true,
            context_tokens: 0,
            usage_input_tokens: None,
            usage_output_tokens: None,
            usage_total_tokens: None,
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 0,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 1,
        };
        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, Timeline::default());
        state.replace_input_buffer(
            "очень длинная строка для поля ввода которая раньше терялась справа и должна остаться видимой tail-marker".to_string(),
        );

        let backend = TestBackend::new(60, 16);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, &state))
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                rendered.push_str(buffer[(x, y)].symbol());
            }
            rendered.push('\n');
        }

        assert!(
            rendered.contains("tail-marker"),
            "wrapped composer lost the input tail\nrendered=\n{rendered}"
        );
    }

    #[test]
    fn composer_renders_multiline_input_inside_the_visible_viewport() {
        let summary = SessionSummary {
            id: "session-compose-multiline".to_string(),
            title: "compose".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: true,
            context_tokens: 0,
            usage_input_tokens: None,
            usage_output_tokens: None,
            usage_total_tokens: None,
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 0,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 1,
        };
        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, Timeline::default());
        state.replace_input_buffer("первая строка\nвторая строка\nтретья строка".to_string());

        let backend = TestBackend::new(70, 16);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, &state))
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                rendered.push_str(buffer[(x, y)].symbol());
            }
            rendered.push('\n');
        }

        assert!(rendered.contains("вторая строка"));
        assert!(rendered.contains("третья строка"));
    }

    #[test]
    fn chat_render_keeps_the_tail_visible_for_long_markdown_dialogs() {
        let summary = SessionSummary {
            id: "session-ansible".to_string(),
            title: "ансибл".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: true,
            context_tokens: 6505,
            usage_input_tokens: Some(6505),
            usage_output_tokens: Some(600),
            usage_total_tokens: Some(7105),
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 8,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 1,
        };
        let mut timeline = Timeline::default();
        timeline.push_user("установи ансибл в виртуальное окружение", 1);
        timeline.push_assistant(
            "Ansible установлен в виртуальное окружение. Вот итог:\n\n| Что | Результат |\n|---|---|\n| **Python** | 3.12.3 |\n| **Ansible** | [core 2.20.5] + ansible 13.5.0 |\n| **Место установки** | `venv/` в рабочей директории |\n| **Исполняемый** | `venv/bin/ansible` |\n\n**Как использовать:**\n\n```bash\nsource venv/bin/activate\nansible --version\nansible-playbook playbook.yml\ndeactivate\n```\n\nИли без активации — напрямую через путь: `venv/bin/ansible`.",
            2,
        );
        timeline.push_user("хорошо, сделай конфигурацию для ансибла, которая - не проверяет отпечатки хостов, использует по умолчанию логин administrator и пароль axelastra9, может что то еще, что посчитаешь нужным", 3);
        timeline.push_assistant(
            "Готово. Создано два файла:\n\n### `ansible.cfg`\n| Параметр | Значение |\n|---|---|\n| `host_key_checking` | `False` — отпечатки не проверяются |\n| `remote_user` | `administrator` |\n| `forks` | 10 одновременных хостов |\n| `timeout` | 30 сек |\n| `log_path` | `ansible.log` |\n| `stdout_callback` | `yaml` — читаемый вывод |\n| `pipelining` | `True` — ускоряет выполнение модулей |\n| `ssh_args` | отключает `StrictHostKeyChecking` и `/dev/null` для known_hosts |\n\n### `inventory`\nСодержит переменные группы `all:vars` с паролем `ansible_password=axelastra9`.\n\n```bash\nsource venv/bin/activate\nansible all -m ping\nansible all -m setup\nansible-playbook playbook.yml\n```",
            4,
        );
        timeline.push_user("ну для sudo надо так же, пароль тот же", 5);
        timeline.push_assistant(
            "Готово. Добавлены в `inventory`:\n\n| Параметр | Значение |\n|---|---|\n| `ansible_become` | `true` — elevate privileges |\n| `ansible_become_method` | `sudo` |\n| `ansible_become_user` | `root` |\n| `ansible_become_password` | `axelastra9` |\n\nТеперь `become` включён по умолчанию для всех хостов — sudo будет работать автоматически без `--ask-become-pass`.",
            6,
        );
        timeline.push_user("ну?", 7);
        timeline.push_assistant(
            "Всё уже на месте. Итоговая конфигурация:\n\n**`ansible.cfg`** — общие настройки (host_key_checking=false, pipelining, yaml-вывод, логи)\n\n**`inventory`** — credentials:\n| Переменная | Значение |\n|---|---|\n| `ansible_user` | `administrator` |\n| `ansible_password` | `axelastra9` |\n| `ansible_become` | `true` |\n| `ansible_become_method` | `sudo` |\n| `ansible_become_user` | `root` |\n| `ansible_become_password` | `axelastra9` |\n\nМожно работать. Например:\n\n```bash\nsource venv/bin/activate\nansible all -i inventory -m ping\nansible all -i inventory -m command -a \"whoami\"\n```\n\nЧто дальше делаем?",
            8,
        );

        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, timeline);

        let backend = TestBackend::new(280, 38);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, &state))
            .expect("draw");
        let viewport =
            super::chat_viewport_debug(&state, ratatui::layout::Rect::new(0, 0, 280, 38))
                .expect("viewport debug");

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                rendered.push_str(buffer[(x, y)].symbol());
            }
            rendered.push('\n');
        }

        assert!(rendered.contains("ansible_become_password"));
        assert!(
            rendered.contains("Что дальше делаем?"),
            "tail missing from rendered chat\nviewport={viewport:?}\nrendered=\n{rendered}"
        );
    }

    #[test]
    fn chat_render_keeps_the_tail_visible_for_narrow_viewports_with_many_entries() {
        let summary = SessionSummary {
            id: "session-govc".to_string(),
            title: "ансибл".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: true,
            context_tokens: 7102,
            usage_input_tokens: Some(7102),
            usage_output_tokens: Some(742),
            usage_total_tokens: Some(7844),
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 16,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 1,
        };
        let mut timeline = Timeline::default();
        timeline.push_user("у нас есть скилл ансибл?", 1);
        timeline.push_assistant("Нет, скилла `skills/ansible/` не существует. Есть много других (bitwarden, confluence, vsphere-govc, timeweb и т.д.), но ansible-скилла в репозитории нет.\n\nХочешь, чтобы я его создал? По аналогии с другими скиллами — структура `skills/ansible/SKILL.md` с описанием установки, конфигурации и типичных команд.", 2);
        timeline.push_user("хорошо, посмотри скилл govc", 3);
        timeline.push_assistant("Вот структура скилла `skills/vsphere-govc/`:\n\n```\nskills/vsphere-govc/\n├── SKILL.md                          # Описание скилла (триггеры, команды, окружение)\n├── data/\n│   └── .env                          # Credentials: GOVC_URL, GOVC_USERNAME, GOVC_PASSWORD + Ansible vars\n├── scripts/\n│   ├── vm-list.sh                    # Список ВМ с IP и power state\n│   ├── gen-inventory.sh              # Генерация Ansible inventory из vSphere\n│   └── snapshot-all.sh               # Массовое создание снапшотов\n└── references/\n    ├── govc-guide.md                 # Полный гайд по govc\n    └── govc-quick-ref.md             # Быстрая справка\n```\n\n**Ключевые элементы:**\n\n| Что | Где |\n|---|---|\n| **SKILL.md** | Frontmatter с `name`, `description` + полная документация |\n| **data/.env** | Все секреты (chmod 600) — GOVC_URL, GOVC_USERNAME, GOVC_PASSWORD + ANSIBLE_USER, ANSIBLE_PASS |\n| **scripts/** | 3 bash-скрипта — типовые операции (vm-list, gen-inventory, snapshot-all) |\n| **references/** | Расширенные справочники |\n\n**Паттерн скилла:**\n1. `SKILL.md` — точка входа, содержит описание, окружение, команды, troubleshooting\n2. `data/.env` — секреты (credentials)\n3. `scripts/` — готовые скрипты для частых операций\n4. `references/` — подробные справочники\n\nХочешь, создам ansible-скилл по такому же шаблону?", 4);
        timeline.push_user("проверь доступ через govc", 5);
        timeline.push_assistant("`govc` не установлен в системе. По скиллу он должен быть v0.53.0 в `/usr/local/bin/govc`. Хочешь, установлю?", 6);
        timeline.push_user("да, установи", 7);
        for delta in [
            ".", ".", ".", ".", "0", "0", "53", "Let", "The", "a", "and", "create", "go",
            "install", "install", "it", "me", "me", "plan", "to", "user", "v", "vc", "wants",
        ] {
            timeline.push_reasoning(delta, 8);
        }
        timeline.push_tool(
            "init_plan",
            "failed",
            "init_plan goal=Установить govc v0.53.0 в /usr/local/bin/govc failed: invalid plan write request: plan already exists",
            8,
        );
        timeline.push_system(
            "chat failed: execution tool error: invalid plan write request: plan already exists",
            8,
        );
        timeline.push_user("устанавливай", 9);
        for delta in [
            ".", ".", ".", ".", "0", "0", "53", "Let", "The", "a", "and", "create", "go",
            "install", "install", "it", "me", "me", "plan", "to", "user", "v", "vc", "wants",
        ] {
            timeline.push_reasoning(delta, 9);
        }
        timeline.push_tool(
            "init_plan",
            "failed",
            "init_plan goal=Установить govc v0.53.0 в /usr/local/bin/govc failed: invalid plan write request: plan already exists",
            9,
        );
        timeline.push_system(
            "chat failed: execution tool error: invalid plan write request: plan already exists",
            9,
        );

        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, timeline);

        let backend = TestBackend::new(138, 70);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, &state))
            .expect("draw");
        let viewport =
            super::chat_viewport_debug(&state, ratatui::layout::Rect::new(0, 0, 138, 70))
                .expect("viewport debug");

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                rendered.push_str(buffer[(x, y)].symbol());
            }
            rendered.push('\n');
        }

        assert!(
            rendered.contains("пользователь: устанавливай"),
            "latest user message missing from rendered chat\nviewport={viewport:?}\nrendered=\n{rendered}"
        );
        assert!(
            rendered.contains("invalid plan write request: plan already exists"),
            "latest tool/system failure missing from rendered chat\nviewport={viewport:?}\nrendered=\n{rendered}"
        );
    }
}
