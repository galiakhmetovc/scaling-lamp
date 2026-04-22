use super::support::*;

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
