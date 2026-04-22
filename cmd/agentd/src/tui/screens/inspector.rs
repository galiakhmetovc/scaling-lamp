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
        KeyCode::Up if state.browser_state().is_some() => TuiAction::BrowserSelectPrevious,
        KeyCode::Down if state.browser_state().is_some() => TuiAction::BrowserSelectNext,
        KeyCode::Enter if state.browser_state().is_some() => TuiAction::BrowserActivate,
        _ => TuiAction::None,
    };
    Ok(action)
}

#[cfg(test)]
mod tests {
    use super::handle_key;
    use crate::tui::app::{BrowserItem, TuiAppState};
    use crate::tui::events::TuiAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn inspector_browser_supports_navigation_and_activation_keys() {
        let mut state = TuiAppState::new(Vec::new(), Some("session-a".to_string()));
        state.open_agent_browser(
            "Агенты".to_string(),
            "↑↓ выбор | Enter выбрать".to_string(),
            vec![
                BrowserItem {
                    id: "default".to_string(),
                    label: "Default (default)".to_string(),
                },
                BrowserItem {
                    id: "judge".to_string(),
                    label: "Judge (judge)".to_string(),
                },
            ],
            0,
            "Агент default".to_string(),
            "id=default".to_string(),
        );

        assert_eq!(
            handle_key(&mut state, KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)).expect("up key"),
            TuiAction::BrowserSelectPrevious
        );
        assert_eq!(
            handle_key(&mut state, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                .expect("down key"),
            TuiAction::BrowserSelectNext
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            )
            .expect("enter key"),
            TuiAction::BrowserActivate
        );
    }
}
