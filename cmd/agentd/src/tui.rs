pub mod app;
pub mod backend;
pub mod events;
pub mod render;
pub mod screens;
pub mod timeline;
pub mod worker;

use crate::bootstrap::{
    AgentScheduleCreateOptions, AgentScheduleUpdatePatch, App, BootstrapError,
    McpConnectorCreateOptions, McpConnectorUpdatePatch,
};
use crate::daemon;
use crate::execution::ChatExecutionEvent;
use crate::help::{HelpTopic, parse_help_topic, render_command_usage_error, render_help};
use crate::http::client::{DaemonConnectOptions, connect_or_autospawn_detailed};
use agent_runtime::tool::{
    KnowledgeReadInput, KnowledgeReadMode, KnowledgeSearchInput, SessionReadInput, SessionReadMode,
    SessionSearchInput,
};
use app::{BrowserItem, BrowserKind, DialogState, TuiAppState};
use backend::TuiBackend;
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEvent, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use events::TuiAction;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::fs;
use std::io::{self, Stdout};
use std::path::PathBuf;
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
        execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)
            .map_err(BootstrapError::Stream)?;
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
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
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

        let event = event::read().map_err(BootstrapError::Stream)?;
        let action = dispatch_terminal_event(&mut state, event)?;
        if state.should_exit() {
            continue;
        }

        dispatch_action(&backend, &mut state, action, &mut redraw)?;
    }
}

fn dispatch_terminal_event(
    state: &mut TuiAppState,
    event: Event,
) -> Result<TuiAction, BootstrapError> {
    match event {
        Event::Key(key) => {
            if !should_dispatch_key_event(key) {
                return Ok(TuiAction::None);
            }
            let action = match state.active_screen() {
                TuiScreen::Sessions => screens::session::handle_key(state, key)?,
                TuiScreen::Chat => screens::chat::handle_key(state, key)?,
                TuiScreen::Agents
                | TuiScreen::Schedules
                | TuiScreen::Mcp
                | TuiScreen::Artifacts => screens::inspector::handle_key(state, key)?,
            };

            Ok(action)
        }
        Event::Paste(text) => {
            if !text.is_empty() {
                let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
                state.insert_input_text(normalized.as_str());
            }
            Ok(TuiAction::None)
        }
        _ => Ok(TuiAction::None),
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
        TuiAction::OpenAgentsScreen => {
            open_agents_browser(app, state, None)?;
        }
        TuiAction::OpenSchedulesScreen => {
            open_schedule_browser(app, state, None)?;
        }
        TuiAction::BrowserSelectPrevious => {
            state.browser_select_previous();
            refresh_browser_preview(app, state)?;
        }
        TuiAction::BrowserSelectNext => {
            state.browser_select_next();
            refresh_browser_preview(app, state)?;
        }
        TuiAction::BrowserActivate => {
            activate_browser_selection(app, state)?;
        }
        TuiAction::BrowserOpenSelected => {
            open_browser_selection(app, state)?;
        }
        TuiAction::BrowserCreate => {
            open_browser_create_dialog(state);
        }
        TuiAction::BrowserMessage => {
            handle_browser_message_action(app, state)?;
        }
        TuiAction::BrowserEdit => {
            open_browser_edit_dialog(app, state)?;
        }
        TuiAction::BrowserToggle => {
            toggle_browser_schedule(app, state)?;
        }
        TuiAction::BrowserDelete => {
            open_browser_delete_dialog(state);
        }
        TuiAction::BrowserPreviewScrollUp => state.browser_preview_scroll_up(),
        TuiAction::BrowserPreviewScrollDown => state.browser_preview_scroll_down(),
        TuiAction::BrowserPreviewScrollPageUp => state.browser_preview_scroll_page_up(),
        TuiAction::BrowserPreviewScrollPageDown => state.browser_preview_scroll_page_down(),
        TuiAction::BrowserPreviewScrollHome => state.browser_preview_scroll_home(),
        TuiAction::BrowserPreviewScrollEnd => state.browser_preview_scroll_end(),
        TuiAction::BrowserSearch => {
            open_browser_search_dialog(state);
        }
        TuiAction::BrowserSearchNext => state.browser_search_next(),
        TuiAction::BrowserSearchPrevious => state.browser_search_previous(),
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
        TuiAction::OpenJudgeDialog => {
            state.open_send_agent_message_dialog(Some("judge".to_string()));
        }
        TuiAction::OpenChainGrantDialog => {
            state.open_grant_chain_dialog(None);
        }
        TuiAction::ActivateSelectedSession => {
            if let Ok(session_id) = state.activate_selected_session() {
                load_session_into_state(app, state, &session_id)?;
            }
        }
        TuiAction::ConfirmDialog => match state.dialog_state() {
            Some(DialogState::CreateSession { value }) => {
                let title = title_or_default(value.as_str(), "Новая сессия");
                let summary = app.create_session_auto(Some(title.as_str()))?;
                let sessions = app.list_session_summaries()?;
                state.sync_sessions(sessions);
                state.close_dialog();
                state.timeline_mut().push_system(
                    &format!("создана сессия {}", summary.title),
                    unix_timestamp()?,
                );
                load_session_into_state(app, state, &summary.id)?;
            }
            Some(DialogState::CreateAgent { value }) => {
                let spec = require_arg(value.as_str(), "/agent")?;
                let (name, template_identifier) = parse_agent_create_spec(spec.as_str())?;
                let message = app.create_agent(&name, template_identifier.as_deref())?;
                state.close_dialog();
                open_agents_browser(app, state, None)?;
                state
                    .timeline_mut()
                    .push_system(&message, unix_timestamp()?);
            }
            Some(DialogState::CreateScheduleForm { form }) => {
                let (id, options) = schedule_form_create_spec(&form)?;
                let message = app.create_agent_schedule_with_options(&id, options)?;
                state.close_dialog();
                open_schedule_browser(app, state, Some(id.as_str()))?;
                state
                    .timeline_mut()
                    .push_system(&message, unix_timestamp()?);
            }
            Some(DialogState::EditScheduleForm { form }) => {
                let (id, patch) = schedule_form_edit_spec(&form)?;
                let message = app.update_agent_schedule(&id, patch)?;
                state.close_dialog();
                open_schedule_browser(app, state, Some(id.as_str()))?;
                state
                    .timeline_mut()
                    .push_system(&message, unix_timestamp()?);
            }
            Some(DialogState::CreateMcpConnectorForm { form }) => {
                let (id, options) = mcp_form_create_spec(&form)?;
                let message = app.create_mcp_connector(&id, options)?;
                state.close_dialog();
                open_mcp_browser(app, state, Some(id.as_str()))?;
                state
                    .timeline_mut()
                    .push_system(&message, unix_timestamp()?);
            }
            Some(DialogState::EditMcpConnectorForm { form }) => {
                let (id, patch) = mcp_form_edit_spec(&form)?;
                let message = app.update_mcp_connector(&id, patch)?;
                state.close_dialog();
                open_mcp_browser(app, state, Some(id.as_str()))?;
                state
                    .timeline_mut()
                    .push_system(&message, unix_timestamp()?);
            }
            Some(DialogState::SendAgentMessageForm { form }) => {
                let (target_agent_id, message) = agent_message_form_spec(&form)?;
                let current_session_id = state
                    .current_session_id()
                    .ok_or_else(|| BootstrapError::Usage {
                        reason: "не выбрана текущая сессия".to_string(),
                    })?
                    .to_string();
                let rendered =
                    app.send_agent_message(&current_session_id, &target_agent_id, &message)?;
                state.close_dialog();
                state
                    .timeline_mut()
                    .push_system(&rendered, unix_timestamp()?);
            }
            Some(DialogState::GrantChainContinuationForm { form }) => {
                let (chain_id, reason) = chain_grant_form_spec(&form)?;
                let current_session_id = state
                    .current_session_id()
                    .ok_or_else(|| BootstrapError::Usage {
                        reason: "не выбрана текущая сессия".to_string(),
                    })?
                    .to_string();
                let rendered =
                    app.grant_chain_continuation(&current_session_id, &chain_id, &reason)?;
                state.close_dialog();
                state
                    .timeline_mut()
                    .push_system(&rendered, unix_timestamp()?);
            }
            Some(DialogState::BrowserSearch { value }) => {
                state.apply_browser_search(value);
                state.close_dialog();
            }
            Some(DialogState::RenameSession { session_id, value }) => {
                let title = title_or_default(value.as_str(), "Новая сессия");
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
                    &format!("сессия переименована в {}", summary.title),
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
                let replacement = app.clear_session(&session_id, Some("Новая сессия"))?;
                state.close_dialog();
                state.sync_sessions(app.list_session_summaries()?);
                load_session_into_state(app, state, &replacement.id)?;
                state
                    .timeline_mut()
                    .push_system("сессия очищена", unix_timestamp()?);
            }
            Some(DialogState::ConfirmDeleteSchedule { id }) => {
                let message = app.delete_agent_schedule(&id)?;
                state.close_dialog();
                open_schedule_browser(app, state, None)?;
                state
                    .timeline_mut()
                    .push_system(&message, unix_timestamp()?);
            }
            Some(DialogState::ConfirmDeleteMcpConnector { id }) => {
                let message = app.delete_mcp_connector(&id)?;
                state.close_dialog();
                open_mcp_browser(app, state, None)?;
                state
                    .timeline_mut()
                    .push_system(&message, unix_timestamp()?);
            }
            None => {}
        },
        TuiAction::SubmitChatInput(input) => {
            if is_command_input(input.as_str()) {
                if let Err(error) = handle_command(app, state, input.trim(), redraw) {
                    match error {
                        BootstrapError::Usage { reason } => {
                            state.timeline_mut().push_system(&reason, unix_timestamp()?);
                        }
                        other => return Err(other),
                    }
                }
            } else {
                submit_chat_message(app, state, input.trim(), QueuedDraftMode::Priority)?;
            }
        }
        TuiAction::QueueChatInput(input) => {
            if is_command_input(input.as_str()) {
                state.timeline_mut().push_system(
                    "команды нельзя ставить в очередь; нажмите Enter, чтобы выполнить сразу",
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

fn open_agents_browser<B>(
    app: &B,
    state: &mut TuiAppState,
    preferred_id: Option<&str>,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let rendered = app.render_agents()?;
    let parsed = parse_agent_browser_items(&rendered);
    if parsed.items.is_empty() {
        state.open_agent_browser(
            "Агенты".to_string(),
            "Н создать".to_string(),
            Vec::new(),
            0,
            "Агенты".to_string(),
            rendered,
        );
        return Ok(());
    }
    let selected_index = preferred_id
        .and_then(|id| parsed.items.iter().position(|item| item.id == id))
        .unwrap_or(parsed.selected_index);
    let selected_id = parsed
        .items
        .get(selected_index)
        .map(|item| item.id.as_str())
        .unwrap_or_default()
        .to_string();
    let preview_content = app.render_agent(Some(selected_id.as_str()))?;
    state.open_agent_browser(
        "Агенты".to_string(),
        "↑↓ выбор | Enter выбрать | Н создать | С написать | О дом".to_string(),
        parsed.items,
        selected_index,
        format!("Агент {selected_id}"),
        preview_content,
    );
    Ok(())
}

fn open_schedule_browser<B>(
    app: &B,
    state: &mut TuiAppState,
    preferred_id: Option<&str>,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let rendered = app.render_agent_schedules()?;
    let items = parse_schedule_browser_items(&rendered);
    if items.is_empty() {
        state.open_schedule_browser(
            "Расписания".to_string(),
            "Н создать".to_string(),
            Vec::new(),
            0,
            "Расписания".to_string(),
            rendered,
        );
        return Ok(());
    }
    let selected_index = preferred_id
        .and_then(|id| items.iter().position(|item| item.id == id))
        .unwrap_or(0);
    let selected_id = items
        .get(selected_index)
        .map(|item| item.id.as_str())
        .unwrap_or_default()
        .to_string();
    let preview_content = app.render_agent_schedule(selected_id.as_str())?;
    state.open_schedule_browser(
        "Расписания".to_string(),
        "↑↓ выбор | Н создать | Р изменить | П вкл/выкл | У удалить".to_string(),
        items,
        selected_index,
        format!("Расписание {selected_id}"),
        preview_content,
    );
    Ok(())
}

fn open_mcp_browser<B>(
    app: &B,
    state: &mut TuiAppState,
    preferred_id: Option<&str>,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let rendered = app.render_mcp_connectors()?;
    let items = parse_mcp_browser_items(&rendered);
    if items.is_empty() {
        state.open_mcp_browser(
            "MCP".to_string(),
            "Н создать".to_string(),
            Vec::new(),
            0,
            "MCP".to_string(),
            rendered,
        );
        return Ok(());
    }
    let selected_index = preferred_id
        .and_then(|id| items.iter().position(|item| item.id == id))
        .unwrap_or(0);
    let selected_id = items
        .get(selected_index)
        .map(|item| item.id.as_str())
        .unwrap_or_default()
        .to_string();
    let preview_content = app.render_mcp_connector(selected_id.as_str())?;
    state.open_mcp_browser(
        "MCP".to_string(),
        "↑↓ выбор | Н создать | Р изменить | П вкл/выкл | С перезапуск | У удалить".to_string(),
        items,
        selected_index,
        format!("MCP {}", selected_id),
        preview_content,
    );
    Ok(())
}

fn open_artifact_browser<B>(
    app: &B,
    state: &mut TuiAppState,
    session_id: &str,
    preferred_id: Option<&str>,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let rendered = app.render_artifacts(session_id)?;
    let items = parse_artifact_browser_items(&rendered);
    if items.is_empty() {
        state.open_artifact_browser(
            "Артефакты".to_string(),
            "↑↓ выбор | Enter полный | / поиск | n/N | PgUp/PgDn".to_string(),
            Vec::new(),
            0,
            "Артефакты".to_string(),
            rendered,
        );
        return Ok(());
    }
    let selected_index = preferred_id
        .and_then(|id| items.iter().position(|item| item.id == id))
        .unwrap_or(0);
    let selected_id = items
        .get(selected_index)
        .map(|item| item.id.as_str())
        .unwrap_or_default()
        .to_string();
    let preview_content = app.read_artifact(session_id, selected_id.as_str())?;
    state.open_artifact_browser(
        "Артефакты".to_string(),
        "↑↓ выбор | Enter полный | / поиск | n/N | PgUp/PgDn".to_string(),
        items,
        selected_index,
        format!("Артефакт {selected_id}"),
        preview_content,
    );
    Ok(())
}

fn refresh_browser_preview<B>(app: &B, state: &mut TuiAppState) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let Some(selected) = state.browser_selected_item().cloned() else {
        return Ok(());
    };
    let Some(kind) = state.browser_state().map(|browser| browser.kind()) else {
        return Ok(());
    };
    let (title, content) = match kind {
        BrowserKind::Agents => (
            format!("Агент {}", selected.id),
            app.render_agent(Some(selected.id.as_str()))?,
        ),
        BrowserKind::Schedules => (
            format!("Расписание {}", selected.id),
            app.render_agent_schedule(selected.id.as_str())?,
        ),
        BrowserKind::Mcp => (
            format!("MCP {}", selected.id),
            app.render_mcp_connector(selected.id.as_str())?,
        ),
        BrowserKind::Artifacts => {
            let session_id = state
                .current_session_id()
                .ok_or_else(|| BootstrapError::Usage {
                    reason: "не выбрана текущая сессия".to_string(),
                })?;
            (
                format!("Артефакт {}", selected.id),
                app.read_artifact(session_id, selected.id.as_str())?,
            )
        }
    };
    state.set_browser_preview(title, content);
    Ok(())
}

fn activate_browser_selection<B>(app: &B, state: &mut TuiAppState) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let Some(selected_id) = state.browser_selected_item().map(|item| item.id.clone()) else {
        return Ok(());
    };
    let Some(kind) = state.browser_state().map(|browser| browser.kind()) else {
        return Ok(());
    };
    match kind {
        BrowserKind::Agents => {
            let message = app.select_agent(selected_id.as_str())?;
            state.sync_sessions(app.list_session_summaries()?);
            open_agents_browser(app, state, Some(selected_id.as_str()))?;
            state
                .timeline_mut()
                .push_system(&message, unix_timestamp()?);
        }
        BrowserKind::Schedules => {
            refresh_browser_preview(app, state)?;
        }
        BrowserKind::Mcp => {
            refresh_browser_preview(app, state)?;
        }
        BrowserKind::Artifacts => state.toggle_browser_full_preview(),
    }
    Ok(())
}

fn open_browser_selection<B>(app: &B, state: &mut TuiAppState) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let Some(selected_id) = state.browser_selected_item().map(|item| item.id.clone()) else {
        return Ok(());
    };
    let Some(kind) = state.browser_state().map(|browser| browser.kind()) else {
        return Ok(());
    };
    match kind {
        BrowserKind::Agents => {
            let home = app.open_agent_home(Some(selected_id.as_str()))?;
            state.set_browser_preview(format!("Дом агента {selected_id}"), home);
        }
        BrowserKind::Schedules | BrowserKind::Mcp | BrowserKind::Artifacts => {
            refresh_browser_preview(app, state)?;
        }
    }
    Ok(())
}

fn open_browser_create_dialog(state: &mut TuiAppState) {
    match state.browser_state().map(|browser| browser.kind()) {
        Some(BrowserKind::Agents) => state.open_create_agent_dialog(),
        Some(BrowserKind::Schedules) => state.open_create_schedule_dialog(),
        Some(BrowserKind::Mcp) => state.open_create_mcp_connector_dialog(),
        Some(BrowserKind::Artifacts) | None => {}
    }
}

fn handle_browser_message_action<B>(app: &B, state: &mut TuiAppState) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    if matches!(
        state.browser_state().map(|browser| browser.kind()),
        Some(BrowserKind::Agents)
    ) {
        let target_agent_id = state.browser_selected_item().map(|item| item.id.clone());
        state.open_send_agent_message_dialog(target_agent_id);
        return Ok(());
    }
    if matches!(
        state.browser_state().map(|browser| browser.kind()),
        Some(BrowserKind::Mcp)
    ) && let Some(selected_id) = state.browser_selected_item().map(|item| item.id.clone())
    {
        let message = app.restart_mcp_connector(selected_id.as_str())?;
        open_mcp_browser(app, state, Some(selected_id.as_str()))?;
        state
            .timeline_mut()
            .push_system(&message, unix_timestamp()?);
    }
    Ok(())
}

fn open_browser_edit_dialog<B>(app: &B, state: &mut TuiAppState) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    if matches!(
        state.browser_state().map(|browser| browser.kind()),
        Some(BrowserKind::Schedules)
    ) && let Some(selected) = state.browser_selected_item()
    {
        let schedule = app.load_agent_schedule(selected.id.as_str())?;
        state.open_edit_schedule_dialog(schedule);
    } else if matches!(
        state.browser_state().map(|browser| browser.kind()),
        Some(BrowserKind::Mcp)
    ) && let Some(selected) = state.browser_selected_item()
    {
        let connector = app.load_mcp_connector(selected.id.as_str())?;
        state.open_edit_mcp_connector_dialog(connector);
    }
    Ok(())
}

