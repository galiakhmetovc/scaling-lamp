pub mod app;
pub mod events;
pub mod render;
pub mod screens;
pub mod timeline;
pub mod worker;

use crate::bootstrap::{App, BootstrapError};
use crate::execution::ChatExecutionEvent;
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
use worker::{ActiveRunHandle, QueuedDraftMode, WorkerEvent, WorkerOutcome};

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
    let mut redraw = |state: &TuiAppState| {
        terminal
            .terminal()
            .draw(|frame| render::render(frame, state))
            .map(|_| ())
            .map_err(BootstrapError::Stream)
    };

    loop {
        pump_background(app, &mut state, &mut redraw)?;
        redraw(&state)?;

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
                submit_chat_message(app, state, input.trim(), QueuedDraftMode::Priority)?;
            }
        }
        TuiAction::QueueChatInput(input) => {
            if input.trim_start().starts_with('/') {
                state.timeline_mut().push_system(
                    "commands cannot be queued; press Enter to execute the command",
                    unix_timestamp()?,
                );
            } else {
                submit_chat_message(app, state, input.trim(), QueuedDraftMode::Deferred)?;
            }
        }
        TuiAction::CyclePreviousCommand => {
            state.cycle_previous_command();
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
            let before = state
                .current_session_summary()
                .map(|summary| summary.compactifications);
            let summary = app.compact_session(&current_session_id)?;
            state.replace_current_summary(summary);
            state.sync_sessions(app.list_session_summaries()?);
            let after = state
                .current_session_summary()
                .map(|summary| summary.compactifications);
            let message = if before == after {
                "compaction skipped: not enough transcript history"
            } else {
                "context compaction completed"
            };
            state.timeline_mut().push_system(message, unix_timestamp()?);
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

fn submit_chat_message(
    app: &App,
    state: &mut TuiAppState,
    message: &str,
    mode: QueuedDraftMode,
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

    if state.has_active_run() || app.latest_pending_approval(&session_id, None)?.is_some() {
        state.queue_draft(message.to_string(), unix_timestamp()?, mode);
        return Ok(());
    }

    start_chat_run(app, state, &session_id, message, unix_timestamp()?)?;
    Ok(())
}

fn approve_pending(
    app: &App,
    state: &mut TuiAppState,
    session_id: &str,
    requested_approval_id: Option<String>,
    _redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), BootstrapError>,
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
    start_approval_run(
        app,
        state,
        session_id,
        &pending.run_id,
        &pending.approval_id,
        unix_timestamp()?,
    )?;
    Ok(())
}

pub fn pump_background(
    app: &App,
    state: &mut TuiAppState,
    redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), BootstrapError>,
) -> Result<(), BootstrapError> {
    let Some(events) = state.active_run_mut().map(ActiveRunHandle::drain_events) else {
        return Ok(());
    };
    let mut outcome = None;
    for event in events {
        match event {
            WorkerEvent::Chat(chat_event) => {
                let at = unix_timestamp()?;
                match chat_event {
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
            }
            WorkerEvent::Finished(result) => outcome = Some(result),
        }
    }

    let finished = outcome.is_some();
    if finished {
        let mut active_run = state.take_active_run().expect("active run");
        active_run.join();
        handle_worker_outcome(
            app,
            state,
            active_run.session_id().to_string(),
            outcome.expect("worker outcome"),
        )?;
    }

    if state.has_active_run() || finished {
        redraw(state)?;
    }

    Ok(())
}

fn start_chat_run(
    app: &App,
    state: &mut TuiAppState,
    session_id: &str,
    message: &str,
    sent_at: i64,
) -> Result<(), BootstrapError> {
    state.timeline_mut().push_user(message, sent_at);
    state.set_active_run(ActiveRunHandle::spawn_chat(
        app.clone(),
        session_id.to_string(),
        message.to_string(),
        sent_at,
    ));
    Ok(())
}

fn start_approval_run(
    app: &App,
    state: &mut TuiAppState,
    session_id: &str,
    run_id: &str,
    approval_id: &str,
    started_at: i64,
) -> Result<(), BootstrapError> {
    let should_interrupt_after_tool_step = state.queued_priority_count() > 0;
    state.set_active_run(ActiveRunHandle::spawn_approval(
        app.clone(),
        session_id.to_string(),
        run_id.to_string(),
        approval_id.to_string(),
        started_at,
    ));
    if should_interrupt_after_tool_step && let Some(active_run) = state.active_run() {
        active_run.queue_interrupt_after_tool_step();
    }
    Ok(())
}

fn handle_worker_outcome(
    app: &App,
    state: &mut TuiAppState,
    session_id: String,
    outcome: WorkerOutcome,
) -> Result<(), BootstrapError> {
    match outcome {
        WorkerOutcome::ChatCompleted(report) => {
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
        WorkerOutcome::ApprovalCompleted(report) => {
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
        WorkerOutcome::ApprovalRequired {
            approval_id,
            reason,
        } => {
            state
                .timeline_mut()
                .push_approval(&approval_id, &reason, unix_timestamp()?);
            state.timeline_mut().finish_turn();
        }
        WorkerOutcome::InterruptedByQueuedInput => {
            state.timeline_mut().push_system(
                "current response interrupted by queued input",
                unix_timestamp()?,
            );
            state.timeline_mut().finish_turn();
        }
        WorkerOutcome::Failed(reason) => {
            state
                .timeline_mut()
                .push_system(&format!("chat failed: {reason}"), unix_timestamp()?);
            state.timeline_mut().finish_turn();
        }
    }

    refresh_current_session(app, state)?;
    schedule_next_draft_if_idle(app, state, &session_id)
}

fn schedule_next_draft_if_idle(
    app: &App,
    state: &mut TuiAppState,
    session_id: &str,
) -> Result<(), BootstrapError> {
    if state.has_active_run() || app.latest_pending_approval(session_id, None)?.is_some() {
        return Ok(());
    }
    let next_draft = state
        .next_priority_draft()
        .or_else(|| state.next_deferred_draft());
    let Some(next_draft) = next_draft else {
        return Ok(());
    };
    start_chat_run(
        app,
        state,
        session_id,
        next_draft.content.as_str(),
        next_draft.queued_at.max(unix_timestamp()?),
    )
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
