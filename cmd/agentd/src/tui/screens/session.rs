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
        KeyCode::Char('н') | KeyCode::Char('Н') => TuiAction::OpenNewSessionDialog,
        KeyCode::Char('n') | KeyCode::Char('N') => TuiAction::OpenNewSessionDialog,
        KeyCode::Char('у') | KeyCode::Char('У') => TuiAction::OpenDeleteDialog,
        KeyCode::Char('d') | KeyCode::Char('D') => TuiAction::OpenDeleteDialog,
        KeyCode::Char('п') | KeyCode::Char('П') => TuiAction::OpenRenameDialog,
        KeyCode::Char('а') | KeyCode::Char('А') => TuiAction::OpenAgentsScreen,
        KeyCode::Char('р') | KeyCode::Char('Р') => TuiAction::OpenSchedulesScreen,
        KeyCode::Char('д') | KeyCode::Char('Д') => TuiAction::OpenDebugScreen,
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

#[cfg(test)]
mod tests {
    use super::handle_key;
    use crate::tui::app::TuiAppState;
    use crate::tui::events::TuiAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn session_screen_accepts_russian_shortcuts() {
        let mut state = TuiAppState::new(Vec::new(), None);

        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('н'), KeyModifiers::NONE)
            )
            .expect("new key"),
            TuiAction::OpenNewSessionDialog
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('у'), KeyModifiers::NONE)
            )
            .expect("delete key"),
            TuiAction::OpenDeleteDialog
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('п'), KeyModifiers::NONE)
            )
            .expect("rename key"),
            TuiAction::OpenRenameDialog
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('а'), KeyModifiers::NONE)
            )
            .expect("agents key"),
            TuiAction::OpenAgentsScreen
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('р'), KeyModifiers::NONE)
            )
            .expect("schedules key"),
            TuiAction::OpenSchedulesScreen
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('д'), KeyModifiers::NONE)
            )
            .expect("debug key"),
            TuiAction::OpenDebugScreen
        );
    }
}
