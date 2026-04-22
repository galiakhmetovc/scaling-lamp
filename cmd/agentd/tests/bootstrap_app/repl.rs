use super::support::*;

#[test]
fn run_with_args_shows_and_sends_chat_turns() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_chat_cli",
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
                                "text":"Chat CLI reply"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":16,"output_tokens":3,"total_tokens":19}
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
            id: "session-chat-cli".to_string(),
            title: "Chat CLI session".to_string(),
            prompt_override: Some("Keep it short.".to_string()),
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
            session_id: "session-chat-cli".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "First".to_string(),
            created_at: 2,
        })
        .expect("put transcript");

    let shown_before = app
        .run_with_args(["chat", "show", "session-chat-cli"])
        .expect("chat show before");
    assert_eq!(shown_before, "[2] user: First");

    let sent = app
        .run_with_args(["chat", "send", "session-chat-cli", "Second", "message"])
        .expect("chat send");
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert!(sent.contains("chat send session_id=session-chat-cli"));
    assert!(sent.contains("response_id=resp_chat_cli"));
    assert!(sent.contains("output=Chat CLI reply"));

    let shown_after = app
        .run_with_args(["chat", "show", "session-chat-cli"])
        .expect("chat show after");
    assert!(shown_after.contains("[2] user: First"));
    assert!(shown_after.contains("user: Second message"));
    assert!(shown_after.contains("assistant: Chat CLI reply"));

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("\"instructions\":\"keep it short.\""));
    assert!(normalized_request.contains("\"text\":\"first\""));
    assert!(normalized_request.contains("\"text\":\"second message\""));
}

#[test]
fn run_with_args_chat_send_reports_waiting_approval_details() {
    let (web_base, _web_requests, _web_handle) = spawn_text_server("/doc", "cli ask doc");
    let first_provider_response = format!(
        r#"{{
                "id":"resp_chat_cli_approval",
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
    let (api_base, _requests, handle) = spawn_json_server_sequence(vec![first_provider_response]);
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
            id: "session-chat-cli-approval".to_string(),
            title: "Chat CLI approval session".to_string(),
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

    let sent = app
        .run_with_args([
            "chat",
            "send",
            "session-chat-cli-approval",
            "Fetch",
            "the",
            "doc",
        ])
        .expect("chat send should report waiting approval");
    handle.join().expect("join server");

    assert!(sent.contains("status=waiting_approval"));
    assert!(sent.contains("session_id=session-chat-cli-approval"));
    assert!(sent.contains("run_id=run-chat-session-chat-cli-approval-"));
    assert!(sent.contains("approval_id=approval-run-chat-session-chat-cli-approval-"));
}

#[test]
fn repl_runs_chat_turns_and_supports_show_and_exit_commands() {
    let (api_base, _requests, handle) =
        spawn_sse_server_sequence(vec![openai_stream_message_response(
            "resp_chat_repl",
            "REPL reply",
        )]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
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
            id: "session-chat-repl".to_string(),
            title: "Chat REPL session".to_string(),
            prompt_override: Some("Keep it short.".to_string()),
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

    let mut input = Cursor::new(b"Hello from repl\n/show\n/exit\n".to_vec());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("ассистент: REPL reply"));
    assert!(rendered.contains("["));
    assert!(rendered.contains("user: Hello from repl"));
    assert!(rendered.contains("выход из чатового режима"));
}

#[test]
fn repl_command_without_required_argument_prints_usage_instead_of_failing() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-repl-usage".to_string(),
            title: "Chat REPL usage session".to_string(),
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

    let mut input = Cursor::new(b"/autoapprove\n/exit\n".to_vec());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-usage"],
        &mut input,
        &mut output,
    )
    .expect("run repl");

    let rendered = String::from_utf8(output).expect("utf8 output");
    assert!(rendered.contains("не хватает аргументов"));
    assert!(rendered.contains("Формат: \\автоапрув <вкл|выкл>"));
    assert!(rendered.contains("\\автоапрув вкл"));
}

#[test]
fn repl_runs_plan_command_and_renders_current_plan() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-repl-plan".to_string(),
            title: "Chat REPL plan session".to_string(),
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
        .put_plan(
            &PlanRecord::try_from(&PlanSnapshot {
                session_id: "session-chat-repl-plan".to_string(),
                goal: Some("Ship /plan command".to_string()),
                items: vec![PlanItem {
                    id: "show-plan".to_string(),
                    content: "Render the current plan in chat".to_string(),
                    status: PlanItemStatus::InProgress,
                    depends_on: Vec::new(),
                    notes: vec!["Use the canonical plan snapshot".to_string()],
                    blocked_reason: None,
                    parent_task_id: None,
                }],
                updated_at: 10,
            })
            .expect("plan record"),
        )
        .expect("put plan");

    let mut input = Cursor::new(b"/plan\n/exit\n".to_vec());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-plan"],
        &mut input,
        &mut output,
    )
    .expect("repl");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("Цель: Ship /plan command"));
    assert!(rendered.contains("[in_progress] show-plan: Render the current plan in chat"));
    assert!(rendered.contains("заметка: Use the canonical plan snapshot"));
}

