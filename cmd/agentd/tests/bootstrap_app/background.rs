use super::support::*;
use agent_runtime::agent::{
    AgentSchedule, AgentScheduleDeliveryMode, AgentScheduleInit, AgentScheduleMode,
};

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

#[test]
fn background_worker_interval_schedule_advances_to_next_future_cadence_without_burst_catch_up() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
            "id":"resp_schedule_interval",
            "model":"gpt-5.4",
            "output":[
                {
                    "id":"msg_schedule_interval",
                    "type":"message",
                    "status":"completed",
                    "role":"assistant",
                    "content":[{"type":"output_text","text":"interval ok"}]
                }
            ],
            "usage":{"input_tokens":10,"output_tokens":3,"total_tokens":13}
        }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        permissions: PermissionConfig {
            mode: PermissionMode::Plan,
            rules: Vec::new(),
        },
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

    let schedule = AgentSchedule::new(AgentScheduleInit {
        id: "interval-cadence".to_string(),
        agent_profile_id: "default".to_string(),
        workspace_root: fs::canonicalize(".").expect("canonical workspace"),
        prompt: "ping cadence".to_string(),
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
        updated_at: 1,
    })
    .expect("build schedule");
    store
        .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
        .expect("put schedule");

    let report = app
        .background_worker_tick(1000)
        .expect("run background worker");
    let request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("provider request");
    handle.join().expect("join server");

    assert_eq!(report.executed_jobs, 1);
    let updated = app
        .agent_schedule("interval-cadence")
        .expect("load updated schedule");
    assert_eq!(updated.last_triggered_at, Some(1000));
    assert_eq!(updated.next_fire_at, 1210);
    assert_eq!(store.list_jobs().expect("list jobs").len(), 1);

    let normalized_request = request.to_ascii_lowercase();
    assert!(normalized_request.contains("ping cadence"));
}

#[test]
fn background_worker_interval_existing_session_skips_busy_target_session() {
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
            id: "session-schedule-target-busy".to_string(),
            title: "Busy Target".to_string(),
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
        .expect("put target session");
    let mut active_run = RunEngine::new("run-busy-target", "session-schedule-target-busy", None, 2);
    active_run.start(2).expect("start active run");
    store
        .put_run(&RunRecord::try_from(active_run.snapshot()).expect("run record"))
        .expect("put active run");

    let schedule = AgentSchedule::new(AgentScheduleInit {
        id: "interval-existing-busy".to_string(),
        agent_profile_id: "default".to_string(),
        workspace_root: fs::canonicalize(".").expect("canonical workspace"),
        prompt: "check busy target".to_string(),
        mode: AgentScheduleMode::Interval,
        delivery_mode: AgentScheduleDeliveryMode::ExistingSession,
        target_session_id: Some("session-schedule-target-busy".to_string()),
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
        updated_at: 1,
    })
    .expect("build schedule");
    store
        .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
        .expect("put schedule");

    let report = app
        .background_worker_tick(10)
        .expect("run background worker");

    assert_eq!(report.executed_jobs, 0);
    assert!(store.list_jobs().expect("list jobs").is_empty());

    let updated = app
        .agent_schedule("interval-existing-busy")
        .expect("load updated schedule");
    assert_eq!(updated.last_triggered_at, None);
    assert_eq!(updated.next_fire_at, 310);
    assert_eq!(updated.last_result.as_deref(), Some("skipped_busy"));
    assert_eq!(updated.last_error, None);
}

