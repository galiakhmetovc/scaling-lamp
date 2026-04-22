use super::support::*;

#[test]
fn tui_like_session_metadata_persists_and_lists_for_ui() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let created = app
        .create_session_auto(Some("Terminal UI"))
        .expect("create session");

    let updated = app
        .update_session_preferences(
            &created.id,
            agentd::bootstrap::SessionPreferencesPatch {
                title: Some("Renamed in TUI".to_string()),
                model: Some(Some("gpt-5.4".to_string())),
                reasoning_visible: Some(false),
                think_level: Some(Some("high".to_string())),
                compactifications: Some(3),
                completion_nudges: Some(Some(2)),
                auto_approve: Some(false),
            },
        )
        .expect("update session preferences");

    assert_eq!(updated.id, created.id);
    assert_eq!(updated.title, "Renamed in TUI");
    assert_eq!(updated.model.as_deref(), Some("gpt-5.4"));
    assert!(!updated.reasoning_visible);
    assert_eq!(updated.think_level.as_deref(), Some("high"));
    assert_eq!(updated.compactifications, 3);
    assert_eq!(updated.completion_nudges, Some(2));

    let sessions = app
        .list_session_summaries()
        .expect("list session summaries");
    let summary = sessions
        .into_iter()
        .find(|summary| summary.id == created.id)
        .expect("session summary");

    assert_eq!(summary.title, "Renamed in TUI");
    assert_eq!(summary.model.as_deref(), Some("gpt-5.4"));
    assert!(!summary.reasoning_visible);
    assert_eq!(summary.think_level.as_deref(), Some("high"));
    assert_eq!(summary.compactifications, 3);
    assert_eq!(summary.completion_nudges, Some(2));
}

#[test]
fn tui_like_session_delete_and_clear_remove_canonical_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("Delete me"))
        .expect("create session");
    let now = 100;
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "transcript-delete-1".to_string(),
            session_id: session.id.clone(),
            run_id: None,
            kind: "user".to_string(),
            content: "hello".to_string(),
            created_at: now,
        })
        .expect("put transcript");

    app.delete_session(&session.id).expect("delete session");

    assert!(
        store
            .get_session(&session.id)
            .expect("get deleted session")
            .is_none()
    );
    assert!(
        store
            .list_transcripts_for_session(&session.id)
            .expect("list deleted transcripts")
            .is_empty()
    );

    let clear_source = app
        .create_session_auto(Some("Clear me"))
        .expect("create clear source");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "transcript-clear-1".to_string(),
            session_id: clear_source.id.clone(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "old state".to_string(),
            created_at: now + 1,
        })
        .expect("put clear transcript");

    let replacement = app
        .clear_session(&clear_source.id, Some("Fresh Session"))
        .expect("clear session");

    assert_ne!(replacement.id, clear_source.id);
    assert_eq!(replacement.title, "Fresh Session");
    assert!(
        store
            .get_session(&clear_source.id)
            .expect("get cleared source")
            .is_none()
    );
    assert!(
        store
            .list_transcripts_for_session(&clear_source.id)
            .expect("list cleared transcripts")
            .is_empty()
    );
    assert!(
        store
            .get_session(&replacement.id)
            .expect("get replacement")
            .is_some()
    );
}

#[test]
fn compact_session_persists_a_context_summary_and_increments_the_counter() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_compact",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_compact",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Condensed earlier context."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":42,"output_tokens":9,"total_tokens":51}
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
    let session = app
        .create_session_auto(Some("Compaction Session"))
        .expect("create session");

    for (index, (kind, content)) in [
        ("user", "covered user one"),
        ("assistant", "covered assistant one"),
        ("user", "recent user one"),
        ("assistant", "recent assistant one"),
        ("user", "recent user two"),
        ("assistant", "recent assistant two"),
        ("user", "recent user three"),
        ("assistant", "recent assistant three"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("compact-transcript-{index}"),
                session_id: session.id.clone(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 10 + index as i64,
            })
            .expect("put transcript");
    }

    let summary = app.compact_session(&session.id).expect("compact session");
    let context_summary = app
        .context_summary(&session.id)
        .expect("load context summary")
        .expect("context summary should exist");
    let raw_request = requests.recv().expect("compaction request");
    handle.join().expect("join server");

    assert_eq!(summary.compactifications, 1);
    assert_eq!(context_summary.session_id, session.id);
    assert_eq!(context_summary.summary_text, "Condensed earlier context.");
    assert_eq!(context_summary.covered_message_count, 2);
    assert!(context_summary.summary_token_estimate > 0);

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/v1/responses"));
    assert!(normalized_request.contains("\"text\":\"covered user one\""));
    assert!(normalized_request.contains("\"text\":\"covered assistant one\""));
    assert!(!normalized_request.contains("\"text\":\"recent user one\""));
}

#[test]
fn compact_session_is_a_noop_when_the_transcript_is_below_threshold() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("Short Session"))
        .expect("create session");

    for (index, (kind, content)) in [
        ("user", "one"),
        ("assistant", "two"),
        ("user", "three"),
        ("assistant", "four"),
        ("user", "five"),
        ("assistant", "six"),
        ("user", "seven"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("short-transcript-{index}"),
                session_id: session.id.clone(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 20 + index as i64,
            })
            .expect("put short transcript");
    }

    let summary = app
        .compact_session(&session.id)
        .expect("compact short session");

    assert_eq!(summary.compactifications, 0);
    assert!(
        app.context_summary(&session.id)
            .expect("load context summary")
            .is_none()
    );
}