#[test]
fn repl_supports_russian_skill_commands_with_session_scoped_overrides() {
    let temp = tempfile::tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(skills_dir.join("rust-debug")).expect("rust skill dir");
    std::fs::create_dir_all(skills_dir.join("postgres")).expect("postgres skill dir");
    std::fs::write(
        skills_dir.join("rust-debug").join("SKILL.md"),
        "---\nname: rust-debug\ndescription: Debug Rust compiler errors and cargo regressions.\n---\n\n# rust-debug\n",
    )
    .expect("write rust skill");
    std::fs::write(
        skills_dir.join("postgres").join("SKILL.md"),
        "---\nname: postgres\ndescription: Investigate PostgreSQL queries and migration issues.\n---\n\n# postgres\n",
    )
    .expect("write postgres skill");

    let mut config = AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    };
    config.daemon.skills_dir = skills_dir;
    let app = build_from_config(config).expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-repl-skills".to_string(),
            title: "Chat REPL skills session".to_string(),
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

    let mut input = Cursor::new(
        "\\скиллы\n\\включить rust-debug\n\\выключить rust-debug\n\\выход\n"
            .as_bytes()
            .to_vec(),
    );
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-skills"],
        &mut input,
        &mut output,
    )
    .expect("repl");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("rust-debug"));
    assert!(rendered.contains("postgres"));
    assert!(rendered.contains("[manual] rust-debug"));
    assert!(rendered.contains("[disabled] rust-debug"));
}

#[test]
fn cli_session_skill_commands_render_and_mutate_session_skill_overrides() {
    let temp = tempfile::tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(skills_dir.join("rust-debug")).expect("rust skill dir");
    std::fs::write(
        skills_dir.join("rust-debug").join("SKILL.md"),
        "---\nname: rust-debug\ndescription: Debug Rust compiler errors and cargo regressions.\n---\n\n# rust-debug\n",
    )
    .expect("write rust skill");

    let mut config = AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    };
    config.daemon.skills_dir = skills_dir;
    let app = build_from_config(config).expect("build app");
    let session = app
        .create_session_auto(Some("CLI Skill Session"))
        .expect("create session");

    let listed = app
        .run_with_args(["session", "skills", session.id.as_str()])
        .expect("list skills");
    assert!(listed.contains("[inactive] rust-debug"));

    let enabled = app
        .run_with_args(["session", "enable-skill", session.id.as_str(), "rust-debug"])
        .expect("enable skill");
    assert!(enabled.contains("[manual] rust-debug"));

    let disabled = app
        .run_with_args([
            "session",
            "disable-skill",
            session.id.as_str(),
            "rust-debug",
        ])
        .expect("disable skill");
    assert!(disabled.contains("[disabled] rust-debug"));
}

