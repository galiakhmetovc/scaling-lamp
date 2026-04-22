use super::support::*;

#[test]
fn execute_chat_turn_creates_a_run_and_appends_transcript_history() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_chat",
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
                                "text":"Hi back"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":3,"total_tokens":21}
            }"#,
    );
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
            id: "session-chat-turn".to_string(),
            title: "Chat turn session".to_string(),
            prompt_override: Some("Be concise.".to_string()),
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
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-1".to_string(),
            session_id: "session-chat-turn".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "Hello there".to_string(),
            created_at: 2,
        })
        .expect("put prior user transcript");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-2".to_string(),
            session_id: "session-chat-turn".to_string(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "General Kenobi".to_string(),
            created_at: 3,
        })
        .expect("put prior assistant transcript");

    let report = app
        .execute_chat_turn("session-chat-turn", "How are you?", 10)
        .expect("execute chat turn");
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert_eq!(report.run_id, "run-chat-session-chat-turn-10");
    assert_eq!(report.response_id, "resp_chat");
    assert_eq!(report.output_text, "Hi back");

    let run = store
        .get_run("run-chat-session-chat-turn-10")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "completed");
    assert_eq!(run.result.as_deref(), Some("Hi back"));

    let transcript = app
        .session_transcript("session-chat-turn")
        .expect("load transcript");
    assert_eq!(transcript.entries.len(), 4);
    assert_eq!(transcript.entries[2].role, "user");
    assert_eq!(transcript.entries[2].content, "How are you?");
    assert_eq!(transcript.entries[3].role, "assistant");
    assert_eq!(transcript.entries[3].content, "Hi back");

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/v1/responses"));
    assert!(normalized_request.contains("\"instructions\":\"be concise.\""));
    assert!(normalized_request.contains("\"text\":\"hello there\""));
    assert!(normalized_request.contains("\"text\":\"general kenobi\""));
    assert!(normalized_request.contains("\"text\":\"how are you?\""));
}

#[test]
fn execute_chat_turn_can_finish_after_an_allowed_web_tool_call() {
    let (web_base, web_requests, web_handle) = spawn_text_server("/doc", "local doc");
    let web_url = format!("{web_base}/doc");
    let first_provider_response = format!(
        r#"{{
                "id":"resp_tool_call",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}\"}}"
                    }}
                ],
                        "usage":{{"input_tokens":19,"output_tokens":7,"total_tokens":26}}
            }}"#,
        web_url
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        r#"{
                    "id":"resp_tool_final",
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
                                    "text":"Fetched local doc"
                                }
                            ]
                        }
                    ],
                    "usage":{"input_tokens":31,"output_tokens":4,"total_tokens":35}
                }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
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
            id: "session-chat-tool".to_string(),
            title: "Chat tool session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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

    let report = app
        .execute_chat_turn("session-chat-tool", "Fetch the local doc", 10)
        .expect("execute chat turn");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let web_request = web_requests.recv().expect("web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert_eq!(report.run_id, "run-chat-session-chat-tool-10");
    assert_eq!(report.response_id, "resp_tool_final");
    assert_eq!(report.output_text, "Fetched local doc");

    let run = store
        .get_run("run-chat-session-chat-tool-10")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "completed");
    assert_eq!(run.result.as_deref(), Some("Fetched local doc"));

    let transcript = app
        .session_transcript("session-chat-tool")
        .expect("load transcript");
    assert_eq!(
        transcript
            .entries
            .first()
            .map(|entry| entry.content.as_str()),
        Some("Fetch the local doc")
    );
    assert_eq!(
        transcript
            .entries
            .last()
            .map(|entry| entry.content.as_str()),
        Some("Fetched local doc")
    );
    assert!(transcript.entries.iter().any(|entry| {
        entry.role == "tool"
            && entry.tool_name.as_deref() == Some("web_fetch")
            && entry.tool_status.as_deref() == Some("completed")
    }));

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"tools\""));
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));
    assert!(normalized_first.contains("\"text\":\"fetch the local doc\""));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"previous_response_id\":\"resp_tool_call\""));
    assert!(normalized_second.contains("\"type\":\"function_call_output\""));
    assert!(normalized_second.contains("local doc"));
    assert!(!normalized_second.contains("\"text\":\"fetch the local doc\""));

    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get "));
    assert!(normalized_web.contains("/doc"));
    assert!(web_base.contains("127.0.0.1"));
}

#[test]
fn execute_chat_turn_recovers_from_invalid_tool_arguments_and_retries() {
    let first_provider_response = r#"{
                "id":"resp_bad_tool_call",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_bad_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_find_bad",
                        "name":"fs_find_in_files",
                        "arguments":"{\"query\":}"
                    }
                ],
                "usage":{"input_tokens":12,"output_tokens":4,"total_tokens":16}
            }"#
    .to_string();
    let second_provider_response = r#"{
                "id":"resp_good_tool_call",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_good_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_find_good",
                        "name":"fs_find_in_files",
                        "arguments":"{\"query\":\"timeweb\",\"limit\":3}"
                    }
                ],
                "usage":{"input_tokens":20,"output_tokens":6,"total_tokens":26}
            }"#
    .to_string();
    let final_provider_response = r#"{
                "id":"resp_tool_retry_final",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_retry_1",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Проверил workspace и обработал ошибку аргументов."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":30,"output_tokens":8,"total_tokens":38}
            }"#
    .to_string();
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        second_provider_response,
        final_provider_response,
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(workspace_root.join("skills/timeweb")).expect("create workspace dirs");
    fs::write(
        workspace_root.join("skills/timeweb/SKILL.md"),
        "# Timeweb\n\nInstalled skill.\n",
    )
    .expect("write skill file");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
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
            id: "session-invalid-tool".to_string(),
            title: "Tool retry session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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

    let previous_dir = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(&workspace_root).expect("switch to workspace");
    let report = app
        .execute_chat_turn(
            "session-invalid-tool",
            "Прочитай скилл timeweb в workspace",
            10,
        )
        .expect("execute chat turn");
    std::env::set_current_dir(previous_dir).expect("restore current dir");

    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let third_request = provider_requests.recv().expect("third provider request");
    provider_handle.join().expect("join provider server");

    assert_eq!(report.run_id, "run-chat-session-invalid-tool-10");
    assert_eq!(report.response_id, "resp_tool_retry_final");
    assert_eq!(
        report.output_text,
        "Проверил workspace и обработал ошибку аргументов."
    );

    let normalized_second = first_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"name\":\"fs_find_in_files\""));

    let normalized_retry = second_request.to_ascii_lowercase();
    assert!(normalized_retry.contains("\"type\":\"function_call_output\""));
    assert!(normalized_retry.contains("invalid tool call"));
    assert!(normalized_retry.contains("fs_find_in_files"));

    let normalized_third = third_request.to_ascii_lowercase();
    assert!(normalized_third.contains("\"previous_response_id\":\"resp_good_tool_call\""));
    assert!(normalized_third.contains("\"type\":\"function_call_output\""));
}

