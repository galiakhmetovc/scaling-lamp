use super::*;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const DEFAULT_SESSION_TOOL_PAGE_LIMIT: usize = 50;

pub(super) fn show_session_via_client(
    client: &DaemonClient,
    id: &str,
) -> Result<String, BootstrapError> {
    let detail = client.session_detail(id)?;
    Ok(render_session_detail(&detail))
}

pub(super) fn show_session(store: &PersistenceStore, id: &str) -> Result<String, BootstrapError> {
    let record = store
        .get_session(id)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "session",
            id: id.to_string(),
        })?;

    let agent_profile_id = record.agent_profile_id.clone();
    Ok(render_session_detail(&SessionDetailResponse {
        id: record.id,
        title: record.title,
        agent_profile_id: agent_profile_id.clone(),
        agent_name: agent_profile_id,
        prompt_override: record.prompt_override,
        settings_json: record.settings_json,
        active_mission_id: record.active_mission_id,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }))
}

pub(super) fn render_session_detail(detail: &SessionDetailResponse) -> String {
    format!(
        "session id={} title={} agent={} ({}) active_mission_id={} settings={}",
        detail.id,
        detail.title,
        detail.agent_name,
        detail.agent_profile_id,
        detail.active_mission_id.as_deref().unwrap_or("<none>"),
        detail.settings_json
    )
}

pub(super) fn show_chat(app: &App, session_id: &str) -> Result<String, BootstrapError> {
    let transcript = app.session_transcript(session_id)?;
    let rendered = transcript.render();
    if rendered.is_empty() {
        return Ok("<empty>".to_string());
    }

    Ok(rendered)
}

pub(super) fn show_chat_via_client(
    client: &DaemonClient,
    session_id: &str,
) -> Result<String, BootstrapError> {
    let transcript = client.session_transcript(session_id)?;
    let rendered = transcript.render();
    if rendered.is_empty() {
        return Ok("<empty>".to_string());
    }

    Ok(rendered)
}

pub(super) fn show_session_list(
    sessions: &[SessionSummary],
    format: SessionListFormat,
) -> Result<String, BootstrapError> {
    Ok(match format {
        SessionListFormat::Human => render_human_session_list(sessions),
        SessionListFormat::Raw => render_raw_session_list(sessions),
    })
}

