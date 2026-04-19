use crate::tui::app::{DialogState, TuiAppState, TuiScreen};
use crate::tui::timeline::{TimelineEntry, TimelineEntryKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

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
                session.title, session.updated_at, session.message_count, approval
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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(area);

    let top = if let Some(summary) = state.current_session_summary() {
        Paragraph::new(format!(
            "{} | model={} | reasoning={} | think={} | ctx={} | compact={} | messages={}",
            summary.title,
            summary.model.as_deref().unwrap_or("<default>"),
            if summary.reasoning_visible {
                "on"
            } else {
                "off"
            },
            summary.think_level.as_deref().unwrap_or("<default>"),
            summary.context_tokens,
            summary.compactifications,
            summary.message_count.max(state.timeline().message_count()),
        ))
        .block(Block::default().borders(Borders::ALL).title("Session"))
    } else {
        Paragraph::new("No active session")
            .block(Block::default().borders(Borders::ALL).title("Session"))
    };

    let timeline_lines = state
        .timeline()
        .entries(
            state
                .current_session_summary()
                .map(|summary| summary.reasoning_visible)
                .unwrap_or(true),
        )
        .into_iter()
        .flat_map(render_timeline_entry)
        .collect::<Vec<_>>();
    let timeline = Paragraph::new(timeline_lines)
        .block(Block::default().title("Chat").borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((state.scroll_offset(), 0));
    let input = Paragraph::new(state.input_buffer()).block(
        Block::default()
            .title("Input | /session /new /rename /clear /approve /model /reasoning /think /compact /exit")
            .borders(Borders::ALL),
    );

    frame.render_widget(top, chunks[0]);
    frame.render_widget(timeline, chunks[1]);
    frame.render_widget(input, chunks[2]);
}

fn render_timeline_entry(entry: &TimelineEntry) -> Vec<Line<'static>> {
    let timestamp = format!("[{}]", entry.timestamp);
    let label = match &entry.kind {
        TimelineEntryKind::User => "user".to_string(),
        TimelineEntryKind::Assistant => "assistant".to_string(),
        TimelineEntryKind::Reasoning => "reasoning".to_string(),
        TimelineEntryKind::Tool { tool_name, status } => format!("tool:{tool_name}:{status}"),
        TimelineEntryKind::Approval { approval_id } => format!("approval:{approval_id}"),
        TimelineEntryKind::System => "system".to_string(),
    };
    vec![Line::from(format!(
        "{timestamp} {label}: {}",
        entry.content
    ))]
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