#[test]
fn execute_chat_turn_requests_operator_reset_when_tool_round_budget_is_exhausted() {
    let first_provider_response = r#"{
                "id":"resp_tool_limit_round_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_limit_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_limit_list",
                        "name":"fs_list",
                        "arguments":"{\"path\":\".\",\"recursive\":false}"
                    }
                ],
                "usage":{"input_tokens":12,"output_tokens":4,"total_tokens":16}
            }"#
    .to_string();
    let resumed_provider_response = r#"{
                "id":"resp_tool_limit_final",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_limit_1",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Продолжил работу после подтверждённого сброса лимита tool rounds."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":7,"total_tokens":25}
            }"#
    .to_string();
    let (provider_api_base, provider_requests, provider_handle) =
        spawn_json_server_sequence(vec![first_provider_response, resumed_provider_response]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            max_tool_rounds: Some(1),
            ..ConfiguredProvider::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-tool-limit".to_string(),
            title: "Chat tool limit session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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

    let error = app
        .execute_chat_turn("session-chat-tool-limit", "Inspect the workspace", 10)
        .expect_err("tool loop should request operator reset");
    let BootstrapError::Execution(ExecutionError::ApprovalRequired {
        approval_id,
        reason,
        ..
    }) = error
    else {
        panic!("expected approval-required error");
    };
    assert!(
        reason.contains("tool-calling limit"),
        "unexpected approval reason: {reason}"
    );

    let pending = app
        .pending_approvals("session-chat-tool-limit")
        .expect("pending approvals");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].approval_id, approval_id);

    let waiting_run = RunSnapshot::try_from(
        store
            .get_run("run-chat-session-chat-tool-limit-10")
            .expect("get run")
            .expect("run exists"),
    )
    .expect("run snapshot");
    assert_eq!(waiting_run.status, RunStatus::WaitingApproval);

    let approved = app
        .approve_run("run-chat-session-chat-tool-limit-10", &approval_id, 20)
        .expect("approve limit reset");
    assert_eq!(approved.run_status, RunStatus::Completed);
    assert_eq!(
        approved.response_id.as_deref(),
        Some("resp_tool_limit_final")
    );
    assert_eq!(
        approved.output_text.as_deref(),
        Some("Продолжил работу после подтверждённого сброса лимита tool rounds.")
    );

    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    provider_handle.join().expect("join provider server");

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"name\":\"fs_list\""));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"previous_response_id\":\"resp_tool_limit_round_1\""));
    assert!(normalized_second.contains("\"type\":\"function_call_output\""));

    let completed_run = RunSnapshot::try_from(
        store
            .get_run("run-chat-session-chat-tool-limit-10")
            .expect("get completed run")
            .expect("completed run exists"),
    )
    .expect("completed run snapshot");
    assert_eq!(completed_run.status, RunStatus::Completed);
    assert!(completed_run.pending_approvals.is_empty());

    let transcript = app
        .session_transcript("session-chat-tool-limit")
        .expect("transcript");
    assert_eq!(
        transcript
            .entries
            .first()
            .map(|entry| entry.content.as_str()),
        Some("Inspect the workspace")
    );
    assert_eq!(
        transcript
            .entries
            .last()
            .map(|entry| entry.content.as_str()),
        Some("Продолжил работу после подтверждённого сброса лимита tool rounds.")
    );
    assert!(transcript.entries.iter().any(|entry| entry.role == "tool"));
}