fn render_raw_session_list(sessions: &[SessionSummary]) -> String {
    if sessions.is_empty() {
        return "sessions total=0\n<empty>".to_string();
    }

    let lines = sessions
        .iter()
        .map(|session| {
            let pending = if session.has_pending_approval {
                "yes"
            } else {
                "no"
            };
            let preview = session
                .last_message_preview
                .as_deref()
                .unwrap_or("<none>")
                .replace('\n', " ");
            format!(
                "session id={} title={} agent={} ({}) messages={} updated_at={} pending_approval={} background={} running={} queued={} preview={}",
                session.id,
                session.title,
                session.agent_name,
                session.agent_profile_id,
                session.message_count,
                session.updated_at,
                pending,
                session.background_job_count,
                session.running_background_job_count,
                session.queued_background_job_count,
                preview
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!("sessions total={}\n{lines}", sessions.len())
}

fn render_human_session_list(sessions: &[SessionSummary]) -> String {
    let mut lines = vec!["Sessions".to_string(), format!("total: {}", sessions.len())];

    if sessions.is_empty() {
        lines.push(String::new());
        lines.push("<empty>".to_string());
        return lines.join("\n");
    }

    for (index, session) in sessions.iter().enumerate() {
        lines.push(String::new());
        lines.push(format!("{}. {}", index + 1, session.title));
        lines.push(format!("   id: {}", session.id));
        lines.push(format!(
            "   agent: {} ({})",
            session.agent_name, session.agent_profile_id
        ));
        if let Some(model) = session.model.as_deref() {
            lines.push(format!("   model: {model}"));
        }
        lines.push(format!(
            "   updated: {}",
            format_unix_timestamp(session.updated_at)
        ));
        lines.push(format!(
            "   created: {}",
            format_unix_timestamp(session.created_at)
        ));
        lines.push(format!("   messages: {}", session.message_count));
        lines.push(format!("   context tokens: {}", session.context_tokens));
        lines.push(format!(
            "   usage: {}",
            format_session_usage(
                session.usage_input_tokens,
                session.usage_output_tokens,
                session.usage_total_tokens
            )
        ));
        lines.push(format!(
            "   pending approval: {}",
            yes_no(session.has_pending_approval)
        ));
        lines.push(format!("   auto approve: {}", yes_no(session.auto_approve)));
        lines.push(format!(
            "   background jobs: {} total, {} running, {} queued",
            session.background_job_count,
            session.running_background_job_count,
            session.queued_background_job_count
        ));
        if let Some(schedule) = session.schedule.as_ref() {
            lines.push(format!(
                "   schedule: {} {} enabled={}",
                schedule.id,
                schedule.mode.as_str(),
                yes_no(schedule.enabled)
            ));
        }
        let preview = session
            .last_message_preview
            .as_deref()
            .unwrap_or("<none>")
            .replace('\n', " ");
        lines.push(format!("   preview: {preview}"));
    }

    lines.join("\n")
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn format_session_usage(input: Option<u32>, output: Option<u32>, total: Option<u32>) -> String {
    match (input, output, total) {
        (None, None, None) => "<none>".to_string(),
        _ => format!(
            "input={}, output={}, total={}",
            input
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            output
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            total
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<none>".to_string())
        ),
    }
}

pub(super) fn show_session_tools(
    store: &PersistenceStore,
    session_id: &str,
    limit: Option<usize>,
    offset: usize,
    format: SessionToolsFormat,
) -> Result<String, BootstrapError> {
    if store.get_session(session_id)?.is_none() {
        return Err(BootstrapError::MissingRecord {
            kind: "session",
            id: session_id.to_string(),
        });
    }

    let calls = store.list_tool_calls_for_session(session_id)?;
    let total = calls.len();
    let limit = limit.unwrap_or(DEFAULT_SESSION_TOOL_PAGE_LIMIT);
    let page_start = offset.min(total);
    let page_end = page_start.saturating_add(limit).min(total);
    let showing = if page_start < page_end {
        format!("{}-{}", page_start + 1, page_end)
    } else {
        "0-0".to_string()
    };
    let next_offset = if page_end < total {
        page_end.to_string()
    } else {
        "<none>".to_string()
    };
    let page = SessionToolsPage {
        session_id,
        total,
        showing,
        limit,
        offset,
        next_offset,
    };

    if page_start == page_end {
        return Ok(render_empty_session_tools(&page, format));
    }

    let page_calls = &calls[page_start..page_end];
    Ok(match format {
        SessionToolsFormat::Human => render_human_session_tools(&page, page_calls),
        SessionToolsFormat::Raw => render_raw_session_tools(&page, page_calls),
    })
}

struct SessionToolsPage<'a> {
    session_id: &'a str,
    total: usize,
    showing: String,
    limit: usize,
    offset: usize,
    next_offset: String,
}

fn render_empty_session_tools(page: &SessionToolsPage<'_>, format: SessionToolsFormat) -> String {
    match format {
        SessionToolsFormat::Human => format!(
            "Session tools\nsession: {}\ntotal: {} | showing: {} | limit: {} | offset: {} | next_offset: {}\n\n<empty>",
            page.session_id, page.total, page.showing, page.limit, page.offset, page.next_offset
        ),
        SessionToolsFormat::Raw if page.total == 0 => format!(
            "session tools session_id={} total=0 showing=0-0 next_offset=<none>\n<empty>",
            page.session_id
        ),
        SessionToolsFormat::Raw => format!(
            "session tools session_id={} total={} showing={} limit={} offset={} next_offset={}\n<empty-page>",
            page.session_id, page.total, page.showing, page.limit, page.offset, page.next_offset
        ),
    }
}

fn render_raw_session_tools(
    page: &SessionToolsPage<'_>,
    calls: &[agent_persistence::ToolCallRecord],
) -> String {
    let header = format!(
        "session tools session_id={} total={} showing={} limit={} offset={} next_offset={}",
        page.session_id, page.total, page.showing, page.limit, page.offset, page.next_offset
    );
    let lines = calls
        .iter()
        .map(|call| {
            let error = call.error.as_deref().unwrap_or("<none>");
            format!(
                "tool_call id={} run_id={} provider_call_id={} tool={} status={} requested_at={} updated_at={} summary={} args={} error={}",
                call.id,
                call.run_id,
                call.provider_tool_call_id,
                call.tool_name,
                call.status,
                call.requested_at,
                call.updated_at,
                call.summary,
                call.arguments_json,
                error
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{header}\n{lines}")
}

fn render_human_session_tools(
    page: &SessionToolsPage<'_>,
    calls: &[agent_persistence::ToolCallRecord],
) -> String {
    let mut lines = vec![
        "Session tools".to_string(),
        format!("session: {}", page.session_id),
        format!(
            "total: {} | showing: {} | limit: {} | offset: {} | next_offset: {}",
            page.total, page.showing, page.limit, page.offset, page.next_offset
        ),
    ];

    let mut current_run_id: Option<&str> = None;
    for (index, call) in calls.iter().enumerate() {
        if current_run_id != Some(call.run_id.as_str()) {
            lines.push(String::new());
            lines.push(format!("Run {}", call.run_id));
            current_run_id = Some(call.run_id.as_str());
        }

        lines.push(format!(
            "  {}. {} [{}]",
            page.offset + index + 1,
            call.tool_name,
            call.status
        ));
        lines.push(format!(
            "     requested: {}",
            format_unix_timestamp(call.requested_at)
        ));
        lines.push(format!(
            "     updated: {}",
            format_unix_timestamp(call.updated_at)
        ));
        lines.push(format!("     summary: {}", call.summary));
        lines.push(format!(
            "     provider_call_id: {}",
            call.provider_tool_call_id
        ));
        lines.push(format!("     tool_call_id: {}", call.id));
        lines.push("     args:".to_string());
        lines.extend(indent_lines(
            &pretty_json_or_raw(&call.arguments_json),
            "       ",
        ));
        lines.push(format!(
            "     error: {}",
            call.error.as_deref().unwrap_or("<none>")
        ));
    }

    lines.join("\n")
}

fn pretty_json_or_raw(raw: &str) -> String {
    serde_json::from_str::<serde_json::Value>(raw)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| raw.to_string())
}

fn indent_lines(text: &str, indent: &str) -> Vec<String> {
    if text.is_empty() {
        return vec![indent.to_string()];
    }
    text.lines().map(|line| format!("{indent}{line}")).collect()
}

fn format_unix_timestamp(epoch_seconds: i64) -> String {
    match OffsetDateTime::from_unix_timestamp(epoch_seconds) {
        Ok(datetime) => match datetime.format(&Rfc3339) {
            Ok(formatted) => format!("{formatted} ({epoch_seconds})"),
            Err(_) => epoch_seconds.to_string(),
        },
        Err(_) => epoch_seconds.to_string(),
    }
}

pub(super) fn send_chat(
    app: &App,
    session_id: &str,
    message: &str,
) -> Result<String, BootstrapError> {
    match super::repl::send_chat_outcome(app, session_id, message)? {
        ChatSendOutcome::Completed {
            session_id,
            run_id,
            response_id,
            output_text,
        } => Ok(format!(
            "chat send session_id={} run_id={} response_id={} output={}",
            session_id,
            run_id.unwrap_or_else(|| "<none>".to_string()),
            response_id,
            output_text
        )),
        ChatSendOutcome::WaitingApproval {
            session_id,
            run_id,
            approval_id,
        } => Ok(format!(
            "chat send session_id={} run_id={} status=waiting_approval approval_id={}",
            session_id,
            run_id.unwrap_or_else(|| "<none>".to_string()),
            approval_id
        )),
    }
}

pub(super) fn send_chat_via_client(
    client: &DaemonClient,
    session_id: &str,
    message: &str,
) -> Result<String, BootstrapError> {
    match super::repl::send_chat_outcome_via_client(client, session_id, message)? {
        ChatSendOutcome::Completed {
            session_id,
            run_id,
            response_id,
            output_text,
        } => Ok(format!(
            "chat send session_id={} run_id={} response_id={} output={}",
            session_id,
            run_id.unwrap_or_else(|| "<daemon>".to_string()),
            response_id,
            output_text
        )),
        ChatSendOutcome::WaitingApproval {
            session_id,
            run_id,
            approval_id,
        } => {
            let mut line = format!("chat send session_id={} ", session_id);
            if let Some(run_id) = run_id {
                line.push_str(&format!("run_id={} ", run_id));
            }
            line.push_str(&format!(
                "status=waiting_approval approval_id={approval_id}"
            ));
            Ok(line)
        }
    }
}

pub(super) fn create_session(
    store: &PersistenceStore,
    id: &str,
    title: &str,
) -> Result<String, BootstrapError> {
    let now = unix_timestamp()?;
    let session = Session {
        id: id.to_string(),
        title: title.to_string(),
        prompt_override: None,
        settings: SessionSettings::default(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: now,
        updated_at: now,
    };
    let record = SessionRecord::try_from(&session).map_err(BootstrapError::RecordConversion)?;
    store.put_session(&record)?;
    Ok(format!(
        "created session {} title={}",
        record.id, record.title
    ))
}

pub(super) fn create_mission(
    store: &PersistenceStore,
    id: &str,
    session_id: &str,
    objective: &str,
) -> Result<String, BootstrapError> {
    if store.get_session(session_id)?.is_none() {
        return Err(BootstrapError::MissingRecord {
            kind: "session",
            id: session_id.to_string(),
        });
    }

    let now = unix_timestamp()?;
    let mission = MissionSpec {
        id: id.to_string(),
        session_id: session_id.to_string(),
        objective: objective.to_string(),
        status: MissionStatus::Ready,
        execution_intent: MissionExecutionIntent::Autonomous,
        schedule: MissionSchedule::once(),
        acceptance_criteria: Vec::new(),
        created_at: now,
        updated_at: now,
        completed_at: None,
    };
    let record = MissionRecord::try_from(&mission).map_err(BootstrapError::RecordConversion)?;
    store.put_mission(&record)?;
    Ok(format!(
        "created mission {} session_id={} objective={}",
        record.id, record.session_id, record.objective
    ))
}

pub(super) fn show_mission(store: &PersistenceStore, id: &str) -> Result<String, BootstrapError> {
    let record = store
        .get_mission(id)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "mission",
            id: id.to_string(),
        })?;

    Ok(format!(
        "mission id={} session_id={} status={} execution_intent={} objective={} schedule={} acceptance={}",
        record.id,
        record.session_id,
        record.status,
        record.execution_intent,
        record.objective,
        record.schedule_json,
        record.acceptance_json
    ))
}

pub(super) fn run_mission_tick(app: &App, now: i64) -> Result<String, BootstrapError> {
    let report = app.supervisor_tick(now, &[])?;
    let actions = if report.actions.is_empty() {
        "<none>".to_string()
    } else {
        report
            .actions
            .iter()
            .map(format_supervisor_action)
            .collect::<Vec<_>>()
            .join(" | ")
    };

    Ok(format!(
        "mission tick now={} queued_jobs={} dispatched_jobs={} blocked_jobs={} completed_missions={} budget_remaining={} actions={}",
        now,
        report.queued_jobs,
        report.dispatched_jobs,
        report.blocked_jobs,
        report.completed_missions,
        report.budget_remaining,
        actions
    ))
}

pub(super) fn show_run(store: &PersistenceStore, id: &str) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, id)?;
    Ok(format!(
        "run id={} session_id={} mission_id={} status={} pending_approvals={} delegates={} evidence_refs={} error={}",
        snapshot.id,
        snapshot.session_id,
        snapshot.mission_id.as_deref().unwrap_or("<none>"),
        snapshot.status.as_str(),
        snapshot.pending_approvals.len(),
        snapshot.delegate_runs.len(),
        snapshot.evidence_refs.len(),
        snapshot.error.as_deref().unwrap_or("<none>")
    ))
}

pub(super) fn show_job(store: &PersistenceStore, id: &str) -> Result<String, BootstrapError> {
    let record = store
        .get_job(id)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "job",
            id: id.to_string(),
        })?;

    Ok(format!(
        "job id={} mission_id={} run_id={} kind={} status={} input={} result={}",
        record.id,
        record.mission_id.as_deref().unwrap_or("<none>"),
        record.run_id.as_deref().unwrap_or("<none>"),
        record.kind,
        record.status,
        record.input_json.as_deref().unwrap_or("<none>"),
        record.result_json.as_deref().unwrap_or("<none>")
    ))
}

