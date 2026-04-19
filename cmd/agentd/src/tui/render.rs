use crate::tui::app::{DialogState, TuiAppState, TuiScreen};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
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
        .enumerate()
        .map(|(index, session)| {
            let prefix =
                if state.selected_session().map(|selected| &selected.id) == Some(&session.id) {
                    "> "
                } else {
                    "  "
                };
            let label = format!("{prefix}{} ({})", session.title, session.id);
            let mut item = ListItem::new(label);
            if index
                == state
                    .sessions()
                    .iter()
                    .position(|candidate| candidate.id == session.id)
                    .unwrap_or(0)
                && state
                    .selected_session()
                    .map(|selected| selected.id.as_str())
                    == Some(session.id.as_str())
            {
                item = item.style(Style::default().add_modifier(Modifier::BOLD));
            }
            item
        })
        .collect::<Vec<_>>();

    let list = List::new(items).block(Block::default().title("Sessions").borders(Borders::ALL));
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

    let title = state
        .current_session_id()
        .map(|id| format!("Chat | {id}"))
        .unwrap_or_else(|| "Chat".to_string());
    let top = Paragraph::new(title).block(Block::default().borders(Borders::ALL));
    let timeline = Paragraph::new("Timeline wiring comes next.")
        .block(Block::default().title("Chat").borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((state.scroll_offset(), 0));
    let input = Paragraph::new(state.input_buffer())
        .block(Block::default().title("Input").borders(Borders::ALL));

    frame.render_widget(top, chunks[0]);
    frame.render_widget(timeline, chunks[1]);
    frame.render_widget(input, chunks[2]);
}

fn render_dialog(frame: &mut Frame<'_>, dialog: DialogState) {
    let area = centered_rect(frame.area(), 60, 20);
    frame.render_widget(Clear, area);
    let content = match dialog {
        DialogState::CreateSession => "Create Session",
        DialogState::ConfirmDelete { .. } => "Confirm Delete",
    };
    let paragraph = Paragraph::new(content).block(Block::default().borders(Borders::ALL));
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
