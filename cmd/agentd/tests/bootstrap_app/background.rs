use super::support::*;

#[test]
fn background_worker_completes_chat_turn_jobs_and_wakes_idle_sessions() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_background_job",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_background_job",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Background work is done."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":12,"output_tokens":5,"total_tokens":17}
            }"#
        .to_string(),
        r#"{
                "id":"resp_wakeup_turn",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_wakeup_turn",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"I saw the background result and resumed the session."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":14,"output_tokens":7,"total_tokens":21}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
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

    store
        .put_session(&SessionRecord {
            id: "session-bg-worker".to_string(),
            title: "Background Worker".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_job(
            &JobRecord::try_from(&agent_runtime::mission::JobSpec::chat_turn(
                "job-bg-chat",
                "session-bg-worker",
                None,
                None,
                "Complete this in the background",
                2,
            ))
            .expect("job record"),
        )
        .expect("put chat job");

    let report = app
        .background_worker_tick(10)
        .expect("run background worker");
    let first_request = requests.recv().expect("job request");
    let second_request = requests.recv().expect("wake-up request");
    handle.join().expect("join server");

    assert_eq!(report.executed_jobs, 1);
    assert_eq!(report.woken_sessions, 1);
    assert_eq!(report.emitted_inbox_events, 1);

    let job = store
        .get_job("job-bg-chat")
        .expect("get job")
        .expect("job exists");
    assert_eq!(job.status, "completed");
    assert_eq!(
        job.last_progress_message.as_deref(),
        Some("background chat turn completed")
    );

    let inbox_events = store
        .list_session_inbox_events_for_session("session-bg-worker")
        .expect("list inbox events");
    assert_eq!(inbox_events.len(), 1);
    assert_eq!(inbox_events[0].status, "processed");

    let transcripts = store
        .list_transcripts_for_session("session-bg-worker")
        .expect("list transcripts");
    assert_eq!(transcripts.len(), 4);
    assert_eq!(transcripts[0].kind, "user");
    assert_eq!(transcripts[1].kind, "assistant");
    assert_eq!(transcripts[2].kind, "system");
    assert!(transcripts[2].content.contains("background job completed"));
    assert_eq!(
        transcripts[3].content,
        "I saw the background result and resumed the session."
    );

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("complete this in the background"));
    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("background job completed"));
    assert!(normalized_second.contains("background work is done"));
}

