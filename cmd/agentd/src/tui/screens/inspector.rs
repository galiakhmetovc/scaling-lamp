use crate::bootstrap::BootstrapError;
use crate::tui::app::{BrowserKind, TuiAppState};
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
        KeyCode::Char('н') | KeyCode::Char('Н') if state.browser_state().is_some() => {
            TuiAction::BrowserCreate
        }
        KeyCode::Char('у') | KeyCode::Char('У')
            if matches!(
                state.browser_state().map(|browser| browser.kind()),
                Some(BrowserKind::Schedules)
            ) =>
        {
            TuiAction::BrowserDelete
        }
        KeyCode::Char('о') | KeyCode::Char('О')
            if matches!(
                state.browser_state().map(|browser| browser.kind()),
                Some(BrowserKind::Agents)
            ) =>
        {
            TuiAction::BrowserOpenSelected
        }
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
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('н'), KeyModifiers::NONE)
            )
            .expect("create key"),
            TuiAction::BrowserCreate
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('о'), KeyModifiers::NONE)
            )
            .expect("open key"),
            TuiAction::BrowserOpenSelected
        );

        state.open_schedule_browser(
            "Расписания".to_string(),
            "↑↓ выбор | Н создать | У удалить".to_string(),
            vec![BrowserItem {
                id: "pulse".to_string(),
                label: "pulse".to_string(),
            }],
            0,
            "Расписание pulse".to_string(),
            "id=pulse".to_string(),
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('у'), KeyModifiers::NONE)
            )
            .expect("delete key"),
            TuiAction::BrowserDelete
        );
    }
}