pub(super) fn execute_job(app: &App, id: &str, now: i64) -> Result<String, BootstrapError> {
    let report = app.execute_mission_turn_job(id, now)?;
    Ok(format!(
        "job execute id={} run_id={} response_id={} output={}",
        report.job_id, report.run_id, report.response_id, report.output_text
    ))
}

pub(super) fn list_approvals(
    store: &PersistenceStore,
    run_id: &str,
) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, run_id)?;
    if snapshot.pending_approvals.is_empty() {
        return Ok(format!("approval run_id={} none", run_id));
    }

    let approvals = snapshot
        .pending_approvals
        .iter()
        .map(|approval| {
            format!(
                "{} tool_call_id={} reason={}",
                approval.id, approval.tool_call_id, approval.reason
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");
    Ok(format!("approval run_id={} {}", run_id, approvals))
}

pub(super) fn approve_run(
    app: &App,
    run_id: &str,
    approval_id: &str,
) -> Result<String, BootstrapError> {
    let report = app.approve_run(run_id, approval_id, unix_timestamp()?)?;
    Ok(format!(
        "approved {} on run {} status={} response_id={} output={} next_approval={}",
        approval_id,
        report.run_id,
        report.run_status.as_str(),
        report.response_id.as_deref().unwrap_or("<none>"),
        report.output_text.as_deref().unwrap_or("<none>"),
        report.approval_id.as_deref().unwrap_or("<none>")
    ))
}

pub(super) fn list_delegates(
    store: &PersistenceStore,
    run_id: &str,
) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, run_id)?;
    if snapshot.delegate_runs.is_empty() {
        return Ok(format!("delegate run_id={} none", run_id));
    }

    let delegates = snapshot
        .delegate_runs
        .iter()
        .map(|delegate| format!("{} label={}", delegate.id, delegate.label))
        .collect::<Vec<_>>()
        .join(" | ");
    Ok(format!("delegate run_id={} {}", run_id, delegates))
}