#[test]
fn background_worker_after_completion_uses_own_terminal_job_state_for_due_time() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
            "id":"resp_after_completion",
            "model":"gpt-5.4",
            "output":[
                {
                    "id":"msg_after_completion",
                    "type":"message",
                    "status":"completed",
                    "role":"assistant",
                    "content":[{"type":"output_text","text":"after completion ok"}]
                }
            ],
            "usage":{"input_tokens":11,"output_tokens":4,"total_tokens":15}
        }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        permissions: PermissionConfig {
            mode: PermissionMode::Plan,
            rules: Vec::new(),
        },
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
            id: "session-after-completion-old".to_string(),
            title: "Old Schedule Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: Some("agent-schedule:after-own".to_string()),
            created_at: 1,
            updated_at: 1,
        })
        .expect("put old session");
    let mut own_job = JobSpec::scheduled_chat_turn(
        "job-after-own-old",
        "session-after-completion-old",
        None,
        None,
        "after-own",
        "poll completion",
        2,
    );
    own_job.status = agent_runtime::mission::JobStatus::Completed;
    own_job.finished_at = Some(20);
    own_job.updated_at = 20;
    own_job.result = Some(JobResult::Summary {
        outcome: "previous cycle ok".to_string(),
    });
    store
        .put_job(&JobRecord::try_from(&own_job).expect("own job record"))
        .expect("put own job");

    let mut unrelated_job = JobSpec::chat_turn(
        "job-after-own-manual",
        "session-after-completion-old",
        None,
        None,
        "manual work",
        3,
    );
    unrelated_job.status = agent_runtime::mission::JobStatus::Completed;
    unrelated_job.finished_at = Some(100);
    unrelated_job.updated_at = 100;
    unrelated_job.result = Some(JobResult::Summary {
        outcome: "manual completion".to_string(),
    });
    store
        .put_job(&JobRecord::try_from(&unrelated_job).expect("manual job record"))
        .expect("put manual job");

    let schedule = AgentSchedule::new(AgentScheduleInit {
        id: "after-own".to_string(),
        agent_profile_id: "default".to_string(),
        workspace_root: fs::canonicalize(".").expect("canonical workspace"),
        prompt: "poll completion".to_string(),
        mode: AgentScheduleMode::AfterCompletion,
        delivery_mode: AgentScheduleDeliveryMode::FreshSession,
        target_session_id: None,
        interval_seconds: 30,
        next_fire_at: 200,
        enabled: true,
        last_triggered_at: Some(5),
        last_finished_at: None,
        last_session_id: Some("session-after-completion-old".to_string()),
        last_job_id: Some("job-after-own-old".to_string()),
        last_result: None,
        last_error: None,
        created_at: 1,
        updated_at: 1,
    })
    .expect("build schedule");
    store
        .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
        .expect("put schedule");

    let report = app
        .background_worker_tick(50)
        .expect("run background worker");
    let request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("provider request");
    handle.join().expect("join server");

    assert_eq!(report.executed_jobs, 1);
    let updated = app.agent_schedule("after-own").expect("load schedule");
    assert_eq!(updated.last_triggered_at, Some(50));
    assert_eq!(updated.last_finished_at, Some(50));
    assert_eq!(updated.next_fire_at, 80);
    assert_ne!(updated.last_job_id.as_deref(), Some("job-after-own-old"));

    let normalized_request = request.to_ascii_lowercase();
    assert!(normalized_request.contains("poll completion"));
}

#[test]
fn background_worker_fires_one_shot_existing_session_schedule_once_and_disables_it() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
            "id":"resp_continue_once",
            "model":"gpt-5.4",
            "output":[
                {
                    "id":"msg_continue_once",
                    "type":"message",
                    "status":"completed",
                    "role":"assistant",
                    "content":[{"type":"output_text","text":"continued once"}]
                }
            ],
            "usage":{"input_tokens":13,"output_tokens":3,"total_tokens":16}
        }"#,
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        permissions: PermissionConfig {
            mode: PermissionMode::Plan,
            rules: Vec::new(),
        },
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
            id: "session-continue-once".to_string(),
            title: "Continue Once".to_string(),
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

    let schedule = AgentSchedule::new(AgentScheduleInit {
        id: "continue-once".to_string(),
        agent_profile_id: "default".to_string(),
        workspace_root: fs::canonicalize(".").expect("canonical workspace"),
        prompt: "Продолжи работу позже с handoff payload.".to_string(),
        mode: AgentScheduleMode::Once,
        delivery_mode: AgentScheduleDeliveryMode::ExistingSession,
        target_session_id: Some("session-continue-once".to_string()),
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
        updated_at: 1,
    })
    .expect("build schedule");
    store
        .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
        .expect("put schedule");

    let first = app.background_worker_tick(10).expect("first tick");
    let request = requests.recv().expect("provider request");
    assert_eq!(first.executed_jobs, 1);

    let updated = app
        .agent_schedule("continue-once")
        .expect("updated schedule");
    assert!(!updated.enabled);
    assert_eq!(updated.last_triggered_at, Some(10));
    assert_eq!(
        updated.target_session_id.as_deref(),
        Some("session-continue-once")
    );

    let second = app.background_worker_tick(400).expect("second tick");
    handle.join().expect("join server");

    assert_eq!(second.executed_jobs, 0);
    assert!(request.contains("Продолжи работу позже"));
    let jobs = store.list_jobs().expect("list jobs");
    assert_eq!(jobs.len(), 1);
}