fn open_browser_delete_dialog(state: &mut TuiAppState) {
    if matches!(
        state.browser_state().map(|browser| browser.kind()),
        Some(BrowserKind::Schedules)
    ) && let Some(selected) = state.browser_selected_item()
    {
        state.open_delete_schedule_dialog(selected.id.clone());
    } else if matches!(
        state.browser_state().map(|browser| browser.kind()),
        Some(BrowserKind::Mcp)
    ) && let Some(selected) = state.browser_selected_item()
    {
        state.open_delete_mcp_connector_dialog(selected.id.clone());
    }
}

fn toggle_browser_schedule<B>(app: &B, state: &mut TuiAppState) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    if !matches!(
        state.browser_state().map(|browser| browser.kind()),
        Some(BrowserKind::Schedules | BrowserKind::Mcp)
    ) {
        return Ok(());
    }
    let Some(selected_id) = state.browser_selected_item().map(|item| item.id.clone()) else {
        return Ok(());
    };
    let message = if matches!(
        state.browser_state().map(|browser| browser.kind()),
        Some(BrowserKind::Schedules)
    ) {
        let schedule = app.load_agent_schedule(selected_id.as_str())?;
        let message = app.set_agent_schedule_enabled(selected_id.as_str(), !schedule.enabled)?;
        open_schedule_browser(app, state, Some(selected_id.as_str()))?;
        message
    } else {
        let connector = app.load_mcp_connector(selected_id.as_str())?;
        let message = app.set_mcp_connector_enabled(selected_id.as_str(), !connector.enabled)?;
        open_mcp_browser(app, state, Some(selected_id.as_str()))?;
        message
    };
    state
        .timeline_mut()
        .push_system(&message, unix_timestamp()?);
    Ok(())
}

fn open_browser_search_dialog(state: &mut TuiAppState) {
    if matches!(
        state.browser_state().map(|browser| browser.kind()),
        Some(BrowserKind::Artifacts)
    ) {
        state.open_browser_search_dialog();
    }
}

#[derive(Debug)]
struct ParsedAgentBrowser {
    items: Vec<BrowserItem>,
    selected_index: usize,
}

fn parse_agent_browser_items(rendered: &str) -> ParsedAgentBrowser {
    let mut items = Vec::new();
    let mut selected_index = 0usize;
    for line in rendered.lines() {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("* ") {
            Some('*')
        } else if trimmed.starts_with("- ") {
            Some('-')
        } else {
            None
        };
        let Some(marker) = marker else {
            continue;
        };
        let Some((id, label)) = parse_agent_browser_line(trimmed) else {
            continue;
        };
        if marker == '*' {
            selected_index = items.len();
        }
        items.push(BrowserItem { id, label });
    }
    ParsedAgentBrowser {
        items,
        selected_index,
    }
}

fn parse_agent_browser_line(line: &str) -> Option<(String, String)> {
    let body = line
        .strip_prefix("* ")
        .or_else(|| line.strip_prefix("- "))?
        .trim();
    let id_start = body.rfind(" (")?;
    let id_end = body[id_start + 2..].find(')')? + id_start + 2;
    let id = body[id_start + 2..id_end].to_string();
    let label = body.to_string();
    Some((id, label))
}

fn parse_schedule_browser_items(rendered: &str) -> Vec<BrowserItem> {
    rendered
        .lines()
        .filter_map(|line| {
            let body = line.trim_start().strip_prefix("- ")?;
            let id = body.split_whitespace().next()?.to_string();
            Some(BrowserItem {
                id,
                label: body.to_string(),
            })
        })
        .collect()
}

fn parse_mcp_browser_items(rendered: &str) -> Vec<BrowserItem> {
    rendered
        .lines()
        .filter_map(|line| {
            let body = line.trim_start().strip_prefix("- ")?;
            let id = body.split_whitespace().next()?.to_string();
            Some(BrowserItem {
                id,
                label: body.to_string(),
            })
        })
        .collect()
}

