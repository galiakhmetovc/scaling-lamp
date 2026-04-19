pub mod app;
pub mod events;
pub mod render;
pub mod screens;
pub mod timeline;

use crate::bootstrap::{App, BootstrapError};
use crate::execution::{ChatExecutionEvent, ExecutionError};
use app::{DialogState, TuiAppState};
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use events::TuiAction;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{self, Stdout};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use timeline::Timeline;

pub use app::{DialogState as TuiDialogState, TuiScreen};

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> Result<Self, BootstrapError> {
        enable_raw_mode().map_err(BootstrapError::Stream)?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).map_err(BootstrapError::Stream)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(BootstrapError::Stream)?;
        Ok(Self { terminal })
    }

    fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

pub fn run(app: &App) -> Result<(), BootstrapError> {
    let mut state = app::TuiAppState::new(app.list_session_summaries()?, None);
    let mut terminal = TerminalGuard::new()?;

    loop {
        terminal
            .terminal()
            .draw(|frame| render::render(frame, &state))
            .map_err(BootstrapError::Stream)?;

        if state.should_exit() {
            return Ok(());
        }

        if !event::poll(Duration::from_millis(100)).map_err(BootstrapError::Stream)? {
            continue;
        }

        let Event::Key(key) = event::read().map_err(BootstrapError::Stream)? else {
            continue;
        };

        let action = match state.active_screen() {
            TuiScreen::Sessions => screens::session::handle_key(&mut state, key)?,
            TuiScreen::Chat => screens::chat::handle_key(&mut state, key)?,
        };

        if key.code == KeyCode::Char('q')
            && key.modifiers.is_empty()
            && state.dialog_state().is_none()
        {
            state.request_exit();
            continue;
        }

        let mut redraw = |state: &TuiAppState| {
            terminal
                .terminal()
                .draw(|frame| render::render(frame, state))
                .map(|_| ())
                .map_err(BootstrapError::Stream)
        };
        dispatch_action(app, &mut state, action, &mut redraw)?;
    }
}

pub fn dispatch_action(
    app: &App,
    state: &mut TuiAppState,
    action: TuiAction,
    redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), BootstrapError>,
) -> Result<(), BootstrapError> {
    match action {
        TuiAction::None => {}
        TuiAction::Exit => state.request_exit(),
        TuiAction::OpenSessionScreen => state.open_session_screen(),
        TuiAction::OpenNewSessionDialog => state.open_new_session_dialog(),
        TuiAction::OpenDeleteDialog => {
            let _ = state.open_delete_dialog();
        }
        TuiAction::OpenRenameDialog => {
            let _ = state.open_rename_dialog();
        }
        TuiAction::OpenClearDialog => {
            let _ = state.open_clear_dialog();
        }
        TuiAction::ActivateSelectedSession => {
            if let Ok(session_id) = state.activate_selected_session() {
                load_session_into_state(app, state, &session_id)?;
            }
        }
        TuiAction::ConfirmDialog => match state.dialog_state() {
            Some(DialogState::CreateSession { value }) => {
                let title = title_or_default(value.as_str(), "New Session");
                let summary = app.create_session_auto(Some(title.as_str()))?;
                let sessions = app.list_session_summaries()?;
                state.sync_sessions(sessions);
                state.close_dialog();
                state.timeline_mut().push_system(
                    &format!("created session {}", summary.title),
                    unix_timestamp()?,
                );
                load_session_into_state(app, state, &summary.id)?;
            }
            Some(DialogState::RenameSession { session_id, value }) => {
                let title = title_or_default(value.as_str(), "New Session");
                let summary = app.update_session_preferences(
                    &session_id,
                    crate::bootstrap::SessionPreferencesPatch {
                        title: Some(title),
                        ..crate::bootstrap::SessionPreferencesPatch::default()
                    },
                )?;
                state.close_dialog();
                state.replace_current_summary(summary.clone());
                state.sync_sessions(app.list_session_summaries()?);
                state.timeline_mut().push_system(
                    &format!("renamed session to {}", summary.title),
                    unix_timestamp()?,
                );
            }
            Some(DialogState::ConfirmDelete { session_id }) => {
                app.delete_session(&session_id)?;
                let sessions = app.list_session_summaries()?;
                state.sync_sessions(sessions);
                state.close_dialog();
                if state.current_session_id() == Some(session_id.as_str()) {
                    state.replace_timeline(Timeline::default());
                }
            }
            Some(DialogState::ConfirmClear { session_id }) => {
                let replacement = app.clear_session(&session_id, Some("New Session"))?;
                state.close_dialog();
                state.sync_sessions(app.list_session_summaries()?);
                load_session_into_state(app, state, &replacement.id)?;
                state
                    .timeline_mut()
                    .push_system("cleared session", unix_timestamp()?);
            }
            None => {}
        },
        TuiAction::SubmitChatInput(input) => {
            if input.trim_start().starts_with('/') {
                handle_command(app, state, input.trim(), redraw)?;
            } else {
                send_chat_message(app, state, input.trim(), redraw)?;
            }
        }
    }

    Ok(())
}