#[test]
fn render_context_state_explains_ctx_summary_and_compaction_policy() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    };
    config.session_defaults.working_memory_limit = 96;
    config.context.compaction_min_messages = 12;
    config.context.compaction_keep_tail_messages = 4;
    let app = build_from_config(config).expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("Context Session"))
        .expect("create session");

    for (index, (kind, content)) in [
        ("user", "one"),
        ("assistant", "two"),
        ("user", "three"),
        ("assistant", "four"),
        ("user", "five"),
        ("assistant", "six"),
        ("user", "seven"),
        ("assistant", "eight"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("context-transcript-{index}"),
                session_id: session.id.clone(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 100 + index as i64,
            })
            .expect("put transcript");
    }
    store
        .put_context_summary(&agent_persistence::ContextSummaryRecord {
            session_id: session.id.clone(),
            summary_text: "Earlier context.".to_string(),
            covered_message_count: 2,
            summary_token_estimate: 4,
            updated_at: 200,
        })
        .expect("put context summary");

    let rendered = app
        .render_context_state(&session.id)
        .expect("render context state");

    assert!(rendered.contains("Context:"));
    assert!(rendered.contains("ctx="));
    assert!(rendered.contains("messages_total=8"));
    assert!(rendered.contains("messages_uncovered=6"));
    assert!(rendered.contains("summary_tokens=4"));
    assert!(rendered.contains("compaction_manual=true"));
    assert!(rendered.contains("threshold_messages=12"));
    assert!(rendered.contains("keep_tail=4"));
    assert!(rendered.contains("summary_covers_messages=2"));
}

#[test]
fn session_head_derives_counts_previews_and_summary_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(workspace_root.join("crates")).expect("create crates dir");
    fs::write(workspace_root.join("README.md"), "hello\n").expect("write readme");
    let mut app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    app.runtime.workspace = WorkspaceRef::new(&workspace_root);
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-head".to_string(),
            title: "Session Head".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings {
                compactifications: 2,
                ..SessionSettings::default()
            })
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
    for (index, (kind, content)) in [
        ("user", "first question"),
        ("assistant", "first answer"),
        ("user", "latest question"),
        ("assistant", "recent answer"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("session-head-transcript-{index}"),
                session_id: "session-head".to_string(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 10 + index as i64,
            })
            .expect("put transcript");
    }
    store
        .put_context_summary(&agent_persistence::ContextSummaryRecord {
            session_id: "session-head".to_string(),
            summary_text: "Condensed state.".to_string(),
            covered_message_count: 2,
            summary_token_estimate: 7,
            updated_at: 20,
        })
        .expect("put context summary");

    let mut run = RunEngine::new("run-pending", "session-head", None, 30);
    run.start(30).expect("start run");
    run.wait_for_approval(
        ApprovalRequest::new("approval-1", "tool-1", "approve", 31),
        31,
    )
    .expect("wait approval");
    run.record_tool_completion("fs_read path=.env -> fs_read path=.env bytes=42", 32)
        .expect("record fs read");
    run.record_tool_completion("fs_list path=. recursive=false -> fs_list entries=2", 33)
        .expect("record fs list");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");

    let head = app.session_head("session-head").expect("session head");

    assert_eq!(head.session_id, "session-head");
    assert_eq!(head.title, "Session Head");
    assert_eq!(head.message_count, 4);
    assert_eq!(head.context_tokens, 15);
    assert_eq!(head.compactifications, 2);
    assert_eq!(head.summary_covered_message_count, 2);
    assert_eq!(head.pending_approval_count, 1);
    assert_eq!(head.last_user_preview.as_deref(), Some("latest question"));
    assert_eq!(
        head.last_assistant_preview.as_deref(),
        Some("recent answer")
    );
    assert_eq!(head.recent_filesystem_activity.len(), 2);
    assert_eq!(head.recent_filesystem_activity[0].action, "list");
    assert_eq!(head.recent_filesystem_activity[0].target, ".");
    assert_eq!(head.recent_filesystem_activity[1].action, "read");
    assert_eq!(head.recent_filesystem_activity[1].target, ".env");
    assert_eq!(head.workspace_tree.len(), 2);
    assert_eq!(head.workspace_tree[0].path, "README.md");
    assert_eq!(head.workspace_tree[1].path, "crates");

    let rendered = head.render();
    assert!(rendered.contains("Session: Session Head"));
    assert!(rendered.contains("Summary Covers: 2 messages"));
    assert!(rendered.contains("Pending Approvals: 1"));
    assert!(rendered.contains("Recent Filesystem Activity:"));
    assert!(rendered.contains("- list ."));
    assert!(rendered.contains("- read .env"));
    assert!(rendered.contains("Workspace Tree:"));
    assert!(rendered.contains("- README.md"));
    assert!(rendered.contains("- crates/"));
}

#[test]
fn session_head_prefers_latest_provider_input_tokens_for_ctx() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-usage".to_string(),
            title: "Session Usage".to_string(),
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
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "session-usage-user".to_string(),
            session_id: "session-usage".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "hello".to_string(),
            created_at: 10,
        })
        .expect("put transcript");

    let mut run = RunEngine::new("run-usage", "session-usage", None, 20);
    run.start(20).expect("start run");
    run.set_latest_provider_usage(
        Some(agent_runtime::provider::ProviderUsage {
            input_tokens: 123,
            output_tokens: 7,
            total_tokens: 130,
        }),
        21,
    )
    .expect("set provider usage");
    run.complete("done", 22).expect("complete run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");

    let head = app.session_head("session-usage").expect("session head");

    assert_eq!(head.context_tokens, 123);
}

