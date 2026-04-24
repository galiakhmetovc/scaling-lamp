use super::*;

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
