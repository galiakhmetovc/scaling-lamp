use crate::bootstrap::BootstrapError;
use crate::tui::app::TuiAppState;
use crate::tui::events::TuiAction;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_key(state: &mut TuiAppState, key: KeyEvent) -> Result<TuiAction, BootstrapError> {
    if state.dialog_state().is_some() {
        return Ok(handle_dialog_key(state, key));
    }

    let action = match key.code {
        KeyCode::Up => {
            state.select_previous_session();
            TuiAction::None
        }
        KeyCode::Down => {
            state.select_next_session();
            TuiAction::None
        }
        KeyCode::Enter => TuiAction::ActivateSelectedSession,
        KeyCode::Esc => {
            state.handle_escape();
            TuiAction::None
        }
        KeyCode::Char('n') | KeyCode::Char('N') => TuiAction::OpenNewSessionDialog,
        KeyCode::Char('d') | KeyCode::Char('D') => TuiAction::OpenDeleteDialog,
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