#[test]
fn create_session_uses_configured_working_memory_limit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    };
    config.session_defaults.working_memory_limit = 96;
    let app = build_from_config(config).expect("build app");

    let summary = app
        .create_session("session-configured-memory", "Configured Memory")
        .expect("create session");
    assert_eq!(summary.id, "session-configured-memory");

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = Session::try_from(
        store
            .get_session("session-configured-memory")
            .expect("get session")
            .expect("session exists"),
    )
    .expect("convert session");

    assert_eq!(session.settings.working_memory_limit, 96);
}

#[test]
fn session_summary_counts_active_background_jobs_and_renders_current_session_jobs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-bg".to_string(),
            title: "Background Session".to_string(),
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
        .put_session(&SessionRecord {
            id: "session-other".to_string(),
            title: "Other Session".to_string(),
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
        .expect("put other session");
    store
        .put_mission(&MissionRecord {
            id: "mission-bg".to_string(),
            session_id: "session-bg".to_string(),
            objective: "Background work".to_string(),
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
    store
        .put_mission(&MissionRecord {
            id: "mission-other".to_string(),
            session_id: "session-other".to_string(),
            objective: "Other work".to_string(),
            status: MissionStatus::Running.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put other mission");
    for job in [
        JobRecord {
            id: "job-queued".to_string(),
            session_id: "session-bg".to_string(),
            mission_id: Some("mission-bg".to_string()),
            run_id: None,
            parent_job_id: None,
            kind: "maintenance".to_string(),
            status: "queued".to_string(),
            input_json: Some(
                serde_json::to_string(&agent_runtime::mission::JobExecutionInput::Maintenance {
                    summary: "queue summary".to_string(),
                })
                .expect("serialize queued input"),
            ),
            result_json: None,
            error: None,
            created_at: 10,
            updated_at: 10,
            started_at: None,
            finished_at: None,
            attempt_count: 0,
            max_attempts: 1,
            lease_owner: None,
            lease_expires_at: None,
            heartbeat_at: None,
            cancel_requested_at: None,
            last_progress_message: Some("queued for execution".to_string()),
            callback_json: None,
            callback_sent_at: None,
        },
        JobRecord {
            id: "job-running".to_string(),
            session_id: "session-bg".to_string(),
            mission_id: Some("mission-bg".to_string()),
            run_id: None,
            parent_job_id: None,
            kind: "delegate".to_string(),
            status: "running".to_string(),
            input_json: Some(
                serde_json::to_string(&agent_runtime::mission::JobExecutionInput::Delegate {
                    label: "worker-a".to_string(),
                    goal: "inspect logs".to_string(),
                    bounded_context: vec!["logs".to_string()],
                    write_scope: DelegateWriteScope {
                        allowed_paths: vec!["logs".to_string()],
                    },
                    expected_output: "summary".to_string(),
                    owner: "local-child".to_string(),
                })
                .expect("serialize running input"),
            ),
            result_json: None,
            error: None,
            created_at: 11,
            updated_at: 11,
            started_at: Some(11),
            finished_at: None,
            attempt_count: 1,
            max_attempts: 3,
            lease_owner: Some("daemon-1".to_string()),
            lease_expires_at: Some(111),
            heartbeat_at: Some(111),
            cancel_requested_at: None,
            last_progress_message: Some("inspecting logs".to_string()),
            callback_json: None,
            callback_sent_at: None,
        },
        JobRecord {
            id: "job-blocked".to_string(),
            session_id: "session-bg".to_string(),
            mission_id: Some("mission-bg".to_string()),
            run_id: None,
            parent_job_id: None,
            kind: "maintenance".to_string(),
            status: "blocked".to_string(),
            input_json: Some(
                serde_json::to_string(&agent_runtime::mission::JobExecutionInput::Maintenance {
                    summary: "blocked summary".to_string(),
                })
                .expect("serialize blocked input"),
            ),
            result_json: None,
            error: None,
            created_at: 12,
            updated_at: 12,
            started_at: None,
            finished_at: None,
            attempt_count: 0,
            max_attempts: 1,
            lease_owner: None,
            lease_expires_at: None,
            heartbeat_at: None,
            cancel_requested_at: None,
            last_progress_message: Some("waiting for approval".to_string()),
            callback_json: None,
            callback_sent_at: None,
        },
        JobRecord {
            id: "job-other".to_string(),
            session_id: "session-other".to_string(),
            mission_id: Some("mission-other".to_string()),
            run_id: None,
            parent_job_id: None,
            kind: "maintenance".to_string(),
            status: "queued".to_string(),
            input_json: Some(
                serde_json::to_string(&agent_runtime::mission::JobExecutionInput::Maintenance {
                    summary: "other summary".to_string(),
                })
                .expect("serialize other input"),
            ),
            result_json: None,
            error: None,
            created_at: 13,
            updated_at: 13,
            started_at: None,
            finished_at: None,
            attempt_count: 0,
            max_attempts: 1,
            lease_owner: None,
            lease_expires_at: None,
            heartbeat_at: None,
            cancel_requested_at: None,
            last_progress_message: Some("other progress".to_string()),
            callback_json: None,
            callback_sent_at: None,
        },
    ] {
        store.put_job(&job).expect("put job");
    }

    let summary = app.session_summary("session-bg").expect("session summary");
    assert_eq!(summary.background_job_count, 3);
    assert_eq!(summary.running_background_job_count, 1);
    assert_eq!(summary.queued_background_job_count, 1);

    let rendered = app
        .render_session_background_jobs("session-bg")
        .expect("render jobs");
    assert!(rendered.contains("Задачи:"));
    assert!(rendered.contains("job-queued"));
    assert!(rendered.contains("job-running"));
    assert!(rendered.contains("job-blocked"));
    assert!(rendered.contains("прогресс: queued for execution"));
    assert!(!rendered.contains("job-other"));
}

#[test]
fn execute_chat_turn_uses_the_context_summary_and_only_the_uncovered_messages() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_compact_for_chat",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_compact_chat",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Compact summary covering earlier context."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":42,"output_tokens":9,"total_tokens":51}
            }"#
        .to_string(),
        r#"{
                "id":"resp_chat_after_compact",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_after_compact",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Answer after compaction."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":44,"output_tokens":6,"total_tokens":50}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(workspace_root.join("src")).expect("create src");
    fs::write(workspace_root.join("README.md"), "workspace readme\n")
        .expect("write workspace readme");
    let mut app = build_from_config(AppConfig {
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
    app.runtime.workspace = WorkspaceRef::new(&workspace_root);
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-compact-chat".to_string(),
            title: "Compacted Chat".to_string(),
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

    for (index, (kind, content)) in [
        ("user", "covered user one"),
        ("assistant", "covered assistant one"),
        ("user", "recent user one"),
        ("assistant", "recent assistant one"),
        ("user", "recent user two"),
        ("assistant", "recent assistant two"),
        ("user", "recent user three"),
        ("assistant", "recent assistant three"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("chat-compact-transcript-{index}"),
                session_id: "session-compact-chat".to_string(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 30 + index as i64,
            })
            .expect("put transcript");
    }
    let mut prior_run = RunEngine::new("run-fs-head", "session-compact-chat", None, 40);
    prior_run.start(40).expect("start prior run");
    prior_run
        .record_tool_completion(
            "fs_patch path=src/main.rs edits=1 -> fs_patch path=src/main.rs edits=1",
            41,
        )
        .expect("record patch");
    store
        .put_run(&RunRecord::try_from(prior_run.snapshot()).expect("prior run record"))
        .expect("put prior run");

    app.compact_session("session-compact-chat")
        .expect("compact session");
    let _first_request = requests.recv().expect("compaction request");

    let report = app
        .execute_chat_turn("session-compact-chat", "latest question", 50)
        .expect("execute compacted chat turn");
    let second_request = requests.recv().expect("chat request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_chat_after_compact");
    assert_eq!(report.output_text, "Answer after compaction.");

    let normalized_request = second_request.to_ascii_lowercase();
    assert!(normalized_request.contains("\"instructions\":\"be concise.\""));
    let session_head_marker = normalized_request
        .find("session: compacted chat")
        .expect("session head marker");
    let summary_marker = normalized_request
        .find("compact summary covering earlier context.")
        .expect("compact summary marker");
    assert!(session_head_marker < summary_marker);
    assert!(normalized_request.contains("summary covers: 2 messages"));
    assert!(normalized_request.contains("recent filesystem activity:"));
    assert!(normalized_request.contains("- patch src/main.rs"));
    assert!(normalized_request.contains("workspace tree:"));
    assert!(normalized_request.contains("- readme.md"));
    assert!(normalized_request.contains("- src/"));
    assert!(normalized_request.contains("compact summary covering earlier context."));
    assert!(normalized_request.contains("\"text\":\"recent user one\""));
    assert!(normalized_request.contains("\"text\":\"recent assistant three\""));
    assert!(normalized_request.contains("\"text\":\"latest question\""));
    assert!(!normalized_request.contains("\"text\":\"covered user one\""));
    assert!(!normalized_request.contains("\"text\":\"covered assistant one\""));
}

#[test]
fn execute_chat_turn_places_system_and_agents_files_before_runtime_blocks() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_prompt_files",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_prompt_files",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Loaded prompt files."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":40,"output_tokens":6,"total_tokens":46}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&workspace_root).expect("create workspace");
    fs::write(
        workspace_root.join("SYSTEM.md"),
        "workspace system prompt should not be loaded\n",
    )
    .expect("write system prompt");
    fs::write(
        workspace_root.join("AGENTS.md"),
        "workspace agents prompt should not be loaded\n",
    )
    .expect("write agents prompt");
    let mut app = build_from_config(AppConfig {
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
    app.runtime.workspace = WorkspaceRef::new(&workspace_root);
    let agent_home = app.agent_home_path("default").expect("default agent home");
    fs::write(
        agent_home.join("SYSTEM.md"),
        "agent system prompt\nalways prefer structured tools.\n",
    )
    .expect("write agent system prompt");
    fs::write(
        agent_home.join("AGENTS.md"),
        "agent instructions:\n- keep edits minimal.\n- explain tradeoffs briefly.\n",
    )
    .expect("write agent agents prompt");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-prompt-files".to_string(),
            title: "Prompt Files".to_string(),
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

    let report = app
        .execute_chat_turn("session-prompt-files", "hello", 10)
        .expect("execute chat turn");
    let first_request = requests.recv().expect("provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_prompt_files");
    assert_eq!(report.output_text, "Loaded prompt files.");

    let normalized = first_request.to_ascii_lowercase();
    let system_prompt_marker = normalized
        .find("agent system prompt")
        .expect("system prompt marker");
    let agents_marker = normalized
        .find("agent instructions:")
        .expect("agents marker");
    let session_marker = normalized
        .find("session: prompt files")
        .expect("session marker");

    assert!(!normalized.contains("workspace system prompt should not be loaded"));
    assert!(!normalized.contains("workspace agents prompt should not be loaded"));
    assert!(system_prompt_marker < agents_marker);
    assert!(agents_marker < session_marker);
}

#[test]
fn execute_chat_turn_places_active_skills_between_agents_and_session_head() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_prompt_skills",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_prompt_skills",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Loaded active skills."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":44,"output_tokens":6,"total_tokens":50}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(&workspace_root).expect("create workspace");
    fs::create_dir_all(skills_dir.join("rust-debug")).expect("create skill dir");
    fs::write(
        workspace_root.join("SYSTEM.md"),
        "workspace system prompt should not be loaded\n",
    )
    .expect("write system prompt");
    fs::write(
        workspace_root.join("AGENTS.md"),
        "workspace agents prompt should not be loaded\n",
    )
    .expect("write agents prompt");
    fs::write(
        skills_dir.join("rust-debug").join("SKILL.md"),
        "---\nname: rust-debug\ndescription: Global rust debug.\n---\n\n# rust-debug\nGlobal skill body should not win.\n",
    )
    .expect("write skill prompt");
    let mut config = AppConfig {
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
    };
    config.daemon.skills_dir = skills_dir;
    let mut app = build_from_config(config).expect("build app");
    app.runtime.workspace = WorkspaceRef::new(&workspace_root);
    let agent_home = app.agent_home_path("default").expect("default agent home");
    fs::write(agent_home.join("SYSTEM.md"), "agent system prompt\n")
        .expect("write agent system prompt");
    fs::write(
        agent_home.join("AGENTS.md"),
        "agent instructions:\n- keep edits minimal.\n",
    )
    .expect("write agent agents prompt");
    fs::create_dir_all(agent_home.join("skills").join("rust-debug")).expect("create local skill");
    fs::write(
        agent_home.join("skills").join("rust-debug").join("SKILL.md"),
        "---\nname: rust-debug\ndescription: Agent-local rust debug.\n---\n\n# rust-debug\nAgent-local skill body wins.\n",
    )
    .expect("write local skill prompt");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-prompt-skills".to_string(),
            title: "Prompt Skills".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings {
                enabled_skills: vec!["rust-debug".to_string()],
                ..SessionSettings::default()
            })
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
        .execute_chat_turn("session-prompt-skills", "hello", 10)
        .expect("execute chat turn");
    let first_request = requests.recv().expect("provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_prompt_skills");
    assert_eq!(report.output_text, "Loaded active skills.");

    let normalized = first_request.to_ascii_lowercase();
    let agents_marker = normalized
        .find("agent instructions:")
        .expect("agents marker");
    let skill_marker = normalized
        .find("agent-local skill body wins.")
        .expect("skill marker");
    let session_marker = normalized
        .find("session: prompt skills")
        .expect("session marker");

    assert!(!normalized.contains("workspace system prompt should not be loaded"));
    assert!(!normalized.contains("workspace agents prompt should not be loaded"));
    assert!(!normalized.contains("global skill body should not win."));
    assert!(agents_marker < skill_marker);
    assert!(skill_marker < session_marker);
}