#[test]
fn background_worker_executes_delegate_jobs_as_child_sessions_and_wakes_parent() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_delegate_child",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_delegate_child",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Сейчас в Москве около +2°C, облачно."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":9,"total_tokens":27}
            }"#
        .to_string(),
        r#"{
                "id":"resp_delegate_parent",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_delegate_parent",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Делегированная задача завершилась, я увидел результат и продолжаю работу."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":16,"output_tokens":12,"total_tokens":28}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
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

    store
        .put_session(&SessionRecord {
            id: "session-delegate-parent".to_string(),
            title: "Delegate Parent".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put parent session");
    let mut job = JobSpec::delegate(
        "job-bg-delegate",
        "session-delegate-parent",
        None,
        None,
        "weather-helper",
        "Узнай текущую погоду в Москве и верни короткую сводку по-русски.",
        vec!["notes/weather.txt".to_string()],
        DelegateWriteScope::new(vec!["notes".to_string()]).expect("write scope"),
        "Короткая погодная сводка по-русски",
        "local-child",
        2,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.updated_at = 2;
    store
        .put_job(&JobRecord::try_from(&job).expect("delegate record"))
        .expect("put delegate job");

    let report = app
        .background_worker_tick(10)
        .expect("run background worker");
    let child_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("child request");
    let wake_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("wake request");
    handle.join().expect("join server");

    assert_eq!(report.executed_jobs, 1);
    assert_eq!(report.woken_sessions, 1);
    assert_eq!(report.emitted_inbox_events, 1);

    let job = JobSpec::try_from(
        store
            .get_job("job-bg-delegate")
            .expect("get delegate job")
            .expect("delegate job exists"),
    )
    .expect("restore delegate job");
    assert_eq!(job.status, agent_runtime::mission::JobStatus::Completed);

    let (child_session_id, package) = match job.result {
        Some(JobResult::Delegation {
            child_session_id,
            package,
        }) => (child_session_id, package),
        other => panic!("expected delegation result, got {other:?}"),
    };
    assert_eq!(package.summary, "Сейчас в Москве около +2°C, облачно.");

    let child_session = store
        .get_session(&child_session_id)
        .expect("get child session")
        .expect("child session exists");
    assert_eq!(
        child_session.parent_session_id.as_deref(),
        Some("session-delegate-parent")
    );
    assert_eq!(
        child_session.parent_job_id.as_deref(),
        Some("job-bg-delegate")
    );
    assert_eq!(
        child_session.delegation_label.as_deref(),
        Some("weather-helper")
    );

    let inbox_events = store
        .list_session_inbox_events_for_session("session-delegate-parent")
        .expect("list inbox events");
    assert_eq!(inbox_events.len(), 1);
    assert_eq!(inbox_events[0].kind, "delegation_result_ready");
    assert_eq!(inbox_events[0].status, "processed");

    let parent_transcripts = store
        .list_transcripts_for_session("session-delegate-parent")
        .expect("list parent transcripts");
    assert!(
        parent_transcripts
            .iter()
            .any(|record| record.content.contains("delegation started"))
    );
    assert!(
        parent_transcripts
            .iter()
            .any(|record| record.content.contains("delegation result ready"))
    );
    assert!(
        parent_transcripts
            .iter()
            .any(|record| record.content.contains("Делегированная задача завершилась"))
    );

    let child_transcripts = store
        .list_transcripts_for_session(&child_session_id)
        .expect("list child transcripts");
    assert_eq!(child_transcripts.len(), 3);
    assert_eq!(child_transcripts[0].kind, "system");
    assert_eq!(child_transcripts[1].kind, "user");
    assert_eq!(child_transcripts[2].kind, "assistant");

    let normalized_child_request = child_request.to_ascii_lowercase();
    assert!(normalized_child_request.contains("weather-helper"));
    assert!(normalized_child_request.contains("local-child"));
    let normalized_wake_request = wake_request.to_ascii_lowercase();
    assert!(normalized_wake_request.contains("delegation result ready"));
    assert!(normalized_wake_request.contains("+2"));
}

#[test]
fn background_worker_blocks_remote_delegate_jobs_without_falling_back_to_local() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some("http://127.0.0.1:9/v1".to_string()),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-delegate-remote".to_string(),
            title: "Delegate Remote Parent".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put parent session");

    let mut active_run = RunEngine::new("run-parent-active", "session-delegate-remote", None, 2);
    active_run.start(2).expect("start parent run");
    store
        .put_run(&RunRecord::try_from(active_run.snapshot()).expect("run record"))
        .expect("put active run");

    let mut job = JobSpec::delegate(
        "job-bg-delegate-remote",
        "session-delegate-remote",
        None,
        None,
        "judge-helper",
        "Проверь артефакты и верни вердикт.",
        vec!["reports/judge.md".to_string()],
        DelegateWriteScope::new(vec!["reports".to_string()]).expect("write scope"),
        "Краткий вердикт",
        "a2a:judge",
        2,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.updated_at = 2;
    store
        .put_job(&JobRecord::try_from(&job).expect("delegate record"))
        .expect("put delegate job");

    let report = app
        .background_worker_tick(10)
        .expect("run background worker");

    assert_eq!(report.executed_jobs, 1);
    assert_eq!(report.woken_sessions, 0);
    assert_eq!(report.emitted_inbox_events, 1);

    let job = JobSpec::try_from(
        store
            .get_job("job-bg-delegate-remote")
            .expect("get remote job")
            .expect("remote job exists"),
    )
    .expect("restore remote job");
    assert_eq!(job.status, agent_runtime::mission::JobStatus::Blocked);
    assert!(
        job.error
            .as_deref()
            .expect("blocked reason")
            .contains("remote delegation peer judge is not configured")
    );
    assert!(
        job.last_progress_message
            .as_deref()
            .expect("blocked progress")
            .contains("remote delegation peer judge is not configured")
    );

    assert!(
        store
            .get_session("session-delegate-job-bg-delegate-remote")
            .expect("get child session")
            .is_none()
    );

    let inbox_events = store
        .list_session_inbox_events_for_session("session-delegate-remote")
        .expect("list inbox events");
    assert_eq!(inbox_events.len(), 1);
    assert_eq!(inbox_events[0].kind, "job_blocked");
    assert_eq!(inbox_events[0].status, "queued");
}

#[test]
fn background_worker_cancels_jobs_with_cancel_requested_at() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some("http://127.0.0.1:9/v1".to_string()),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-bg-cancel".to_string(),
            title: "Background Cancel".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let mut job = agent_runtime::mission::JobSpec::chat_turn(
        "job-bg-cancel",
        "session-bg-cancel",
        None,
        None,
        "This should never execute",
        2,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.cancel_requested_at = Some(5);
    job.updated_at = 5;

    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");

    let report = app
        .background_worker_tick(10)
        .expect("run background worker");

    assert_eq!(report.executed_jobs, 0);

    let job = store
        .get_job("job-bg-cancel")
        .expect("get job")
        .expect("job exists");
    assert_eq!(job.status, "cancelled");
    assert_eq!(
        job.last_progress_message.as_deref(),
        Some("background job cancelled")
    );

    let transcripts = store
        .list_transcripts_for_session("session-bg-cancel")
        .expect("list transcripts");
    assert!(transcripts.is_empty());
}
