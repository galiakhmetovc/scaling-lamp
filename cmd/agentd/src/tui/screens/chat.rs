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
        KeyCode::Backspace => {
            state.pop_input_char();
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