#[test]
fn background_worker_existing_session_rebinds_deleted_target_session() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
            "id":"resp_rebind_schedule",
            "model":"gpt-5.4",
            "output":[
                {
                    "id":"msg_rebind_schedule",
                    "type":"message",
                    "status":"completed",
                    "role":"assistant",
                    "content":[{"type":"output_text","text":"replacement session ok"}]
                }
            ],
            "usage":{"input_tokens":12,"output_tokens":4,"total_tokens":16}
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

    let schedule = AgentSchedule::new(AgentScheduleInit {
        id: "existing-rebind".to_string(),
        agent_profile_id: "default".to_string(),
        workspace_root: fs::canonicalize(".").expect("canonical workspace"),
        prompt: "rebind target session".to_string(),
        mode: AgentScheduleMode::Interval,
        delivery_mode: AgentScheduleDeliveryMode::ExistingSession,
        target_session_id: Some("session-missing-target".to_string()),
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
        updated_at: 1,
    })
    .expect("build schedule");
    store
        .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
        .expect("put schedule");

    let report = app
        .background_worker_tick(10)
        .expect("run background worker");
    let request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("provider request");
    handle.join().expect("join server");

    assert_eq!(report.executed_jobs, 1);
    let updated = app
        .agent_schedule("existing-rebind")
        .expect("load updated schedule");
    let rebound_session_id = updated
        .target_session_id
        .clone()
        .expect("rebound target session id");
    assert_ne!(rebound_session_id, "session-missing-target");
    assert_eq!(
        updated.last_session_id.as_deref(),
        Some(rebound_session_id.as_str())
    );

    let rebound_session = store
        .get_session(&rebound_session_id)
        .expect("get rebound session")
        .expect("rebound session exists");
    assert_eq!(rebound_session.agent_profile_id, "default");

    let normalized_request = request.to_ascii_lowercase();
    assert!(normalized_request.contains("rebind target session"));
}

#[test]
fn background_worker_schedule_recovers_when_permission_policy_denies_tool_call() {
    let first_provider_response = r#"{
        "id":"resp_schedule_exec_denied",
        "model":"gpt-5.4",
        "output":[
            {
                "id":"fc_exec_denied",
                "type":"function_call",
                "status":"completed",
                "call_id":"call_exec_start",
                "name":"exec_start",
                "arguments":"{\"executable\":\"/bin/echo\",\"args\":[\"hi\"]}"
            }
        ],
        "usage":{"input_tokens":19,"output_tokens":7,"total_tokens":26}
    }"#
    .to_string();
    let second_provider_response = r#"{
        "id":"resp_schedule_exec_denied_final",
        "model":"gpt-5.4",
        "output":[
            {
                "id":"msg_1",
                "type":"message",
                "status":"completed",
                "role":"assistant",
                "content":[
                    {
                        "type":"output_text",
                        "text":"Recovered after permission denial"
                    }
                ]
            }
        ],
        "usage":{"input_tokens":31,"output_tokens":4,"total_tokens":35}
    }"#
    .to_string();
    let (api_base, requests, handle) =
        spawn_json_server_sequence(vec![first_provider_response, second_provider_response]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        permissions: PermissionConfig {
            mode: PermissionMode::Plan,
            rules: Vec::new(),
        },
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

    let schedule = AgentSchedule::new(AgentScheduleInit {
        id: "judge-failure".to_string(),
        agent_profile_id: "default".to_string(),
        workspace_root: fs::canonicalize(".").expect("canonical workspace"),
        prompt: "try to run exec".to_string(),
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
        updated_at: 1,
    })
    .expect("build schedule");
    store
        .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
        .expect("put schedule");

    let report = app
        .background_worker_tick(10)
        .expect("run background worker");
    let first_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("first provider request");
    let second_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("second provider request");
    handle.join().expect("join server");

    assert_eq!(report.executed_jobs, 1);
    let jobs = store.list_jobs().expect("list jobs");
    let updated = app.agent_schedule("judge-failure").expect("load schedule");
    assert_eq!(updated.last_result.as_deref(), Some("completed"));
    assert_eq!(updated.last_error, None);
    assert_eq!(updated.last_finished_at, Some(10));

    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, "completed");

    let normalized_first_request = first_request.to_ascii_lowercase();
    assert!(normalized_first_request.contains("try to run exec"));
    let normalized_second_request = second_request.to_ascii_lowercase();
    assert!(normalized_second_request.contains("denied by permission policy"));
    assert!(normalized_second_request.contains("\"call_id\":\"call_exec_start\""));
}