#[test]
fn execute_chat_turn_falls_back_to_builtin_agent_prompts_when_agent_files_are_missing() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_prompt_fallback",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_prompt_fallback",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Loaded fallback prompt files."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":36,"output_tokens":6,"total_tokens":42}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&workspace_root).expect("create workspace");
    let mut app = build_from_config(AppConfig {
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
    app.runtime.workspace = WorkspaceRef::new(&workspace_root);
    let agent_home = app.agent_home_path("default").expect("default agent home");
    fs::remove_file(agent_home.join("SYSTEM.md")).expect("remove system prompt");
    fs::remove_file(agent_home.join("AGENTS.md")).expect("remove agents prompt");

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-prompt-fallback".to_string(),
            title: "Prompt Fallback".to_string(),
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

    let report = app
        .execute_chat_turn("session-prompt-fallback", "hello", 10)
        .expect("execute chat turn");
    let first_request = requests.recv().expect("provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_prompt_fallback");
    assert_eq!(report.output_text, "Loaded fallback prompt files.");

    let normalized = first_request.to_ascii_lowercase();
    assert!(normalized.contains("assistant autonomous coding agent runtime profile"));
    assert!(normalized.contains("assistant agent profile."));
    assert!(normalized.contains("never invent tool names"));
    assert!(normalized.contains("do not call `fs_read_text` on directories"));
}

#[test]
fn execute_chat_turn_offloads_large_fs_read_text_results_into_artifacts() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_offload_fs_read_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_fs_read_large",
                        "type":"function_call",
                        "call_id":"call_fs_read_large",
                        "name":"fs_read_text",
                        "arguments":"{\"path\":\"docs/large.txt\"}"
                    }
                ],
                "usage":{"input_tokens":40,"output_tokens":12,"total_tokens":52}
            }"#
        .to_string(),
        r#"{
                "id":"resp_offload_fs_read_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_offload_fs_read",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Large file result was offloaded."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":6,"total_tokens":24}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(workspace_root.join("docs")).expect("create docs");
    let large_content = format!(
        "{}\n{}\n{}\n",
        "OFFLOAD-MARKER-LARGE-BLOCK".repeat(120),
        "second-line".repeat(80),
        "third-line".repeat(80)
    );
    fs::write(workspace_root.join("docs/large.txt"), &large_content).expect("write large file");
    let mut app = build_from_config(AppConfig {
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
    app.runtime.workspace = WorkspaceRef::new(&workspace_root);
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-offload-large-read".to_string(),
            title: "Large Read".to_string(),
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

    let report = app
        .execute_chat_turn("session-offload-large-read", "read the big file", 10)
        .expect("execute chat turn");
    let _first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");
    handle.join().expect("join server");

    assert_eq!(report.output_text, "Large file result was offloaded.");

    let snapshot = ContextOffloadSnapshot::try_from(
        store
            .get_context_offload("session-offload-large-read")
            .expect("get context offload")
            .expect("context offload exists"),
    )
    .expect("restore offload snapshot");
    assert_eq!(snapshot.refs.len(), 1);
    assert!(snapshot.refs[0].label.contains("fs_read_text"));
    assert!(snapshot.refs[0].summary.contains("docs/large.txt"));

    let payload = store
        .get_context_offload_payload(&snapshot.refs[0].artifact_id)
        .expect("get payload")
        .expect("payload exists");
    assert!(String::from_utf8_lossy(&payload.bytes).contains("OFFLOAD-MARKER-LARGE-BLOCK"));

    let second_request_body = second_request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("extract second provider request body");
    let second_request_json: serde_json::Value =
        serde_json::from_str(second_request_body).expect("parse second provider request");
    let tool_output = second_request_json["input"][0]["output"]
        .as_str()
        .expect("tool output string");
    let tool_output_json: serde_json::Value =
        serde_json::from_str(tool_output).expect("parse compact tool output");

    assert_eq!(tool_output_json["offloaded"], serde_json::json!(true));
    assert!(tool_output_json.get("artifact_id").is_some());
    assert_eq!(
        tool_output_json["path"],
        serde_json::json!("docs/large.txt")
    );
    assert!(
        !tool_output
            .to_ascii_lowercase()
            .contains(&"offload-marker-large-block".repeat(20))
    );
}

#[test]
fn execute_chat_turn_includes_the_plan_snapshot_before_context_summary() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_plan_chat",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_plan_chat",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Answer with plan context."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":28,"output_tokens":6,"total_tokens":34}
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
            id: "session-plan-chat".to_string(),
            title: "Planned Chat".to_string(),
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
                session_id: "session-plan-chat".to_string(),
                goal: Some("Ship planning tools".to_string()),
                items: vec![
                    PlanItem {
                        id: "inspect".to_string(),
                        content: "Inspect planning seams".to_string(),
                        status: PlanItemStatus::Pending,
                        depends_on: Vec::new(),
                        notes: Vec::new(),
                        blocked_reason: None,
                        parent_task_id: None,
                    },
                    PlanItem {
                        id: "persist".to_string(),
                        content: "Persist canonical plan state".to_string(),
                        status: PlanItemStatus::InProgress,
                        depends_on: vec!["inspect".to_string()],
                        notes: Vec::new(),
                        blocked_reason: None,
                        parent_task_id: None,
                    },
                ],
                updated_at: 3,
            })
            .expect("plan record"),
        )
        .expect("put plan");
    store
        .put_context_summary(&agent_persistence::ContextSummaryRecord {
            session_id: "session-plan-chat".to_string(),
            summary_text: "Compact summary text.".to_string(),
            covered_message_count: 0,
            summary_token_estimate: 4,
            updated_at: 4,
        })
        .expect("put context summary");

    let report = app
        .execute_chat_turn("session-plan-chat", "what next?", 10)
        .expect("execute chat turn");
    let request = requests.recv().expect("provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_plan_chat");
    let normalized = request.to_ascii_lowercase();
    let session_marker = normalized.find("session: planned chat").expect("session");
    let plan_marker = normalized.find("plan:").expect("plan marker");
    let summary_marker = normalized
        .find("compact summary text.")
        .expect("summary marker");
    assert!(session_marker < plan_marker);
    assert!(plan_marker < summary_marker);
    assert!(normalized.contains("[pending] inspect: inspect planning seams"));
    assert!(normalized.contains("[in_progress] persist: persist canonical plan state"));
}

