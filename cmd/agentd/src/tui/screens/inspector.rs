use crate::bootstrap::BootstrapError;
use crate::tui::app::TuiAppState;
use crate::tui::events::TuiAction;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key(state: &mut TuiAppState, key: KeyEvent) -> Result<TuiAction, BootstrapError> {
    let action = match key.code {
        KeyCode::Esc => {
            state.handle_escape();
            TuiAction::None
        }
        _ => TuiAction::None,
    };
    Ok(action)
}
