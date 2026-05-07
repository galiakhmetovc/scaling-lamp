use super::app::{BrowserItem, BrowserKind, TuiAppState, TuiScreen};
use super::backend::TuiBackend;
use super::browser_items::{
    parse_agent_browser_items, parse_artifact_browser_items, parse_mcp_browser_items,
    parse_schedule_browser_items,
};
use crate::bootstrap::{BootstrapError, SessionTask};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn open_agents_browser<B>(
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

pub(super) fn open_schedule_browser<B>(
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

pub(super) fn open_mcp_browser<B>(
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

pub(super) fn open_artifact_browser<B>(
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

pub(super) fn open_task_browser<B>(
    app: &B,
    state: &mut TuiAppState,
    session_id: &str,
    preferred_id: Option<&str>,
) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let tasks = app.session_tasks(session_id)?;
    if tasks.is_empty() {
        state.open_task_browser(
            format!("Задачи {session_id}"),
            "↑↓ выбор | Enter полный | / поиск | n/N | PgUp/PgDn".to_string(),
            Vec::new(),
            0,
            "Задачи".to_string(),
            "Делегированные задачи: нет".to_string(),
        );
        return Ok(());
    }
    let items = tasks
        .iter()
        .map(|task| BrowserItem::new(task.id.clone(), task_browser_label(task)))
        .collect::<Vec<_>>();
    let selected_index = preferred_id
        .and_then(|id| items.iter().position(|item| item.id == id))
        .unwrap_or(0);
    let selected_id = items
        .get(selected_index)
        .map(|item| item.id.as_str())
        .unwrap_or_default()
        .to_string();
    let preview_content = app.render_task(selected_id.as_str())?;
    state.open_task_browser(
        format!("Задачи {session_id}"),
        "↑↓ выбор | Enter полный | / поиск | n/N | PgUp/PgDn".to_string(),
        items,
        selected_index,
        format!("Задача {selected_id}"),
        preview_content,
    );
    Ok(())
}

pub(super) fn open_debug_browser<B>(app: &B, state: &mut TuiAppState) -> Result<(), BootstrapError>
where
    B: TuiBackend,
{
    let session_id = if state.active_screen() == TuiScreen::Sessions {
        state
            .selected_session()
            .map(|session| session.id.clone())
            .ok_or_else(|| BootstrapError::Usage {
                reason: "не выбрана сессия для debug-view".to_string(),
            })?
    } else {
        state
            .current_session_id()
            .map(str::to_string)
            .ok_or_else(|| BootstrapError::Usage {
                reason: "не выбрана текущая сессия".to_string(),
            })?
    };
    let view = app.session_debug_view(session_id.as_str())?;
    let items = view
        .entries
        .into_iter()
        .map(|entry| {
            BrowserItem::with_preview(entry.id, entry.label, entry.detail_title, entry.detail)
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        state.open_debug_browser(
            format!("Debug {session_id}"),
            "↑↓ выбор | Enter полный | / поиск | n/N | PgUp/PgDn".to_string(),
            Vec::new(),
            0,
            "Debug".to_string(),
            "В сессии пока нет сообщений, вызовов тулов или артефактов.".to_string(),
        );
        return Ok(());
    }
    let preview_title = items[0]
        .preview_title
        .clone()
        .unwrap_or_else(|| items[0].id.clone());
    let preview_content = items[0].preview_content.clone().unwrap_or_default();
    state.open_debug_browser(
        format!("Debug {session_id}"),
        "↑↓ выбор | Enter полный | / поиск | n/N | PgUp/PgDn".to_string(),
        items,
        0,
        preview_title,
        preview_content,
    );
    Ok(())
}

pub(super) fn refresh_browser_preview<B>(
    app: &B,
    state: &mut TuiAppState,
) -> Result<(), BootstrapError>
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
        BrowserKind::Tasks => (
            format!("Задача {}", selected.id),
            app.render_task(selected.id.as_str())?,
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
        BrowserKind::Debug => (
            selected
                .preview_title
                .clone()
                .unwrap_or_else(|| selected.id.clone()),
            selected.preview_content.clone().unwrap_or_default(),
        ),
    };
    state.set_browser_preview(title, content);
    Ok(())
}

pub(super) fn activate_browser_selection<B>(
    app: &B,
    state: &mut TuiAppState,
) -> Result<(), BootstrapError>
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
        BrowserKind::Tasks | BrowserKind::Artifacts | BrowserKind::Debug => {
            state.toggle_browser_full_preview()
        }
    }
    Ok(())
}

pub(super) fn open_browser_selection<B>(
    app: &B,
    state: &mut TuiAppState,
) -> Result<(), BootstrapError>
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
        BrowserKind::Schedules
        | BrowserKind::Mcp
        | BrowserKind::Tasks
        | BrowserKind::Artifacts
        | BrowserKind::Debug => refresh_browser_preview(app, state)?,
    }
    Ok(())
}

pub(super) fn open_browser_create_dialog(state: &mut TuiAppState) {
    match state.browser_state().map(|browser| browser.kind()) {
        Some(BrowserKind::Agents) => state.open_create_agent_dialog(),
        Some(BrowserKind::Schedules) => state.open_create_schedule_dialog(),
        Some(BrowserKind::Mcp) => state.open_create_mcp_connector_dialog(),
        Some(BrowserKind::Tasks | BrowserKind::Artifacts | BrowserKind::Debug) | None => {}
    }
}

pub(super) fn handle_browser_message_action<B>(
    app: &B,
    state: &mut TuiAppState,
) -> Result<(), BootstrapError>
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

pub(super) fn open_browser_edit_dialog<B>(
    app: &B,
    state: &mut TuiAppState,
) -> Result<(), BootstrapError>
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

pub(super) fn open_browser_delete_dialog(state: &mut TuiAppState) {
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

pub(super) fn toggle_browser_schedule<B>(
    app: &B,
    state: &mut TuiAppState,
) -> Result<(), BootstrapError>
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

pub(super) fn open_browser_search_dialog(state: &mut TuiAppState) {
    if matches!(
        state.browser_state().map(|browser| browser.kind()),
        Some(BrowserKind::Tasks | BrowserKind::Artifacts | BrowserKind::Debug)
    ) {
        state.open_browser_search_dialog();
    }
}

fn task_browser_label(task: &SessionTask) -> String {
    let executor = task
        .executor_agent_id
        .as_deref()
        .map(|value| format!(" executor={value}"))
        .unwrap_or_default();
    format!(
        "[{}] {} ({}){} updated={}",
        task.status, task.id, task.kind, executor, task.updated_at
    )
}

fn unix_timestamp() -> Result<i64, BootstrapError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(BootstrapError::Clock)?
        .as_secs() as i64)
}