#[test]
fn execute_chat_turn_auto_nudges_when_model_stops_with_unfinished_plan_work() {
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_completion_nudge_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_list",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_fs_list",
                        "name":"fs_list",
                        "arguments":"{\"path\":\".\",\"recursive\":false}"
                    }
                ],
                "usage":{"input_tokens":12,"output_tokens":5,"total_tokens":17}
            }"#
        .to_string(),
        r#"{
                "id":"resp_completion_nudge_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_stop_early",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Нашёл нужный скилл и пока на этом остановлюсь."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":9,"total_tokens":27}
            }"#
        .to_string(),
        r#"{
                "id":"resp_completion_nudge_3",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_complete_task",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_complete_task",
                        "name":"set_task_status",
                        "arguments":"{\"task_id\":\"download-twcli\",\"new_status\":\"completed\"}"
                    }
                ],
                "usage":{"input_tokens":14,"output_tokens":6,"total_tokens":20}
            }"#
        .to_string(),
        r#"{
                "id":"resp_completion_nudge_4",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_done",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Довёл задачу до конца."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    let settings = SessionSettings {
        completion_nudges: Some(1),
        ..SessionSettings::default()
    };
    store
        .put_session(&SessionRecord {
            id: "session-completion-nudge".to_string(),
            title: "Completion nudge".to_string(),
            prompt_override: Some("Keep working until the task is done.".to_string()),
            settings_json: serde_json::to_string(&settings).expect("serialize settings"),
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
        .put_plan(
            &PlanRecord::try_from(&PlanSnapshot {
                session_id: "session-completion-nudge".to_string(),
                goal: Some("Довести работу над twcli".to_string()),
                items: vec![PlanItem {
                    id: "download-twcli".to_string(),
                    content: "Скачать и положить twcli рядом со скиллом".to_string(),
                    status: PlanItemStatus::Pending,
                    depends_on: Vec::new(),
                    notes: Vec::new(),
                    blocked_reason: None,
                    parent_task_id: None,
                }],
                updated_at: 1,
            })
            .expect("plan record"),
        )
        .expect("put plan");

    let report = app
        .execute_chat_turn("session-completion-nudge", "доведи задачу до конца", 10)
        .expect("chat turn");
    let first_request = provider_requests.recv().expect("first request");
    let second_request = provider_requests.recv().expect("second request");
    let third_request = provider_requests.recv().expect("third request");
    let fourth_request = provider_requests.recv().expect("fourth request");
    provider_handle.join().expect("join provider");

    assert_eq!(report.response_id, "resp_completion_nudge_4");
    assert_eq!(report.output_text, "Довёл задачу до конца.");
    assert!(first_request.contains("\"name\":\"fs_list\""));
    assert!(second_request.contains("\"call_id\":\"call_fs_list\""));
    assert!(third_request.contains("\"previous_response_id\":\"resp_completion_nudge_2\""));
    assert!(third_request.contains("Ты остановился раньше времени."));
    assert!(fourth_request.contains("\"call_id\":\"call_complete_task\""));

    let run = RunSnapshot::try_from(
        store
            .get_run("run-chat-session-completion-nudge-10")
            .expect("get run")
            .expect("run exists"),
    )
    .expect("run snapshot");
    assert_eq!(run.status, RunStatus::Completed);
    assert!(run.pending_approvals.is_empty());
}

#[test]
fn execute_chat_turn_requests_operator_approval_after_completion_nudges_are_exhausted() {
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_completion_approval_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_list",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_fs_list",
                        "name":"fs_list",
                        "arguments":"{\"path\":\".\",\"recursive\":false}"
                    }
                ],
                "usage":{"input_tokens":12,"output_tokens":5,"total_tokens":17}
            }"#
        .to_string(),
        r#"{
                "id":"resp_completion_approval_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_stop_early_1",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Пока остановлюсь на промежуточном результате."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":9,"total_tokens":27}
            }"#
        .to_string(),
        r#"{
                "id":"resp_completion_approval_3",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_stop_early_2",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Я снова остановился слишком рано."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":9,"total_tokens":27}
            }"#
        .to_string(),
        r#"{
                "id":"resp_completion_approval_4",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_complete_task",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_complete_task",
                        "name":"set_task_status",
                        "arguments":"{\"task_id\":\"download-twcli\",\"new_status\":\"completed\"}"
                    }
                ],
                "usage":{"input_tokens":14,"output_tokens":6,"total_tokens":20}
            }"#
        .to_string(),
        r#"{
                "id":"resp_completion_approval_5",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_done",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Завершил работу после подтверждения оператора."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    let settings = SessionSettings {
        completion_nudges: Some(1),
        ..SessionSettings::default()
    };
    store
        .put_session(&SessionRecord {
            id: "session-completion-approval".to_string(),
            title: "Completion approval".to_string(),
            prompt_override: Some("Keep working until the task is done.".to_string()),
            settings_json: serde_json::to_string(&settings).expect("serialize settings"),
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
        .put_plan(
            &PlanRecord::try_from(&PlanSnapshot {
                session_id: "session-completion-approval".to_string(),
                goal: Some("Довести работу над twcli".to_string()),
                items: vec![PlanItem {
                    id: "download-twcli".to_string(),
                    content: "Скачать и положить twcli рядом со скиллом".to_string(),
                    status: PlanItemStatus::Pending,
                    depends_on: Vec::new(),
                    notes: Vec::new(),
                    blocked_reason: None,
                    parent_task_id: None,
                }],
                updated_at: 1,
            })
            .expect("plan record"),
        )
        .expect("put plan");

    let error = app
        .execute_chat_turn("session-completion-approval", "доведи задачу до конца", 10)
        .expect_err("completion gate should require approval");
    let BootstrapError::Execution(ExecutionError::ApprovalRequired {
        approval_id,
        reason,
        ..
    }) = error
    else {
        panic!("expected approval-required error");
    };
    assert!(
        reason.contains("stopped early"),
        "unexpected completion approval reason: {reason}"
    );

    let first_request = provider_requests.recv().expect("first request");
    let second_request = provider_requests.recv().expect("second request");
    let third_request = provider_requests.recv().expect("third request");

    assert!(first_request.contains("\"name\":\"fs_list\""));
    assert!(second_request.contains("\"call_id\":\"call_fs_list\""));
    assert!(third_request.contains("\"previous_response_id\":\"resp_completion_approval_2\""));
    assert!(third_request.contains("Ты остановился раньше времени."));

    let approved = app
        .approve_run("run-chat-session-completion-approval-10", &approval_id, 20)
        .expect("approve completion continuation");
    let fourth_request = provider_requests.recv().expect("fourth request");
    let fifth_request = provider_requests.recv().expect("fifth request");
    provider_handle.join().expect("join provider");

    assert_eq!(approved.run_status, RunStatus::Completed);
    assert_eq!(
        approved.output_text.as_deref(),
        Some("Завершил работу после подтверждения оператора.")
    );
    assert!(fourth_request.contains("\"previous_response_id\":\"resp_completion_approval_3\""));
    assert!(fourth_request.contains("Ты остановился раньше времени."));
    assert!(fifth_request.contains("\"call_id\":\"call_complete_task\""));
}

#[test]
fn execute_chat_turn_allows_more_than_eight_unique_tool_rounds() {
    let mut provider_responses = Vec::new();
    for round in 1..=9 {
        provider_responses.push(format!(
            r#"{{
                "id":"resp_tool_round_{round}",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_{round}",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_glob_{round}",
                        "name":"fs_glob",
                        "arguments":"{{\"path\":\".\",\"pattern\":\"**/*round-{round}*\"}}"
                    }}
                ],
                "usage":{{"input_tokens":12,"output_tokens":4,"total_tokens":16}}
            }}"#
        ));
    }
    provider_responses.push(
        r#"{
                "id":"resp_tool_round_final",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_tool_round_final",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Завершил длинную цепочку tool rounds."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":40,"output_tokens":8,"total_tokens":48}
            }"#
        .to_string(),
    );
    let (provider_api_base, provider_requests, provider_handle) =
        spawn_json_server_sequence(provider_responses);
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&workspace_root).expect("create workspace");
    fs::write(
        workspace_root.join("round-9-target.txt"),
        "marker for the final glob round\n",
    )
    .expect("write workspace marker");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
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
            id: "session-many-rounds".to_string(),
            title: "Many rounds session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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

    let previous_dir = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(&workspace_root).expect("switch to workspace");
    let report = app
        .execute_chat_turn(
            "session-many-rounds",
            "Пройди длинную цепочку поиска по workspace",
            10,
        )
        .expect("execute chat turn");
    std::env::set_current_dir(previous_dir).expect("restore current dir");

    for _ in 0..10 {
        provider_requests.recv().expect("provider request");
    }
    provider_handle.join().expect("join provider server");

    assert_eq!(report.run_id, "run-chat-session-many-rounds-10");
    assert_eq!(report.response_id, "resp_tool_round_final");
    assert_eq!(report.output_text, "Завершил длинную цепочку tool rounds.");

    let run = store
        .get_run("run-chat-session-many-rounds-10")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "completed");
    assert_eq!(
        run.result.as_deref(),
        Some("Завершил длинную цепочку tool rounds.")
    );
}