#[test]
fn repl_accepts_cp1251_terminal_input_without_utf8_failure() {
    let (api_base, _requests, handle) =
        spawn_sse_server_sequence(vec![openai_stream_message_response(
            "resp_cp1251_repl",
            "cp1251 ok",
        )]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
        permissions: PermissionConfig::default(),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat-repl-cp1251".to_string(),
            title: "Chat REPL cp1251 session".to_string(),
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

    let encoded = encoding_rs::WINDOWS_1251.encode("привет\n/exit\n").0;
    let mut input = Cursor::new(encoded.into_owned());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-cp1251"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("ассистент: cp1251 ok"));
    assert!(!rendered.contains("stream did not contain valid UTF-8"));
}

#[test]
fn repl_surfaces_waiting_approval_and_can_approve_latest_pending_turn() {
    let (web_base, _web_requests, _web_handle) = spawn_text_server("/doc", "repl ask doc");
    let first_provider_response = openai_stream_tool_call_response(
        "resp_repl_waiting",
        "call_web_fetch",
        "web_fetch",
        &format!(r#"{{"url":"{}"}}"#, web_base),
    );
    let second_provider_response =
        openai_stream_message_response("resp_repl_approved", "repl approval completed");
    let (api_base, _requests, handle) =
        spawn_sse_server_sequence(vec![first_provider_response, second_provider_response]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
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
            id: "session-chat-repl-approval".to_string(),
            title: "Chat REPL approval session".to_string(),
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

    let mut input = Cursor::new(b"Fetch the doc\n/approve\n/show\n/exit\n".to_vec());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-approval"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("инструмент: web_fetch | ожидает апрува"));
    assert!(rendered.contains("инструмент: web_fetch | подтверждён"));
    assert!(rendered.contains("инструмент: web_fetch | завершён"));
    assert!(rendered.contains("ассистент: repl approval completed"));
}

#[test]
fn repl_rehydrates_latest_pending_approval_after_restart() {
    let (web_base, _web_requests, _web_handle) = spawn_text_server("/doc", "rehydrated doc");
    let first_provider_response = openai_stream_tool_call_response(
        "resp_repl_restart_waiting",
        "call_web_fetch",
        "web_fetch",
        &format!(r#"{{"url":"{}"}}"#, web_base),
    );
    let second_provider_response = openai_stream_message_response(
        "resp_repl_restart_done",
        "approval after restart completed",
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let state_root = temp.path().join("state-root");
    let (api_base, _requests, handle) = spawn_sse_server_sequence(vec![first_provider_response]);
    let app = build_from_config(AppConfig {
        data_dir: state_root.clone(),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
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
            id: "session-chat-repl-restart".to_string(),
            title: "Chat REPL restart session".to_string(),
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

    let mut first_input = Cursor::new(b"Fetch the doc\n/exit\n".to_vec());
    let mut first_output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-restart"],
        &mut first_input,
        &mut first_output,
    )
    .expect("first repl");
    handle.join().expect("join first server");

    let (api_base, _requests, _handle) = spawn_sse_server_sequence(vec![second_provider_response]);
    let app = build_from_config(AppConfig {
        data_dir: state_root,
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
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
    .expect("rebuild app");

    let mut second_input = Cursor::new(b"/approve\n/exit\n".to_vec());
    let mut second_output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-restart"],
        &mut second_input,
        &mut second_output,
    )
    .expect("second repl");

    let rendered = String::from_utf8(second_output).expect("utf8");
    assert!(rendered.contains("инструмент: web_fetch | подтверждён"));
    assert!(rendered.contains("инструмент: web_fetch | завершён"));
    assert!(rendered.contains("ассистент: approval after restart completed"));
}

#[test]
fn repl_rejects_new_turns_while_an_approval_is_pending() {
    let (web_base, _web_requests, _web_handle) = spawn_text_server("/doc", "pending doc");
    let first_provider_response = openai_stream_tool_call_response(
        "resp_repl_pending_only",
        "call_web_fetch",
        "web_fetch",
        &format!(r#"{{"url":"{}"}}"#, web_base),
    );
    let (api_base, _requests, handle) = spawn_sse_server_sequence(vec![first_provider_response]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
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
            id: "session-chat-repl-pending".to_string(),
            title: "Chat REPL pending session".to_string(),
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

    let mut input = Cursor::new(b"Fetch the doc\nSecond turn should be blocked\n/exit\n".to_vec());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-pending"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("инструмент: web_fetch | ожидает апрува"));
    assert!(
        rendered.contains("сначала завершите ожидающий апрув, потом отправляйте новое сообщение")
    );
    assert!(!rendered.contains("ассистент: Second turn"));
}

#[test]
fn repl_stream_renders_tool_status_instead_of_raw_command_reports() {
    let (web_base, _web_requests, _web_handle) = spawn_text_server("/doc", "streaming tool result");
    let first_stream = format!(
        "data: {{\"id\":\"chatcmpl-stream-tool-1\",\"model\":\"glm-5-turbo\",\"choices\":[{{\"index\":0,\"delta\":{{\"reasoning_content\":\"inspect doc before fetching. \",\"tool_calls\":[{{\"index\":0,\"id\":\"call_web_fetch\",\"type\":\"function\",\"function\":{{\"name\":\"web_fetch\",\"arguments\":\"{{\\\"url\\\":\\\"{}\\\"}}\"}}}}]}},\"finish_reason\":\"tool_calls\"}}]}}\n\n\
data: [DONE]\n\n",
        web_base
    );
    let second_stream =
            "data: {\"id\":\"chatcmpl-stream-tool-2\",\"model\":\"glm-5-turbo\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"streaming \"},\"finish_reason\":null}]}\n\n\
data: {\"id\":\"chatcmpl-stream-tool-2\",\"model\":\"glm-5-turbo\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"tool result\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n"
                .to_string();
    let (api_base, _requests, handle) =
        spawn_sse_server_sequence(vec![first_stream, second_stream]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::ZaiChatCompletions,
            api_base: Some(api_base),
            api_key: Some("zai-key".to_string()),
            default_model: Some("glm-5-turbo".to_string()),
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
            id: "session-chat-repl-stream".to_string(),
            title: "Chat REPL stream session".to_string(),
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

    let mut input = Cursor::new(
        b"Fetch the doc and reply with the exact body only\n/approve\n/exit\n".to_vec(),
    );
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-stream"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("размышления: inspect doc before fetching."));
    assert!(rendered.contains("инструмент: web_fetch | ожидает апрува"));
    assert!(rendered.contains("инструмент: web_fetch | завершён"));
    assert!(rendered.contains("ассистент: streaming tool result"));
    assert!(!rendered.contains("chat send session_id=session-chat-repl-stream"));
    assert!(!rendered.contains("approved approval-"));
}

#[test]
fn zai_repl_stream_can_finish_after_exec_start_and_exec_wait_tool_calls() {
    let first_stream =
        "data: {\"id\":\"chatcmpl-stream-exec-1\",\"model\":\"glm-5-turbo\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"run a quick command first. \",\"tool_calls\":[{\"index\":0,\"id\":\"call_exec_start\",\"type\":\"function\",\"function\":{\"name\":\"exec_start\",\"arguments\":\"{\\\"executable\\\":\\\"/bin/sh\\\",\\\"args\\\":[\\\"-c\\\",\\\"printf exec-ok\\\"],\\\"cwd\\\":null}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: [DONE]\n\n"
            .to_string();
    let second_stream =
        "data: {\"id\":\"chatcmpl-stream-exec-2\",\"model\":\"glm-5-turbo\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"now wait for completion. \",\"tool_calls\":[{\"index\":0,\"id\":\"call_exec_wait\",\"type\":\"function\",\"function\":{\"name\":\"exec_wait\",\"arguments\":\"{\\\"process_id\\\":\\\"exec-1\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: [DONE]\n\n"
            .to_string();
    let third_stream =
        "data: {\"id\":\"chatcmpl-stream-exec-3\",\"model\":\"glm-5-turbo\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"command completed\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n"
            .to_string();
    let (api_base, _requests, handle) =
        spawn_sse_server_sequence(vec![first_stream, second_stream, third_stream]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::ZaiChatCompletions,
            api_base: Some(api_base),
            api_key: Some("zai-key".to_string()),
            default_model: Some("glm-5-turbo".to_string()),
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
            id: "session-chat-repl-exec-stream".to_string(),
            title: "Chat REPL exec stream session".to_string(),
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

    let mut input = Cursor::new(b"run a command\n/exit\n".to_vec());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-exec-stream"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("инструмент: exec_start | завершён"));
    assert!(rendered.contains("инструмент: exec_wait | завершён"));
    assert!(rendered.contains("ассистент: command completed"));
}

#[test]
fn openai_repl_stream_renders_reasoning_summary_and_assistant_text() {
    let stream = "data: {\"type\":\"response.reasoning_summary_text.delta\",\"item_id\":\"rs_123\",\"output_index\":0,\"summary_index\":0,\"delta\":\"Compare the request with prior context. \"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_123\",\"output_index\":1,\"content_index\":0,\"delta\":\"hello \"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_123\",\"output_index\":1,\"content_index\":0,\"delta\":\"from openai\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_stream_repl\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"rs_123\",\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"Compare the request with prior context. \"}]},{\"id\":\"msg_123\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello from openai\",\"annotations\":[]}]}],\"usage\":{\"input_tokens\":11,\"output_tokens\":7,\"total_tokens\":18}}}\n\n".to_string();
    let (api_base, _requests, handle) = spawn_sse_server_sequence(vec![stream]);
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        daemon: Default::default(),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(api_base),
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
            id: "session-chat-repl-openai-stream".to_string(),
            title: "Chat REPL openai stream session".to_string(),
            prompt_override: Some("Be brief.".to_string()),
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

    let mut input = Cursor::new(b"Say hi\n/exit\n".to_vec());
    let mut output = Vec::new();
    app.run_with_io(
        ["chat", "repl", "session-chat-repl-openai-stream"],
        &mut input,
        &mut output,
    )
    .expect("repl");
    handle.join().expect("join server");

    let rendered = String::from_utf8(output).expect("utf8");
    assert!(rendered.contains("размышления: Compare the request with prior context."));
    assert!(rendered.contains("ассистент: hello from openai"));
}