#[test]
fn execute_chat_turn_can_finish_after_plan_write_and_plan_read_tool_calls() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_plan_tools_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_plan_write",
                        "type":"function_call",
                        "call_id":"call_plan_write",
                        "name":"plan_write",
                        "arguments":"{\"items\":[{\"id\":\"inspect\",\"content\":\"Inspect planning seams\",\"status\":\"in_progress\"},{\"id\":\"persist\",\"content\":\"Persist plan snapshot\",\"status\":\"pending\"}]}"
                    }
                ],
                "usage":{"input_tokens":30,"output_tokens":10,"total_tokens":40}
            }"#
        .to_string(),
        r#"{
                "id":"resp_plan_tools_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_plan_read",
                        "type":"function_call",
                        "call_id":"call_plan_read",
                        "name":"plan_read",
                        "arguments":"{}"
                    }
                ],
                "usage":{"input_tokens":20,"output_tokens":8,"total_tokens":28}
            }"#
        .to_string(),
        r#"{
                "id":"resp_plan_tools_3",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_plan_tools",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Plan updated and read back."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":24,"output_tokens":6,"total_tokens":30}
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
            id: "session-plan-tools".to_string(),
            title: "Plan Tools".to_string(),
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

    let report = app
        .execute_chat_turn("session-plan-tools", "make a plan", 10)
        .expect("execute chat turn");
    let _first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");
    let third_request = requests.recv().expect("third provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_plan_tools_3");
    assert_eq!(report.output_text, "Plan updated and read back.");

    let plan = PlanSnapshot::try_from(
        store
            .get_plan("session-plan-tools")
            .expect("get plan")
            .expect("plan exists"),
    )
    .expect("restore plan");
    assert_eq!(plan.items.len(), 2);
    assert_eq!(plan.items[0].id, "inspect");
    assert_eq!(plan.items[0].status, PlanItemStatus::InProgress);
    assert_eq!(plan.items[1].id, "persist");
    assert_eq!(plan.items[1].status, PlanItemStatus::Pending);

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"type\":\"function_call_output\""));
    assert!(normalized_second.contains("\"call_id\":\"call_plan_write\""));
    assert!(normalized_second.contains("plan_write"));
    assert!(normalized_second.contains("inspect planning seams"));

    let normalized_third = third_request.to_ascii_lowercase();
    assert!(normalized_third.contains("\"call_id\":\"call_plan_read\""));
    assert!(normalized_third.contains("plan_read"));
    assert!(normalized_third.contains("inspect planning seams"));
}