#[test]
fn execute_chat_turn_pauses_for_transient_provider_failure_and_can_retry_via_approval() {
    let (provider_api_base, provider_requests, provider_handle) =
        spawn_json_server_status_sequence(vec![
            (
                500,
                r#"{"error":{"code":"1234","message":"Internal network failure, error id: transient-1"}}"#
                    .to_string(),
            ),
            (
                200,
                r#"{
                    "id":"resp_retry_ok",
                    "model":"gpt-5.4",
                    "output":[
                        {
                            "id":"msg_retry_ok",
                            "type":"message",
                            "status":"completed",
                            "role":"assistant",
                            "content":[
                                {
                                    "type":"output_text",
                                    "text":"Recovered after provider retry"
                                }
                            ]
                        }
                    ],
                    "usage":{"input_tokens":20,"output_tokens":5,"total_tokens":25}
                }"#
                .to_string(),
            ),
        ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
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
            id: "session-provider-retry".to_string(),
            title: "Provider retry session".to_string(),
            prompt_override: Some("Keep going.".to_string()),
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

    let error = app
        .execute_chat_turn("session-provider-retry", "Say hi", 10)
        .expect_err("chat turn should pause for provider retry approval");
    assert!(error.to_string().contains("approval"));

    let waiting_run = store
        .get_run("run-chat-session-provider-retry-10")
        .expect("get waiting run")
        .expect("waiting run exists");
    assert_eq!(waiting_run.status, "waiting_approval");

    let approvals = app
        .run_with_args(["approval", "list", "run-chat-session-provider-retry-10"])
        .expect("approval list");
    assert!(approvals.contains("retry the provider request"));
    assert!(approvals.contains("500 Internal Server Error"));

    let approval_id = approvals
        .split_whitespace()
        .find(|token| token.starts_with("approval-"))
        .expect("approval id in list")
        .to_string();

    let approved = app
        .run_with_args([
            "approval",
            "approve",
            "run-chat-session-provider-retry-10",
            &approval_id,
        ])
        .expect("approval approve");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    provider_handle.join().expect("join provider server");

    assert!(approved.contains("run-chat-session-provider-retry-10"));
    assert!(approved.contains("Recovered after provider retry"));

    let completed_run = store
        .get_run("run-chat-session-provider-retry-10")
        .expect("get completed run")
        .expect("completed run exists");
    assert_eq!(completed_run.status, "completed");
    assert_eq!(
        completed_run.result.as_deref(),
        Some("Recovered after provider retry")
    );

    let transcript = app
        .session_transcript("session-provider-retry")
        .expect("load transcript");
    assert_eq!(
        transcript
            .entries
            .last()
            .map(|entry| entry.content.as_str()),
        Some("Recovered after provider retry")
    );
    assert!(first_request.to_ascii_lowercase().contains("/v1/responses"));
    assert!(
        second_request
            .to_ascii_lowercase()
            .contains("/v1/responses")
    );
}

#[test]
fn execute_chat_turn_can_finish_after_an_allowed_web_tool_call_with_zai() {
    let (web_base, web_requests, web_handle) = spawn_text_server("/doc", "local doc");
    let first_provider_response = format!(
        r#"{{
                "id":"chatcmpl-tool-zai-1",
                "model":"glm-5.1",
                "choices":[
                    {{
                        "index":0,
                        "finish_reason":"tool_calls",
                        "message":{{
                            "role":"assistant",
                            "content":"",
                            "tool_calls":[
                                {{
                                    "id":"call_web_fetch",
                                    "type":"function",
                                    "function":{{
                                        "name":"web_fetch",
                                        "arguments":"{{\"url\":\"{}\"}}"
                                    }}
                                }}
                            ]
                        }}
                    }}
                ],
                "usage":{{"prompt_tokens":19,"completion_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        r#"{
                    "id":"chatcmpl-tool-zai-2",
                    "model":"glm-5.1",
                    "choices":[
                        {
                            "index":0,
                            "finish_reason":"stop",
                            "message":{
                                "role":"assistant",
                                "content":"Fetched local doc through z.ai"
                            }
                        }
                    ],
                    "usage":{"prompt_tokens":31,"completion_tokens":4,"total_tokens":35}
                }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::ZaiChatCompletions,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("glm-5.1".to_string()),
            ..ConfiguredProvider::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-tool-zai".to_string(),
            title: "Chat tool zai session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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

    let report = app
        .execute_chat_turn("session-chat-tool-zai", "Fetch the local doc", 10)
        .expect("execute chat turn");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let web_request = web_requests.recv().expect("web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert_eq!(report.run_id, "run-chat-session-chat-tool-zai-10");
    assert_eq!(report.response_id, "chatcmpl-tool-zai-2");
    assert_eq!(report.output_text, "Fetched local doc through z.ai");

    let run = store
        .get_run("run-chat-session-chat-tool-zai-10")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "completed");
    assert_eq!(
        run.result.as_deref(),
        Some("Fetched local doc through z.ai")
    );

    let transcript = app
        .session_transcript("session-chat-tool-zai")
        .expect("load transcript");
    assert_eq!(
        transcript
            .entries
            .first()
            .map(|entry| entry.content.as_str()),
        Some("Fetch the local doc")
    );
    assert_eq!(
        transcript
            .entries
            .last()
            .map(|entry| entry.content.as_str()),
        Some("Fetched local doc through z.ai")
    );
    assert!(transcript.entries.iter().any(|entry| entry.role == "tool"));

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("/chat/completions"));
    assert!(normalized_first.contains("\"tool_choice\":\"auto\""));
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));
    assert!(normalized_first.contains("\"content\":\"fetch the local doc\""));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"role\":\"assistant\""));
    assert!(normalized_second.contains("\"tool_calls\""));
    assert!(normalized_second.contains("\"tool_call_id\":\"call_web_fetch\""));
    assert!(normalized_second.contains("local doc"));

    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get "));
    assert!(normalized_web.contains("/doc"));
}

