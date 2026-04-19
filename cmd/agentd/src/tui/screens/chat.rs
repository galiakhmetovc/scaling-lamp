use crate::bootstrap::{App, BootstrapError};
use crate::tui::app::TuiAppState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_key(
    _app: &App,
    state: &mut TuiAppState,
    key: KeyEvent,
) -> Result<(), BootstrapError> {
    match key.code {
        KeyCode::Esc => state.handle_escape(),
        KeyCode::Up => state.scroll_up(),
        KeyCode::Down => state.scroll_down(),
        KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
            state.input_buffer_mut().push(c);
        }
        KeyCode::Backspace => {
            state.input_buffer_mut().pop();
        }
        _ => {}
    }

    Ok(())
}