#[test]
fn background_worker_scheduled_fresh_session_launch_auto_approves_exec_tools() {
    let provider_responses = vec![
        r#"{
            "id":"resp_schedule_exec_start",
            "model":"gpt-5.4",
            "output":[
                {
                    "id":"fc_exec_start",
                    "type":"function_call",
                    "status":"completed",
                    "call_id":"call_exec_start",
                    "name":"exec_start",
                    "arguments":"{\"executable\":\"/bin/sh\",\"args\":[\"-c\",\"printf scheduled-ok\"]}"
                }
            ],
            "usage":{"input_tokens":19,"output_tokens":7,"total_tokens":26}
        }"#
        .to_string(),
        r#"{
            "id":"resp_schedule_exec_wait",
            "model":"gpt-5.4",
            "output":[
                {
                    "id":"fc_exec_wait",
                    "type":"function_call",
                    "status":"completed",
                    "call_id":"call_exec_wait",
                    "name":"exec_wait",
                    "arguments":"{\"process_id\":\"exec-1\"}"
                }
            ],
            "usage":{"input_tokens":21,"output_tokens":8,"total_tokens":29}
        }"#
        .to_string(),
        r#"{
            "id":"resp_schedule_exec_final",
            "model":"gpt-5.4",
            "output":[
                {
                    "id":"msg_schedule_exec_final",
                    "type":"message",
                    "status":"completed",
                    "role":"assistant",
                    "content":[{"type":"output_text","text":"scheduled exec complete"}]
                }
            ],
            "usage":{"input_tokens":24,"output_tokens":6,"total_tokens":30}
        }"#
        .to_string(),
    ];
    let (api_base, requests, handle) = spawn_json_server_sequence(provider_responses);
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

    let schedule = AgentSchedule::new(AgentScheduleInit {
        id: "fresh-autoapprove".to_string(),
        agent_profile_id: "default".to_string(),
        workspace_root: fs::canonicalize(".").expect("canonical workspace"),
        prompt: "run the scheduled command".to_string(),
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
        updated_at: 1,
    })
    .expect("build schedule");
    store
        .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
        .expect("put schedule");

    let report = app
        .background_worker_tick(10)
        .expect("run background worker");
    let first_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("first provider request");
    let second_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("second provider request");
    let third_request = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("third provider request");
    handle.join().expect("join server");

    assert_eq!(report.executed_jobs, 1);
    let jobs = store.list_jobs().expect("list jobs");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, "completed");

    let runs = store.list_runs().expect("list runs");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "completed");
    assert!(runs[0].pending_approvals_json.contains("[]"));

    let updated = store
        .get_agent_schedule("fresh-autoapprove")
        .expect("get schedule")
        .map(AgentSchedule::try_from)
        .transpose()
        .expect("parse schedule")
        .expect("schedule exists");
    assert_eq!(updated.last_result.as_deref(), Some("completed"));
    assert_eq!(updated.last_error, None);

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"name\":\"exec_start\""));
    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"type\":\"function_call_output\""));
    let normalized_third = third_request.to_ascii_lowercase();
    assert!(normalized_third.contains("\"type\":\"function_call_output\""));
}