#[test]
fn execute_chat_turn_can_finish_after_exec_start_and_exec_wait_tool_calls() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_exec_tools_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_exec_start",
                        "type":"function_call",
                        "call_id":"call_exec_start",
                        "name":"exec_start",
                        "arguments":"{\"executable\":\"/bin/sh\",\"args\":[\"-c\",\"printf exec-ok\"],\"cwd\":null}"
                    }
                ],
                "usage":{"input_tokens":30,"output_tokens":10,"total_tokens":40}
            }"#
        .to_string(),
        r#"{
                "id":"resp_exec_tools_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_exec_wait",
                        "type":"function_call",
                        "call_id":"call_exec_wait",
                        "name":"exec_wait",
                        "arguments":"{\"process_id\":\"exec-1\"}"
                    }
                ],
                "usage":{"input_tokens":24,"output_tokens":8,"total_tokens":32}
            }"#
        .to_string(),
        r#"{
                "id":"resp_exec_tools_3",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_exec_tools",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Executed command."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":20,"output_tokens":4,"total_tokens":24}
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
        permissions: PermissionConfig {
            mode: PermissionMode::BypassPermissions,
            rules: Vec::new(),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-exec-tools".to_string(),
            title: "Exec Tools".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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

    let report = app
        .execute_chat_turn("session-exec-tools", "run a command", 10)
        .expect("execute chat turn");
    let _first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");
    let third_request = requests.recv().expect("third provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_exec_tools_3");
    assert_eq!(report.output_text, "Executed command.");

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"call_id\":\"call_exec_start\""));
    assert!(normalized_second.contains("\"type\":\"function_call_output\""));
    assert!(normalized_second.contains("process_start"));
    assert!(normalized_second.contains("exec-1"));

    let normalized_third = third_request.to_ascii_lowercase();
    assert!(normalized_third.contains("\"call_id\":\"call_exec_wait\""));
    assert!(normalized_third.contains("exec-ok"));
}

#[test]
fn approval_approve_resumes_an_openai_chat_tool_call_and_completes_the_run() {
    let (web_base, web_requests, web_handle) = spawn_text_server("/doc", "approved doc");
    let first_provider_response = format!(
        r#"{{
                "id":"resp_tool_approval_call",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}\"}}"
                    }}
                ],
                "usage":{{"input_tokens":19,"output_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        r#"{
                    "id":"resp_tool_approval_final",
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
                                    "text":"Fetched approved doc after approval"
                                }
                            ]
                        }
                    ],
                    "usage":{"input_tokens":31,"output_tokens":4,"total_tokens":35}
                }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Ask,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-approval".to_string(),
            title: "Chat approval session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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

    let error = app
        .execute_chat_turn("session-chat-approval", "Fetch the approved doc", 10)
        .expect_err("chat turn should pause for approval");
    assert!(error.to_string().contains("approval"));

    let waiting_run = store
        .get_run("run-chat-session-chat-approval-10")
        .expect("get waiting run")
        .expect("waiting run exists");
    assert_eq!(waiting_run.status, "waiting_approval");

    let approvals = app
        .run_with_args(["approval", "list", "run-chat-session-chat-approval-10"])
        .expect("approval list");
    assert!(approvals.contains("approval-run-chat-session-chat-approval-10-web_fetch"));
    assert!(approvals.contains("call_web_fetch"));

    let approved = app
        .run_with_args([
            "approval",
            "approve",
            "run-chat-session-chat-approval-10",
            "approval-run-chat-session-chat-approval-10-web_fetch",
        ])
        .expect("approval approve");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let web_request = web_requests.recv().expect("web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert!(approved.contains("run-chat-session-chat-approval-10"));
    assert!(approved.contains("resp_tool_approval_final"));
    assert!(approved.contains("Fetched approved doc after approval"));

    let completed_run = store
        .get_run("run-chat-session-chat-approval-10")
        .expect("get completed run")
        .expect("completed run exists");
    assert_eq!(completed_run.status, "completed");
    assert_eq!(
        completed_run.result.as_deref(),
        Some("Fetched approved doc after approval")
    );

    let transcript = app
        .session_transcript("session-chat-approval")
        .expect("load transcript");
    assert_eq!(
        transcript
            .entries
            .first()
            .map(|entry| entry.content.as_str()),
        Some("Fetch the approved doc")
    );
    assert_eq!(
        transcript
            .entries
            .last()
            .map(|entry| entry.content.as_str()),
        Some("Fetched approved doc after approval")
    );
    assert!(transcript.entries.iter().any(|entry| entry.role == "tool"));

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));
    assert!(normalized_first.contains("\"text\":\"fetch the approved doc\""));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"previous_response_id\":\"resp_tool_approval_call\""));
    assert!(normalized_second.contains("\"type\":\"function_call_output\""));
    assert!(normalized_second.contains("approved doc"));

    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get "));
    assert!(normalized_web.contains("/doc"));
}