fn parse_artifact_browser_items(rendered: &str) -> Vec<BrowserItem> {
    rendered
        .lines()
        .filter_map(|line| {
            let body = line.trim_start().strip_prefix("- ")?;
            let id = body.split_whitespace().next()?.to_string();
            Some(BrowserItem {
                id,
                label: body.to_string(),
            })
        })
        .collect()
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
    let mut parts = raw.splitn(2, ' ');
    let command = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default().trim();

    match canonical_command(command) {
        Some("/session") => state.open_session_screen(),
        Some("/new") => {
            let summary = app.create_session_auto(Some("Новая сессия"))?;
            state.sync_sessions(app.list_session_summaries()?);
            load_session_into_state(app, state, &summary.id)?;
        }
        Some("/agents") => {
            open_agents_browser(app, state, None)?;
        }
        Some("/agent") => {
            let current_session_id = state.current_session_id().map(str::to_string);
            let message = handle_agent_command(app, current_session_id.as_deref(), rest)?;
            if rest.is_empty() || rest.starts_with("показать") || rest.starts_with("show") {
                let identifier = option_arg(rest.strip_prefix("показать").unwrap_or(rest))
                    .or_else(|| option_arg(rest.strip_prefix("show").unwrap_or(rest)));
                if let Some(identifier) = identifier.as_deref() {
                    state.open_agent_screen(format!("Агент {identifier}"), message);
                } else {
                    open_agents_browser(app, state, None)?;
                }
            } else {
                state
                    .timeline_mut()
                    .push_system(&message, unix_timestamp()?);
            }
            state.sync_sessions(app.list_session_summaries()?);
        }
        Some("/judge") => {
            let current_session_id = state
                .current_session_id()
                .ok_or_else(|| BootstrapError::Usage {
                    reason: "не выбрана текущая сессия".to_string(),
                })?
                .to_string();
            let message = app.send_agent_message(
                &current_session_id,
                "judge",
                require_arg(rest, "/judge")?.as_str(),
            )?;
            state
                .timeline_mut()
                .push_system(&message, unix_timestamp()?);
        }
        Some("/schedules") => {
            open_schedule_browser(app, state, None)?;
        }
        Some("/mcp") => {
            let message = handle_mcp_command(app, rest)?;
            if rest.is_empty() || rest.starts_with("показать") || rest.starts_with("show") {
                let selected_id = option_arg(rest.strip_prefix("показать").unwrap_or(rest))
                    .or_else(|| option_arg(rest.strip_prefix("show").unwrap_or(rest)));
                if let Some(selected_id) = selected_id.as_deref() {
                    state.open_mcp_screen(format!("MCP {selected_id}"), message);
                } else {
                    open_mcp_browser(app, state, None)?;
                }
            } else {
                state
                    .timeline_mut()
                    .push_system(&message, unix_timestamp()?);
            }
        }
        Some("/schedule") => {
            let message = handle_schedule_command(app, rest)?;
            if rest.is_empty() || rest.starts_with("показать") || rest.starts_with("show") {
                let selected_id = option_arg(rest.strip_prefix("показать").unwrap_or(rest))
                    .or_else(|| option_arg(rest.strip_prefix("show").unwrap_or(rest)));
                if let Some(selected_id) = selected_id.as_deref() {
                    state.open_schedule_screen(format!("Расписание {selected_id}"), message);
                } else {
                    open_schedule_browser(app, state, None)?;
                }
            } else {
                state
                    .timeline_mut()
                    .push_system(&message, unix_timestamp()?);
            }
        }
        Some("/version") => {
            let about = app.render_version_info()?;
            state.timeline_mut().push_system(&about, unix_timestamp()?);
        }
        Some("/update") => {
            let message = app.update_runtime(option_arg(rest).as_deref())?;
            state
                .timeline_mut()
                .push_system(&message, unix_timestamp()?);
        }
        Some("/rename") => {
            let _ = state.open_rename_dialog();
        }
        Some("/clear") => {
            let _ = state.open_clear_dialog();
        }
        Some("/help") => {
            let topic = parse_help_topic(option_arg(rest).as_deref()).map_err(|reason| {
                BootstrapError::Usage {
                    reason: render_command_usage_error("/help", reason.as_str()),
                }
            })?;
            state
                .timeline_mut()
                .push_system(&render_help(topic), unix_timestamp()?);
        }
        Some("/settings") => {
            state
                .timeline_mut()
                .push_system(&render_help(HelpTopic::Settings), unix_timestamp()?);
        }
        Some(command) => {
            let current_session_id = state
                .current_session_id()
                .ok_or_else(|| BootstrapError::Usage {
                    reason: "не выбрана текущая сессия".to_string(),
                })?
                .to_string();
            match command {
                "/system" => {
                    let system = app.render_system(&current_session_id)?;
                    state.timeline_mut().push_system(&system, unix_timestamp()?);
                }
                "/context" => {
                    let context = app.render_context(&current_session_id)?;
                    state
                        .timeline_mut()
                        .push_system(&context, unix_timestamp()?);
                }
                "/plan" => {
                    let plan = app.render_plan(&current_session_id)?;
                    state.timeline_mut().push_system(&plan, unix_timestamp()?);
                }
                "/chain" => {
                    let message = handle_chain_command(app, &current_session_id, rest)?;
                    state
                        .timeline_mut()
                        .push_system(&message, unix_timestamp()?);
                }
                "/status" => {
                    let run = app.render_active_run(&current_session_id)?;
                    state.timeline_mut().push_system(&run, unix_timestamp()?);
                }
                "/processes" => {
                    let run = app.render_active_run(&current_session_id)?;
                    state.timeline_mut().push_system(&run, unix_timestamp()?);
                }
                "/pause" => {
                    let message = if let Some(active_run) = state.active_run() {
                        active_run.queue_interrupt_after_tool_step();
                        let stopped =
                            app.cancel_active_run(&current_session_id, unix_timestamp()?)?;
                        format!("пауза пока реализована как операторская остановка: {stopped}")
                    } else {
                        "пауза не нужна: активного хода нет".to_string()
                    };
                    state
                        .timeline_mut()
                        .push_system(&message, unix_timestamp()?);
                }
                "/stop" => {
                    if let Some(active_run) = state.active_run() {
                        active_run.queue_interrupt_after_tool_step();
                    }
                    let message = app.cancel_active_run(&current_session_id, unix_timestamp()?)?;
                    state
                        .timeline_mut()
                        .push_system(&message, unix_timestamp()?);
                }
                "/cancel" => {
                    if let Some(active_run) = state.active_run() {
                        active_run.queue_interrupt_after_tool_step();
                    }
                    let message =
                        app.cancel_all_session_work(&current_session_id, unix_timestamp()?)?;
                    state
                        .timeline_mut()
                        .push_system(&message, unix_timestamp()?);
                }
                "/jobs" => {
                    let jobs = app.render_active_jobs(&current_session_id)?;
                    state.timeline_mut().push_system(&jobs, unix_timestamp()?);
                }
                "/memory" => {
                    let memory = handle_memory_command(app, rest)?;
                    state.timeline_mut().push_system(&memory, unix_timestamp()?);
                }
                "/artifacts" => {
                    open_artifact_browser(app, state, &current_session_id, None)?;
                }
                "/artifact" => {
                    let artifact_id = require_arg(rest, "/artifact")?;
                    open_artifact_browser(
                        app,
                        state,
                        &current_session_id,
                        Some(artifact_id.as_str()),
                    )?;
                }
                "/debug" => {
                    let backend_saved = app.write_debug_bundle(&current_session_id)?;
                    let saved = write_combined_tui_debug_bundle(
                        app,
                        state,
                        &current_session_id,
                        backend_saved.as_str(),
                    )?;
                    state.timeline_mut().push_system(
                        &format!("отладочный пакет сохранён: {saved}"),
                        unix_timestamp()?,
                    );
                }
                "/skills" => {
                    let rendered = render_session_skills(app.session_skills(&current_session_id)?);
                    state
                        .timeline_mut()
                        .push_system(&rendered, unix_timestamp()?);
                }
                "/completion" => {
                    let value = require_arg(rest, "/completion")?;
                    let completion_nudges = parse_completion_nudges(value.as_str())?;
                    let summary = app.update_session_preferences(
                        &current_session_id,
                        crate::bootstrap::SessionPreferencesPatch {
                            completion_nudges: Some(completion_nudges),
                            ..crate::bootstrap::SessionPreferencesPatch::default()
                        },
                    )?;
                    state.replace_current_summary(summary);
                    state.sync_sessions(app.list_session_summaries()?);
                    state.timeline_mut().push_system(
                        &format!(
                            "режим доводки: {}",
                            describe_completion_mode(completion_nudges)
                        ),
                        unix_timestamp()?,
                    );
                }
                "/autoapprove" => {
                    let value = require_arg(rest, "/autoapprove")?;
                    let auto_approve = parse_auto_approve(value.as_str())?;
                    let summary = app.update_session_preferences(
                        &current_session_id,
                        crate::bootstrap::SessionPreferencesPatch {
                            auto_approve: Some(auto_approve),
                            ..crate::bootstrap::SessionPreferencesPatch::default()
                        },
                    )?;
                    state.replace_current_summary(summary);
                    state.sync_sessions(app.list_session_summaries()?);
                    state.timeline_mut().push_system(
                        &format!(
                            "автоапрув {}",
                            if auto_approve {
                                "включён"
                            } else {
                                "выключен"
                            }
                        ),
                        unix_timestamp()?,
                    );
                    if auto_approve {
                        schedule_next_draft_if_idle(app, state, &current_session_id)?;
                    }
                }
                "/enable" => {
                    let skill_name = require_arg(rest, "/enable")?;
                    let updated = app.enable_session_skill(&current_session_id, &skill_name)?;
                    let rendered = render_session_skills(updated);
                    state
                        .timeline_mut()
                        .push_system(&rendered, unix_timestamp()?);
                }
                "/disable" => {
                    let skill_name = require_arg(rest, "/disable")?;
                    let updated = app.disable_session_skill(&current_session_id, &skill_name)?;
                    let rendered = render_session_skills(updated);
                    state
                        .timeline_mut()
                        .push_system(&rendered, unix_timestamp()?);
                }
                "/approve" => {
                    approve_pending(app, state, &current_session_id, option_arg(rest), redraw)?
                }
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
                        "on" | "вкл" | "enable" => true,
                        "off" | "выкл" | "disable" => false,
                        value => {
                            return Err(BootstrapError::Usage {
                                reason: render_command_usage_error(
                                    "/reasoning",
                                    &format!(
                                        "неподдерживаемый режим размышлений {value}; ожидается вкл|выкл"
                                    ),
                                ),
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
                        "компактификация пропущена: истории ещё недостаточно"
                    } else {
                        "компактификация контекста завершена"
                    };
                    state.timeline_mut().push_system(message, unix_timestamp()?);
                }
                "/exit" => state.request_exit(),
                _ => {
                    state
                        .timeline_mut()
                        .push_system(&format!("неизвестная команда {raw}"), unix_timestamp()?);
                }
            }
        }
        _ => {
            state
                .timeline_mut()
                .push_system(&format!("неизвестная команда {raw}"), unix_timestamp()?);
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
            reason: "не выбрана текущая сессия".to_string(),
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
            &format!("для session_id={session_id} нет ожидающего апрува"),
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
    let was_following_tail = state.scroll_offset() == 0;
    let now = unix_timestamp()?;
    let mut outcome = None;
    for event in events {
        match event {
            WorkerEvent::Chat(chat_event) => {
                let at = now;
                match chat_event {
                    ChatExecutionEvent::ReasoningDelta(delta) => {
                        state.timeline_mut().push_reasoning_delta(&delta, at);
                    }
                    ChatExecutionEvent::AssistantTextDelta(delta) => {
                        state.timeline_mut().push_assistant_delta(&delta, at);
                    }
                    ChatExecutionEvent::ProviderLoopProgress {
                        current_round,
                        max_rounds,
                    } => {
                        state.set_provider_loop_progress(current_round, max_rounds);
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

    if outcome.is_none()
        && let Some(message) = state
            .active_run_mut()
            .and_then(|active_run| active_run.heartbeat_notice(now, 30))
    {
        state.timeline_mut().push_system(&message, now);
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

    if was_following_tail {
        state.scroll_to_bottom();
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
            state.clear_provider_loop_progress();
            if !report.output_text.is_empty() {
                state
                    .timeline_mut()
                    .finalize_assistant_output(&report.output_text, unix_timestamp()?);
            }
            state.timeline_mut().finish_turn();
        }
        WorkerOutcome::ApprovalCompleted(report) => {
            state.clear_provider_loop_progress();
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
            state.scroll_to_bottom();
            state
                .timeline_mut()
                .push_approval(&approval_id, &reason, unix_timestamp()?);
            state.timeline_mut().finish_turn();
        }
        WorkerOutcome::Cancelled => {
            state.clear_provider_loop_progress();
            state
                .timeline_mut()
                .push_system("текущий ход остановлен оператором", unix_timestamp()?);
            state.timeline_mut().finish_turn();
        }
        WorkerOutcome::InterruptedByQueuedInput => {
            state.clear_provider_loop_progress();
            let interrupted_for_pause = state.queued_priority_count() == 0;
            let message = if interrupted_for_pause {
                "текущий ход поставлен на паузу оператором"
            } else {
                "текущий ответ прерван сообщением из очереди"
            };
            state.timeline_mut().push_system(message, unix_timestamp()?);
            state.timeline_mut().finish_turn();
        }
        WorkerOutcome::Failed(reason) => {
            state.clear_provider_loop_progress();
            state
                .timeline_mut()
                .push_system(&format!("ошибка чата: {reason}"), unix_timestamp()?);
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
    if state.has_active_run() {
        return Ok(());
    }
    if state
        .current_session_summary()
        .is_some_and(|summary| summary.auto_approve)
        && let Some(pending) = app.latest_pending_approval(session_id, None)?
    {
        state.timeline_mut().remove_approval(&pending.approval_id);
        state.timeline_mut().push_system(
            &format!("автоапрув ожидающего запроса: {}", pending.reason),
            unix_timestamp()?,
        );
        state.scroll_to_bottom();
        start_approval_run(
            app,
            state,
            session_id,
            &pending.run_id,
            &pending.approval_id,
            unix_timestamp()?,
        )?;
        return Ok(());
    }
    if app.latest_pending_approval(session_id, None)?.is_some() {
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
    let timeline = load_session_timeline(app, session_id)?;
    state.set_current_session(summary, timeline);
    state.scroll_to_bottom();
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
        let previous_timeline = state.timeline().clone();
        let mut timeline = load_session_timeline(app, &session_id)?;
        timeline.merge_ephemeral_from(&previous_timeline);
        state.replace_timeline(timeline);
    }
    Ok(())
}

fn load_session_timeline<B>(app: &B, session_id: &str) -> Result<Timeline, BootstrapError>
where
    B: TuiBackend,
{
    let transcript = app.session_transcript(session_id)?;
    let pending = app.pending_approvals(session_id)?;
    Ok(Timeline::from_session_view(&transcript, &pending))
}

fn write_combined_tui_debug_bundle<B>(
    app: &B,
    state: &TuiAppState,
    session_id: &str,
    backend_debug_bundle_path: &str,
) -> Result<String, BootstrapError>
where
    B: TuiBackend,
{
    let saved_at = unix_timestamp()?;
    let output_path = tui_debug_bundle_output_path(session_id, backend_debug_bundle_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(BootstrapError::Stream)?;
    }

    let backend_context = app.render_context(session_id)?;
    let (terminal_width, terminal_height) =
        crossterm::terminal::size().map_or((0, 0), |(width, height)| (width, height));
    let viewport = render::chat_viewport_debug(
        state,
        ratatui::layout::Rect::new(0, 0, terminal_width, terminal_height),
    );
    let backend_bundle_contents = fs::read_to_string(backend_debug_bundle_path).ok();

    let mut lines = vec![
        "TUI Debug Bundle".to_string(),
        format!("generated_at={saved_at}"),
        format!("version={}", crate::about::APP_VERSION),
        format!("backend_debug_bundle_path={backend_debug_bundle_path}"),
        format!("screen={:?}", state.active_screen()),
        format!("session_id={session_id}"),
    ];

    if let Some(summary) = state.current_session_summary() {
        lines.push(format!("session_title={}", summary.title));
        match (
            summary.usage_input_tokens,
            summary.usage_output_tokens,
            summary.usage_total_tokens,
        ) {
            (Some(input), Some(output), Some(total)) => {
                lines.push(format!(
                    "summary_usage=input:{input} output:{output} total:{total}"
                ));
            }
            _ => lines.push(format!("summary_approx_ctx={}", summary.context_tokens)),
        }
        lines.push(format!("summary_messages={}", summary.message_count));
        lines.push(format!(
            "summary_reasoning_visible={}",
            summary.reasoning_visible
        ));
    }

    lines.push(String::new());
    lines.push("Viewport:".to_string());
    if let Some(viewport) = viewport {
        lines.push(format!("terminal_width={}", viewport.terminal_width));
        lines.push(format!("terminal_height={}", viewport.terminal_height));
        lines.push(format!("composer_height={}", viewport.composer_height));
        lines.push(format!(
            "timeline_viewport_width={}",
            viewport.timeline_viewport_width
        ));
        lines.push(format!(
            "timeline_viewport_height={}",
            viewport.timeline_viewport_height
        ));
        lines.push(format!(
            "timeline_total_lines={}",
            viewport.timeline_total_lines
        ));
        lines.push(format!(
            "timeline_scroll_top={}",
            viewport.timeline_scroll_top
        ));
        lines.push(format!("scroll_offset={}", viewport.scroll_offset));
        lines.push(format!("reasoning_visible={}", viewport.reasoning_visible));
        lines.push(format!(
            "visible_entry_count={}",
            viewport.visible_entry_count
        ));
        lines.push(format!("total_entry_count={}", viewport.total_entry_count));
    } else {
        lines.push("viewport_unavailable=true".to_string());
    }

    lines.push(String::new());
    lines.push("Composer:".to_string());
    lines.push(format!("input_cursor={}", state.input_cursor()));
    lines.push(format!("input_buffer_len={}", state.input_buffer().len()));
    lines.push(format!(
        "input_line_count={}",
        state.input_buffer().split('\n').count()
    ));
    lines.push(format!("queued_priority={}", state.queued_priority_count()));
    lines.push(format!("queued_deferred={}", state.queued_deferred_count()));

    lines.push(String::new());
    lines.push("Backend Context Snapshot:".to_string());
    lines.push(backend_context);

    lines.push(String::new());
    lines.push("Backend Bundle Contents:".to_string());
    match backend_bundle_contents {
        Some(contents) => lines.push(contents),
        None => lines.push("<unavailable from local TUI process>".to_string()),
    }

    fs::write(&output_path, lines.join("\n")).map_err(BootstrapError::Stream)?;
    Ok(output_path.display().to_string())
}

fn tui_debug_bundle_output_path(session_id: &str, backend_debug_bundle_path: &str) -> PathBuf {
    let backend_path = PathBuf::from(backend_debug_bundle_path);
    if backend_path.exists()
        && let Some(parent) = backend_path.parent()
    {
        let stem = backend_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("debug-bundle");
        return parent.join(format!("{stem}-tui.txt"));
    }

    let root = std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir());
    root.join(".teamd-debug").join(format!(
        "tui-{}-{}.txt",
        sanitize_tui_debug_identifier(session_id),
        unix_timestamp().unwrap_or_default()
    ))
}

fn sanitize_tui_debug_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn require_arg(raw: &str, command: &str) -> Result<String, BootstrapError> {
    if raw.trim().is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(command, "не хватает аргументов"),
        });
    }
    Ok(raw.trim().to_string())
}

fn option_arg(raw: &str) -> Option<String> {
    (!raw.trim().is_empty()).then(|| raw.trim().to_string())
}

fn parse_completion_nudges(raw: &str) -> Result<Option<u32>, BootstrapError> {
    let trimmed = raw.trim();
    if matches!(trimmed, "off" | "выкл" | "disable") {
        return Ok(None);
    }
    trimmed
        .parse::<u32>()
        .map(Some)
        .map_err(|_| BootstrapError::Usage {
            reason: render_command_usage_error(
                "/completion",
                &format!(
                    "неподдерживаемый режим доводки {trimmed}; ожидается выкл или неотрицательное число"
                ),
            ),
        })
}

fn describe_completion_mode(completion_nudges: Option<u32>) -> String {
    match completion_nudges {
        None => "выключен".to_string(),
        Some(0) => "включён: после первой ранней остановки сразу нужен апрув оператора".to_string(),
        Some(value) => format!("включён: {value} автоматических пинка перед апрувом"),
    }
}

fn parse_auto_approve(raw: &str) -> Result<bool, BootstrapError> {
    match raw.trim() {
        "on" | "1" | "yes" | "да" | "вкл" | "enable" => Ok(true),
        "off" | "0" | "no" | "нет" | "выкл" | "disable" => Ok(false),
        value => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/autoapprove",
                &format!("неподдерживаемый режим автоапрува {value}; ожидается вкл|выкл"),
            ),
        }),
    }
}

fn is_command_input(input: &str) -> bool {
    let trimmed = input.trim_start();
    trimmed.starts_with('/') || trimmed.starts_with('\\')
}

fn canonical_command(command: &str) -> Option<&'static str> {
    let normalized = command.trim_end_matches(['\\', '/']);
    match normalized {
        "/session" | "\\сессии" => Some("/session"),
        "/new" | "\\новая" => Some("/new"),
        "/agents" | "\\агенты" => Some("/agents"),
        "/agent" | "\\агент" => Some("/agent"),
        "/judge" | "/судья" | "\\судья" => Some("/judge"),
        "/schedules" | "\\расписания" => Some("/schedules"),
        "/schedule" | "\\расписание" => Some("/schedule"),
        "/mcp" | "\\mcp" => Some("/mcp"),
        "/memory" | "/память" | "\\память" => Some("/memory"),
        "/chain" | "/цепочка" | "\\цепочка" => Some("/chain"),
        "/rename" | "\\переименовать" => Some("/rename"),
        "/clear" | "\\очистить" => Some("/clear"),
        "/help" | "\\помощь" => Some("/help"),
        "/version" | "/версия" | "\\версия" => Some("/version"),
        "/update" | "/обновить" | "\\обновить" => Some("/update"),
        "/settings" | "\\настройки" => Some("/settings"),
        "/debug" | "\\отладка" => Some("/debug"),
        "/system" | "/система" | "\\система" => Some("/system"),
        "/plan" | "\\план" => Some("/plan"),
        "/status" | "\\статус" => Some("/status"),
        "/processes" | "\\процессы" => Some("/processes"),
        "/pause" | "\\пауза" => Some("/pause"),
        "/stop" | "\\стоп" => Some("/stop"),
        "/cancel" | "\\отмена" => Some("/cancel"),
        "/jobs" | "\\задачи" => Some("/jobs"),
        "/artifacts" | "/артефакты" | "\\артефакты" => Some("/artifacts"),
        "/artifact" | "/артефакт" | "\\артефакт" => Some("/artifact"),
        "/context" | "\\контекст" => Some("/context"),
        "/completion" | "\\доводка" => Some("/completion"),
        "/autoapprove" | "\\автоапрув" => Some("/autoapprove"),
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

fn handle_agent_command<B>(
    app: &B,
    session_id: Option<&str>,
    raw: &str,
) -> Result<String, BootstrapError>
where
    B: TuiBackend,
{
    let trimmed = raw.trim();
    let (action, tail) = match trimmed.split_once(' ') {
        Some((action, tail)) => (action.trim(), tail.trim()),
        None => (trimmed, ""),
    };

    match action {
        "" => app.render_agent(None),
        "показать" | "show" => app.render_agent(option_arg(tail).as_deref()),
        "выбрать" | "select" => app.select_agent(&require_arg(tail, "/agent")?),
        "создать" | "create" => {
            let spec = require_arg(tail, "/agent")?;
            let (name, template_identifier) = parse_agent_create_spec(spec.as_str())?;
            app.create_agent(&name, template_identifier.as_deref())
        }
        "открыть" | "open" => app.open_agent_home(option_arg(tail).as_deref()),
        "написать" | "message" => {
            let session_id = session_id.ok_or_else(|| BootstrapError::Usage {
                reason: "не выбрана текущая сессия".to_string(),
            })?;
            let (target_agent_id, message) =
                split_head_tail(tail).ok_or_else(|| BootstrapError::Usage {
                    reason: render_command_usage_error("/agent", "не хватает аргументов"),
                })?;
            app.send_agent_message(session_id, target_agent_id, message)
        }
        _ => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/agent",
                "неизвестная подкоманда агента; ожидается показать|выбрать|создать|открыть|написать",
            ),
        }),
    }
}

fn parse_agent_create_spec(raw: &str) -> Result<(String, Option<String>), BootstrapError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/agent", "не хватает аргументов"),
        });
    }

    for delimiter in [" из ", " from "] {
        if let Some((name, template)) = trimmed.split_once(delimiter) {
            let name = name.trim().to_string();
            let template = template.trim().to_string();
            if name.is_empty() || template.is_empty() {
                break;
            }
            return Ok((name, Some(template)));
        }
    }

    Ok((trimmed.to_string(), None))
}

fn handle_schedule_command<B>(app: &B, raw: &str) -> Result<String, BootstrapError>
where
    B: TuiBackend,
{
    let trimmed = raw.trim();
    let (action, tail) = match trimmed.split_once(' ') {
        Some((action, tail)) => (action.trim(), tail.trim()),
        None => (trimmed, ""),
    };

    match action {
        "" => app.render_agent_schedules(),
        "показать" | "show" => app.render_agent_schedule(&require_arg(tail, "/schedule")?),
        "создать" | "create" => {
            let spec = require_arg(tail, "/schedule")?;
            let (id, options) = parse_schedule_create_spec(spec.as_str())?;
            app.create_agent_schedule_with_options(&id, options)
        }
        "изменить" | "edit" => {
            let spec = require_arg(tail, "/schedule")?;
            let (id, patch) = parse_schedule_edit_spec(spec.as_str())?;
            app.update_agent_schedule(&id, patch)
        }
        "включить" | "enable" => app.update_agent_schedule(
            &require_arg(tail, "/schedule")?,
            AgentScheduleUpdatePatch {
                enabled: Some(true),
                ..AgentScheduleUpdatePatch::default()
            },
        ),
        "выключить" | "disable" => app.update_agent_schedule(
            &require_arg(tail, "/schedule")?,
            AgentScheduleUpdatePatch {
                enabled: Some(false),
                ..AgentScheduleUpdatePatch::default()
            },
        ),
        "удалить" | "delete" | "remove" => {
            app.delete_agent_schedule(&require_arg(tail, "/schedule")?)
        }
        _ => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "неизвестная подкоманда расписания; ожидается показать|создать|изменить|включить|выключить|удалить",
            ),
        }),
    }
}

fn handle_mcp_command<B>(app: &B, raw: &str) -> Result<String, BootstrapError>
where
    B: TuiBackend,
{
    let trimmed = raw.trim();
    let (action, tail) = match trimmed.split_once(' ') {
        Some((action, tail)) => (action.trim(), tail.trim()),
        None => (trimmed, ""),
    };

    match action {
        "" => app.render_mcp_connectors(),
        "показать" | "show" => app.render_mcp_connector(&require_arg(tail, "/mcp")?),
        "создать" | "create" => {
            let spec = require_arg(tail, "/mcp")?;
            let (id, options) = parse_mcp_create_spec(spec.as_str())?;
            app.create_mcp_connector(&id, options)
        }
        "изменить" | "edit" => {
            let spec = require_arg(tail, "/mcp")?;
            let (id, patch) = parse_mcp_edit_spec(spec.as_str())?;
            app.update_mcp_connector(&id, patch)
        }
        "включить" | "enable" => {
            app.set_mcp_connector_enabled(&require_arg(tail, "/mcp")?, true)
        }
        "выключить" | "disable" => {
            app.set_mcp_connector_enabled(&require_arg(tail, "/mcp")?, false)
        }
        "перезапустить" | "restart" => {
            app.restart_mcp_connector(&require_arg(tail, "/mcp")?)
        }
        "удалить" | "delete" | "remove" => {
            app.delete_mcp_connector(&require_arg(tail, "/mcp")?)
        }
        _ => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/mcp",
                "неизвестная подкоманда mcp; ожидается показать|создать|изменить|включить|выключить|перезапустить|удалить",
            ),
        }),
    }
}