#[test]
fn background_worker_scheduled_existing_session_launch_auto_approves_exec_tools() {
    let provider_responses = vec![
        r#"{
            "id":"resp_existing_exec_start",
            "model":"gpt-5.4",
            "output":[
                {
                    "id":"fc_existing_exec_start",
                    "type":"function_call",
                    "status":"completed",
                    "call_id":"call_exec_start",
                    "name":"exec_start",
                    "arguments":"{\"executable\":\"/bin/sh\",\"args\":[\"-c\",\"printf existing-ok\"]}"
                }
            ],
            "usage":{"input_tokens":19,"output_tokens":7,"total_tokens":26}
        }"#
        .to_string(),
        r#"{
            "id":"resp_existing_exec_wait",
            "model":"gpt-5.4",
            "output":[
                {
                    "id":"fc_existing_exec_wait",
                    "type":"function_call",
                    "status":"completed",
                    "call_id":"call_exec_wait",
                    "name":"exec_wait",
                    "arguments":"{\"process_id\":\"exec-1\"}"
                }
            ],
            "usage":{"input_tokens":21,"output_tokens":8,"total_tokens":29}
        }"#
        .to_string(),
        r#"{
            "id":"resp_existing_exec_final",
            "model":"gpt-5.4",
            "output":[
                {
                    "id":"msg_existing_exec_final",
                    "type":"message",
                    "status":"completed",
                    "role":"assistant",
                    "content":[{"type":"output_text","text":"existing scheduled exec complete"}]
                }
            ],
            "usage":{"input_tokens":24,"output_tokens":6,"total_tokens":30}
        }"#
        .to_string(),
    ];
    let (api_base, requests, handle) = spawn_json_server_sequence(provider_responses);
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

    store
        .put_session(&SessionRecord {
            id: "session-existing-autoapprove".to_string(),
            title: "Existing Autoapprove".to_string(),
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
        .expect("put existing session");

    let schedule = AgentSchedule::new(AgentScheduleInit {
        id: "existing-autoapprove".to_string(),
        agent_profile_id: "default".to_string(),
        workspace_root: fs::canonicalize(".").expect("canonical workspace"),
        prompt: "run the existing scheduled command".to_string(),
        mode: AgentScheduleMode::Interval,
        delivery_mode: AgentScheduleDeliveryMode::ExistingSession,
        target_session_id: Some("session-existing-autoapprove".to_string()),
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
        updated_at: 1,
    })
    .expect("build schedule");
    store
        .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
        .expect("put schedule");

    let report = app
        .background_worker_tick(10)
        .expect("run background worker");
    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("first provider request");
    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("second provider request");
    let _ = requests
        .recv_timeout(Duration::from_secs(2))
        .expect("third provider request");
    handle.join().expect("join server");

    assert_eq!(report.executed_jobs, 1);
    let jobs = store.list_jobs().expect("list jobs");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, "completed");

    let transcript = app
        .session_transcript("session-existing-autoapprove")
        .expect("render transcript");
    assert!(transcript.entries.iter().any(|entry| {
        entry.role == "расписание: existing-autoapprove"
            && entry.content == "run the existing scheduled command"
    }));
    assert!(transcript.entries.iter().any(|entry| {
        entry.role == "assistant" && entry.content == "existing scheduled exec complete"
    }));

    let updated = store
        .get_agent_schedule("existing-autoapprove")
        .expect("get schedule")
        .map(AgentSchedule::try_from)
        .transpose()
        .expect("parse schedule")
        .expect("schedule exists");
    assert_eq!(updated.last_result.as_deref(), Some("completed"));
}

#[test]
fn background_worker_scheduled_provider_retry_becomes_terminal_failure_instead_of_waiting_approval()
{
    let (api_base, requests, handle) = spawn_json_server_status_sequence(vec![
        (
            503,
            r#"{"error":{"message":"temporary upstream issue"}}"#.to_string(),
        ),
        (
            503,
            r#"{"error":{"message":"temporary upstream issue"}}"#.to_string(),
        ),
        (
            503,
            r#"{"error":{"message":"temporary upstream issue"}}"#.to_string(),
        ),
        (
            503,
            r#"{"error":{"message":"temporary upstream issue"}}"#.to_string(),
        ),
    ]);
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

    let schedule = AgentSchedule::new(AgentScheduleInit {
        id: "retry-terminal".to_string(),
        agent_profile_id: "default".to_string(),
        workspace_root: fs::canonicalize(".").expect("canonical workspace"),
        prompt: "trigger provider retry".to_string(),
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
        updated_at: 1,
    })
    .expect("build schedule");
    store
        .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
        .expect("put schedule");

    let report = app
        .background_worker_tick(10)
        .expect("run background worker");
    let requests = (0..4)
        .map(|_| {
            requests
                .recv_timeout(Duration::from_secs(2))
                .expect("provider request")
        })
        .collect::<Vec<_>>();
    handle.join().expect("join server");

    assert_eq!(report.executed_jobs, 1);
    let jobs = store.list_jobs().expect("list jobs");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, "failed");

    let runs = store.list_runs().expect("list runs");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "failed");
    assert!(runs[0].pending_approvals_json.contains("[]"));

    let updated = store
        .get_agent_schedule("retry-terminal")
        .expect("get schedule")
        .map(AgentSchedule::try_from)
        .transpose()
        .expect("parse schedule")
        .expect("schedule exists");
    assert_eq!(updated.last_result.as_deref(), Some("failed"));
    assert!(
        updated
            .last_error
            .as_deref()
            .expect("schedule last error")
            .contains("503")
    );

    assert!(requests.iter().all(|request| {
        request
            .to_ascii_lowercase()
            .contains("trigger provider retry")
    }));
}