#[test]
fn approval_approve_resumes_a_zai_chat_tool_call_and_completes_the_run() {
    let (web_base, web_requests, web_handle) = spawn_text_server("/doc", "approved zai doc");
    let first_provider_response = format!(
        r#"{{
                "id":"chatcmpl-approval-zai-1",
                "model":"glm-5.1",
                "choices":[
                    {{
                        "index":0,
                        "finish_reason":"tool_calls",
                        "message":{{
                            "role":"assistant",
                            "content":"",
                            "tool_calls":[
                                {{
                                    "id":"call_web_fetch",
                                    "type":"function",
                                    "function":{{
                                        "name":"web_fetch",
                                        "arguments":"{{\"url\":\"{}\"}}"
                                    }}
                                }}
                            ]
                        }}
                    }}
                ],
                "usage":{{"prompt_tokens":19,"completion_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        r#"{
                    "id":"chatcmpl-approval-zai-2",
                    "model":"glm-5.1",
                    "choices":[
                        {
                            "index":0,
                            "finish_reason":"stop",
                            "message":{
                                "role":"assistant",
                                "content":"Fetched approved zai doc after approval"
                            }
                        }
                    ],
                    "usage":{"prompt_tokens":31,"completion_tokens":4,"total_tokens":35}
                }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::ZaiChatCompletions,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("glm-5.1".to_string()),
            ..ConfiguredProvider::default()
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Ask,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-approval-zai".to_string(),
            title: "Chat approval zai session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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

    let error = app
        .execute_chat_turn(
            "session-chat-approval-zai",
            "Fetch the approved zai doc",
            10,
        )
        .expect_err("chat turn should pause for approval");
    assert!(error.to_string().contains("approval"));

    let approvals = app
        .run_with_args(["approval", "list", "run-chat-session-chat-approval-zai-10"])
        .expect("approval list");
    assert!(approvals.contains("approval-run-chat-session-chat-approval-zai-10-web_fetch"));
    assert!(approvals.contains("call_web_fetch"));

    let approved = app
        .run_with_args([
            "approval",
            "approve",
            "run-chat-session-chat-approval-zai-10",
            "approval-run-chat-session-chat-approval-zai-10-web_fetch",
        ])
        .expect("approval approve");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let web_request = web_requests.recv().expect("web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert!(approved.contains("run-chat-session-chat-approval-zai-10"));
    assert!(approved.contains("chatcmpl-approval-zai-2"));
    assert!(approved.contains("Fetched approved zai doc after approval"));

    let completed_run = store
        .get_run("run-chat-session-chat-approval-zai-10")
        .expect("get completed run")
        .expect("completed run exists");
    assert_eq!(completed_run.status, "completed");
    assert_eq!(
        completed_run.result.as_deref(),
        Some("Fetched approved zai doc after approval")
    );

    let transcript = app
        .session_transcript("session-chat-approval-zai")
        .expect("load transcript");
    assert_eq!(
        transcript
            .entries
            .first()
            .map(|entry| entry.content.as_str()),
        Some("Fetch the approved zai doc")
    );
    assert_eq!(
        transcript
            .entries
            .last()
            .map(|entry| entry.content.as_str()),
        Some("Fetched approved zai doc after approval")
    );
    assert!(transcript.entries.iter().any(|entry| entry.role == "tool"));

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"tool_choice\":\"auto\""));
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));
    assert!(normalized_first.contains("\"content\":\"fetch the approved zai doc\""));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"role\":\"assistant\""));
    assert!(normalized_second.contains("\"tool_calls\""));
    assert!(normalized_second.contains("\"tool_call_id\":\"call_web_fetch\""));
    assert!(normalized_second.contains("approved zai doc"));

    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get "));
    assert!(normalized_web.contains("/doc"));
}

#[test]
fn approval_approve_resumes_a_mission_turn_and_completes_the_job() {
    let (web_base, web_requests, web_handle) = spawn_text_server("/doc", "mission approved doc");
    let first_provider_response = format!(
        r#"{{
                "id":"resp_mission_approval_call",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}\"}}"
                    }}
                ],
                "usage":{{"input_tokens":19,"output_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        r#"{
                    "id":"resp_mission_approval_final",
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
                                    "text":"Mission fetched approved doc"
                                }
                            ]
                        }
                    ],
                    "usage":{"input_tokens":31,"output_tokens":4,"total_tokens":35}
                }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Ask,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-mission-approval".to_string(),
            title: "Mission approval session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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
        .put_mission(&MissionRecord {
            id: "mission-approval".to_string(),
            session_id: "session-mission-approval".to_string(),
            objective: "Fetch an approved doc".to_string(),
            status: MissionStatus::Ready.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put mission");
    store
        .put_job(
            &JobRecord::try_from(&JobSpec::mission_turn(
                "job-mission-approval",
                "session-mission-approval",
                "mission-approval",
                None,
                None,
                "Fetch the approved mission doc",
                3,
            ))
            .expect("job record"),
        )
        .expect("put job");

    let error = app
        .execute_mission_turn_job("job-mission-approval", 10)
        .expect_err("mission turn should pause for approval");
    assert!(error.to_string().contains("approval"));

    let approval_output = app
        .run_with_args(["approval", "list", "run-job-mission-approval"])
        .expect("approval list");
    assert!(approval_output.contains("approval-run-job-mission-approval-web_fetch"));

    let approved = app
        .run_with_args([
            "approval",
            "approve",
            "run-job-mission-approval",
            "approval-run-job-mission-approval-web_fetch",
        ])
        .expect("approval approve");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let web_request = web_requests.recv().expect("web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert!(approved.contains("status=completed"));
    assert!(approved.contains("resp_mission_approval_final"));
    assert!(approved.contains("Mission fetched approved doc"));

    let completed_run = store
        .get_run("run-job-mission-approval")
        .expect("get completed run")
        .expect("completed run exists");
    assert_eq!(completed_run.status, "completed");
    assert_eq!(
        completed_run.result.as_deref(),
        Some("Mission fetched approved doc")
    );

    let completed_job = store
        .get_job("job-mission-approval")
        .expect("get completed job")
        .expect("completed job exists");
    assert_eq!(completed_job.status, "completed");
    assert!(
        completed_job
            .result_json
            .as_deref()
            .unwrap_or_default()
            .contains("Mission fetched approved doc")
    );

    let transcript = app
        .session_transcript("session-mission-approval")
        .expect("load transcript");
    assert_eq!(
        transcript
            .entries
            .first()
            .map(|entry| entry.content.as_str()),
        Some("Fetch the approved mission doc")
    );
    assert_eq!(
        transcript
            .entries
            .last()
            .map(|entry| entry.content.as_str()),
        Some("Mission fetched approved doc")
    );
    assert!(transcript.entries.iter().any(|entry| entry.role == "tool"));

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));
    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"previous_response_id\":\"resp_mission_approval_call\""));
    assert!(normalized_second.contains("mission approved doc"));
    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get "));
    assert!(normalized_web.contains("/doc"));
}