fn handle_memory_command<B>(app: &B, raw: &str) -> Result<String, BootstrapError>
where
    B: TuiBackend,
{
    let trimmed = raw.trim();
    let (action, tail) = match trimmed.split_once(' ') {
        Some((action, tail)) => (action.trim(), tail.trim()),
        None => (trimmed, ""),
    };

    match action {
        "сессии" | "sessions" => app.render_session_memory_search(SessionSearchInput {
            query: require_arg(tail, "/memory")?,
            limit: None,
            offset: Some(0),
            tiers: None,
            agent_identifier: None,
            updated_after: None,
            updated_before: None,
        }),
        "сессия" | "session" => {
            let value = require_arg(tail, "/memory")?;
            let (session_id, mode) = parse_memory_session_read(value.as_str())?;
            app.render_session_memory_read(SessionReadInput {
                session_id,
                mode: Some(mode),
                cursor: None,
                max_items: None,
                max_bytes: None,
                include_tools: Some(true),
            })
        }
        "знания" | "knowledge" => app.render_knowledge_search(KnowledgeSearchInput {
            query: require_arg(tail, "/memory")?,
            limit: None,
            offset: Some(0),
            kinds: None,
            roots: None,
        }),
        "файл" | "file" => {
            let value = require_arg(tail, "/memory")?;
            let (path, mode) = parse_memory_knowledge_read(value.as_str());
            app.render_knowledge_read(KnowledgeReadInput {
                path,
                mode: Some(mode),
                cursor: None,
                max_bytes: None,
                max_lines: None,
            })
        }
        _ => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/memory",
                "неизвестная подкоманда памяти; ожидается сессии|сессия|знания|файл",
            ),
        }),
    }
}

fn parse_memory_session_read(raw: &str) -> Result<(String, SessionReadMode), BootstrapError> {
    let trimmed = raw.trim();
    let Some((session_id, maybe_mode)) = trimmed.split_once(' ') else {
        return Ok((trimmed.to_string(), SessionReadMode::Summary));
    };
    Ok((
        session_id.trim().to_string(),
        parse_session_read_mode(maybe_mode.trim())?,
    ))
}

fn parse_session_read_mode(raw: &str) -> Result<SessionReadMode, BootstrapError> {
    match raw {
        "" | "summary" | "сводка" => Ok(SessionReadMode::Summary),
        "timeline" | "таймлайн" => Ok(SessionReadMode::Timeline),
        "transcript" | "транскрипт" => Ok(SessionReadMode::Transcript),
        "artifacts" | "артефакты" => Ok(SessionReadMode::Artifacts),
        other => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/memory",
                &format!("неизвестный режим чтения сессии {other}"),
            ),
        }),
    }
}

fn parse_memory_knowledge_read(raw: &str) -> (String, KnowledgeReadMode) {
    let trimmed = raw.trim();
    if let Some((path, mode)) = trimmed.rsplit_once(' ') {
        let mode = match mode.trim() {
            "full" | "полный" => Some(KnowledgeReadMode::Full),
            "excerpt" | "выдержка" => Some(KnowledgeReadMode::Excerpt),
            _ => None,
        };
        if let Some(mode) = mode {
            return (path.trim().to_string(), mode);
        }
    }
    (trimmed.to_string(), KnowledgeReadMode::Excerpt)
}

fn handle_chain_command<B>(app: &B, session_id: &str, raw: &str) -> Result<String, BootstrapError>
where
    B: TuiBackend,
{
    let trimmed = raw.trim();
    let (action, tail) = match trimmed.split_once(' ') {
        Some((action, tail)) => (action.trim(), tail.trim()),
        None => (trimmed, ""),
    };

    match action {
        "продолжить" | "grant" | "continue" => {
            let (chain_id, reason) =
                split_head_tail(tail).ok_or_else(|| BootstrapError::Usage {
                    reason: render_command_usage_error("/chain", "не хватает аргументов"),
                })?;
            app.grant_chain_continuation(session_id, chain_id, reason)
        }
        _ => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/chain",
                "неизвестная подкоманда цепочки; ожидается продолжить",
            ),
        }),
    }
}

fn split_head_tail(raw: &str) -> Option<(&str, &str)> {
    let (head, tail) = raw.split_once(' ')?;
    let head = head.trim();
    let tail = tail.trim();
    if head.is_empty() || tail.is_empty() {
        return None;
    }
    Some((head, tail))
}

fn parse_schedule_create_spec(
    raw: &str,
) -> Result<(String, AgentScheduleCreateOptions), BootstrapError> {
    let trimmed = raw.trim();
    let Some((head, prompt)) = trimmed.split_once("::") else {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "не хватает prompt; используйте формат с разделителем ::",
            ),
        });
    };
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/schedule", "prompt не должен быть пустым"),
        });
    }

    let parsed = parse_schedule_field_tokens(head)?;
    let Some(id) = parsed.id else {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/schedule", "не хватает id расписания"),
        });
    };
    let Some(interval_seconds) = parsed.interval_seconds else {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/schedule", "не хватает interval_seconds"),
        });
    };

    Ok((
        id,
        AgentScheduleCreateOptions {
            agent_identifier: parsed.agent_identifier,
            prompt,
            mode: parsed
                .mode
                .unwrap_or(agent_runtime::agent::AgentScheduleMode::Interval),
            delivery_mode: parsed
                .delivery_mode
                .unwrap_or(agent_runtime::agent::AgentScheduleDeliveryMode::FreshSession),
            target_session_id: parsed.target_session_id,
            interval_seconds,
            enabled: parsed.enabled.unwrap_or(true),
        },
    ))
}

fn parse_schedule_edit_spec(
    raw: &str,
) -> Result<(String, AgentScheduleUpdatePatch), BootstrapError> {
    let trimmed = raw.trim();
    let (head, prompt) = match trimmed.split_once("::") {
        Some((head, prompt)) => (head.trim(), Some(prompt.trim().to_string())),
        None => (trimmed, None),
    };
    let parsed = parse_schedule_field_tokens(head)?;
    let Some(id) = parsed.id else {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/schedule", "не хватает id расписания"),
        });
    };
    let patch = AgentScheduleUpdatePatch {
        agent_identifier: parsed.agent_identifier,
        prompt: prompt.filter(|value| !value.is_empty()),
        mode: parsed.mode,
        delivery_mode: parsed.delivery_mode,
        target_session_id: parsed.target_session_id,
        interval_seconds: parsed.interval_seconds,
        enabled: parsed.enabled,
    };
    if patch == AgentScheduleUpdatePatch::default() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "для edit укажите хотя бы одно поле или новый prompt",
            ),
        });
    }
    Ok((id, patch))
}

#[derive(Default)]
struct ParsedScheduleFields {
    id: Option<String>,
    agent_identifier: Option<String>,
    mode: Option<agent_runtime::agent::AgentScheduleMode>,
    delivery_mode: Option<agent_runtime::agent::AgentScheduleDeliveryMode>,
    target_session_id: Option<String>,
    interval_seconds: Option<u64>,
    enabled: Option<bool>,
}

fn parse_schedule_field_tokens(raw: &str) -> Result<ParsedScheduleFields, BootstrapError> {
    let mut parsed = ParsedScheduleFields::default();
    for token in raw.split_whitespace() {
        if token.trim().is_empty() {
            continue;
        }
        if let Some((key, value)) = token.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if value.is_empty() {
                return Err(BootstrapError::Usage {
                    reason: render_command_usage_error(
                        "/schedule",
                        &format!("пустое значение для поля {key}"),
                    ),
                });
            }
            match key {
                "id" | "ид" => parsed.id = Some(value.to_string()),
                "agent" | "агент" => parsed.agent_identifier = Some(value.to_string()),
                "mode" | "режим" => parsed.mode = Some(parse_schedule_mode(value)?),
                "delivery" | "доставка" => {
                    parsed.delivery_mode = Some(parse_schedule_delivery_mode(value)?)
                }
                "session" | "сессия" => parsed.target_session_id = Some(value.to_string()),
                "interval" | "секунды" => {
                    parsed.interval_seconds = Some(parse_schedule_interval_seconds(value)?)
                }
                "enabled" | "включено" => {
                    parsed.enabled = Some(parse_schedule_enabled(value)?)
                }
                other => {
                    return Err(BootstrapError::Usage {
                        reason: render_command_usage_error(
                            "/schedule",
                            &format!("неизвестное поле {other}"),
                        ),
                    });
                }
            }
            continue;
        }

        if parsed.id.is_none() {
            parsed.id = Some(token.to_string());
            continue;
        }
        if parsed.interval_seconds.is_none() {
            parsed.interval_seconds = Some(parse_schedule_interval_seconds(token)?);
            continue;
        }
        if let Some(agent_identifier) = parse_schedule_agent_override(token)? {
            parsed.agent_identifier = Some(agent_identifier);
            continue;
        }
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "лишние аргументы в спецификации расписания",
            ),
        });
    }
    Ok(parsed)
}

fn parse_schedule_agent_override(raw: &str) -> Result<Option<String>, BootstrapError> {
    for prefix in ["agent=", "агент="] {
        if let Some(value) = raw.strip_prefix(prefix) {
            let value = value.trim();
            if value.is_empty() {
                return Err(BootstrapError::Usage {
                    reason: render_command_usage_error(
                        "/schedule",
                        "после agent= должен быть id или имя агента",
                    ),
                });
            }
            return Ok(Some(value.to_string()));
        }
    }
    Ok(None)
}

fn parse_schedule_interval_seconds(raw: &str) -> Result<u64, BootstrapError> {
    let interval_seconds = raw.parse::<u64>().map_err(|_| BootstrapError::Usage {
        reason: render_command_usage_error(
            "/schedule",
            "interval_seconds должен быть положительным целым числом",
        ),
    })?;
    if interval_seconds == 0 {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "interval_seconds должен быть больше нуля",
            ),
        });
    }
    Ok(interval_seconds)
}

fn parse_schedule_mode(
    raw: &str,
) -> Result<agent_runtime::agent::AgentScheduleMode, BootstrapError> {
    match raw {
        "interval" => Ok(agent_runtime::agent::AgentScheduleMode::Interval),
        "after_completion" => Ok(agent_runtime::agent::AgentScheduleMode::AfterCompletion),
        "once" => Ok(agent_runtime::agent::AgentScheduleMode::Once),
        other => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                &format!("неподдерживаемый mode {other}; ожидается interval|after_completion|once"),
            ),
        }),
    }
}

fn parse_schedule_delivery_mode(
    raw: &str,
) -> Result<agent_runtime::agent::AgentScheduleDeliveryMode, BootstrapError> {
    match raw {
        "fresh_session" => Ok(agent_runtime::agent::AgentScheduleDeliveryMode::FreshSession),
        "existing_session" => Ok(agent_runtime::agent::AgentScheduleDeliveryMode::ExistingSession),
        other => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                &format!(
                    "неподдерживаемый delivery {other}; ожидается fresh_session|existing_session"
                ),
            ),
        }),
    }
}

fn parse_schedule_enabled(raw: &str) -> Result<bool, BootstrapError> {
    match raw {
        "true" | "yes" | "on" | "1" | "да" | "вкл" => Ok(true),
        "false" | "no" | "off" | "0" | "нет" | "выкл" => Ok(false),
        other => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                &format!("неподдерживаемый enabled {other}; ожидается true|false"),
            ),
        }),
    }
}

fn schedule_form_create_spec(
    form: &app::ScheduleFormState,
) -> Result<(String, AgentScheduleCreateOptions), BootstrapError> {
    let id = require_nonempty_schedule_field(form.id(), "id")?;
    let prompt = require_nonempty_schedule_field(form.prompt(), "prompt")?;
    Ok((
        id,
        AgentScheduleCreateOptions {
            agent_identifier: optional_schedule_field(form.agent_identifier()),
            prompt,
            mode: parse_schedule_mode(form.mode().trim())?,
            delivery_mode: parse_schedule_delivery_mode(form.delivery_mode().trim())?,
            target_session_id: optional_schedule_field(form.target_session_id()),
            interval_seconds: parse_schedule_interval_seconds(form.interval_seconds().trim())?,
            enabled: parse_schedule_enabled(form.enabled().trim())?,
        },
    ))
}

fn schedule_form_edit_spec(
    form: &app::ScheduleFormState,
) -> Result<(String, AgentScheduleUpdatePatch), BootstrapError> {
    let id = require_nonempty_schedule_field(form.id(), "id")?;
    let prompt = require_nonempty_schedule_field(form.prompt(), "prompt")?;
    Ok((
        id,
        AgentScheduleUpdatePatch {
            agent_identifier: optional_schedule_field(form.agent_identifier()),
            prompt: Some(prompt),
            mode: Some(parse_schedule_mode(form.mode().trim())?),
            delivery_mode: Some(parse_schedule_delivery_mode(form.delivery_mode().trim())?),
            target_session_id: optional_schedule_field(form.target_session_id()),
            interval_seconds: Some(parse_schedule_interval_seconds(
                form.interval_seconds().trim(),
            )?),
            enabled: Some(parse_schedule_enabled(form.enabled().trim())?),
        },
    ))
}

fn agent_message_form_spec(
    form: &app::AgentMessageFormState,
) -> Result<(String, String), BootstrapError> {
    let target_agent_id =
        require_nonempty_command_field(form.target_agent_id(), "/agent", "agent")?;
    let message = require_nonempty_command_field(form.message(), "/agent", "message")?;
    Ok((target_agent_id, message))
}

fn chain_grant_form_spec(
    form: &app::ChainGrantFormState,
) -> Result<(String, String), BootstrapError> {
    let chain_id = require_nonempty_command_field(form.chain_id(), "/chain", "chain_id")?;
    let reason = require_nonempty_command_field(form.reason(), "/chain", "reason")?;
    Ok((chain_id, reason))
}

fn mcp_form_create_spec(
    form: &app::McpConnectorFormState,
) -> Result<(String, McpConnectorCreateOptions), BootstrapError> {
    let id = require_nonempty_command_field(form.id(), "/mcp", "id")?;
    let command = require_nonempty_command_field(form.command(), "/mcp", "command")?;
    Ok((
        id,
        McpConnectorCreateOptions {
            transport: agent_runtime::mcp::McpConnectorTransport::Stdio,
            command,
            args: parse_mcp_args(form.args()),
            env: parse_mcp_env(form.env(), "/mcp")?,
            cwd: optional_mcp_field(form.cwd()),
            enabled: parse_mcp_enabled(form.enabled())?,
        },
    ))
}

fn mcp_form_edit_spec(
    form: &app::McpConnectorFormState,
) -> Result<(String, McpConnectorUpdatePatch), BootstrapError> {
    let id = require_nonempty_command_field(form.id(), "/mcp", "id")?;
    let command = require_nonempty_command_field(form.command(), "/mcp", "command")?;
    Ok((
        id,
        McpConnectorUpdatePatch {
            command: Some(command),
            args: Some(parse_mcp_args(form.args())),
            env: Some(parse_mcp_env(form.env(), "/mcp")?),
            cwd: Some(optional_mcp_field(form.cwd())),
            enabled: Some(parse_mcp_enabled(form.enabled())?),
        },
    ))
}

fn parse_mcp_create_spec(raw: &str) -> Result<(String, McpConnectorCreateOptions), BootstrapError> {
    let (id, assignments) = split_head_tail(raw).ok_or_else(|| BootstrapError::Usage {
        reason: render_command_usage_error("/mcp", "не хватает аргументов"),
    })?;
    let fields = parse_mcp_assignment_fields(assignments, "/mcp")?;
    Ok((
        id.to_string(),
        McpConnectorCreateOptions {
            transport: agent_runtime::mcp::McpConnectorTransport::Stdio,
            command: required_mcp_assignment(&fields, "command", "/mcp")?,
            args: parse_mcp_args(fields.get("args").map(String::as_str).unwrap_or_default()),
            env: parse_mcp_env(
                fields.get("env").map(String::as_str).unwrap_or_default(),
                "/mcp",
            )?,
            cwd: optional_mcp_field(fields.get("cwd").map(String::as_str).unwrap_or_default()),
            enabled: parse_mcp_enabled(
                fields.get("enabled").map(String::as_str).unwrap_or("true"),
            )?,
        },
    ))
}

fn parse_mcp_edit_spec(raw: &str) -> Result<(String, McpConnectorUpdatePatch), BootstrapError> {
    let (id, assignments) = split_head_tail(raw).ok_or_else(|| BootstrapError::Usage {
        reason: render_command_usage_error("/mcp", "не хватает аргументов"),
    })?;
    let fields = parse_mcp_assignment_fields(assignments, "/mcp")?;
    if fields.is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/mcp", "не указаны поля для изменения"),
        });
    }
    Ok((
        id.to_string(),
        McpConnectorUpdatePatch {
            command: fields.get("command").map(|value| value.trim().to_string()),
            args: fields
                .get("args")
                .map(|value| parse_mcp_args(value.as_str())),
            env: if fields.contains_key("env") {
                Some(parse_mcp_env(
                    fields.get("env").map(String::as_str).unwrap_or_default(),
                    "/mcp",
                )?)
            } else {
                None
            },
            cwd: if fields.contains_key("cwd") {
                Some(optional_mcp_field(
                    fields.get("cwd").map(String::as_str).unwrap_or_default(),
                ))
            } else {
                None
            },
            enabled: if let Some(value) = fields.get("enabled") {
                Some(parse_mcp_enabled(value)?)
            } else {
                None
            },
        },
    ))
}