pub(super) fn show_verification(
    store: &PersistenceStore,
    run_id: &str,
) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, run_id)?;
    let refs = if snapshot.evidence_refs.is_empty() {
        "<none>".to_string()
    } else {
        snapshot.evidence_refs.join(",")
    };
    Ok(format!("verification run_id={} refs={}", run_id, refs))
}

pub(super) fn render_session_skills_list(
    skills: Vec<crate::bootstrap::SessionSkillStatus>,
) -> Result<String, BootstrapError> {
    if skills.is_empty() {
        return Ok("Скиллы: ничего не найдено".to_string());
    }

    let mut lines = vec!["Скиллы:".to_string()];
    lines.extend(
        skills
            .into_iter()
            .map(|skill| format!("- [{}] {}: {}", skill.mode, skill.name, skill.description)),
    );
    Ok(lines.join("\n"))
}

pub(super) fn run_provider_smoke(app: &App, prompt: &str) -> Result<String, BootstrapError> {
    let driver = app.provider_driver()?;
    let response = driver.complete(&ProviderRequest {
        model: None,
        instructions: Some("Reply tersely.".to_string()),
        messages: vec![ProviderMessage::new(MessageRole::User, prompt)],
        think_level: None,
        previous_response_id: None,
        continuation_messages: Vec::new(),
        tools: Vec::new(),
        tool_outputs: Vec::new(),
        max_output_tokens: app.config.provider.max_output_tokens,
        stream: ProviderStreamMode::Disabled,
    })?;

    Ok(format!(
        "provider name={} response_id={} model={} finish_reason={} usage_total_tokens={} output={}",
        driver.descriptor().name,
        response.response_id,
        response.model,
        match response.finish_reason {
            FinishReason::Completed => "completed",
            FinishReason::Incomplete => "incomplete",
        },
        response
            .usage
            .map(|usage| usage.total_tokens)
            .unwrap_or_default(),
        response.output_text
    ))
}

