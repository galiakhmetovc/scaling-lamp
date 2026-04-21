pub mod app;
pub mod backend;
pub mod events;
pub mod render;
pub mod screens;
pub mod timeline;
pub mod worker;

use crate::bootstrap::{App, BootstrapError};
use crate::daemon;
use crate::execution::ChatExecutionEvent;
use crate::http::client::{DaemonConnectOptions, connect_or_autospawn_detailed};
use app::{DialogState, TuiAppState};
use backend::TuiBackend;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
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
    run_daemon_backed(app, DaemonConnectOptions::default())
}

pub fn run_daemon_backed(app: &App, options: DaemonConnectOptions) -> Result<(), BootstrapError> {
    let connection = connect_or_autospawn_detailed(&app.config, &options, || {
        daemon::spawn_local_process().map_err(BootstrapError::Stream)
    })?;
    let client = connection.client().clone();
    let result = run_with_backend(client);
    let shutdown_result = connection.shutdown_if_autospawned();
    match (result, shutdown_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
    }
}

pub fn run_with_backend<B>(backend: B) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let mut state = app::TuiAppState::new(backend.list_session_summaries()?, None);
    let mut terminal = TerminalGuard::new()?;
    let mut redraw = |state: &TuiAppState| {
        terminal
            .terminal()
            .draw(|frame| render::render(frame, state))
            .map(|_| ())
            .map_err(BootstrapError::Stream)
    };

    loop {
        pump_background(&backend, &mut state, &mut redraw)?;
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

        if !should_dispatch_key_event(key) {
            continue;
        }

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

        dispatch_action(&backend, &mut state, action, &mut redraw)?;
    }
}