fn parse_mcp_assignment_fields(
    raw: &str,
    command: &str,
) -> Result<std::collections::BTreeMap<String, String>, BootstrapError> {
    let mut fields = std::collections::BTreeMap::new();
    for token in raw.split_whitespace() {
        let Some((key, value)) = token.split_once('=') else {
            return Err(BootstrapError::Usage {
                reason: render_command_usage_error(
                    command,
                    &format!("ожидается field=value, получено {token}"),
                ),
            });
        };
        fields.insert(key.trim().to_string(), value.to_string());
    }
    Ok(fields)
}

fn required_mcp_assignment(
    fields: &std::collections::BTreeMap<String, String>,
    key: &str,
    command: &str,
) -> Result<String, BootstrapError> {
    fields
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| BootstrapError::Usage {
            reason: render_command_usage_error(command, &format!("не хватает {key}")),
        })
}

fn parse_mcp_args(raw: &str) -> Vec<String> {
    raw.split(',')
        .filter_map(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}

fn parse_mcp_env(
    raw: &str,
    command: &str,
) -> Result<std::collections::BTreeMap<String, String>, BootstrapError> {
    let mut env = std::collections::BTreeMap::new();
    for pair in raw.split(';') {
        let trimmed = pair.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            return Err(BootstrapError::Usage {
                reason: render_command_usage_error(
                    command,
                    &format!("ожидается env KEY=VALUE, получено {trimmed}"),
                ),
            });
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(BootstrapError::Usage {
                reason: render_command_usage_error(command, "ключ env не должен быть пустым"),
            });
        }
        env.insert(key.to_string(), value.to_string());
    }
    Ok(env)
}

fn parse_mcp_enabled(raw: &str) -> Result<bool, BootstrapError> {
    match raw.trim() {
        "true" | "yes" | "1" | "on" => Ok(true),
        "false" | "no" | "0" | "off" => Ok(false),
        other => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/mcp",
                &format!("неподдерживаемый enabled {other}; ожидается true|false"),
            ),
        }),
    }
}

fn optional_mcp_field(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn optional_schedule_field(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn require_nonempty_schedule_field(raw: &str, field: &str) -> Result<String, BootstrapError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                &format!("поле {field} не должно быть пустым"),
            ),
        });
    }
    Ok(trimmed.to_string())
}

fn require_nonempty_command_field(
    raw: &str,
    command: &str,
    field: &str,
) -> Result<String, BootstrapError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                command,
                &format!("поле {field} не должно быть пустым"),
            ),
        });
    }
    Ok(trimmed.to_string())
}

fn render_session_skills(skills: Vec<crate::bootstrap::SessionSkillStatus>) -> String {
    if skills.is_empty() {
        return "Скиллы: ничего не найдено".to_string();
    }

    let mut lines = vec!["Скиллы:".to_string()];
    lines.extend(skills.into_iter().map(|skill| {
        format!(
            "- [{}] {}: {}",
            translate_skill_mode(skill.mode.as_str()),
            skill.name,
            skill.description
        )
    }));
    lines.join("\n")
}

