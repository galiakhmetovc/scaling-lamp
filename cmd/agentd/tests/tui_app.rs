use agentd::bootstrap::SessionSummary;
use agentd::tui::app::{DialogState, TuiAppState, TuiScreen};

fn summary(id: &str, title: &str) -> SessionSummary {
    SessionSummary {
        id: id.to_string(),
        title: title.to_string(),
        model: Some("glm-5-turbo".to_string()),
        reasoning_visible: true,
        think_level: Some("medium".to_string()),
        compactifications: 0,
        created_at: 10,
        updated_at: 20,
    }
}

#[test]
fn tui_shell_navigation_starts_in_session_screen_without_current_session() {
    let app = TuiAppState::new(vec![summary("session-a", "Session A")], None);

    assert_eq!(app.active_screen(), TuiScreen::Sessions);
}

#[test]
fn tui_shell_navigation_opens_chat_from_selected_session() {
    let mut app = TuiAppState::new(
        vec![
            summary("session-a", "Session A"),
            summary("session-b", "Session B"),
        ],
        None,
    );

    app.select_next_session();
    app.activate_selected_session().expect("activate session");

    assert_eq!(app.active_screen(), TuiScreen::Chat);
    assert_eq!(app.current_session_id(), Some("session-b"));
}

#[test]
fn tui_shell_navigation_returns_to_previous_chat_on_escape() {
    let mut app = TuiAppState::new(
        vec![summary("session-a", "Session A")],
        Some("session-a".to_string()),
    );

    app.open_session_screen();
    assert_eq!(app.active_screen(), TuiScreen::Sessions);

    app.handle_escape();

    assert_eq!(app.active_screen(), TuiScreen::Chat);
    assert_eq!(app.current_session_id(), Some("session-a"));
}

#[test]
fn tui_shell_navigation_opens_expected_dialogs() {
    let mut app = TuiAppState::new(
        vec![summary("session-a", "Session A")],
        Some("session-a".to_string()),
    );

    app.open_new_session_dialog();
    assert_eq!(app.dialog_state(), Some(DialogState::CreateSession));
    app.close_dialog();

    app.open_session_screen();
    app.open_delete_dialog().expect("delete dialog");
    assert_eq!(
        app.dialog_state(),
        Some(DialogState::ConfirmDelete {
            session_id: "session-a".to_string(),
        })
    );
}
