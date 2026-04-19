use crate::bootstrap::{App, BootstrapError};
use crate::tui::app::TuiAppState;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key(
    _app: &App,
    state: &mut TuiAppState,
    key: KeyEvent,
) -> Result<(), BootstrapError> {
    match key.code {
        KeyCode::Up => state.select_previous_session(),
        KeyCode::Down => state.select_next_session(),
        KeyCode::Enter => {
            let _ = state.activate_selected_session();
        }
        KeyCode::Esc => state.handle_escape(),
        KeyCode::Char('n') | KeyCode::Char('N') => state.open_new_session_dialog(),
        KeyCode::Char('d') | KeyCode::Char('D') => {
            let _ = state.open_delete_dialog();
        }
        _ => {}
    }

    Ok(())
}