pub fn dispatch_action<B>(
    app: &B,
    state: &mut TuiAppState,
    action: TuiAction,
    redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), BootstrapError>,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
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
            if is_command_input(input.as_str()) {
                handle_command(app, state, input.trim(), redraw)?;
            } else {
                submit_chat_message(app, state, input.trim(), QueuedDraftMode::Priority)?;
            }
        }
        TuiAction::QueueChatInput(input) => {
            if is_command_input(input.as_str()) {
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

    redraw(state)?;
    Ok(())
}

fn should_dispatch_key_event(key: KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

fn handle_command<B>(
    app: &B,
    state: &mut TuiAppState,
    raw: &str,
    redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), BootstrapError>,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let current_session_id = state
        .current_session_id()
        .ok_or_else(|| BootstrapError::Usage {
            reason: "no current session selected".to_string(),
        })?
        .to_string();
    let mut parts = raw.splitn(2, ' ');
    let command = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default().trim();

    match canonical_command(command) {
        Some("/session") => state.open_session_screen(),
        Some("/new") => {
            let summary = app.create_session_auto(Some("New Session"))?;
            state.sync_sessions(app.list_session_summaries()?);
            load_session_into_state(app, state, &summary.id)?;
        }
        Some("/rename") => {
            let _ = state.open_rename_dialog();
        }
        Some("/clear") => {
            let _ = state.open_clear_dialog();
        }
        Some("/plan") => {
            let plan = app.render_plan(&current_session_id)?;
            state.timeline_mut().push_system(&plan, unix_timestamp()?);
        }
        Some("/jobs") => {
            let jobs = app.render_active_jobs(&current_session_id)?;
            state.timeline_mut().push_system(&jobs, unix_timestamp()?);
        }
        Some("/skills") => {
            let rendered = render_session_skills(app.session_skills(&current_session_id)?);
            state
                .timeline_mut()
                .push_system(&rendered, unix_timestamp()?);
        }
        Some("/enable") => {
            let skill_name = require_arg(rest, "\\включить")?;
            let updated = app.enable_session_skill(&current_session_id, &skill_name)?;
            let rendered = render_session_skills(updated);
            state
                .timeline_mut()
                .push_system(&rendered, unix_timestamp()?);
        }
        Some("/disable") => {
            let skill_name = require_arg(rest, "\\выключить")?;
            let updated = app.disable_session_skill(&current_session_id, &skill_name)?;
            let rendered = render_session_skills(updated);
            state
                .timeline_mut()
                .push_system(&rendered, unix_timestamp()?);
        }
        Some("/approve") => {
            approve_pending(app, state, &current_session_id, option_arg(rest), redraw)?
        }
        Some("/model") => {
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
        Some("/reasoning") => {
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
        Some("/think") => {
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
        Some("/compact") => {
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
        Some("/exit") => state.request_exit(),
        _ => {
            state
                .timeline_mut()
                .push_system(&format!("unknown command {raw}"), unix_timestamp()?);
        }
    }

    Ok(())
}

fn submit_chat_message<B>(
    app: &B,
    state: &mut TuiAppState,
    message: &str,
    mode: QueuedDraftMode,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
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

fn approve_pending<B>(
    app: &B,
    state: &mut TuiAppState,
    session_id: &str,
    requested_approval_id: Option<String>,
    _redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), BootstrapError>,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
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

pub fn pump_background<B>(
    app: &B,
    state: &mut TuiAppState,
    redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), BootstrapError>,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
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
                    ChatExecutionEvent::ToolStatus {
                        tool_name,
                        summary,
                        status,
                    } => {
                        state
                            .timeline_mut()
                            .update_tool_status(&tool_name, &summary, status, at);
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

fn start_chat_run<B>(
    app: &B,
    state: &mut TuiAppState,
    session_id: &str,
    message: &str,
    sent_at: i64,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    state.scroll_to_bottom();
    state.timeline_mut().push_user(message, sent_at);
    state.set_active_run(ActiveRunHandle::spawn_chat(
        app.clone(),
        session_id.to_string(),
        message.to_string(),
        sent_at,
    ));
    Ok(())
}

fn start_approval_run<B>(
    app: &B,
    state: &mut TuiAppState,
    session_id: &str,
    run_id: &str,
    approval_id: &str,
    started_at: i64,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
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

fn handle_worker_outcome<B>(
    app: &B,
    state: &mut TuiAppState,
    session_id: String,
    outcome: WorkerOutcome,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    match outcome {
        WorkerOutcome::ChatCompleted(report) => {
            if !report.output_text.is_empty() {
                state
                    .timeline_mut()
                    .finalize_assistant_output(&report.output_text, unix_timestamp()?);
            }
            state.timeline_mut().finish_turn();
        }
        WorkerOutcome::ApprovalCompleted(report) => {
            if let Some(output_text) = report.output_text
                && !output_text.is_empty()
            {
                state
                    .timeline_mut()
                    .finalize_assistant_output(&output_text, unix_timestamp()?);
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

fn schedule_next_draft_if_idle<B>(
    app: &B,
    state: &mut TuiAppState,
    session_id: &str,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
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

fn load_session_into_state<B>(
    app: &B,
    state: &mut TuiAppState,
    session_id: &str,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let summary = app.session_summary(session_id)?;
    let transcript = app.session_transcript(session_id)?;
    let pending = app.pending_approvals(session_id)?;
    let timeline = Timeline::from_session_view(&transcript, &pending);
    state.set_current_session(summary, timeline);
    Ok(())
}

fn refresh_current_session<B>(app: &B, state: &mut TuiAppState) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let sessions = app.list_session_summaries()?;
    state.sync_sessions(sessions);
    if let Some(session_id) = state.current_session_id().map(ToString::to_string) {
        let summary = app.session_summary(&session_id)?;
        state.replace_current_summary(summary);
        let pending = app.pending_approvals(&session_id)?;
        state.timeline_mut().sync_pending_approvals(&pending);
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

fn is_command_input(input: &str) -> bool {
    let trimmed = input.trim_start();
    trimmed.starts_with('/') || trimmed.starts_with('\\')
}

fn canonical_command(command: &str) -> Option<&'static str> {
    match command {
        "/session" | "\\сессии" => Some("/session"),
        "/new" | "\\новая" => Some("/new"),
        "/rename" | "\\переименовать" => Some("/rename"),
        "/clear" | "\\очистить" => Some("/clear"),
        "/plan" | "\\план" => Some("/plan"),
        "/jobs" | "\\задачи" => Some("/jobs"),
        "/skills" | "\\скиллы" => Some("/skills"),
        "/enable" | "\\включить" => Some("/enable"),
        "/disable" | "\\выключить" => Some("/disable"),
        "/approve" | "\\апрув" => Some("/approve"),
        "/model" | "\\модель" => Some("/model"),
        "/reasoning" | "\\размышления" => Some("/reasoning"),
        "/think" | "\\думай" => Some("/think"),
        "/compact" | "\\компакт" => Some("/compact"),
        "/exit" | "\\выход" => Some("/exit"),
        _ => None,
    }
}

fn render_session_skills(skills: Vec<crate::bootstrap::SessionSkillStatus>) -> String {
    if skills.is_empty() {
        return "skills: none discovered".to_string();
    }

    let mut lines = vec!["Skills:".to_string()];
    lines.extend(
        skills
            .into_iter()
            .map(|skill| format!("- [{}] {}: {}", skill.mode, skill.name, skill.description)),
    );
    lines.join("\n")
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

#[cfg(test)]
mod tests {
    use super::should_dispatch_key_event;
    use super::*;
    use crate::bootstrap::{
        SessionPendingApproval, SessionPreferencesPatch, SessionSkillStatus, SessionSummary,
        SessionTranscriptView,
    };
    use crate::execution::{ApprovalContinuationReport, ChatTurnExecutionReport};
    use crate::tui::backend::TuiBackend;
    use crate::tui::timeline::TimelineEntryKind;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use std::sync::atomic::AtomicBool;

    #[derive(Clone)]
    struct FakeBackend {
        summary: SessionSummary,
        pending: Vec<SessionPendingApproval>,
    }

    impl TuiBackend for FakeBackend {
        fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError> {
            Ok(vec![self.summary.clone()])
        }

        fn create_session_auto(
            &self,
            _title: Option<&str>,
        ) -> Result<SessionSummary, BootstrapError> {
            panic!("unused in test")
        }

        fn update_session_preferences(
            &self,
            _session_id: &str,
            _patch: SessionPreferencesPatch,
        ) -> Result<SessionSummary, BootstrapError> {
            panic!("unused in test")
        }

        fn delete_session(&self, _session_id: &str) -> Result<(), BootstrapError> {
            panic!("unused in test")
        }

        fn clear_session(
            &self,
            _session_id: &str,
            _title: Option<&str>,
        ) -> Result<SessionSummary, BootstrapError> {
            panic!("unused in test")
        }

        fn session_summary(&self, _session_id: &str) -> Result<SessionSummary, BootstrapError> {
            Ok(self.summary.clone())
        }

        fn session_transcript(
            &self,
            _session_id: &str,
        ) -> Result<SessionTranscriptView, BootstrapError> {
            Ok(SessionTranscriptView {
                session_id: self.summary.id.clone(),
                entries: Vec::new(),
            })
        }

        fn pending_approvals(
            &self,
            _session_id: &str,
        ) -> Result<Vec<SessionPendingApproval>, BootstrapError> {
            Ok(self.pending.clone())
        }

        fn session_skills(
            &self,
            _session_id: &str,
        ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
            panic!("unused in test")
        }

        fn enable_session_skill(
            &self,
            _session_id: &str,
            _skill_name: &str,
        ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
            panic!("unused in test")
        }

        fn disable_session_skill(
            &self,
            _session_id: &str,
            _skill_name: &str,
        ) -> Result<Vec<SessionSkillStatus>, BootstrapError> {
            panic!("unused in test")
        }

        fn latest_pending_approval(
            &self,
            _session_id: &str,
            _requested_approval_id: Option<&str>,
        ) -> Result<Option<SessionPendingApproval>, BootstrapError> {
            Ok(self.pending.first().cloned())
        }

        fn render_plan(&self, _session_id: &str) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn render_active_jobs(&self, _session_id: &str) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn compact_session(&self, _session_id: &str) -> Result<SessionSummary, BootstrapError> {
            panic!("unused in test")
        }

        fn execute_chat_turn_with_control_and_observer(
            &self,
            _session_id: &str,
            _message: &str,
            _now: i64,
            _interrupt_after_tool_step: Option<&AtomicBool>,
            _observer: &mut dyn FnMut(ChatExecutionEvent),
        ) -> Result<ChatTurnExecutionReport, BootstrapError> {
            panic!("unused in test")
        }

        fn approve_run_with_control_and_observer(
            &self,
            _run_id: &str,
            _approval_id: &str,
            _now: i64,
            _interrupt_after_tool_step: Option<&AtomicBool>,
            _observer: &mut dyn FnMut(ChatExecutionEvent),
        ) -> Result<ApprovalContinuationReport, BootstrapError> {
            panic!("unused in test")
        }
    }

    #[test]
    fn should_dispatch_key_event_ignores_release_events() {
        let release = KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        };

        assert!(!should_dispatch_key_event(release));
    }

    #[test]
    fn should_dispatch_key_event_accepts_press_and_repeat_events() {
        let press = KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        };
        let repeat = KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Repeat,
            state: KeyEventState::NONE,
        };

        assert!(should_dispatch_key_event(press));
        assert!(should_dispatch_key_event(repeat));
    }

    #[test]
    fn handle_worker_outcome_rehydrates_pending_approval_from_backend_state() {
        let backend = FakeBackend {
            summary: SessionSummary {
                id: "session-a".to_string(),
                title: "Session A".to_string(),
                model: Some("glm-5-turbo".to_string()),
                reasoning_visible: true,
                think_level: None,
                compactifications: 0,
                context_tokens: 0,
                has_pending_approval: true,
                last_message_preview: None,
                message_count: 1,
                background_job_count: 0,
                running_background_job_count: 0,
                queued_background_job_count: 0,
                created_at: 1,
                updated_at: 2,
            },
            pending: vec![SessionPendingApproval {
                run_id: "run-1".to_string(),
                approval_id: "approval-1".to_string(),
                reason: "tool write requires approval".to_string(),
                requested_at: 10,
            }],
        };
        let mut state = TuiAppState::new(
            vec![backend.summary.clone()],
            Some(backend.summary.id.clone()),
        );
        state.set_current_session(backend.summary.clone(), Timeline::default());

        handle_worker_outcome(
            &backend,
            &mut state,
            backend.summary.id.clone(),
            WorkerOutcome::Failed("stream ended unexpectedly".to_string()),
        )
        .expect("handle worker outcome");

        assert!(state.timeline().entries(true).iter().any(|entry| {
            matches!(
                entry.kind,
                TimelineEntryKind::Approval {
                    ref approval_id
                } if approval_id == "approval-1"
            )
        }));
    }
}