#[test]
fn execute_chat_turn_can_finish_after_granular_plan_tool_calls() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_plan_granular_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_init_plan",
                        "type":"function_call",
                        "call_id":"call_init_plan",
                        "name":"init_plan",
                        "arguments":"{\"goal\":\"Refactor auth\"}"
                    }
                ],
                "usage":{"input_tokens":28,"output_tokens":8,"total_tokens":36}
            }"#
        .to_string(),
        r#"{
                "id":"resp_plan_granular_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_add_task",
                        "type":"function_call",
                        "call_id":"call_add_task",
                        "name":"add_task",
                        "arguments":"{\"description\":\"Inspect auth module\",\"depends_on\":[]}"
                    }
                ],
                "usage":{"input_tokens":24,"output_tokens":8,"total_tokens":32}
            }"#
        .to_string(),
        r#"{
                "id":"resp_plan_granular_3",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_set_status",
                        "type":"function_call",
                        "call_id":"call_set_status",
                        "name":"set_task_status",
                        "arguments":"{\"task_id\":\"inspect-auth-module\",\"new_status\":\"in_progress\"}"
                    }
                ],
                "usage":{"input_tokens":22,"output_tokens":8,"total_tokens":30}
            }"#
        .to_string(),
        r#"{
                "id":"resp_plan_granular_4",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_plan_snapshot",
                        "type":"function_call",
                        "call_id":"call_plan_snapshot",
                        "name":"plan_snapshot",
                        "arguments":"{}"
                    }
                ],
                "usage":{"input_tokens":22,"output_tokens":8,"total_tokens":30}
            }"#
        .to_string(),
        r#"{
                "id":"resp_plan_granular_5",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_plan_granular",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Plan initialized, updated, and read back."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":22,"output_tokens":6,"total_tokens":28}
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
            id: "session-plan-granular".to_string(),
            title: "Plan Granular".to_string(),
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

    let report = app
        .execute_chat_turn("session-plan-granular", "make a structured plan", 10)
        .expect("execute chat turn");
    let _first_request = requests.recv().expect("first provider request");
    let _second_request = requests.recv().expect("second provider request");
    let _third_request = requests.recv().expect("third provider request");
    let fourth_request = requests.recv().expect("fourth provider request");
    let fifth_request = requests.recv().expect("fifth provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_plan_granular_5");
    assert_eq!(
        report.output_text,
        "Plan initialized, updated, and read back."
    );

    let plan = PlanSnapshot::try_from(
        store
            .get_plan("session-plan-granular")
            .expect("get plan")
            .expect("plan exists"),
    )
    .expect("restore plan");
    assert_eq!(plan.goal.as_deref(), Some("Refactor auth"));
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.items[0].id, "inspect-auth-module");
    assert_eq!(plan.items[0].status, PlanItemStatus::InProgress);

    let normalized_fourth = fourth_request.to_ascii_lowercase();
    assert!(normalized_fourth.contains("\"call_id\":\"call_set_status\""));
    assert!(normalized_fourth.contains("inspect-auth-module"));

    let normalized_fifth = fifth_request.to_ascii_lowercase();
    assert!(normalized_fifth.contains("\"call_id\":\"call_plan_snapshot\""));
    assert!(normalized_fifth.contains("refactor auth"));
    assert!(normalized_fifth.contains("inspect auth module"));
}

