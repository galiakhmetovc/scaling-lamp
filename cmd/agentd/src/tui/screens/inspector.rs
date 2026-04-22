use crate::bootstrap::BootstrapError;
use crate::tui::app::{BrowserKind, TuiAppState};
use crate::tui::events::TuiAction;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key(state: &mut TuiAppState, key: KeyEvent) -> Result<TuiAction, BootstrapError> {
    if state.dialog_state().is_some() {
        return Ok(handle_dialog_key(state, key));
    }

    let action = match key.code {
        KeyCode::Esc => {
            state.handle_escape();
            TuiAction::None
        }
        KeyCode::Up if state.browser_full_preview() => TuiAction::BrowserPreviewScrollUp,
        KeyCode::Down if state.browser_full_preview() => TuiAction::BrowserPreviewScrollDown,
        KeyCode::PageUp if state.browser_state().is_some() => TuiAction::BrowserPreviewScrollPageUp,
        KeyCode::PageDown if state.browser_state().is_some() => {
            TuiAction::BrowserPreviewScrollPageDown
        }
        KeyCode::Home if state.browser_state().is_some() => TuiAction::BrowserPreviewScrollHome,
        KeyCode::End if state.browser_state().is_some() => TuiAction::BrowserPreviewScrollEnd,
        KeyCode::Up if state.browser_state().is_some() => TuiAction::BrowserSelectPrevious,
        KeyCode::Down if state.browser_state().is_some() => TuiAction::BrowserSelectNext,
        KeyCode::Enter if state.browser_state().is_some() => TuiAction::BrowserActivate,
        KeyCode::Char('н') | KeyCode::Char('Н') if state.browser_state().is_some() => {
            TuiAction::BrowserCreate
        }
        KeyCode::Char('/')
            if matches!(
                state.browser_state().map(|browser| browser.kind()),
                Some(BrowserKind::Artifacts)
            ) =>
        {
            TuiAction::BrowserSearch
        }
        KeyCode::Char('f')
            if key.modifiers == crossterm::event::KeyModifiers::CONTROL
                && matches!(
                    state.browser_state().map(|browser| browser.kind()),
                    Some(BrowserKind::Artifacts)
                ) =>
        {
            TuiAction::BrowserSearch
        }
        KeyCode::Char('n')
            if matches!(
                state.browser_state().map(|browser| browser.kind()),
                Some(BrowserKind::Artifacts)
            ) =>
        {
            TuiAction::BrowserSearchNext
        }
        KeyCode::Char('N')
            if matches!(
                state.browser_state().map(|browser| browser.kind()),
                Some(BrowserKind::Artifacts)
            ) =>
        {
            TuiAction::BrowserSearchPrevious
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
        KeyCode::Char(c)
            if key.modifiers.is_empty()
                || key.modifiers == crossterm::event::KeyModifiers::SHIFT =>
        {
            state.append_dialog_input(c);
            TuiAction::None
        }
        _ => TuiAction::None,
    }
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
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)
            )
            .expect("page down key"),
            TuiAction::BrowserPreviewScrollPageDown
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

        state.open_artifact_browser(
            "Артефакты".to_string(),
            "↑↓ выбор | Enter полный".to_string(),
            vec![BrowserItem {
                id: "artifact-1".to_string(),
                label: "artifact-1 [ref]".to_string(),
            }],
            0,
            "Артефакт artifact-1".to_string(),
            "payload".to_string(),
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE)
            )
            .expect("search key"),
            TuiAction::BrowserSearch
        );
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)
            )
            .expect("next match key"),
            TuiAction::BrowserSearchNext
        );
        state.toggle_browser_full_preview();
        assert_eq!(
            handle_key(&mut state, KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                .expect("full preview down key"),
            TuiAction::BrowserPreviewScrollDown
        );
    }

    #[test]
    fn inspector_dialog_accepts_text_input() {
        let mut state = TuiAppState::new(Vec::new(), Some("session-a".to_string()));
        state.open_create_agent_dialog();

        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Char('р'), KeyModifiers::NONE)
            )
            .expect("text key"),
            TuiAction::None
        );
        assert_eq!(state.dialog_input(), Some("р"));
        assert_eq!(
            handle_key(
                &mut state,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            )
            .expect("confirm key"),
            TuiAction::ConfirmDialog
        );
    }
}
