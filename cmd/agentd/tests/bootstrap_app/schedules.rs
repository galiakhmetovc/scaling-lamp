use super::support::*;
use agent_runtime::agent::{AgentScheduleDeliveryMode, AgentScheduleMode};
use agent_runtime::tool::{
    ContinueLaterInput, ScheduleCreateInput, ScheduleDeleteInput, ScheduleListInput,
    ScheduleReadInput, ScheduleUpdateInput,
};
use agentd::bootstrap::{AgentScheduleCreateOptions, AgentScheduleUpdatePatch};

fn seed_running_tool_context(
    store: &PersistenceStore,
    session_id: &str,
    agent_profile_id: &str,
    mission_id: &str,
    job_id: &str,
    run_id: &str,
) {
    store
        .put_session(&SessionRecord {
            id: session_id.to_string(),
            title: session_id.to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: fs::canonicalize(".")
                .expect("canonical workspace")
                .display()
                .to_string(),
            agent_profile_id: agent_profile_id.to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_mission(&MissionRecord {
            id: mission_id.to_string(),
            session_id: session_id.to_string(),
            objective: "schedule tool".to_string(),
            status: MissionStatus::Running.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put mission");

    let mut job = JobSpec::mission_turn(
        job_id,
        session_id,
        mission_id,
        Some(run_id),
        None,
        "tool",
        3,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.started_at = Some(4);
    job.updated_at = 4;

    let mut run = RunEngine::new(run_id, session_id, Some(mission_id), 4);
    run.start(4).expect("start run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");
}

#[test]
fn agent_schedule_creation_uses_current_workspace_and_selected_agent() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    app.select_agent_profile("judge").expect("select judge");
    let schedule = app
        .create_agent_schedule("judge-pulse", 300, "check the latest diff", None)
        .expect("create schedule");

    assert_eq!(schedule.id, "judge-pulse");
    assert_eq!(schedule.agent_profile_id, "judge");
    assert_eq!(
        schedule.workspace_root,
        fs::canonicalize(".").expect("canonical workspace")
    );

    let rendered = app
        .render_agent_schedule("judge-pulse")
        .expect("render schedule");
    assert!(rendered.contains("id=judge-pulse"));
    assert!(rendered.contains("agent_profile_id=judge"));
    assert!(rendered.contains("interval_seconds=300"));
    assert!(rendered.contains("check the latest diff"));
}

#[test]
fn agent_schedule_update_can_change_mode_delivery_target_prompt_and_enabled() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    app.select_agent_profile("judge").expect("select judge");
    let target = app
        .create_session_auto(Some("Schedule Target"))
        .expect("create target session");

    app.create_agent_schedule("judge-pulse", 300, "check the latest diff", None)
        .expect("create schedule");

    let updated = app
        .update_agent_schedule(
            "judge-pulse",
            AgentScheduleUpdatePatch {
                agent_identifier: Some("judge".to_string()),
                prompt: Some("check the latest diff and leave a verdict".to_string()),
                mode: Some(AgentScheduleMode::AfterCompletion),
                delivery_mode: Some(AgentScheduleDeliveryMode::ExistingSession),
                target_session_id: Some(target.id.clone()),
                interval_seconds: Some(120),
                enabled: Some(false),
            },
        )
        .expect("update schedule");

    assert_eq!(updated.id, "judge-pulse");
    assert_eq!(updated.agent_profile_id, "judge");
    assert_eq!(updated.mode, AgentScheduleMode::AfterCompletion);
    assert_eq!(
        updated.delivery_mode,
        AgentScheduleDeliveryMode::ExistingSession
    );
    assert_eq!(
        updated.target_session_id.as_deref(),
        Some(target.id.as_str())
    );
    assert_eq!(updated.interval_seconds, 120);
    assert!(!updated.enabled);
    assert_eq!(updated.prompt, "check the latest diff and leave a verdict");

    let rendered = app
        .render_agent_schedule("judge-pulse")
        .expect("render updated schedule");
    assert!(rendered.contains("mode=after_completion"));
    assert!(rendered.contains("delivery_mode=existing_session"));
    assert!(rendered.contains("enabled=false"));
    assert!(rendered.contains("target_session_id="));
}

#[test]
fn agent_schedule_create_with_options_and_toggle_round_trips_enabled_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let created = app
        .create_agent_schedule_with_options(
            "pulse",
            AgentScheduleCreateOptions {
                agent_identifier: None,
                interval_seconds: 60,
                prompt: "watch the queue".to_string(),
                mode: AgentScheduleMode::Interval,
                delivery_mode: AgentScheduleDeliveryMode::FreshSession,
                target_session_id: None,
                enabled: false,
            },
        )
        .expect("create schedule with options");
    assert!(!created.enabled);

    let enabled = app
        .set_agent_schedule_enabled("pulse", true)
        .expect("enable schedule");
    assert!(enabled.enabled);

    let disabled = app
        .set_agent_schedule_enabled("pulse", false)
        .expect("disable schedule");
    assert!(!disabled.enabled);
}

#[test]
fn schedule_tool_execution_can_create_self_targeted_existing_session_schedule_and_list_it() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        permissions: PermissionConfig {
            mode: PermissionMode::AcceptEdits,
            rules: Vec::new(),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    seed_running_tool_context(
        &store,
        "session-schedule-tool",
        "default",
        "mission-schedule-tool",
        "job-schedule-tool",
        "run-schedule-tool",
    );

    let report = app
        .request_tool_approval(
            "job-schedule-tool",
            "run-schedule-tool",
            &ToolCall::ScheduleCreate(ScheduleCreateInput {
                id: "continue-self".to_string(),
                agent_identifier: None,
                prompt: "Продолжи работу позже.".to_string(),
                mode: None,
                delivery_mode: Some(AgentScheduleDeliveryMode::ExistingSession),
                target_session_id: None,
                interval_seconds: 600,
                enabled: None,
            }),
            20,
        )
        .expect("schedule create tool");
    assert_eq!(report.run_status, RunStatus::Completed);
    assert!(
        report
            .output_summary
            .as_deref()
            .unwrap_or_default()
            .contains("schedule_create")
    );

    let schedule = app
        .agent_schedule("continue-self")
        .expect("created schedule");
    assert_eq!(schedule.agent_profile_id, "default");
    assert_eq!(schedule.mode, AgentScheduleMode::Interval);
    assert_eq!(
        schedule.delivery_mode,
        AgentScheduleDeliveryMode::ExistingSession
    );
    assert_eq!(
        schedule.target_session_id.as_deref(),
        Some("session-schedule-tool")
    );

    let read_report = app
        .request_tool_approval(
            {
                seed_running_tool_context(
                    &store,
                    "session-schedule-tool",
                    "default",
                    "mission-schedule-tool-read",
                    "job-schedule-tool-read",
                    "run-schedule-tool-read",
                );
                "job-schedule-tool-read"
            },
            "run-schedule-tool-read",
            &ToolCall::ScheduleRead(ScheduleReadInput {
                id: "continue-self".to_string(),
            }),
            21,
        )
        .expect("schedule read tool");
    assert_eq!(read_report.run_status, RunStatus::Completed);

    let list_report = app
        .request_tool_approval(
            {
                seed_running_tool_context(
                    &store,
                    "session-schedule-tool",
                    "default",
                    "mission-schedule-tool-list",
                    "job-schedule-tool-list",
                    "run-schedule-tool-list",
                );
                "job-schedule-tool-list"
            },
            "run-schedule-tool-list",
            &ToolCall::ScheduleList(ScheduleListInput {
                limit: None,
                offset: None,
                agent_identifier: None,
            }),
            22,
        )
        .expect("schedule list tool");
    assert_eq!(list_report.run_status, RunStatus::Completed);
    assert!(
        list_report
            .output_summary
            .as_deref()
            .unwrap_or_default()
            .contains("schedule_list")
    );
}

#[test]
fn continue_later_tool_creates_self_targeted_one_shot_schedule_with_handoff_payload() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        permissions: PermissionConfig {
            mode: PermissionMode::AcceptEdits,
            rules: Vec::new(),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    seed_running_tool_context(
        &store,
        "session-continue-later",
        "default",
        "mission-continue-later",
        "job-continue-later",
        "run-continue-later",
    );

    let report = app
        .request_tool_approval(
            "job-continue-later",
            "run-continue-later",
            &ToolCall::ContinueLater(ContinueLaterInput {
                delay_seconds: 900,
                handoff_payload: "Проверь статус установки и продолжи с места остановки."
                    .to_string(),
                delivery_mode: None,
            }),
            20,
        )
        .expect("continue_later tool");
    assert_eq!(report.run_status, RunStatus::Completed);
    assert!(
        report
            .output_summary
            .as_deref()
            .unwrap_or_default()
            .contains("continue_later")
    );

    let schedules = app.list_agent_schedules().expect("list schedules");
    assert_eq!(schedules.len(), 1);
    let schedule = &schedules[0];
    assert_eq!(schedule.agent_profile_id, "default");
    assert_eq!(schedule.mode, AgentScheduleMode::Once);
    assert_eq!(
        schedule.delivery_mode,
        AgentScheduleDeliveryMode::ExistingSession
    );
    assert_eq!(
        schedule.target_session_id.as_deref(),
        Some("session-continue-later")
    );
    assert_eq!(schedule.interval_seconds, 900);
    assert_eq!(schedule.next_fire_at, 920);
    assert!(schedule.enabled);
    assert!(schedule.prompt.contains("Проверь статус установки"));
}

#[test]
fn schedule_tool_execution_can_update_and_delete_schedule() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        permissions: PermissionConfig {
            mode: PermissionMode::AcceptEdits,
            rules: Vec::new(),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    seed_running_tool_context(
        &store,
        "session-schedule-tool-update",
        "default",
        "mission-schedule-tool-update",
        "job-schedule-tool-update",
        "run-schedule-tool-update",
    );
    app.create_agent_schedule("pulse", 300, "watch the queue", Some("default"))
        .expect("create schedule");

    let update_report = app
        .request_tool_approval(
            "job-schedule-tool-update",
            "run-schedule-tool-update",
            &ToolCall::ScheduleUpdate(ScheduleUpdateInput {
                id: "pulse".to_string(),
                agent_identifier: None,
                prompt: Some("watch the queue carefully".to_string()),
                mode: Some(AgentScheduleMode::AfterCompletion),
                delivery_mode: Some(AgentScheduleDeliveryMode::FreshSession),
                target_session_id: None,
                interval_seconds: Some(120),
                enabled: Some(false),
            }),
            20,
        )
        .expect("schedule update tool");
    assert_eq!(update_report.run_status, RunStatus::Completed);

    let updated = app.agent_schedule("pulse").expect("updated schedule");
    assert_eq!(updated.mode, AgentScheduleMode::AfterCompletion);
    assert!(!updated.enabled);
    assert_eq!(updated.interval_seconds, 120);
    assert_eq!(updated.prompt, "watch the queue carefully");

    let delete_report = app
        .request_tool_approval(
            {
                seed_running_tool_context(
                    &store,
                    "session-schedule-tool-update",
                    "default",
                    "mission-schedule-tool-delete",
                    "job-schedule-tool-delete",
                    "run-schedule-tool-delete",
                );
                "job-schedule-tool-delete"
            },
            "run-schedule-tool-delete",
            &ToolCall::ScheduleDelete(ScheduleDeleteInput {
                id: "pulse".to_string(),
            }),
            21,
        )
        .expect("schedule delete tool");
    assert_eq!(delete_report.run_status, RunStatus::Completed);
    assert!(matches!(
        app.agent_schedule("pulse"),
        Err(BootstrapError::MissingRecord {
            kind: "agent schedule",
            ..
        })
    ));
}

#[test]
fn background_worker_fires_due_agent_schedule_into_fresh_session_without_self_wakeup() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
            "id":"resp_schedule",
            "model":"gpt-5.4",
            "output":[
                {
                    "id":"msg_schedule",
                    "type":"message",
                    "status":"completed",
                    "role":"assistant",
                    "content":[{"type":"output_text","text":"Judge schedule completed."}]
                }
            ],
            "usage":{"input_tokens":16,"output_tokens":4,"total_tokens":20}
        }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    app.create_agent_schedule(
        "judge-pulse",
        300,
        "check the latest diff and summarize it",
        Some("judge"),
    )
    .expect("create schedule");
    let mut schedule = app.agent_schedule("judge-pulse").expect("load schedule");
    schedule.next_fire_at = 10;
    schedule.created_at = 1;
    schedule.updated_at = 1;
    store
        .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
        .expect("persist due schedule");

    let report = app
        .background_worker_tick(10)
        .expect("run background worker");
    let request = requests.recv().expect("schedule provider request");
    handle.join().expect("join server");

    assert_eq!(report.executed_jobs, 1);
    assert_eq!(report.emitted_inbox_events, 0);
    assert_eq!(report.woken_sessions, 0);

    let sessions = store.list_sessions().expect("list sessions");
    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.agent_profile_id, "judge");
    assert_eq!(session.title, "Расписание: judge-pulse");
    assert_eq!(
        session.delegation_label.as_deref(),
        Some("agent-schedule:judge-pulse")
    );
    let summaries = app
        .list_session_summaries()
        .expect("list session summaries");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].scheduled_by.as_deref(), Some("judge-pulse"));

    let jobs = store.list_jobs().expect("list jobs");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].kind, "scheduled_chat_turn");
    assert_eq!(jobs[0].status, "completed");

    let transcripts = store
        .list_transcripts_for_session(&session.id)
        .expect("list transcripts");
    assert_eq!(transcripts.len(), 3);
    assert!(transcripts.iter().any(|entry| {
        entry.kind == "system" && entry.content.contains("schedule_id: judge-pulse")
    }));
    assert!(transcripts.iter().any(|entry| {
        entry.kind == "user" && entry.content == "check the latest diff and summarize it"
    }));
    assert!(transcripts.iter().any(|entry| {
        entry.kind == "assistant" && entry.content == "Judge schedule completed."
    }));

    let inbox_events = store
        .list_session_inbox_events_for_session(&session.id)
        .expect("list inbox events");
    assert!(inbox_events.is_empty());

    let updated_schedule = app.agent_schedule("judge-pulse").expect("updated schedule");
    assert_eq!(updated_schedule.last_triggered_at, Some(10));
    assert_eq!(updated_schedule.next_fire_at, 310);
    assert_eq!(
        updated_schedule.last_session_id.as_deref(),
        Some(session.id.as_str())
    );
    assert_eq!(
        updated_schedule.last_job_id.as_deref(),
        Some(jobs[0].id.as_str())
    );

    let normalized_request = request.to_ascii_lowercase();
    assert!(normalized_request.contains("check the latest diff and summarize it"));
    assert!(normalized_request.contains("judge"));
}

#[test]
fn session_transcript_renders_schedule_origin_messages_with_visible_label() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let session = app
        .create_session_auto(Some("Scheduled target"))
        .expect("create session");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_transcript(&agent_persistence::TranscriptRecord::from(
            &TranscriptEntry::system(
                "transcript-schedule-meta",
                session.id.clone(),
                None,
                scheduled_input_metadata("judge-pulse", "transcript-schedule-user"),
                10,
            ),
        ))
        .expect("put schedule metadata");
    store
        .put_transcript(&agent_persistence::TranscriptRecord::from(
            &TranscriptEntry::user(
                "transcript-schedule-user",
                session.id.clone(),
                None,
                "check the latest diff and summarize it",
                11,
            ),
        ))
        .expect("put scheduled user message");

    let transcript = app
        .session_transcript(&session.id)
        .expect("render transcript view");

    assert_eq!(transcript.entries.len(), 1);
    assert_eq!(transcript.entries[0].role, "расписание: judge-pulse");
    assert_eq!(
        transcript.entries[0].content,
        "check the latest diff and summarize it"
    );
}