fn handle_command(
    app: &App,
    state: &mut TuiAppState,
    raw: &str,
    redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), BootstrapError>,
) -> Result<(), BootstrapError> {
    let current_session_id = state
        .current_session_id()
        .ok_or_else(|| BootstrapError::Usage {
            reason: "no current session selected".to_string(),
        })?
        .to_string();
    let mut parts = raw.splitn(2, ' ');
    let command = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default().trim();

    match command {
        "/session" => state.open_session_screen(),
        "/new" => {
            let summary = app.create_session_auto(Some("New Session"))?;
            state.sync_sessions(app.list_session_summaries()?);
            load_session_into_state(app, state, &summary.id)?;
        }
        "/rename" => {
            let _ = state.open_rename_dialog();
        }
        "/clear" => {
            let _ = state.open_clear_dialog();
        }
        "/approve" => approve_pending(app, state, &current_session_id, option_arg(rest), redraw)?,
        "/model" => {
            let summary = app.update_session_preferences(
                &current_session_id,
                crate::bootstrap::SessionPreferencesPatch {
                    model: Some(Some(require_arg(rest, "/model")?)),
                    ..crate::bootstrap::SessionPreferencesPatch::default()
                },
            )?;
            state.replace_current_summary(summary);
            state.sync_sessions(app.list_session_summaries()?);
        }
        "/reasoning" => {
            let visible = match require_arg(rest, "/reasoning")?.as_str() {
                "on" => true,
                "off" => false,
                value => {
                    return Err(BootstrapError::Usage {
                        reason: format!("unsupported reasoning mode {value}; expected on|off"),
                    });
                }
            };
            let summary = app.update_session_preferences(
                &current_session_id,
                crate::bootstrap::SessionPreferencesPatch {
                    reasoning_visible: Some(visible),
                    ..crate::bootstrap::SessionPreferencesPatch::default()
                },
            )?;
            state.replace_current_summary(summary);
            state.sync_sessions(app.list_session_summaries()?);
        }
        "/think" => {
            let summary = app.update_session_preferences(
                &current_session_id,
                crate::bootstrap::SessionPreferencesPatch {
                    think_level: Some(Some(require_arg(rest, "/think")?)),
                    ..crate::bootstrap::SessionPreferencesPatch::default()
                },
            )?;
            state.replace_current_summary(summary);
            state.sync_sessions(app.list_session_summaries()?);
        }
        "/compact" => {
            let summary = app.compact_session_placeholder(&current_session_id)?;
            state.replace_current_summary(summary);
            state.sync_sessions(app.list_session_summaries()?);
            state
                .timeline_mut()
                .push_system("compact placeholder executed", unix_timestamp()?);
        }
        "/exit" => state.request_exit(),
        _ => {
            state
                .timeline_mut()
                .push_system(&format!("unknown command {raw}"), unix_timestamp()?);
        }
    }

    Ok(())
}

fn send_chat_message(
    app: &App,
    state: &mut TuiAppState,
    message: &str,
    redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), BootstrapError>,
) -> Result<(), BootstrapError> {
    if message.is_empty() {
        return Ok(());
    }

    let session_id = state
        .current_session_id()
        .ok_or_else(|| BootstrapError::Usage {
            reason: "no current session selected".to_string(),
        })?
        .to_string();
    if app.latest_pending_approval(&session_id, None)?.is_some() {
        state.timeline_mut().push_system(
            "finish the pending approval before sending another message",
            unix_timestamp()?,
        );
        return Ok(());
    }

    let sent_at = unix_timestamp()?;
    state.timeline_mut().push_user(message, sent_at);
    redraw(state)?;

    let mut emit_error = None;
    let mut emit = |event: ChatExecutionEvent| {
        if emit_error.is_some() {
            return;
        }
        let at = match unix_timestamp() {
            Ok(now) => now,
            Err(error) => {
                emit_error = Some(error);
                return;
            }
        };
        match event {
            ChatExecutionEvent::ReasoningDelta(delta) => {
                state.timeline_mut().push_reasoning_delta(&delta, at);
            }
            ChatExecutionEvent::AssistantTextDelta(delta) => {
                state.timeline_mut().push_assistant_delta(&delta, at);
            }
            ChatExecutionEvent::ToolStatus { tool_name, status } => {
                state
                    .timeline_mut()
                    .update_tool_status(&tool_name, status, at);
            }
        }
        if let Err(error) = redraw(state) {
            emit_error = Some(error);
        }
    };
    let result = app.execute_chat_turn_with_observer(&session_id, message, sent_at, &mut emit);
    if let Some(error) = emit_error {
        return Err(error);
    }

    match result {
        Ok(report) => {
            if !report.output_text.is_empty()
                && !state
                    .timeline()
                    .entries(true)
                    .last()
                    .map(|entry| matches!(entry.kind, timeline::TimelineEntryKind::Assistant))
                    .unwrap_or(false)
            {
                state
                    .timeline_mut()
                    .push_assistant(&report.output_text, unix_timestamp()?);
            }
            state.timeline_mut().finish_turn();
        }
        Err(BootstrapError::Execution(ExecutionError::ApprovalRequired { .. })) => {
            state.timeline_mut().finish_turn();
        }
        Err(error) => {
            state
                .timeline_mut()
                .push_system(&format!("chat failed: {error}"), unix_timestamp()?);
            state.timeline_mut().finish_turn();
            return Err(error);
        }
    }

    refresh_current_session(app, state)?;
    Ok(())
}