#[test]
fn execute_chat_turn_treats_repeated_init_plan_as_idempotent() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_plan_repeat_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_init_plan_1",
                        "type":"function_call",
                        "call_id":"call_init_plan_1",
                        "name":"init_plan",
                        "arguments":"{\"goal\":\"Install govc\"}"
                    }
                ],
                "usage":{"input_tokens":24,"output_tokens":8,"total_tokens":32}
            }"#
        .to_string(),
        r#"{
                "id":"resp_plan_repeat_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_init_plan_2",
                        "type":"function_call",
                        "call_id":"call_init_plan_2",
                        "name":"init_plan",
                        "arguments":"{\"goal\":\"Install govc\"}"
                    }
                ],
                "usage":{"input_tokens":24,"output_tokens":8,"total_tokens":32}
            }"#
        .to_string(),
        r#"{
                "id":"resp_plan_repeat_3",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_add_task_repeat",
                        "type":"function_call",
                        "call_id":"call_add_task_repeat",
                        "name":"add_task",
                        "arguments":"{\"description\":\"Download govc\",\"depends_on\":[]}"
                    }
                ],
                "usage":{"input_tokens":24,"output_tokens":8,"total_tokens":32}
            }"#
        .to_string(),
        r#"{
                "id":"resp_plan_repeat_4",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_plan_repeat",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Plan reused without crashing."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":20,"output_tokens":6,"total_tokens":26}
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
            id: "session-plan-repeat".to_string(),
            title: "Plan Repeat".to_string(),
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

    let report = app
        .execute_chat_turn("session-plan-repeat", "install govc", 10)
        .expect("execute chat turn");
    let _first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");
    let third_request = requests.recv().expect("third provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_plan_repeat_4");
    assert_eq!(report.output_text, "Plan reused without crashing.");

    let plan = PlanSnapshot::try_from(
        store
            .get_plan("session-plan-repeat")
            .expect("get plan")
            .expect("plan exists"),
    )
    .expect("restore plan");
    assert_eq!(plan.goal.as_deref(), Some("Install govc"));
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.items[0].id, "download-govc");
    assert_eq!(plan.items[0].status, PlanItemStatus::Pending);

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"call_id\":\"call_init_plan_1\""));
    assert!(normalized_second.contains("function_call_output"));

    let normalized_third = third_request.to_ascii_lowercase();
    assert!(normalized_third.contains("\"call_id\":\"call_init_plan_2\""));
    assert!(normalized_third.contains("function_call_output"));
}