#[test]
fn execute_chat_turn_fails_when_the_provider_repeats_the_same_tool_signature() {
    let (web_base, web_requests, web_handle) =
        spawn_text_server_sequence(vec!["loop doc", "loop doc"]);
    let repeated_tool_response = format!(
        r#"{{
                "id":"resp_tool_loop",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_loop",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}\"}}"
                    }}
                ],
                "usage":{{"input_tokens":19,"output_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        repeated_tool_response.clone(),
        repeated_tool_response.clone(),
        repeated_tool_response,
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
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
            id: "session-chat-loop".to_string(),
            title: "Chat loop session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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

    let error = app
        .execute_chat_turn("session-chat-loop", "Fetch the local doc", 10)
        .expect_err("repeated tool signature must fail");
    let first_request = provider_requests
        .recv_timeout(Duration::from_secs(1))
        .expect("first provider request");
    let second_request = provider_requests
        .recv_timeout(Duration::from_secs(1))
        .expect("second provider request");
    let third_request = provider_requests
        .recv_timeout(Duration::from_secs(1))
        .expect("third provider request");
    let web_request = web_requests
        .recv_timeout(Duration::from_secs(1))
        .expect("web request");
    let second_web_request = web_requests
        .recv_timeout(Duration::from_secs(1))
        .expect("second web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert!(
        error
            .to_string()
            .contains("provider repeated tool-call signature 3 times in a row")
    );

    let run = store
        .get_run("run-chat-session-chat-loop-10")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "failed");
    assert!(
        run.error
            .as_deref()
            .unwrap_or_default()
            .contains("provider repeated tool-call signature 3 times in a row")
    );

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"name\":\"web_fetch\""));
    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"previous_response_id\":\"resp_tool_loop\""));
    let normalized_third = third_request.to_ascii_lowercase();
    assert!(normalized_third.contains("\"previous_response_id\":\"resp_tool_loop\""));
    let normalized_web = web_request.to_ascii_lowercase();
    assert!(normalized_web.contains("get "));
    let normalized_second_web = second_web_request.to_ascii_lowercase();
    assert!(normalized_second_web.contains("get "));
}

#[test]
fn execute_chat_turn_only_sends_new_tool_outputs_for_each_continuation_round() {
    let (web_base, web_requests, web_handle) =
        spawn_text_server_sequence(vec!["doc one", "doc two"]);
    let first_provider_response = format!(
        r#"{{
                "id":"resp_tool_chain_1",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_chain_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch_1",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}/doc-1\"}}"
                    }}
                ],
                "usage":{{"input_tokens":19,"output_tokens":7,"total_tokens":26}}
            }}"#,
        web_base
    );
    let second_provider_response = format!(
        r#"{{
                "id":"resp_tool_chain_2",
                "model":"gpt-5.4",
                "output":[
                    {{
                        "id":"fc_chain_2",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch_2",
                        "name":"web_fetch",
                        "arguments":"{{\"url\":\"{}/doc-2\"}}"
                    }}
                ],
                "usage":{{"input_tokens":27,"output_tokens":8,"total_tokens":35}}
            }}"#,
        web_base
    );
    let (provider_api_base, provider_requests, provider_handle) = spawn_json_server_sequence(vec![
        first_provider_response,
        second_provider_response,
        r#"{
                    "id":"resp_tool_chain_3",
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
                                    "text":"two step tool chain ok"
                                }
                            ]
                        }
                    ],
                    "usage":{"input_tokens":39,"output_tokens":5,"total_tokens":44}
                }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
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
            id: "session-chat-chain".to_string(),
            title: "Chat chain session".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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

    let report = app
        .execute_chat_turn("session-chat-chain", "Fetch two docs", 10)
        .expect("execute chat turn");
    let _first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    let third_request = provider_requests.recv().expect("third provider request");
    let first_web_request = web_requests.recv().expect("first web request");
    let second_web_request = web_requests.recv().expect("second web request");
    provider_handle.join().expect("join provider server");
    web_handle.join().expect("join web server");

    assert_eq!(report.output_text, "two step tool chain ok");

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"previous_response_id\":\"resp_tool_chain_1\""));
    assert!(normalized_second.contains("doc one"));

    let normalized_third = third_request.to_ascii_lowercase();
    assert!(normalized_third.contains("\"previous_response_id\":\"resp_tool_chain_2\""));
    assert!(normalized_third.contains("doc two"));
    assert!(!normalized_third.contains("doc one"));

    let normalized_first_web = first_web_request.to_ascii_lowercase();
    assert!(normalized_first_web.contains("get /doc-1 http/1.1"));
    let normalized_second_web = second_web_request.to_ascii_lowercase();
    assert!(normalized_second_web.contains("get /doc-2 http/1.1"));
}