fn approve_pending(
    app: &App,
    state: &mut TuiAppState,
    session_id: &str,
    requested_approval_id: Option<String>,
    redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), BootstrapError>,
) -> Result<(), BootstrapError> {
    let Some(pending) =
        app.latest_pending_approval(session_id, requested_approval_id.as_deref())?
    else {
        state.timeline_mut().push_system(
            &format!("no pending approval for session_id={session_id}"),
            unix_timestamp()?,
        );
        return Ok(());
    };
    state.timeline_mut().remove_approval(&pending.approval_id);

    let mut emit_error = None;
    let mut emit = |event: ChatExecutionEvent| {
        if emit_error.is_some() {
            return;
        }
        let at = match unix_timestamp() {
            Ok(now) => now,
            Err(error) => {
                emit_error = Some(error);
                return;
            }
        };
        match event {
            ChatExecutionEvent::ReasoningDelta(delta) => {
                state.timeline_mut().push_reasoning_delta(&delta, at);
            }
            ChatExecutionEvent::AssistantTextDelta(delta) => {
                state.timeline_mut().push_assistant_delta(&delta, at);
            }
            ChatExecutionEvent::ToolStatus { tool_name, status } => {
                state
                    .timeline_mut()
                    .update_tool_status(&tool_name, status, at);
            }
        }
        if let Err(error) = redraw(state) {
            emit_error = Some(error);
        }
    };
    let result = app.approve_run_with_observer(
        &pending.run_id,
        &pending.approval_id,
        unix_timestamp()?,
        &mut emit,
    );
    if let Some(error) = emit_error {
        return Err(error);
    }

    match result {
        Ok(report) => {
            if let Some(output_text) = report.output_text
                && !output_text.is_empty()
                && !state
                    .timeline()
                    .entries(true)
                    .last()
                    .map(|entry| matches!(entry.kind, timeline::TimelineEntryKind::Assistant))
                    .unwrap_or(false)
            {
                state
                    .timeline_mut()
                    .push_assistant(&output_text, unix_timestamp()?);
            }
            state.timeline_mut().finish_turn();
        }
        Err(error) => {
            state
                .timeline_mut()
                .push_system(&format!("approval failed: {error}"), unix_timestamp()?);
            state.timeline_mut().finish_turn();
            return Err(error);
        }
    }

    refresh_current_session(app, state)?;
    Ok(())
}

fn load_session_into_state(
    app: &App,
    state: &mut TuiAppState,
    session_id: &str,
) -> Result<(), BootstrapError> {
    let summary = app.session_summary(session_id)?;
    let transcript = app.session_transcript(session_id)?;
    let pending = app.pending_approvals(session_id)?;
    let timeline = Timeline::from_session_view(&transcript, &pending);
    state.set_current_session(summary, timeline);
    Ok(())
}

fn refresh_current_session(app: &App, state: &mut TuiAppState) -> Result<(), BootstrapError> {
    let sessions = app.list_session_summaries()?;
    state.sync_sessions(sessions);
    if let Some(session_id) = state.current_session_id().map(ToString::to_string) {
        let summary = app.session_summary(&session_id)?;
        state.replace_current_summary(summary);
    }
    Ok(())
}

fn require_arg(raw: &str, command: &str) -> Result<String, BootstrapError> {
    if raw.trim().is_empty() {
        return Err(BootstrapError::Usage {
            reason: format!("{command} requires an argument"),
        });
    }
    Ok(raw.trim().to_string())
}

fn option_arg(raw: &str) -> Option<String> {
    (!raw.trim().is_empty()).then(|| raw.trim().to_string())
}

fn title_or_default(raw: &str, default: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn unix_timestamp() -> Result<i64, BootstrapError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(BootstrapError::Clock)?;
    Ok(duration.as_secs() as i64)
}
