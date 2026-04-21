use crate::bootstrap::BootstrapError;
use crate::tui::app::TuiAppState;
use crate::tui::events::TuiAction;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_key(state: &mut TuiAppState, key: KeyEvent) -> Result<TuiAction, BootstrapError> {
    if state.dialog_state().is_some() {
        return Ok(handle_dialog_key(state, key));
    }

    let action = match key.code {
        KeyCode::Esc => TuiAction::OpenSessionScreen,
        KeyCode::BackTab => TuiAction::CyclePreviousCommand,
        KeyCode::Enter => {
            let input = state.take_input_buffer();
            if input.trim().is_empty() {
                TuiAction::None
            } else {
                TuiAction::SubmitChatInput(input)
            }
        }
        KeyCode::Tab => {
            let input = state.take_input_buffer();
            if input.trim().is_empty() {
                TuiAction::None
            } else {
                TuiAction::QueueChatInput(input)
            }
        }
        KeyCode::Up => {
            state.scroll_up();
            TuiAction::None
        }
        KeyCode::Down => {
            state.scroll_down();
            TuiAction::None
        }
        KeyCode::PageUp => {
            state.scroll_page_up();
            TuiAction::None
        }
        KeyCode::PageDown => {
            state.scroll_page_down();
            TuiAction::None
        }
        KeyCode::Left => {
            state.move_input_cursor_left();
            TuiAction::None
        }
        KeyCode::Right => {
            state.move_input_cursor_right();
            TuiAction::None
        }
        KeyCode::Home => {
            state.move_input_cursor_home();
            TuiAction::None
        }
        KeyCode::End => {
            state.move_input_cursor_end();
            TuiAction::None
        }
        KeyCode::Backspace => {
            state.pop_input_char();
            TuiAction::None
        }
        KeyCode::Delete => {
            state.delete_input_char();
            TuiAction::None
        }
        KeyCode::Char('a') if key.modifiers == KeyModifiers::CONTROL => {
            state.move_input_cursor_home();
            TuiAction::None
        }
        KeyCode::Char('e') if key.modifiers == KeyModifiers::CONTROL => {
            state.move_input_cursor_end();
            TuiAction::None
        }
        KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
            state.push_input_char(c);
            TuiAction::None
        }
        _ => TuiAction::None,
    };

    Ok(action)
}

fn handle_dialog_key(state: &mut TuiAppState, key: KeyEvent) -> TuiAction {
    match key.code {
        KeyCode::Esc => {
            state.close_dialog();
            TuiAction::None
        }
        KeyCode::Enter => TuiAction::ConfirmDialog,
        KeyCode::Backspace => {
            state.pop_dialog_input();
            TuiAction::None
        }
        KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
            state.append_dialog_input(c);
            TuiAction::None
        }
        _ => TuiAction::None,
    }
}