#[test]
fn judge_session_turn_recovers_when_provider_calls_a_forbidden_tool() {
    let first_provider_response = r#"{
                "id":"resp_judge_tool_call",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_exec_start",
                        "name":"exec_start",
                        "arguments":"{\"executable\":\"echo\",\"args\":[\"hi\"]}"
                    }
                ],
                "usage":{"input_tokens":19,"output_tokens":7,"total_tokens":26}
            }"#
    .to_string();
    let second_provider_response = r#"{
                "id":"resp_judge_final",
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
                                "text":"Judge stayed read-only"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":31,"output_tokens":4,"total_tokens":35}
            }"#
    .to_string();
    let (provider_api_base, provider_requests, provider_handle) =
        spawn_json_server_sequence(vec![first_provider_response, second_provider_response]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
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
            id: "session-judge-chat".to_string(),
            title: "Judge chat".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            agent_profile_id: "judge".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let report = app
        .execute_chat_turn("session-judge-chat", "Проверь, что ты read-only", 10)
        .expect("execute chat turn");
    let first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    provider_handle.join().expect("join provider server");

    assert_eq!(report.response_id, "resp_judge_final");
    assert_eq!(report.output_text, "Judge stayed read-only");

    let run = store
        .get_run("run-chat-session-judge-chat-10")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "completed");
    assert_eq!(run.result.as_deref(), Some("Judge stayed read-only"));

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"tools\""));
    assert!(!normalized_first.contains("\"name\":\"exec_start\""));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("exec_start"));
    assert!(normalized_second.contains("not allowed by agent profile judge"));
    assert!(normalized_second.contains("agent_allowed_tools"));
}

#[test]
fn execute_chat_turn_recovers_when_tool_call_returns_web_fetch_error() {
    let first_provider_response = r#"{
                "id":"resp_invalid_web_tool_call",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch",
                        "name":"web_fetch",
                        "arguments":"{\"url\":\"not-a-url\"}"
                    }
                ],
                "usage":{"input_tokens":19,"output_tokens":7,"total_tokens":26}
            }"#
    .to_string();
    let second_provider_response = r#"{
                "id":"resp_invalid_web_final",
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
                                "text":"Recovered after invalid web request"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":31,"output_tokens":4,"total_tokens":35}
            }"#
    .to_string();
    let (provider_api_base, provider_requests, provider_handle) =
        spawn_json_server_sequence(vec![first_provider_response, second_provider_response]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
        permissions: PermissionConfig {
            mode: PermissionMode::BypassPermissions,
            rules: Vec::new(),
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-invalid-web-tool".to_string(),
            title: "Invalid web tool".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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

    let report = app
        .execute_chat_turn("session-invalid-web-tool", "Fetch an invalid url", 10)
        .expect("execute chat turn");
    let _first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    provider_handle.join().expect("join provider server");

    assert_eq!(report.response_id, "resp_invalid_web_final");
    assert_eq!(report.output_text, "Recovered after invalid web request");

    let run = store
        .get_run("run-chat-session-invalid-web-tool-10")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "completed");
    assert_eq!(
        run.result.as_deref(),
        Some("Recovered after invalid web request")
    );

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(
        normalized_second.contains("\"call_id\":\"call_web_fetch\""),
        "{normalized_second}"
    );
    assert!(
        normalized_second.contains("\"type\":\"function_call_output\""),
        "{normalized_second}"
    );
    assert!(
        normalized_second.contains("web http error"),
        "{normalized_second}"
    );
    assert!(
        normalized_second.contains("not-a-url"),
        "{normalized_second}"
    );
}

#[test]
fn execute_chat_turn_recovers_when_permission_policy_denies_tool_call() {
    let first_provider_response = r#"{
                "id":"resp_permission_denied_tool_call",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_1",
                        "type":"function_call",
                        "status":"completed",
                        "call_id":"call_web_fetch",
                        "name":"web_fetch",
                        "arguments":"{\"url\":\"https://example.com/restricted\"}"
                    }
                ],
                "usage":{"input_tokens":19,"output_tokens":7,"total_tokens":26}
            }"#
    .to_string();
    let second_provider_response = r#"{
                "id":"resp_permission_denied_final",
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
    let (provider_api_base, provider_requests, provider_handle) =
        spawn_json_server_sequence(vec![first_provider_response, second_provider_response]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(format!("{provider_api_base}/v1")),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
        permissions: PermissionConfig {
            mode: PermissionMode::Auto,
            rules: vec![PermissionRule {
                action: PermissionAction::Deny,
                tool: Some("web_fetch".to_string()),
                family: None,
                path_prefix: None,
            }],
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-permission-denied-tool".to_string(),
            title: "Permission denied tool".to_string(),
            prompt_override: Some("Use tools when useful.".to_string()),
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

    let report = app
        .execute_chat_turn(
            "session-permission-denied-tool",
            "Fetch a restricted url",
            10,
        )
        .expect("execute chat turn");
    let _first_request = provider_requests.recv().expect("first provider request");
    let second_request = provider_requests.recv().expect("second provider request");
    provider_handle.join().expect("join provider server");

    assert_eq!(report.response_id, "resp_permission_denied_final");
    assert_eq!(report.output_text, "Recovered after permission denial");

    let run = store
        .get_run("run-chat-session-permission-denied-tool-10")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "completed");
    assert_eq!(
        run.result.as_deref(),
        Some("Recovered after permission denial")
    );

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"call_id\":\"call_web_fetch\""));
    assert!(normalized_second.contains("\"type\":\"function_call_output\""));
    assert!(normalized_second.contains("denied by permission policy"));
    assert!(normalized_second.contains("web_fetch"));
}