#[test]
fn execute_chat_turn_can_retrieve_offloaded_context_via_artifact_read() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_offload_tool_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_artifact_read",
                        "type":"function_call",
                        "call_id":"call_artifact_read",
                        "name":"artifact_read",
                        "arguments":"{\"artifact_id\":\"artifact-offload-1\"}"
                    }
                ],
                "usage":{"input_tokens":40,"output_tokens":12,"total_tokens":52}
            }"#
        .to_string(),
        r#"{
                "id":"resp_offload_tool_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_offload_tool",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Recovered offloaded context."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":24,"output_tokens":6,"total_tokens":30}
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
            id: "session-offload-tools".to_string(),
            title: "Offload Tools".to_string(),
            prompt_override: Some(
                "Use retrieval tools when offloaded context is relevant.".to_string(),
            ),
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
        .put_context_offload(
            &agent_persistence::ContextOffloadRecord::try_from(&ContextOffloadSnapshot {
                session_id: "session-offload-tools".to_string(),
                refs: vec![ContextOffloadRef {
                    id: "offload-1".to_string(),
                    label: "Earlier shell dump".to_string(),
                    summary: "Migration diagnostics from the previous turn".to_string(),
                    artifact_id: "artifact-offload-1".to_string(),
                    token_estimate: 120,
                    message_count: 4,
                    created_at: 2,
                }],
                updated_at: 3,
            })
            .expect("offload record"),
            &[ContextOffloadPayload {
                artifact_id: "artifact-offload-1".to_string(),
                bytes: b"line one\nimportant diagnostic detail\nline three\n".to_vec(),
            }],
        )
        .expect("put context offload");

    let report = app
        .execute_chat_turn(
            "session-offload-tools",
            "Recover the earlier diagnostics",
            10,
        )
        .expect("execute chat turn");
    let first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_offload_tool_2");
    assert_eq!(report.output_text, "Recovered offloaded context.");

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"name\":\"artifact_read\""));
    assert!(normalized_first.contains("offloaded context references"));
    assert!(normalized_first.contains("artifact-offload-1"));
    assert!(normalized_first.contains("earlier shell dump"));
    assert!(!normalized_first.contains("important diagnostic detail"));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"call_id\":\"call_artifact_read\""));
    assert!(normalized_second.contains("artifact_read"));
    assert!(normalized_second.contains("important diagnostic detail"));
}

#[test]
fn provider_request_preview_filters_tools_by_agent_allowlist() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some("http://127.0.0.1:65535/v1".to_string()),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-5.4".to_string()),
            ..ConfiguredProvider::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    for (session_id, title, agent_profile_id) in [
        ("session-preview-default", "Default Preview", "default"),
        ("session-preview-judge", "Judge Preview", "judge"),
    ] {
        store
            .put_session(&SessionRecord {
                id: session_id.to_string(),
                title: title.to_string(),
                prompt_override: None,
                settings_json: serde_json::to_string(&SessionSettings::default())
                    .expect("serialize settings"),
                agent_profile_id: agent_profile_id.to_string(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: None,
                created_at: 1,
                updated_at: 1,
            })
            .expect("put session");
    }

    let default_preview = app
        .render_provider_request_preview("session-preview-default")
        .expect("default provider preview");
    let judge_preview = app
        .render_provider_request_preview("session-preview-judge")
        .expect("judge provider preview");

    assert!(default_preview.contains("\"name\": \"exec_start\""));
    assert!(default_preview.contains("\"name\": \"fs_write_text\""));
    assert!(default_preview.contains("\"name\": \"plan_snapshot\""));

    assert!(judge_preview.contains("\"name\": \"fs_read_text\""));
    assert!(judge_preview.contains("\"name\": \"plan_snapshot\""));
    assert!(!judge_preview.contains("\"name\": \"exec_start\""));
    assert!(!judge_preview.contains("\"name\": \"fs_write_text\""));
}