fn translate_skill_mode(mode: &str) -> &str {
    match mode {
        "inactive" => "неактивен",
        "automatic" => "авто",
        "manual" => "вручную",
        "enabled" => "включён",
        "disabled" => "выключен",
        other => other,
    }
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
        AgentScheduleCreateOptions, AgentScheduleUpdatePatch, AgentScheduleView,
        McpConnectorCreateOptions, McpConnectorUpdatePatch, McpConnectorView,
        SessionPendingApproval, SessionPreferencesPatch, SessionSkillStatus, SessionSummary,
        SessionTranscriptView,
    };
    use crate::execution::{ApprovalContinuationReport, ChatTurnExecutionReport};
    use crate::tui::app::TuiScreen;
    use crate::tui::backend::TuiBackend;
    use crate::tui::timeline::TimelineEntryKind;
    use agent_runtime::agent::{AgentScheduleDeliveryMode, AgentScheduleMode};
    use agent_runtime::mcp::McpConnectorTransport;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakeBackend {
        summary: SessionSummary,
        pending: Vec<SessionPendingApproval>,
        transcript: SessionTranscriptView,
        debug_bundle: String,
    }

    impl TuiBackend for FakeBackend {
        fn render_agents(&self) -> Result<String, BootstrapError> {
            Ok(format!(
                "Агенты: текущий={}\n* {} ({})",
                self.summary.agent_profile_id,
                self.summary.agent_name,
                self.summary.agent_profile_id
            ))
        }

        fn render_agent(&self, _identifier: Option<&str>) -> Result<String, BootstrapError> {
            Ok(format!(
                "id={}\nname={}",
                self.summary.agent_profile_id, self.summary.agent_name
            ))
        }

        fn select_agent(&self, identifier: &str) -> Result<String, BootstrapError> {
            Ok(format!(
                "текущий агент: {} ({identifier})",
                self.summary.agent_name
            ))
        }

        fn create_agent(
            &self,
            name: &str,
            template_identifier: Option<&str>,
        ) -> Result<String, BootstrapError> {
            Ok(format!(
                "создан агент {name} (test) из шаблона {}",
                template_identifier.unwrap_or("default")
            ))
        }

        fn open_agent_home(&self, _identifier: Option<&str>) -> Result<String, BootstrapError> {
            Ok("/tmp/test-agent".to_string())
        }

        fn send_agent_message(
            &self,
            session_id: &str,
            target_agent_id: &str,
            message: &str,
        ) -> Result<String, BootstrapError> {
            Ok(format!(
                "сообщение отправлено агенту {target_agent_id} из {session_id}: {message}"
            ))
        }

        fn grant_chain_continuation(
            &self,
            session_id: &str,
            chain_id: &str,
            reason: &str,
        ) -> Result<String, BootstrapError> {
            Ok(format!(
                "цепочка {chain_id} продолжена из {session_id}: {reason}"
            ))
        }

        fn render_agent_schedules(&self) -> Result<String, BootstrapError> {
            Ok(
                "Расписания: workspace=/tmp/test\n- pulse agent=default interval=300 next_fire_at=10"
                    .to_string(),
            )
        }

        fn render_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
            Ok(format!("id={id}"))
        }

        fn load_agent_schedule(&self, id: &str) -> Result<AgentScheduleView, BootstrapError> {
            Ok(AgentScheduleView {
                id: id.to_string(),
                agent_profile_id: self.summary.agent_profile_id.clone(),
                workspace_root: PathBuf::from("/tmp/test"),
                prompt: "проверь очередь".to_string(),
                mode: AgentScheduleMode::Interval,
                delivery_mode: AgentScheduleDeliveryMode::FreshSession,
                target_session_id: None,
                interval_seconds: 300,
                next_fire_at: 10,
                enabled: true,
                last_triggered_at: None,
                last_finished_at: None,
                last_session_id: None,
                last_job_id: None,
                last_result: None,
                last_error: None,
                created_at: 1,
                updated_at: 2,
            })
        }

        fn create_agent_schedule_with_options(
            &self,
            id: &str,
            options: AgentScheduleCreateOptions,
        ) -> Result<String, BootstrapError> {
            Ok(format!(
                "создано расписание {id} agent={} interval={}s",
                options
                    .agent_identifier
                    .as_deref()
                    .unwrap_or(&self.summary.agent_profile_id),
                options.interval_seconds
            ))
        }

        fn update_agent_schedule(
            &self,
            id: &str,
            patch: AgentScheduleUpdatePatch,
        ) -> Result<String, BootstrapError> {
            Ok(format!(
                "обновлено расписание {id} enabled={}",
                patch.enabled.unwrap_or(true)
            ))
        }

        fn set_agent_schedule_enabled(
            &self,
            id: &str,
            enabled: bool,
        ) -> Result<String, BootstrapError> {
            Ok(format!(
                "расписание {id} {}",
                if enabled {
                    "включено"
                } else {
                    "выключено"
                }
            ))
        }

        fn create_agent_schedule(
            &self,
            id: &str,
            interval_seconds: u64,
            _prompt: &str,
            agent_identifier: Option<&str>,
        ) -> Result<String, BootstrapError> {
            Ok(format!(
                "создано расписание {id} agent={} interval={}s",
                agent_identifier.unwrap_or(&self.summary.agent_profile_id),
                interval_seconds
            ))
        }

        fn delete_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
            Ok(format!("расписание {id} удалено"))
        }

        fn render_mcp_connectors(&self) -> Result<String, BootstrapError> {
            Ok("MCP коннекторы:\n- docs transport=stdio enabled=yes state=running pid=4242 restarts=0 command=npx args=-y,@modelcontextprotocol/server-filesystem,/workspace cwd=/srv/mcp".to_string())
        }

        fn render_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
            Ok(format!(
                "id={id}\ntransport=stdio\nenabled=true\nstate=running\npid=4242\nstarted_at=10\nstopped_at=<none>\nrestart_count=0\nlast_error=<none>\ncommand=npx\nargs=-y,@modelcontextprotocol/server-filesystem,/workspace\ncwd=/srv/mcp\nenv=DEBUG=1;TRACE=yes\ncreated_at=1\nupdated_at=2"
            ))
        }

        fn load_mcp_connector(&self, id: &str) -> Result<McpConnectorView, BootstrapError> {
            Ok(McpConnectorView {
                id: id.to_string(),
                transport: McpConnectorTransport::Stdio,
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                    "/workspace".to_string(),
                ],
                env: std::collections::BTreeMap::from([
                    ("DEBUG".to_string(), "1".to_string()),
                    ("TRACE".to_string(), "yes".to_string()),
                ]),
                cwd: Some("/srv/mcp".to_string()),
                enabled: true,
                created_at: 1,
                updated_at: 2,
                runtime: crate::mcp::McpConnectorRuntimeStatus {
                    state: crate::mcp::McpConnectorState::Running,
                    pid: Some(4242),
                    started_at: Some(10),
                    stopped_at: None,
                    last_error: None,
                    restart_count: 0,
                },
            })
        }

        fn create_mcp_connector(
            &self,
            id: &str,
            _options: McpConnectorCreateOptions,
        ) -> Result<String, BootstrapError> {
            Ok(format!("создан MCP коннектор {id}"))
        }

        fn update_mcp_connector(
            &self,
            id: &str,
            _patch: McpConnectorUpdatePatch,
        ) -> Result<String, BootstrapError> {
            Ok(format!("обновлён MCP коннектор {id}"))
        }

        fn set_mcp_connector_enabled(
            &self,
            id: &str,
            enabled: bool,
        ) -> Result<String, BootstrapError> {
            Ok(format!(
                "MCP коннектор {id} {}",
                if enabled {
                    "включен"
                } else {
                    "выключен"
                }
            ))
        }

        fn restart_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
            Ok(format!("MCP коннектор {id} перезапущен"))
        }

        fn delete_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
            Ok(format!("MCP коннектор {id} удалён"))
        }

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
            Ok(self.transcript.clone())
        }

        fn pending_approvals(
            &self,
            _session_id: &str,
        ) -> Result<Vec<SessionPendingApproval>, BootstrapError> {
            Ok(self.pending.clone())
        }

        fn render_session_memory_search(
            &self,
            input: SessionSearchInput,
        ) -> Result<String, BootstrapError> {
            Ok(format!("memory sessions query={}", input.query))
        }

        fn render_session_memory_read(
            &self,
            input: SessionReadInput,
        ) -> Result<String, BootstrapError> {
            Ok(format!(
                "memory session id={} mode={}",
                input.session_id,
                input.mode.unwrap_or(SessionReadMode::Summary).as_str()
            ))
        }

        fn render_knowledge_search(
            &self,
            input: KnowledgeSearchInput,
        ) -> Result<String, BootstrapError> {
            Ok(format!("memory knowledge query={}", input.query))
        }

        fn render_knowledge_read(
            &self,
            input: KnowledgeReadInput,
        ) -> Result<String, BootstrapError> {
            Ok(format!(
                "memory file path={} mode={}",
                input.path,
                input.mode.unwrap_or(KnowledgeReadMode::Excerpt).as_str()
            ))
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

        fn render_context(&self, _session_id: &str) -> Result<String, BootstrapError> {
            Ok("Context:\nctx=0".to_string())
        }

        fn render_system(&self, _session_id: &str) -> Result<String, BootstrapError> {
            Ok("Системные блоки:\n<test>".to_string())
        }

        fn render_plan(&self, _session_id: &str) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn render_artifacts(&self, _session_id: &str) -> Result<String, BootstrapError> {
            Ok("Артефакты:\n- artifact-1 [ref-1] Tool trace".to_string())
        }

        fn read_artifact(
            &self,
            _session_id: &str,
            artifact_id: &str,
        ) -> Result<String, BootstrapError> {
            Ok(format!("artifact_id={artifact_id}"))
        }

        fn render_active_run(&self, _session_id: &str) -> Result<String, BootstrapError> {
            Ok("Ход: активного выполнения нет".to_string())
        }

        fn cancel_active_run(
            &self,
            _session_id: &str,
            _now: i64,
        ) -> Result<String, BootstrapError> {
            Ok("ход остановлен".to_string())
        }

        fn cancel_all_session_work(
            &self,
            _session_id: &str,
            _now: i64,
        ) -> Result<String, BootstrapError> {
            Ok("отмена выполнена".to_string())
        }

        fn render_version_info(&self) -> Result<String, BootstrapError> {
            Ok("версия=test".to_string())
        }

        fn update_runtime(&self, _tag: Option<&str>) -> Result<String, BootstrapError> {
            Ok("обновлено".to_string())
        }

        fn render_active_jobs(&self, _session_id: &str) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn write_debug_bundle(&self, _session_id: &str) -> Result<String, BootstrapError> {
            Ok(self.debug_bundle.clone())
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

    #[derive(Clone)]
    struct BrowserBackend {
        summary: SessionSummary,
        state: Arc<Mutex<BrowserBackendState>>,
    }

    #[derive(Clone)]
    struct BrowserBackendState {
        current_agent_id: String,
        agents: Vec<(String, String)>,
        schedules: Vec<BrowserSchedule>,
        connectors: Vec<BrowserMcpConnector>,
        sent_messages: Vec<(String, String, String)>,
        chain_grants: Vec<(String, String, String)>,
    }

    #[derive(Clone)]
    struct BrowserSchedule {
        id: String,
        agent_profile_id: String,
        prompt: String,
        mode: AgentScheduleMode,
        delivery_mode: AgentScheduleDeliveryMode,
        target_session_id: Option<String>,
        interval_seconds: u64,
        enabled: bool,
    }

    #[derive(Clone)]
    struct BrowserMcpConnector {
        id: String,
        command: String,
        args: Vec<String>,
        cwd: Option<String>,
        env: std::collections::BTreeMap<String, String>,
        enabled: bool,
        state: crate::mcp::McpConnectorState,
        restart_count: u32,
    }

    impl TuiBackend for BrowserBackend {
        fn render_agents(&self) -> Result<String, BootstrapError> {
            let state = self.state.lock().expect("browser backend state");
            let mut lines = vec![format!("Агенты: текущий={}", state.current_agent_id)];
            for (id, name) in &state.agents {
                let marker = if id == &state.current_agent_id {
                    "*"
                } else {
                    "-"
                };
                lines.push(format!(
                    "{marker} {name} ({id}) template=default tools=4 home=/tmp/{id}"
                ));
            }
            Ok(lines.join("\n"))
        }

        fn render_agent(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
            let id = identifier.unwrap_or("default");
            let state = self.state.lock().expect("browser backend state");
            let name = state
                .agents
                .iter()
                .find(|(agent_id, _)| agent_id == id)
                .map(|(_, name)| name.as_str())
                .unwrap_or("Unknown");
            Ok(format!("id={id}\nname={name}"))
        }

        fn select_agent(&self, identifier: &str) -> Result<String, BootstrapError> {
            self.state
                .lock()
                .expect("browser backend state")
                .current_agent_id = identifier.to_string();
            Ok(format!("текущий агент: {identifier}"))
        }

        fn create_agent(
            &self,
            name: &str,
            _template_identifier: Option<&str>,
        ) -> Result<String, BootstrapError> {
            let id = name.trim().to_lowercase().replace(' ', "-");
            self.state
                .lock()
                .expect("browser backend state")
                .agents
                .push((id.clone(), name.trim().to_string()));
            Ok(format!(
                "создан агент {} ({id}) из шаблона default",
                name.trim()
            ))
        }

        fn open_agent_home(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
            let id = identifier.unwrap_or("default");
            Ok(format!("/tmp/{id}"))
        }

        fn send_agent_message(
            &self,
            session_id: &str,
            target_agent_id: &str,
            message: &str,
        ) -> Result<String, BootstrapError> {
            self.state
                .lock()
                .expect("browser backend state")
                .sent_messages
                .push((
                    session_id.to_string(),
                    target_agent_id.to_string(),
                    message.to_string(),
                ));
            Ok(format!(
                "сообщение отправлено агенту {target_agent_id} из {session_id}: {message}"
            ))
        }

        fn grant_chain_continuation(
            &self,
            session_id: &str,
            chain_id: &str,
            reason: &str,
        ) -> Result<String, BootstrapError> {
            self.state
                .lock()
                .expect("browser backend state")
                .chain_grants
                .push((
                    session_id.to_string(),
                    chain_id.to_string(),
                    reason.to_string(),
                ));
            Ok(format!(
                "цепочка {chain_id} продолжена из {session_id}: {reason}"
            ))
        }

        fn render_agent_schedules(&self) -> Result<String, BootstrapError> {
            let state = self.state.lock().expect("browser backend state");
            if state.schedules.is_empty() {
                return Ok("Расписания: для workspace /tmp/test ничего не настроено".to_string());
            }
            let mut lines = vec!["Расписания: workspace=/tmp/test".to_string()];
            for schedule in &state.schedules {
                lines.push(format!(
                    "- {} agent={} mode={} delivery={} enabled={} interval={} next_fire_at=10",
                    schedule.id,
                    schedule.agent_profile_id,
                    schedule.mode.as_str(),
                    schedule.delivery_mode.as_str(),
                    schedule.enabled,
                    schedule.interval_seconds
                ));
            }
            Ok(lines.join("\n"))
        }

        fn render_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
            let state = self.state.lock().expect("browser backend state");
            let schedule = state
                .schedules
                .iter()
                .find(|schedule| schedule.id == id)
                .expect("schedule exists");
            Ok(format!(
                "id={}\nagent={}\nmode={}\ndelivery={}\nenabled={}\ninterval={}s\nprompt={}",
                schedule.id,
                schedule.agent_profile_id,
                schedule.mode.as_str(),
                schedule.delivery_mode.as_str(),
                schedule.enabled,
                schedule.interval_seconds,
                schedule.prompt
            ))
        }

        fn load_agent_schedule(&self, id: &str) -> Result<AgentScheduleView, BootstrapError> {
            let state = self.state.lock().expect("browser backend state");
            let schedule = state
                .schedules
                .iter()
                .find(|schedule| schedule.id == id)
                .expect("schedule exists");
            Ok(AgentScheduleView {
                id: schedule.id.clone(),
                agent_profile_id: schedule.agent_profile_id.clone(),
                workspace_root: PathBuf::from("/tmp/test"),
                prompt: schedule.prompt.clone(),
                mode: schedule.mode,
                delivery_mode: schedule.delivery_mode,
                target_session_id: schedule.target_session_id.clone(),
                interval_seconds: schedule.interval_seconds,
                next_fire_at: 10,
                enabled: schedule.enabled,
                last_triggered_at: None,
                last_finished_at: None,
                last_session_id: None,
                last_job_id: None,
                last_result: None,
                last_error: None,
                created_at: 1,
                updated_at: 2,
            })
        }

        fn create_agent_schedule_with_options(
            &self,
            id: &str,
            options: AgentScheduleCreateOptions,
        ) -> Result<String, BootstrapError> {
            self.state
                .lock()
                .expect("browser backend state")
                .schedules
                .push(BrowserSchedule {
                    id: id.to_string(),
                    agent_profile_id: options
                        .agent_identifier
                        .clone()
                        .unwrap_or_else(|| "default".to_string()),
                    prompt: options.prompt.clone(),
                    mode: options.mode,
                    delivery_mode: options.delivery_mode,
                    target_session_id: options.target_session_id.clone(),
                    interval_seconds: options.interval_seconds,
                    enabled: options.enabled,
                });
            Ok(format!(
                "создано расписание {id} agent={} interval={}s",
                options
                    .agent_identifier
                    .unwrap_or_else(|| "default".to_string()),
                options.interval_seconds
            ))
        }

        fn update_agent_schedule(
            &self,
            id: &str,
            patch: AgentScheduleUpdatePatch,
        ) -> Result<String, BootstrapError> {
            let mut state = self.state.lock().expect("browser backend state");
            let schedule = state
                .schedules
                .iter_mut()
                .find(|schedule| schedule.id == id)
                .expect("schedule exists");
            if let Some(agent_identifier) = patch.agent_identifier {
                schedule.agent_profile_id = agent_identifier;
            }
            if let Some(prompt) = patch.prompt {
                schedule.prompt = prompt;
            }
            if let Some(mode) = patch.mode {
                schedule.mode = mode;
            }
            if let Some(delivery_mode) = patch.delivery_mode {
                schedule.delivery_mode = delivery_mode;
            }
            if patch.target_session_id.is_some() {
                schedule.target_session_id = patch.target_session_id;
            }
            if let Some(interval_seconds) = patch.interval_seconds {
                schedule.interval_seconds = interval_seconds;
            }
            if let Some(enabled) = patch.enabled {
                schedule.enabled = enabled;
            }
            Ok(format!(
                "обновлено расписание {id} enabled={}",
                schedule.enabled
            ))
        }

        fn set_agent_schedule_enabled(
            &self,
            id: &str,
            enabled: bool,
        ) -> Result<String, BootstrapError> {
            let mut state = self.state.lock().expect("browser backend state");
            let schedule = state
                .schedules
                .iter_mut()
                .find(|schedule| schedule.id == id)
                .expect("schedule exists");
            schedule.enabled = enabled;
            Ok(format!(
                "расписание {id} {}",
                if enabled {
                    "включено"
                } else {
                    "выключено"
                }
            ))
        }

        fn create_agent_schedule(
            &self,
            id: &str,
            interval_seconds: u64,
            prompt: &str,
            agent_identifier: Option<&str>,
        ) -> Result<String, BootstrapError> {
            self.create_agent_schedule_with_options(
                id,
                AgentScheduleCreateOptions {
                    agent_identifier: agent_identifier.map(str::to_string),
                    prompt: prompt.to_string(),
                    mode: AgentScheduleMode::Interval,
                    delivery_mode: AgentScheduleDeliveryMode::FreshSession,
                    target_session_id: None,
                    interval_seconds,
                    enabled: true,
                },
            )
        }

        fn delete_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
            self.state
                .lock()
                .expect("browser backend state")
                .schedules
                .retain(|schedule| schedule.id != id);
            Ok(format!("расписание {id} удалено"))
        }

        fn render_mcp_connectors(&self) -> Result<String, BootstrapError> {
            let state = self.state.lock().expect("browser backend state");
            if state.connectors.is_empty() {
                return Ok("MCP коннекторы: ничего не настроено".to_string());
            }
            let mut lines = vec!["MCP коннекторы:".to_string()];
            for connector in &state.connectors {
                lines.push(format!(
                    "- {} transport=stdio enabled={} state={} pid=<none> restarts={} command={} args={} cwd={}",
                    connector.id,
                    if connector.enabled { "yes" } else { "no" },
                    connector.state.as_str(),
                    connector.restart_count,
                    connector.command,
                    connector.args.join(","),
                    connector.cwd.as_deref().unwrap_or("<none>")
                ));
            }
            Ok(lines.join("\n"))
        }

        fn render_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
            let state = self.state.lock().expect("browser backend state");
            let connector = state
                .connectors
                .iter()
                .find(|connector| connector.id == id)
                .expect("connector exists");
            Ok(format!(
                "id={}\ntransport=stdio\nenabled={}\nstate={}\npid=<none>\nstarted_at=<none>\nstopped_at=<none>\nrestart_count={}\nlast_error=<none>\ncommand={}\nargs={}\ncwd={}\nenv={}\ncreated_at=1\nupdated_at=2",
                connector.id,
                connector.enabled,
                connector.state.as_str(),
                connector.restart_count,
                connector.command,
                connector.args.join(","),
                connector.cwd.as_deref().unwrap_or("<none>"),
                connector
                    .env
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join(";")
            ))
        }

        fn load_mcp_connector(&self, id: &str) -> Result<McpConnectorView, BootstrapError> {
            let state = self.state.lock().expect("browser backend state");
            let connector = state
                .connectors
                .iter()
                .find(|connector| connector.id == id)
                .expect("connector exists");
            Ok(McpConnectorView {
                id: connector.id.clone(),
                transport: McpConnectorTransport::Stdio,
                command: connector.command.clone(),
                args: connector.args.clone(),
                env: connector.env.clone(),
                cwd: connector.cwd.clone(),
                enabled: connector.enabled,
                created_at: 1,
                updated_at: 2,
                runtime: crate::mcp::McpConnectorRuntimeStatus {
                    state: connector.state,
                    pid: None,
                    started_at: None,
                    stopped_at: None,
                    last_error: None,
                    restart_count: connector.restart_count,
                },
            })
        }

        fn create_mcp_connector(
            &self,
            id: &str,
            options: McpConnectorCreateOptions,
        ) -> Result<String, BootstrapError> {
            self.state
                .lock()
                .expect("browser backend state")
                .connectors
                .push(BrowserMcpConnector {
                    id: id.to_string(),
                    command: options.command.clone(),
                    args: options.args.clone(),
                    cwd: options.cwd.clone(),
                    env: options.env.clone(),
                    enabled: options.enabled,
                    state: if options.enabled {
                        crate::mcp::McpConnectorState::Running
                    } else {
                        crate::mcp::McpConnectorState::Stopped
                    },
                    restart_count: 0,
                });
            Ok(format!("создан MCP коннектор {id}"))
        }

        fn update_mcp_connector(
            &self,
            id: &str,
            patch: McpConnectorUpdatePatch,
        ) -> Result<String, BootstrapError> {
            let mut state = self.state.lock().expect("browser backend state");
            let connector = state
                .connectors
                .iter_mut()
                .find(|connector| connector.id == id)
                .expect("connector exists");
            if let Some(command) = patch.command {
                connector.command = command;
            }
            if let Some(args) = patch.args {
                connector.args = args;
            }
            if let Some(env) = patch.env {
                connector.env = env;
            }
            if let Some(cwd) = patch.cwd {
                connector.cwd = cwd;
            }
            if let Some(enabled) = patch.enabled {
                connector.enabled = enabled;
                connector.state = if enabled {
                    crate::mcp::McpConnectorState::Running
                } else {
                    crate::mcp::McpConnectorState::Stopped
                };
            }
            Ok(format!("обновлён MCP коннектор {id}"))
        }

        fn set_mcp_connector_enabled(
            &self,
            id: &str,
            enabled: bool,
        ) -> Result<String, BootstrapError> {
            let mut state = self.state.lock().expect("browser backend state");
            let connector = state
                .connectors
                .iter_mut()
                .find(|connector| connector.id == id)
                .expect("connector exists");
            connector.enabled = enabled;
            connector.state = if enabled {
                crate::mcp::McpConnectorState::Running
            } else {
                crate::mcp::McpConnectorState::Stopped
            };
            Ok(format!(
                "MCP коннектор {id} {}",
                if enabled {
                    "включен"
                } else {
                    "выключен"
                }
            ))
        }

        fn restart_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
            let mut state = self.state.lock().expect("browser backend state");
            let connector = state
                .connectors
                .iter_mut()
                .find(|connector| connector.id == id)
                .expect("connector exists");
            connector.restart_count += 1;
            connector.state = if connector.enabled {
                crate::mcp::McpConnectorState::Running
            } else {
                crate::mcp::McpConnectorState::Stopped
            };
            Ok(format!("MCP коннектор {id} перезапущен"))
        }

        fn delete_mcp_connector(&self, id: &str) -> Result<String, BootstrapError> {
            self.state
                .lock()
                .expect("browser backend state")
                .connectors
                .retain(|connector| connector.id != id);
            Ok(format!("MCP коннектор {id} удалён"))
        }

        fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError> {
            let state = self.state.lock().expect("browser backend state");
            let mut summary = self.summary.clone();
            summary.agent_profile_id = state.current_agent_id.clone();
            summary.agent_name = state
                .agents
                .iter()
                .find(|(id, _)| id == &state.current_agent_id)
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| summary.agent_name.clone());
            Ok(vec![summary])
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
            Ok(Vec::new())
        }

        fn render_session_memory_search(
            &self,
            input: SessionSearchInput,
        ) -> Result<String, BootstrapError> {
            Ok(format!("browser memory sessions query={}", input.query))
        }

        fn render_session_memory_read(
            &self,
            input: SessionReadInput,
        ) -> Result<String, BootstrapError> {
            Ok(format!(
                "browser memory session id={} mode={}",
                input.session_id,
                input.mode.unwrap_or(SessionReadMode::Summary).as_str()
            ))
        }

        fn render_knowledge_search(
            &self,
            input: KnowledgeSearchInput,
        ) -> Result<String, BootstrapError> {
            Ok(format!("browser memory knowledge query={}", input.query))
        }

        fn render_knowledge_read(
            &self,
            input: KnowledgeReadInput,
        ) -> Result<String, BootstrapError> {
            Ok(format!(
                "browser memory file path={} mode={}",
                input.path,
                input.mode.unwrap_or(KnowledgeReadMode::Excerpt).as_str()
            ))
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
            Ok(None)
        }

        fn render_context(&self, _session_id: &str) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn render_system(&self, _session_id: &str) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn render_plan(&self, _session_id: &str) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn render_artifacts(&self, _session_id: &str) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn read_artifact(
            &self,
            _session_id: &str,
            _artifact_id: &str,
        ) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn render_active_jobs(&self, _session_id: &str) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn render_active_run(&self, _session_id: &str) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn cancel_active_run(
            &self,
            _session_id: &str,
            _now: i64,
        ) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn cancel_all_session_work(
            &self,
            _session_id: &str,
            _now: i64,
        ) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn render_version_info(&self) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn update_runtime(&self, _tag: Option<&str>) -> Result<String, BootstrapError> {
            panic!("unused in test")
        }

        fn write_debug_bundle(&self, _session_id: &str) -> Result<String, BootstrapError> {
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
    fn terminal_paste_inserts_multiline_text_without_submitting_or_queueing() {
        let mut state = TuiAppState::new(
            vec![SessionSummary {
                id: "session-a".to_string(),
                title: "Session A".to_string(),
                agent_profile_id: "default".to_string(),
                agent_name: "Default".to_string(),
                scheduled_by: None,
                schedule: None,
                model: Some("glm-5-turbo".to_string()),
                reasoning_visible: true,
                think_level: None,
                compactifications: 0,
                completion_nudges: None,
                auto_approve: false,
                context_tokens: 0,
                usage_input_tokens: None,
                usage_output_tokens: None,
                usage_total_tokens: None,
                has_pending_approval: false,
                last_message_preview: None,
                message_count: 0,
                background_job_count: 0,
                running_background_job_count: 0,
                queued_background_job_count: 0,
                created_at: 1,
                updated_at: 2,
            }],
            Some("session-a".to_string()),
        );
        let action = dispatch_terminal_event(
            &mut state,
            Event::Paste("first line\nsecond line\nthird line".to_string()),
        )
        .expect("paste event");

        assert_eq!(action, TuiAction::None);
        assert_eq!(state.input_buffer(), "first line\nsecond line\nthird line");
        assert_eq!(state.input_cursor(), state.input_buffer().len());
    }

    #[test]
    fn dispatch_terminal_event_does_not_treat_plain_q_or_shortcut_letter_as_exit() {
        let mut state = TuiAppState::new(
            vec![SessionSummary {
                id: "session-a".to_string(),
                title: "Session A".to_string(),
                agent_profile_id: "default".to_string(),
                agent_name: "Default".to_string(),
                scheduled_by: None,
                schedule: None,
                model: Some("glm-5-turbo".to_string()),
                reasoning_visible: true,
                think_level: None,
                compactifications: 0,
                completion_nudges: None,
                auto_approve: false,
                context_tokens: 0,
                usage_input_tokens: None,
                usage_output_tokens: None,
                usage_total_tokens: None,
                has_pending_approval: false,
                last_message_preview: None,
                message_count: 0,
                background_job_count: 0,
                running_background_job_count: 0,
                queued_background_job_count: 0,
                created_at: 1,
                updated_at: 2,
            }],
            Some("session-a".to_string()),
        );

        let action = dispatch_terminal_event(
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
        )
        .expect("q key");
        assert_eq!(action, TuiAction::None);
        assert!(!state.should_exit());
        assert_eq!(state.input_buffer(), "q");

        let action = dispatch_terminal_event(
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('й'), KeyModifiers::NONE)),
        )
        .expect("russian q key");
        assert_eq!(action, TuiAction::None);
        assert!(!state.should_exit());
        assert_eq!(state.input_buffer(), "qй");
    }

    #[test]
    fn handle_worker_outcome_rehydrates_pending_approval_from_backend_state() {
        let backend = FakeBackend {
            summary: SessionSummary {
                id: "session-a".to_string(),
                title: "Session A".to_string(),
                agent_profile_id: "default".to_string(),
                agent_name: "Default".to_string(),
                scheduled_by: None,
                schedule: None,
                model: Some("glm-5-turbo".to_string()),
                reasoning_visible: true,
                think_level: None,
                compactifications: 0,
                completion_nudges: None,
                auto_approve: false,
                context_tokens: 0,
                usage_input_tokens: None,
                usage_output_tokens: None,
                usage_total_tokens: None,
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
            transcript: SessionTranscriptView {
                session_id: "session-a".to_string(),
                entries: Vec::new(),
            },
            debug_bundle: "unused".to_string(),
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

    #[test]
    fn canonical_command_accepts_trailing_slashes_and_backslashes() {
        assert_eq!(canonical_command("\\апрув\\"), Some("/approve"));
        assert_eq!(canonical_command("/approve/"), Some("/approve"));
        assert_eq!(canonical_command("\\контекст\\"), Some("/context"));
    }

    #[test]
    fn canonical_command_accepts_debug_aliases() {
        assert_eq!(canonical_command("/debug"), Some("/debug"));
        assert_eq!(canonical_command("\\отладка"), Some("/debug"));
    }

    #[test]
    fn canonical_command_accepts_help_aliases() {
        assert_eq!(canonical_command("\\помощь"), Some("/help"));
        assert_eq!(canonical_command("\\настройки"), Some("/settings"));
        assert_eq!(canonical_command("\\пауза"), Some("/pause"));
        assert_eq!(canonical_command("\\отмена"), Some("/cancel"));
    }

    #[test]
    fn pause_command_reports_when_no_active_run_exists() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let backend = FakeBackend {
            summary: SessionSummary {
                id: "session-a".to_string(),
                title: "Session A".to_string(),
                agent_profile_id: "default".to_string(),
                agent_name: "Default".to_string(),
                scheduled_by: None,
                schedule: None,
                model: Some("glm-5-turbo".to_string()),
                reasoning_visible: true,
                think_level: None,
                compactifications: 0,
                completion_nudges: None,
                auto_approve: false,
                context_tokens: 0,
                usage_input_tokens: None,
                usage_output_tokens: None,
                usage_total_tokens: None,
                has_pending_approval: false,
                last_message_preview: None,
                message_count: 0,
                background_job_count: 0,
                running_background_job_count: 0,
                queued_background_job_count: 0,
                created_at: 1,
                updated_at: 2,
            },
            pending: Vec::new(),
            transcript: SessionTranscriptView {
                session_id: "session-a".to_string(),
                entries: Vec::new(),
            },
            debug_bundle: "unused".to_string(),
        };
        let mut state = TuiAppState::new(
            vec![backend.summary.clone()],
            Some(backend.summary.id.clone()),
        );
        state.set_current_session(backend.summary.clone(), Timeline::default());

        handle_command(&backend, &mut state, "\\пауза", &mut redraw).expect("pause command");

        let entries = state.timeline().entries(true);
        let last = entries.last().expect("timeline entry");
        assert!(last.content.contains("пауза не нужна: активного хода нет"));
    }

    #[test]
    fn submit_command_without_required_argument_stays_in_timeline() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let backend = FakeBackend {
            summary: SessionSummary {
                id: "session-a".to_string(),
                title: "Session A".to_string(),
                agent_profile_id: "default".to_string(),
                agent_name: "Default".to_string(),
                scheduled_by: None,
                schedule: None,
                model: Some("glm-5-turbo".to_string()),
                reasoning_visible: true,
                think_level: None,
                compactifications: 0,
                completion_nudges: None,
                auto_approve: false,
                context_tokens: 0,
                usage_input_tokens: None,
                usage_output_tokens: None,
                usage_total_tokens: None,
                has_pending_approval: false,
                last_message_preview: None,
                message_count: 0,
                background_job_count: 0,
                running_background_job_count: 0,
                queued_background_job_count: 0,
                created_at: 1,
                updated_at: 2,
            },
            pending: Vec::new(),
            transcript: SessionTranscriptView {
                session_id: "session-a".to_string(),
                entries: Vec::new(),
            },
            debug_bundle: "unused".to_string(),
        };
        let mut state = TuiAppState::new(
            vec![backend.summary.clone()],
            Some(backend.summary.id.clone()),
        );
        state.set_current_session(backend.summary.clone(), Timeline::default());

        dispatch_action(
            &backend,
            &mut state,
            TuiAction::SubmitChatInput("\\доводка".to_string()),
            &mut redraw,
        )
        .expect("command should stay inside timeline");

        let entries = state.timeline().entries(true);
        let last = entries.last().expect("timeline entry");
        assert!(matches!(last.kind, TimelineEntryKind::System));
        assert!(last.content.contains("не хватает аргументов"));
        assert!(last.content.contains("Формат: \\доводка <N|выкл>"));
        assert!(last.content.contains("\\доводка 3"));
    }

    #[test]
    fn debug_command_reports_saved_path() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let backend_bundle = temp.path().join("backend-debug.txt");
        std::fs::write(&backend_bundle, "Debug Bundle\nctx=42\n").expect("write backend bundle");

        let backend = FakeBackend {
            summary: SessionSummary {
                id: "session-a".to_string(),
                title: "Session A".to_string(),
                agent_profile_id: "default".to_string(),
                agent_name: "Default".to_string(),
                scheduled_by: None,
                schedule: None,
                model: Some("glm-5-turbo".to_string()),
                reasoning_visible: true,
                think_level: None,
                compactifications: 0,
                completion_nudges: None,
                auto_approve: false,
                context_tokens: 0,
                usage_input_tokens: None,
                usage_output_tokens: None,
                usage_total_tokens: None,
                has_pending_approval: false,
                last_message_preview: None,
                message_count: 0,
                background_job_count: 0,
                running_background_job_count: 0,
                queued_background_job_count: 0,
                created_at: 1,
                updated_at: 2,
            },
            pending: Vec::new(),
            transcript: SessionTranscriptView {
                session_id: "session-a".to_string(),
                entries: Vec::new(),
            },
            debug_bundle: backend_bundle.display().to_string(),
        };
        let mut state = TuiAppState::new(
            vec![backend.summary.clone()],
            Some(backend.summary.id.clone()),
        );
        state.set_current_session(backend.summary.clone(), Timeline::default());
        state.scroll_page_up();

        handle_command(&backend, &mut state, "\\отладка", &mut redraw).expect("handle command");

        let entries = state.timeline().entries(true);
        let last = entries.last().expect("timeline entry");
        assert!(matches!(last.kind, TimelineEntryKind::System));
        let saved_path = last
            .content
            .strip_prefix("отладочный пакет сохранён: ")
            .expect("saved path prefix");
        let saved = std::fs::read_to_string(saved_path).expect("read saved bundle");
        assert!(saved.contains("TUI Debug Bundle"));
        assert!(saved.contains("backend_debug_bundle_path="));
        assert!(saved.contains("Viewport:"));
        assert!(saved.contains("scroll_offset=10"));
        assert!(saved.contains("timeline_scroll_top="));
        assert!(saved.contains("Backend Bundle Contents:"));
        assert!(saved.contains("Debug Bundle\nctx=42"));
    }

    #[test]
    fn help_command_reports_judge_topic() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let backend = FakeBackend {
            summary: SessionSummary {
                id: "session-a".to_string(),
                title: "Session A".to_string(),
                agent_profile_id: "default".to_string(),
                agent_name: "Default".to_string(),
                scheduled_by: None,
                schedule: None,
                model: Some("glm-5-turbo".to_string()),
                reasoning_visible: true,
                think_level: None,
                compactifications: 0,
                completion_nudges: None,
                auto_approve: false,
                context_tokens: 0,
                usage_input_tokens: None,
                usage_output_tokens: None,
                usage_total_tokens: None,
                has_pending_approval: false,
                last_message_preview: None,
                message_count: 0,
                background_job_count: 0,
                running_background_job_count: 0,
                queued_background_job_count: 0,
                created_at: 1,
                updated_at: 2,
            },
            pending: Vec::new(),
            transcript: SessionTranscriptView {
                session_id: "session-a".to_string(),
                entries: Vec::new(),
            },
            debug_bundle: "unused".to_string(),
        };
        let mut state = TuiAppState::new(
            vec![backend.summary.clone()],
            Some(backend.summary.id.clone()),
        );
        state.set_current_session(backend.summary.clone(), Timeline::default());

        handle_command(&backend, &mut state, "\\помощь судья", &mut redraw)
            .expect("handle help command");

        let entries = state.timeline().entries(true);
        let last = entries.last().expect("timeline entry");
        assert!(last.content.contains("\\агент выбрать judge"));
        assert!(last.content.contains("[daemon.a2a_peers.judge]"));
        assert!(last.content.contains("message_agent"));
    }

    #[test]
    fn agent_and_schedule_commands_open_dedicated_screens() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let backend = FakeBackend {
            summary: SessionSummary {
                id: "session-a".to_string(),
                title: "Session A".to_string(),
                agent_profile_id: "default".to_string(),
                agent_name: "Default".to_string(),
                scheduled_by: None,
                schedule: None,
                model: Some("glm-5-turbo".to_string()),
                reasoning_visible: true,
                think_level: None,
                compactifications: 0,
                completion_nudges: None,
                auto_approve: false,
                context_tokens: 0,
                usage_input_tokens: None,
                usage_output_tokens: None,
                usage_total_tokens: None,
                has_pending_approval: false,
                last_message_preview: None,
                message_count: 0,
                background_job_count: 0,
                running_background_job_count: 0,
                queued_background_job_count: 0,
                created_at: 1,
                updated_at: 2,
            },
            pending: Vec::new(),
            transcript: SessionTranscriptView {
                session_id: "session-a".to_string(),
                entries: Vec::new(),
            },
            debug_bundle: "unused".to_string(),
        };
        let mut state = TuiAppState::new(
            vec![backend.summary.clone()],
            Some(backend.summary.id.clone()),
        );
        state.set_current_session(backend.summary.clone(), Timeline::default());

        handle_command(&backend, &mut state, "\\агенты", &mut redraw).expect("agents command");
        assert_eq!(state.active_screen(), TuiScreen::Agents);
        assert!(state.browser_state().is_some());
        assert!(
            state
                .browser_selected_item()
                .expect("selected agent")
                .id
                .contains("default")
        );

        handle_command(&backend, &mut state, "\\расписания", &mut redraw)
            .expect("schedules command");
        assert_eq!(state.active_screen(), TuiScreen::Schedules);
        assert!(state.browser_state().is_some());
        assert!(
            state
                .browser_selected_item()
                .expect("selected schedule")
                .id
                .contains("pulse")
        );
    }

    #[test]
    fn artifact_commands_open_dedicated_screens() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let backend = FakeBackend {
            summary: SessionSummary {
                id: "session-a".to_string(),
                title: "Session A".to_string(),
                agent_profile_id: "default".to_string(),
                agent_name: "Default".to_string(),
                scheduled_by: None,
                schedule: None,
                model: Some("glm-5-turbo".to_string()),
                reasoning_visible: true,
                think_level: None,
                compactifications: 0,
                completion_nudges: None,
                auto_approve: false,
                context_tokens: 0,
                usage_input_tokens: None,
                usage_output_tokens: None,
                usage_total_tokens: None,
                has_pending_approval: false,
                last_message_preview: None,
                message_count: 0,
                background_job_count: 0,
                running_background_job_count: 0,
                queued_background_job_count: 0,
                created_at: 1,
                updated_at: 2,
            },
            pending: Vec::new(),
            transcript: SessionTranscriptView {
                session_id: "session-a".to_string(),
                entries: Vec::new(),
            },
            debug_bundle: "unused".to_string(),
        };
        let mut state = TuiAppState::new(
            vec![backend.summary.clone()],
            Some(backend.summary.id.clone()),
        );
        state.set_current_session(backend.summary.clone(), Timeline::default());

        handle_command(&backend, &mut state, "\\артефакты", &mut redraw)
            .expect("artifacts command");
        assert_eq!(state.active_screen(), TuiScreen::Artifacts);
        assert!(state.browser_state().is_some());
        assert!(
            state
                .browser_selected_item()
                .expect("selected artifact")
                .id
                .contains("artifact-1")
        );

        handle_command(&backend, &mut state, "\\артефакт artifact-1", &mut redraw)
            .expect("artifact command");
        assert_eq!(state.active_screen(), TuiScreen::Artifacts);
        assert!(state.browser_state().is_some());
        assert!(
            state
                .browser_state()
                .expect("artifact browser")
                .preview_content()
                .contains("artifact_id=artifact-1")
        );
    }

    #[test]
    fn artifact_browser_can_toggle_full_preview_and_apply_search() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let backend = FakeBackend {
            summary: SessionSummary {
                id: "session-a".to_string(),
                title: "Session A".to_string(),
                agent_profile_id: "default".to_string(),
                agent_name: "Default".to_string(),
                scheduled_by: None,
                schedule: None,
                model: Some("glm-5-turbo".to_string()),
                reasoning_visible: true,
                think_level: None,
                compactifications: 0,
                completion_nudges: None,
                auto_approve: false,
                context_tokens: 0,
                usage_input_tokens: None,
                usage_output_tokens: None,
                usage_total_tokens: None,
                has_pending_approval: false,
                last_message_preview: None,
                message_count: 0,
                background_job_count: 0,
                running_background_job_count: 0,
                queued_background_job_count: 0,
                created_at: 1,
                updated_at: 2,
            },
            pending: Vec::new(),
            transcript: SessionTranscriptView {
                session_id: "session-a".to_string(),
                entries: Vec::new(),
            },
            debug_bundle: "unused".to_string(),
        };
        let mut state = TuiAppState::new(
            vec![backend.summary.clone()],
            Some(backend.summary.id.clone()),
        );
        state.set_current_session(backend.summary.clone(), Timeline::default());

        handle_command(&backend, &mut state, "\\артефакты", &mut redraw)
            .expect("artifacts command");
        dispatch_action(
            &backend,
            &mut state,
            TuiAction::BrowserActivate,
            &mut redraw,
        )
        .expect("open full preview");
        assert!(state.browser_full_preview());

        dispatch_action(&backend, &mut state, TuiAction::BrowserSearch, &mut redraw)
            .expect("open search dialog");
        assert!(matches!(
            state.dialog_state(),
            Some(DialogState::BrowserSearch { .. })
        ));
        state.set_dialog_input("artifact_id".to_string());
        dispatch_action(&backend, &mut state, TuiAction::ConfirmDialog, &mut redraw)
            .expect("confirm search");
        assert_eq!(
            state
                .browser_state()
                .expect("artifact browser")
                .search_query(),
            Some("artifact_id")
        );
    }

    #[test]
    fn agent_browser_navigation_and_selection_use_backend_actions() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let summary = SessionSummary {
            id: "session-a".to_string(),
            title: "Session A".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            scheduled_by: None,
            schedule: None,
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: false,
            context_tokens: 0,
            usage_input_tokens: None,
            usage_output_tokens: None,
            usage_total_tokens: None,
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 0,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 2,
        };
        let backend = BrowserBackend {
            summary: summary.clone(),
            state: Arc::new(Mutex::new(BrowserBackendState {
                current_agent_id: "default".to_string(),
                agents: vec![
                    ("default".to_string(), "Default".to_string()),
                    ("judge".to_string(), "Judge".to_string()),
                ],
                schedules: vec![BrowserSchedule {
                    id: "pulse".to_string(),
                    agent_profile_id: "default".to_string(),
                    prompt: "проверь очередь".to_string(),
                    mode: AgentScheduleMode::Interval,
                    delivery_mode: AgentScheduleDeliveryMode::FreshSession,
                    target_session_id: None,
                    interval_seconds: 300,
                    enabled: true,
                }],
                connectors: Vec::new(),
                sent_messages: Vec::new(),
                chain_grants: Vec::new(),
            })),
        };
        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, Timeline::default());

        handle_command(&backend, &mut state, "\\агенты", &mut redraw).expect("agents command");
        assert_eq!(
            state.browser_state().expect("browser").preview_content(),
            "id=default\nname=Default"
        );

        dispatch_action(
            &backend,
            &mut state,
            TuiAction::BrowserSelectNext,
            &mut redraw,
        )
        .expect("select next");
        assert_eq!(
            state.browser_state().expect("browser").preview_content(),
            "id=judge\nname=Judge"
        );

        dispatch_action(
            &backend,
            &mut state,
            TuiAction::BrowserActivate,
            &mut redraw,
        )
        .expect("activate browser selection");
        assert!(
            backend
                .render_agents()
                .expect("render agents")
                .contains("Агенты: текущий=judge")
        );
        assert_eq!(
            state.browser_selected_item().map(|item| item.id.as_str()),
            Some("judge")
        );
    }

    #[test]
    fn browser_actions_can_open_agent_home_create_agent_and_manage_schedules() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let summary = SessionSummary {
            id: "session-a".to_string(),
            title: "Session A".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            scheduled_by: None,
            schedule: None,
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: false,
            context_tokens: 0,
            usage_input_tokens: None,
            usage_output_tokens: None,
            usage_total_tokens: None,
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 0,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 2,
        };
        let backend = BrowserBackend {
            summary: summary.clone(),
            state: Arc::new(Mutex::new(BrowserBackendState {
                current_agent_id: "default".to_string(),
                agents: vec![
                    ("default".to_string(), "Default".to_string()),
                    ("judge".to_string(), "Judge".to_string()),
                ],
                schedules: vec![BrowserSchedule {
                    id: "pulse".to_string(),
                    agent_profile_id: "default".to_string(),
                    prompt: "проверь очередь".to_string(),
                    mode: AgentScheduleMode::Interval,
                    delivery_mode: AgentScheduleDeliveryMode::FreshSession,
                    target_session_id: None,
                    interval_seconds: 300,
                    enabled: true,
                }],
                connectors: Vec::new(),
                sent_messages: Vec::new(),
                chain_grants: Vec::new(),
            })),
        };
        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, Timeline::default());

        handle_command(&backend, &mut state, "\\агенты", &mut redraw).expect("agents command");
        dispatch_action(
            &backend,
            &mut state,
            TuiAction::BrowserOpenSelected,
            &mut redraw,
        )
        .expect("open agent home");
        assert_eq!(
            state.browser_state().expect("browser").preview_content(),
            "/tmp/default"
        );

        dispatch_action(&backend, &mut state, TuiAction::BrowserCreate, &mut redraw)
            .expect("open create agent dialog");
        assert!(matches!(
            state.dialog_state(),
            Some(DialogState::CreateAgent { .. })
        ));
        state.set_dialog_input("Ревьюер из judge".to_string());
        dispatch_action(&backend, &mut state, TuiAction::ConfirmDialog, &mut redraw)
            .expect("confirm create agent");
        assert!(
            backend
                .render_agents()
                .expect("render agents")
                .contains("Ревьюер (ревьюер)")
        );

        handle_command(&backend, &mut state, "\\расписания", &mut redraw)
            .expect("schedules command");
        dispatch_action(&backend, &mut state, TuiAction::BrowserCreate, &mut redraw)
            .expect("open create schedule dialog");
        assert!(matches!(
            state.dialog_state(),
            Some(DialogState::CreateScheduleForm { .. })
        ));
        state.set_dialog_input("pulse2".to_string());
        for _ in 0..7 {
            state.dialog_next_field();
        }
        state.set_dialog_input("проверь очередь".to_string());
        dispatch_action(&backend, &mut state, TuiAction::ConfirmDialog, &mut redraw)
            .expect("confirm create schedule");
        assert!(
            backend
                .render_agent_schedules()
                .expect("render schedules")
                .contains("pulse2")
        );

        dispatch_action(&backend, &mut state, TuiAction::BrowserToggle, &mut redraw)
            .expect("toggle schedule");
        assert!(
            backend
                .render_agent_schedule("pulse2")
                .expect("render toggled schedule")
                .contains("enabled=false")
        );

        dispatch_action(&backend, &mut state, TuiAction::BrowserEdit, &mut redraw)
            .expect("open edit schedule dialog");
        assert!(matches!(
            state.dialog_state(),
            Some(DialogState::EditScheduleForm { .. })
        ));
        for _ in 0..6 {
            state.dialog_next_field();
        }
        state.set_dialog_input("обнови очередь и проверь ADET".to_string());
        dispatch_action(&backend, &mut state, TuiAction::ConfirmDialog, &mut redraw)
            .expect("confirm edit schedule");
        assert!(
            backend
                .render_agent_schedule("pulse2")
                .expect("render edited schedule")
                .contains("обнови очередь и проверь ADET")
        );

        dispatch_action(&backend, &mut state, TuiAction::BrowserDelete, &mut redraw)
            .expect("open delete schedule dialog");
        assert!(matches!(
            state.dialog_state(),
            Some(DialogState::ConfirmDeleteSchedule { .. })
        ));
        dispatch_action(&backend, &mut state, TuiAction::ConfirmDialog, &mut redraw)
            .expect("confirm delete schedule");
        assert!(
            !backend
                .render_agent_schedules()
                .expect("render schedules")
                .contains("pulse2")
        );
    }

    #[test]
    fn mcp_commands_open_browser_and_manage_connectors() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let summary = SessionSummary {
            id: "session-a".to_string(),
            title: "Session A".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            scheduled_by: None,
            schedule: None,
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: false,
            context_tokens: 0,
            usage_input_tokens: None,
            usage_output_tokens: None,
            usage_total_tokens: None,
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 0,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 2,
        };
        let backend = BrowserBackend {
            summary: summary.clone(),
            state: Arc::new(Mutex::new(BrowserBackendState {
                current_agent_id: "default".to_string(),
                agents: vec![
                    ("default".to_string(), "Default".to_string()),
                    ("judge".to_string(), "Judge".to_string()),
                ],
                schedules: Vec::new(),
                connectors: vec![BrowserMcpConnector {
                    id: "docs".to_string(),
                    command: "npx".to_string(),
                    args: vec![
                        "-y".to_string(),
                        "@modelcontextprotocol/server-filesystem".to_string(),
                        "/workspace".to_string(),
                    ],
                    cwd: Some("/srv/mcp".to_string()),
                    env: std::collections::BTreeMap::from([
                        ("DEBUG".to_string(), "1".to_string()),
                        ("TRACE".to_string(), "yes".to_string()),
                    ]),
                    enabled: true,
                    state: crate::mcp::McpConnectorState::Running,
                    restart_count: 0,
                }],
                sent_messages: Vec::new(),
                chain_grants: Vec::new(),
            })),
        };
        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, Timeline::default());

        handle_command(&backend, &mut state, "\\mcp", &mut redraw).expect("mcp command");
        assert_eq!(state.active_screen(), TuiScreen::Mcp);
        assert_eq!(
            state.browser_selected_item().map(|item| item.id.as_str()),
            Some("docs")
        );
        assert!(
            state
                .browser_state()
                .expect("mcp browser")
                .preview_content()
                .contains("command=npx")
        );

        dispatch_action(&backend, &mut state, TuiAction::BrowserCreate, &mut redraw)
            .expect("open create connector dialog");
        assert!(matches!(
            state.dialog_state(),
            Some(DialogState::CreateMcpConnectorForm { .. })
        ));
        state.set_dialog_input("git".to_string());
        state.dialog_next_field();
        state.set_dialog_input("uvx".to_string());
        state.dialog_next_field();
        state.set_dialog_input("mcp-server-git".to_string());
        state.dialog_next_field();
        state.set_dialog_input("/srv/git".to_string());
        state.dialog_next_field();
        state.set_dialog_input("TRACE=1".to_string());
        dispatch_action(&backend, &mut state, TuiAction::ConfirmDialog, &mut redraw)
            .expect("confirm create connector");
        assert!(
            backend
                .render_mcp_connectors()
                .expect("render mcp")
                .contains("git")
        );

        dispatch_action(&backend, &mut state, TuiAction::BrowserToggle, &mut redraw)
            .expect("toggle connector");
        assert!(
            backend
                .render_mcp_connector("git")
                .expect("render connector")
                .contains("enabled=false")
        );

        dispatch_action(&backend, &mut state, TuiAction::BrowserMessage, &mut redraw)
            .expect("restart connector");
        assert!(
            backend
                .render_mcp_connector("git")
                .expect("render restarted connector")
                .contains("restart_count=1")
        );

        dispatch_action(&backend, &mut state, TuiAction::BrowserEdit, &mut redraw)
            .expect("open edit connector dialog");
        assert!(matches!(
            state.dialog_state(),
            Some(DialogState::EditMcpConnectorForm { .. })
        ));
        state.dialog_next_field();
        state.dialog_next_field();
        state.dialog_next_field();
        state.dialog_next_field();
        state.set_dialog_input("true".to_string());
        dispatch_action(&backend, &mut state, TuiAction::ConfirmDialog, &mut redraw)
            .expect("confirm edit connector");
        assert!(
            backend
                .render_mcp_connector("git")
                .expect("render edited connector")
                .contains("enabled=true")
        );

        dispatch_action(&backend, &mut state, TuiAction::BrowserDelete, &mut redraw)
            .expect("open delete connector dialog");
        assert!(matches!(
            state.dialog_state(),
            Some(DialogState::ConfirmDeleteMcpConnector { .. })
        ));
        dispatch_action(&backend, &mut state, TuiAction::ConfirmDialog, &mut redraw)
            .expect("confirm delete connector");
        assert!(
            !backend
                .render_mcp_connectors()
                .expect("render mcp after delete")
                .contains("git")
        );
    }

    #[test]
    fn browser_message_action_opens_prefilled_dialog_and_sends_message() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let summary = SessionSummary {
            id: "session-a".to_string(),
            title: "Session A".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            scheduled_by: None,
            schedule: None,
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: false,
            context_tokens: 0,
            usage_input_tokens: None,
            usage_output_tokens: None,
            usage_total_tokens: None,
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 0,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 2,
        };
        let backend = BrowserBackend {
            summary: summary.clone(),
            state: Arc::new(Mutex::new(BrowserBackendState {
                current_agent_id: "default".to_string(),
                agents: vec![
                    ("default".to_string(), "Default".to_string()),
                    ("judge".to_string(), "Judge".to_string()),
                ],
                schedules: Vec::new(),
                connectors: Vec::new(),
                sent_messages: Vec::new(),
                chain_grants: Vec::new(),
            })),
        };
        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, Timeline::default());

        handle_command(&backend, &mut state, "\\агенты", &mut redraw).expect("agents command");
        dispatch_action(
            &backend,
            &mut state,
            TuiAction::BrowserSelectNext,
            &mut redraw,
        )
        .expect("select judge");
        dispatch_action(&backend, &mut state, TuiAction::BrowserMessage, &mut redraw)
            .expect("open message dialog");

        match state.dialog_state() {
            Some(DialogState::SendAgentMessageForm { form }) => {
                assert_eq!(form.target_agent_id(), "judge");
            }
            other => panic!("unexpected dialog state: {other:?}"),
        }
        state.dialog_next_field();
        state.set_dialog_input("Проверь последний вывод".to_string());
        dispatch_action(&backend, &mut state, TuiAction::ConfirmDialog, &mut redraw)
            .expect("confirm send message");

        let locked = backend.state.lock().expect("browser backend state");
        assert_eq!(
            locked.sent_messages,
            vec![(
                "session-a".to_string(),
                "judge".to_string(),
                "Проверь последний вывод".to_string()
            )]
        );
    }

    #[test]
    fn judge_and_chain_commands_route_operator_actions() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let summary = SessionSummary {
            id: "session-a".to_string(),
            title: "Session A".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            scheduled_by: None,
            schedule: None,
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: false,
            context_tokens: 0,
            usage_input_tokens: None,
            usage_output_tokens: None,
            usage_total_tokens: None,
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 0,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 2,
        };
        let backend = BrowserBackend {
            summary: summary.clone(),
            state: Arc::new(Mutex::new(BrowserBackendState {
                current_agent_id: "default".to_string(),
                agents: vec![
                    ("default".to_string(), "Default".to_string()),
                    ("judge".to_string(), "Judge".to_string()),
                ],
                schedules: Vec::new(),
                connectors: Vec::new(),
                sent_messages: Vec::new(),
                chain_grants: Vec::new(),
            })),
        };
        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, Timeline::default());

        handle_command(
            &backend,
            &mut state,
            "\\судья проверь этот результат",
            &mut redraw,
        )
        .expect("judge command");
        handle_command(
            &backend,
            &mut state,
            "\\цепочка продолжить chain-123 нужен ещё один hop",
            &mut redraw,
        )
        .expect("chain command");

        let locked = backend.state.lock().expect("browser backend state");
        assert_eq!(
            locked.sent_messages,
            vec![(
                "session-a".to_string(),
                "judge".to_string(),
                "проверь этот результат".to_string()
            )]
        );
        assert_eq!(
            locked.chain_grants,
            vec![(
                "session-a".to_string(),
                "chain-123".to_string(),
                "нужен ещё один hop".to_string()
            )]
        );
    }

    #[test]
    fn memory_commands_render_results_into_timeline() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let summary = SessionSummary {
            id: "session-a".to_string(),
            title: "Session A".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            scheduled_by: None,
            schedule: None,
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: false,
            context_tokens: 0,
            usage_input_tokens: None,
            usage_output_tokens: None,
            usage_total_tokens: None,
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 0,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 2,
        };
        let backend = FakeBackend {
            summary: summary.clone(),
            pending: Vec::new(),
            transcript: SessionTranscriptView {
                session_id: "session-a".to_string(),
                entries: Vec::new(),
            },
            debug_bundle: "unused".to_string(),
        };
        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, Timeline::default());

        handle_command(&backend, &mut state, "\\память сессии offline", &mut redraw)
            .expect("memory sessions");
        handle_command(
            &backend,
            &mut state,
            "\\память сессия session-a transcript",
            &mut redraw,
        )
        .expect("memory session");
        handle_command(
            &backend,
            &mut state,
            "\\память знания README.md",
            &mut redraw,
        )
        .expect("memory knowledge");
        handle_command(
            &backend,
            &mut state,
            "\\память файл README.md full",
            &mut redraw,
        )
        .expect("memory file");

        let rendered = state
            .timeline()
            .entries(true)
            .iter()
            .map(|entry| entry.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("memory sessions query=offline"));
        assert!(rendered.contains("memory session id=session-a mode=transcript"));
        assert!(rendered.contains("memory knowledge query=README.md"));
        assert!(rendered.contains("memory file path=README.md mode=full"));
    }

    #[test]
    fn context_command_renders_offload_overview_into_timeline() {
        fn redraw(_: &TuiAppState) -> Result<(), BootstrapError> {
            Ok(())
        }

        let summary = SessionSummary {
            id: "session-a".to_string(),
            title: "Session A".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            scheduled_by: None,
            schedule: None,
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: false,
            context_tokens: 0,
            usage_input_tokens: None,
            usage_output_tokens: None,
            usage_total_tokens: None,
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 0,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 2,
        };
        let backend = FakeBackend {
            summary: summary.clone(),
            pending: Vec::new(),
            transcript: SessionTranscriptView {
                session_id: "session-a".to_string(),
                entries: Vec::new(),
            },
            debug_bundle: "unused".to_string(),
        };
        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, Timeline::default());

        handle_command(&backend, &mut state, "\\контекст", &mut redraw).expect("context");

        let rendered = state
            .timeline()
            .entries(true)
            .iter()
            .map(|entry| entry.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("Context:"));
        assert!(rendered.contains("ctx=0"));
    }

    #[test]
    fn chat_shortcuts_open_interagent_dialogs() {
        let mut state = TuiAppState::new(
            vec![SessionSummary {
                id: "session-a".to_string(),
                title: "Session A".to_string(),
                agent_profile_id: "default".to_string(),
                agent_name: "Default".to_string(),
                scheduled_by: None,
                schedule: None,
                model: Some("glm-5-turbo".to_string()),
                reasoning_visible: true,
                think_level: None,
                compactifications: 0,
                completion_nudges: None,
                auto_approve: false,
                context_tokens: 0,
                usage_input_tokens: None,
                usage_output_tokens: None,
                usage_total_tokens: None,
                has_pending_approval: false,
                last_message_preview: None,
                message_count: 0,
                background_job_count: 0,
                running_background_job_count: 0,
                queued_background_job_count: 0,
                created_at: 1,
                updated_at: 2,
            }],
            Some("session-a".to_string()),
        );

        let action = crate::tui::screens::chat::handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
        )
        .expect("ctrl+j");
        assert_eq!(action, TuiAction::OpenJudgeDialog);

        let action = crate::tui::screens::chat::handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
        )
        .expect("ctrl+g");
        assert_eq!(action, TuiAction::OpenChainGrantDialog);
    }

    #[test]
    fn refresh_current_session_rebuilds_timeline_from_backend_transcript() {
        let summary = SessionSummary {
            id: "session-a".to_string(),
            title: "Session A".to_string(),
            agent_profile_id: "default".to_string(),
            agent_name: "Default".to_string(),
            scheduled_by: None,
            schedule: None,
            model: Some("glm-5-turbo".to_string()),
            reasoning_visible: true,
            think_level: None,
            compactifications: 0,
            completion_nudges: None,
            auto_approve: false,
            context_tokens: 0,
            usage_input_tokens: None,
            usage_output_tokens: None,
            usage_total_tokens: None,
            has_pending_approval: false,
            last_message_preview: None,
            message_count: 2,
            background_job_count: 0,
            running_background_job_count: 0,
            queued_background_job_count: 0,
            created_at: 1,
            updated_at: 2,
        };
        let backend = FakeBackend {
            summary: summary.clone(),
            pending: Vec::new(),
            transcript: SessionTranscriptView {
                session_id: "session-a".to_string(),
                entries: vec![
                    crate::bootstrap::SessionTranscriptLine {
                        role: "user".to_string(),
                        content: "hi".to_string(),
                        run_id: None,
                        created_at: 10,
                        tool_name: None,
                        tool_status: None,
                        approval_id: None,
                    },
                    crate::bootstrap::SessionTranscriptLine {
                        role: "tool".to_string(),
                        content: "exec_start executable=echo argc=1 -> process_result process_id=exec-1 status=Exited exit_code=Some(0)".to_string(),
                        run_id: Some("run-1".to_string()),
                        created_at: 11,
                        tool_name: Some("exec_start".to_string()),
                        tool_status: Some("completed".to_string()),
                        approval_id: None,
                    },
                    crate::bootstrap::SessionTranscriptLine {
                        role: "assistant".to_string(),
                        content: "done".to_string(),
                        run_id: Some("run-1".to_string()),
                        created_at: 12,
                        tool_name: None,
                        tool_status: None,
                        approval_id: None,
                    },
                ],
            },
            debug_bundle: "unused".to_string(),
        };
        let mut state = TuiAppState::new(vec![summary.clone()], Some(summary.id.clone()));
        state.set_current_session(summary, Timeline::default());
        state.scroll_page_up();

        refresh_current_session(&backend, &mut state).expect("refresh current session");

        let entries = state.timeline().entries(true);
        assert!(entries.iter().any(|entry| {
            matches!(
                entry.kind,
                TimelineEntryKind::Tool {
                    ref tool_name,
                    ref status,
                    ..
                } if tool_name == "exec_start" && status == "completed"
            )
        }));
        assert_eq!(state.scroll_offset(), 0);
    }
}