pub(super) fn format_supervisor_action(
    action: &agent_runtime::scheduler::SupervisorAction,
) -> String {
    match action {
        agent_runtime::scheduler::SupervisorAction::QueueJob(job) => {
            format!("queue_job:{}", job.id)
        }
        agent_runtime::scheduler::SupervisorAction::DispatchJob { job_id, .. } => {
            format!("dispatch_job:{job_id}")
        }
        agent_runtime::scheduler::SupervisorAction::RequestApproval { job_id, .. } => {
            format!("request_approval:{job_id}")
        }
        agent_runtime::scheduler::SupervisorAction::DeferMission { mission_id, .. } => {
            format!("defer_mission:{mission_id}")
        }
        agent_runtime::scheduler::SupervisorAction::CompleteMission { mission_id } => {
            format!("complete_mission:{mission_id}")
        }
    }
}

pub(super) fn render_status(app: &App) -> Result<String, BootstrapError> {
    let connection = Connection::open(&app.persistence.stores.metadata_db)?;
    let session_count = count_rows(&connection, "sessions")?;
    let mission_count = count_rows(&connection, "missions")?;
    let run_count = count_rows(&connection, "runs")?;
    let job_count = count_rows(&connection, "jobs")?;

    Ok(format!(
        "status data_dir={} permission_mode={} sessions={} missions={} runs={} jobs={} components={} state_db={}",
        app.config.data_dir.display(),
        app.config.permissions.mode.as_str(),
        session_count,
        mission_count,
        run_count,
        job_count,
        app.runtime.component_count(),
        app.persistence.stores.metadata_db.display()
    ))
}

pub(super) fn render_diagnostics_tail(
    app: &App,
    max_lines: Option<usize>,
) -> Result<String, BootstrapError> {
    app.render_diagnostics_tail(
        max_lines.unwrap_or(app.config.runtime_limits.diagnostic_tail_lines),
    )
}

pub(super) fn render_daemon_status(status: &StatusResponse) -> Result<String, BootstrapError> {
    Ok(format!(
        "status data_dir={} permission_mode={} sessions={} missions={} runs={} jobs={} components={} state_db={}",
        status.data_dir,
        status.permission_mode,
        status.session_count,
        status.mission_count,
        status.run_count,
        status.job_count,
        status.components,
        status.state_db
    ))
}

pub(super) fn count_rows(connection: &Connection, table: &str) -> Result<i64, BootstrapError> {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(BootstrapError::Sqlite)
}

pub(super) fn load_run_snapshot(
    store: &PersistenceStore,
    run_id: &str,
) -> Result<RunSnapshot, BootstrapError> {
    let record = store
        .get_run(run_id)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "run",
            id: run_id.to_string(),
        })?;
    RunSnapshot::try_from(record).map_err(BootstrapError::RecordConversion)
}
