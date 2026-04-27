use super::support::*;
use agentd::mcp::{
    McpDiscoveredPrompt, McpDiscoveredPromptArgument, McpDiscoveredResource, McpDiscoveredTool,
    MockMcpConnectorRuntime, SharedMcpRegistry,
};

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
    assert!(rendered.contains("compaction_manual_available=true"));
    assert!(rendered.contains("auto_trigger_ratio=0.70"));
    assert!(rendered.contains("context_window_override=<resolver>"));
    assert!(rendered.contains("threshold_messages=12"));
    assert!(rendered.contains("keep_tail=4"));
    assert!(rendered.contains("summary_covers_messages=2"));
}

#[test]
fn render_session_artifacts_includes_offload_totals_and_open_hint() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("Artifacts Session"))
        .expect("create session");

    store
        .put_context_offload(
            &agent_persistence::ContextOffloadRecord::try_from(&ContextOffloadSnapshot {
                session_id: session.id.clone(),
                refs: vec![ContextOffloadRef {
                    id: "offload-artifacts-1".to_string(),
                    label: "Tool trace".to_string(),
                    summary: "Large tool output".to_string(),
                    artifact_id: "artifact-offload-1".to_string(),
                    token_estimate: 120,
                    message_count: 3,
                    created_at: 77,
                    pinned: false,
                    explicit_read_count: 0,
                }],
                updated_at: 101,
            })
            .expect("offload record"),
            &[ContextOffloadPayload {
                artifact_id: "artifact-offload-1".to_string(),
                bytes: b"payload".to_vec(),
            }],
        )
        .expect("put offload");

    let rendered = app
        .render_session_artifacts(&session.id)
        .expect("render session artifacts");

    assert!(rendered.contains("Артефакты:"));
    assert!(rendered.contains("refs=1"));
    assert!(rendered.contains("tokens=120"));
    assert!(rendered.contains("messages=3"));
    assert!(rendered.contains("updated_at=101"));
    assert!(rendered.contains("\\артефакт artifact-offload-1"));
    assert!(rendered.contains("- artifact-offload-1 [offload-artifacts-1] Tool trace"));
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
    run.record_tool_completion(
        "exec_start cwd=. command=/bin/sh -c 'echo ready' -> exec_start process_id=exec-1 pid_ref=pid:101 cwd=. command=/bin/sh -c 'echo ready'",
        34,
    )
    .expect("record exec start");
    run.record_tool_completion(
        "exec_wait process_id=exec-1 -> process_result process_id=exec-1 status=Exited exit_code=Some(0)",
        35,
    )
    .expect("record exec wait");
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
    assert_eq!(head.recent_process_activity.len(), 2);
    assert_eq!(head.recent_process_activity[0].action, "finish");
    assert_eq!(
        head.recent_process_activity[0].target,
        "exec-1 status=Exited exit=0"
    );
    assert_eq!(head.recent_process_activity[1].action, "start");
    assert_eq!(
        head.recent_process_activity[1].target,
        "/bin/sh -c 'echo ready'"
    );
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
    assert!(rendered.contains("Recent Process Activity:"));
    assert!(rendered.contains("- finish exec-1 status=Exited exit=0"));
    assert!(rendered.contains("- start /bin/sh -c 'echo ready'"));
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
fn execute_chat_turn_auto_compacts_before_provider_turn_when_prompt_reaches_threshold() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_auto_compact",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_auto_compact",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Auto compact summary."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":30,"output_tokens":8,"total_tokens":38}
            }"#
        .to_string(),
        r#"{
                "id":"resp_after_auto_compact",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_after_auto_compact",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Answer after auto compaction."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":6,"total_tokens":24}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
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
    config.context.compaction_min_messages = 8;
    config.context.compaction_keep_tail_messages = 6;
    config.context.auto_compaction_trigger_ratio = 0.5;
    config.context.context_window_tokens_override = Some(400);
    let app = build_from_config(config).expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("Auto Compact Session"))
        .expect("create session");

    for (index, (kind, content)) in [
        ("user", "covered user one with enough text to matter"),
        (
            "assistant",
            "covered assistant one with enough text to matter",
        ),
        ("user", "recent user one with enough text to matter"),
        (
            "assistant",
            "recent assistant one with enough text to matter",
        ),
        ("user", "recent user two with enough text to matter"),
        (
            "assistant",
            "recent assistant two with enough text to matter",
        ),
        ("user", "recent user three with enough text to matter"),
        (
            "assistant",
            "recent assistant three with enough text to matter",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("auto-compact-transcript-{index}"),
                session_id: session.id.clone(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 100 + index as i64,
            })
            .expect("put transcript");
    }

    let report = app
        .execute_chat_turn(
            &session.id,
            "latest question that should trigger auto compaction",
            200,
        )
        .expect("execute chat turn with auto compaction");
    let compact_request = requests.recv().expect("auto compact request");
    let chat_request = requests.recv().expect("chat request after auto compact");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_after_auto_compact");
    assert_eq!(report.output_text, "Answer after auto compaction.");

    let summary = app
        .context_summary(&session.id)
        .expect("load context summary")
        .expect("context summary should exist");
    assert_eq!(summary.summary_text, "Auto compact summary.");
    assert!(summary.covered_message_count > 0);

    let updated = app
        .session_summary(&session.id)
        .expect("session summary after auto compaction");
    assert_eq!(updated.compactifications, 1);

    let normalized_compact = compact_request.to_ascii_lowercase();
    assert!(normalized_compact.contains("summarize the provided earlier conversation"));
    assert!(
        normalized_compact.contains("\"text\":\"covered user one with enough text to matter\"")
    );

    let normalized_chat = chat_request.to_ascii_lowercase();
    assert!(normalized_chat.contains("auto compact summary."));
    assert!(
        normalized_chat
            .contains("\"text\":\"latest question that should trigger auto compaction\"")
    );
    assert!(!normalized_chat.contains("\"text\":\"covered user one with enough text to matter\""));
}

#[test]
fn execute_chat_turn_skips_auto_compaction_below_threshold() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_without_auto_compact",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_without_auto_compact",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Answer without auto compaction."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":12,"output_tokens":5,"total_tokens":17}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
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
    config.context.auto_compaction_trigger_ratio = 0.7;
    config.context.context_window_tokens_override = Some(10_000);
    let app = build_from_config(config).expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("No Auto Compact Session"))
        .expect("create session");

    for (index, (kind, content)) in [("user", "hello"), ("assistant", "world")]
        .into_iter()
        .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("no-auto-compact-transcript-{index}"),
                session_id: session.id.clone(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 300 + index as i64,
            })
            .expect("put transcript");
    }

    let report = app
        .execute_chat_turn(&session.id, "follow-up question", 400)
        .expect("execute chat turn without auto compaction");
    let chat_request = requests.recv().expect("single chat request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_without_auto_compact");
    assert_eq!(report.output_text, "Answer without auto compaction.");
    assert!(
        app.context_summary(&session.id)
            .expect("load context summary")
            .is_none()
    );
    assert_eq!(
        app.session_summary(&session.id)
            .expect("session summary")
            .compactifications,
        0
    );

    let normalized_chat = chat_request.to_ascii_lowercase();
    assert!(normalized_chat.contains("\"text\":\"hello\""));
    assert!(normalized_chat.contains("\"text\":\"follow-up question\""));
}

#[test]
fn execute_mission_turn_job_auto_compacts_before_provider_turn_when_prompt_reaches_threshold() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_auto_compact_mission",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_auto_compact_mission",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Mission auto compact summary."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":32,"output_tokens":8,"total_tokens":40}
            }"#
        .to_string(),
        r#"{
                "id":"resp_mission_after_auto_compact",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_mission_after_auto_compact",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Mission answer after auto compaction."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":20,"output_tokens":6,"total_tokens":26}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
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
    config.context.compaction_min_messages = 8;
    config.context.compaction_keep_tail_messages = 6;
    config.context.auto_compaction_trigger_ratio = 0.5;
    config.context.context_window_tokens_override = Some(400);
    let app = build_from_config(config).expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("Mission Auto Compact Session"))
        .expect("create session");

    let mission_id = "mission-auto-compact";
    store
        .put_mission(&MissionRecord {
            id: mission_id.to_string(),
            session_id: session.id.clone(),
            objective: "Mission objective".to_string(),
            status: MissionStatus::Running.as_str().to_string(),
            execution_intent: agent_runtime::mission::MissionExecutionIntent::Autonomous
                .as_str()
                .to_string(),
            schedule_json: serde_json::to_string(&agent_runtime::mission::MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 10,
            updated_at: 10,
            completed_at: None,
        })
        .expect("put mission");

    let job = JobSpec::mission_turn(
        "job-mission-auto-compact",
        &session.id,
        mission_id,
        None,
        None,
        "Mission goal that should trigger auto compaction.",
        11,
    );
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put mission job");

    for (index, (kind, content)) in [
        (
            "user",
            "mission covered user one with enough text to matter",
        ),
        (
            "assistant",
            "mission covered assistant one with enough text to matter",
        ),
        ("user", "mission recent user one with enough text to matter"),
        (
            "assistant",
            "mission recent assistant one with enough text to matter",
        ),
        ("user", "mission recent user two with enough text to matter"),
        (
            "assistant",
            "mission recent assistant two with enough text to matter",
        ),
        (
            "user",
            "mission recent user three with enough text to matter",
        ),
        (
            "assistant",
            "mission recent assistant three with enough text to matter",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("mission-auto-compact-transcript-{index}"),
                session_id: session.id.clone(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 50 + index as i64,
            })
            .expect("put transcript");
    }

    let report = app
        .execute_mission_turn_job("job-mission-auto-compact", 200)
        .expect("execute mission turn job");
    let compact_request = requests.recv().expect("mission auto compact request");
    let mission_request = requests.recv().expect("mission request after auto compact");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_mission_after_auto_compact");
    assert_eq!(report.output_text, "Mission answer after auto compaction.");
    assert_eq!(
        app.session_summary(&session.id)
            .expect("session summary after mission auto compaction")
            .compactifications,
        1
    );
    assert_eq!(
        app.context_summary(&session.id)
            .expect("load mission context summary")
            .expect("mission context summary should exist")
            .summary_text,
        "Mission auto compact summary."
    );

    let normalized_compact = compact_request.to_ascii_lowercase();
    assert!(normalized_compact.contains("summarize the provided earlier conversation"));

    let normalized_mission = mission_request.to_ascii_lowercase();
    assert!(normalized_mission.contains("mission auto compact summary."));
    assert!(normalized_mission.contains("mission goal that should trigger auto compaction."));
    assert!(
        !normalized_mission
            .contains("\"text\":\"mission covered user one with enough text to matter\"")
    );
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
fn execute_chat_turn_can_list_read_and_enable_skills_with_tools() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_skill_tools_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_skill_list",
                        "type":"function_call",
                        "call_id":"call_skill_list",
                        "name":"skill_list",
                        "arguments":"{\"include_inactive\":true,\"limit\":10}"
                    }
                ],
                "usage":{"input_tokens":30,"output_tokens":10,"total_tokens":40}
            }"#
        .to_string(),
        r#"{
                "id":"resp_skill_tools_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_skill_read",
                        "type":"function_call",
                        "call_id":"call_skill_read",
                        "name":"skill_read",
                        "arguments":"{\"name\":\"rust-debug\",\"max_bytes\":64}"
                    }
                ],
                "usage":{"input_tokens":22,"output_tokens":8,"total_tokens":30}
            }"#
        .to_string(),
        r#"{
                "id":"resp_skill_tools_3",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_skill_enable",
                        "type":"function_call",
                        "call_id":"call_skill_enable",
                        "name":"skill_enable",
                        "arguments":"{\"name\":\"rust-debug\"}"
                    }
                ],
                "usage":{"input_tokens":22,"output_tokens":8,"total_tokens":30}
            }"#
        .to_string(),
        r#"{
                "id":"resp_skill_tools_4",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_skill_tools",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Skill inspected and enabled."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":24,"output_tokens":6,"total_tokens":30}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    fs::create_dir_all(skills_dir.join("rust-debug")).expect("create skill dir");
    fs::write(
        skills_dir.join("rust-debug").join("SKILL.md"),
        "---\nname: rust-debug\ndescription: Inspect Rust failures.\n---\n\n# rust-debug\nUse this skill when debugging Rust compiler and test failures. Always start from the first failing diagnostic.\n",
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
    let app = build_from_config(config).expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-skill-tools".to_string(),
            title: "Skill Tool Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
        .execute_chat_turn("session-skill-tools", "inspect available skills", 10)
        .expect("execute chat turn");
    let _first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");
    let third_request = requests.recv().expect("third provider request");
    let fourth_request = requests.recv().expect("fourth provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_skill_tools_4");
    assert_eq!(report.output_text, "Skill inspected and enabled.");

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"call_id\":\"call_skill_list\""));
    assert!(normalized_second.contains("skill_list"));
    assert!(normalized_second.contains("rust-debug"));
    assert!(normalized_second.contains("inspect rust failures"));

    let normalized_third = third_request.to_ascii_lowercase();
    assert!(normalized_third.contains("\"call_id\":\"call_skill_read\""));
    assert!(normalized_third.contains("skill_read"));
    assert!(normalized_third.contains("debugging rust compiler"));
    assert!(normalized_third.contains("body_truncated"));

    let normalized_fourth = fourth_request.to_ascii_lowercase();
    assert!(normalized_fourth.contains("\"call_id\":\"call_skill_enable\""));
    assert!(normalized_fourth.contains("skill_enable"));
    assert!(normalized_fourth.contains("manual"));

    let session = Session::try_from(
        store
            .get_session("session-skill-tools")
            .expect("get session")
            .expect("session exists"),
    )
    .expect("restore session");
    assert_eq!(
        session.settings.enabled_skills,
        vec!["rust-debug".to_string()]
    );
    assert!(session.settings.disabled_skills.is_empty());
}

#[test]
fn execute_chat_turn_can_read_aggregate_autonomy_state_with_tool() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_autonomy_state_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_autonomy_state",
                        "type":"function_call",
                        "call_id":"call_autonomy_state",
                        "name":"autonomy_state_read",
                        "arguments":"{\"max_items\":5,\"include_inactive_schedules\":true}"
                    }
                ],
                "usage":{"input_tokens":30,"output_tokens":10,"total_tokens":40}
            }"#
        .to_string(),
        r#"{
                "id":"resp_autonomy_state_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_autonomy_state",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Autonomy state inspected."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":24,"output_tokens":6,"total_tokens":30}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&workspace_root).expect("create workspace");
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
    config.daemon.a2a_peers.insert(
        "judge".to_string(),
        agent_persistence::A2APeerConfig {
            base_url: "https://judge.example.test".to_string(),
            bearer_token: Some("secret".to_string()),
        },
    );
    let mut app = build_from_config(config).expect("build app");
    app.runtime.workspace = WorkspaceRef::new(&workspace_root);
    let workspace_root = std::fs::canonicalize(&workspace_root).expect("canonical workspace");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-autonomy-state".to_string(),
            title: "Autonomy State".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: workspace_root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: Some("agent-chain:chain-autonomy".to_string()),
            created_at: 1,
            updated_at: 1,
        })
        .expect("put parent session");

    let schedule =
        agent_runtime::agent::AgentSchedule::new(agent_runtime::agent::AgentScheduleInit {
            id: "continue-later-session-autonomy-state".to_string(),
            agent_profile_id: "default".to_string(),
            workspace_root: workspace_root.clone(),
            prompt: "continue autonomy work".to_string(),
            mode: agent_runtime::agent::AgentScheduleMode::Once,
            delivery_mode: agent_runtime::agent::AgentScheduleDeliveryMode::ExistingSession,
            target_session_id: Some("session-autonomy-state".to_string()),
            interval_seconds: 120,
            next_fire_at: 200,
            enabled: true,
            last_triggered_at: None,
            last_finished_at: None,
            last_session_id: None,
            last_job_id: None,
            last_result: None,
            last_error: None,
            created_at: 2,
            updated_at: 2,
        })
        .expect("schedule");
    store
        .put_agent_schedule(&AgentScheduleRecord::from(&schedule))
        .expect("put schedule");

    store
        .put_job(&JobRecord {
            id: "job-autonomy-delegate".to_string(),
            session_id: "session-autonomy-state".to_string(),
            mission_id: None,
            run_id: None,
            parent_job_id: None,
            kind: "delegate".to_string(),
            status: "running".to_string(),
            input_json: None,
            result_json: None,
            error: None,
            created_at: 3,
            updated_at: 5,
            started_at: Some(4),
            finished_at: None,
            attempt_count: 1,
            max_attempts: 1,
            lease_owner: None,
            lease_expires_at: None,
            heartbeat_at: None,
            cancel_requested_at: None,
            last_progress_message: Some("delegated child session running".to_string()),
            callback_json: None,
            callback_sent_at: None,
        })
        .expect("put active job");

    store
        .put_session(&SessionRecord {
            id: "session-delegate-job-autonomy-delegate".to_string(),
            title: "Delegate Child".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize child settings"),
            workspace_root: workspace_root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: Some("session-autonomy-state".to_string()),
            parent_job_id: Some("job-autonomy-delegate".to_string()),
            delegation_label: Some("delegate:review".to_string()),
            created_at: 4,
            updated_at: 6,
        })
        .expect("put child session");

    store
        .put_session_inbox_event(&agent_persistence::SessionInboxEventRecord {
            id: "inbox-autonomy".to_string(),
            session_id: "session-autonomy-state".to_string(),
            job_id: Some("job-autonomy-delegate".to_string()),
            kind: "delegation_result_ready".to_string(),
            payload_json: "{}".to_string(),
            status: "queued".to_string(),
            created_at: 7,
            available_at: 8,
            claimed_at: None,
            processed_at: None,
            error: None,
        })
        .expect("put inbox");

    let chain = agent_runtime::interagent::AgentMessageChain::new(
        "chain-autonomy",
        "session-origin",
        "default",
        1,
        3,
        Some("session-origin".to_string()),
        agent_runtime::interagent::AgentChainState::Active,
    )
    .expect("chain");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "transcript-autonomy-chain".to_string(),
            session_id: "session-autonomy-state".to_string(),
            run_id: None,
            kind: "system".to_string(),
            content: chain.to_transcript_metadata(),
            created_at: 9,
        })
        .expect("put chain transcript");

    let report = app
        .execute_chat_turn("session-autonomy-state", "inspect autonomy", 10)
        .expect("execute chat turn");
    let _first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_autonomy_state_2");
    assert_eq!(report.output_text, "Autonomy state inspected.");

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"call_id\":\"call_autonomy_state\""));
    assert!(normalized_second.contains("autonomy_state_read"));
    assert!(normalized_second.contains("continue-later-session-autonomy-state"));
    assert!(normalized_second.contains("job-autonomy-delegate"));
    assert!(normalized_second.contains("session-delegate-job-autonomy-delegate"));
    assert!(normalized_second.contains("delegation_result_ready"));
    assert!(normalized_second.contains("chain-autonomy"));
    assert!(normalized_second.contains("judge.example.test"));
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
fn execute_chat_turn_offloads_large_web_fetch_results_into_artifacts() {
    let large_html = format!(
        "<html><head><title>Readable web page</title>\
         <style>.hidden{{display:none}}</style>\
         <script>console.log('ignore me');</script></head>\
         <body><article><h1>Readable web page</h1><p>{}</p><p>{}</p></article></body></html>",
        "WEB-OFFLOAD-MARKER ".repeat(700),
        "second readable paragraph ".repeat(500)
    );
    let (web_base, _web_requests, web_handle) =
        spawn_http_server("/page", "text/html; charset=utf-8", large_html);
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        format!(
            r#"{{
                    "id":"resp_offload_web_fetch_1",
                    "model":"gpt-5.4",
                    "output":[
                        {{
                            "id":"fc_web_fetch_large",
                            "type":"function_call",
                            "call_id":"call_web_fetch_large",
                            "name":"web_fetch",
                            "arguments":"{{\"url\":\"{}\"}}"
                        }}
                    ],
                    "usage":{{"input_tokens":40,"output_tokens":12,"total_tokens":52}}
                }}"#,
            web_base
        ),
        r#"{
                "id":"resp_offload_web_fetch_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_offload_web_fetch",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Large web fetch result was offloaded."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":6,"total_tokens":24}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
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
    app.runtime.workspace = WorkspaceRef::new(temp.path());
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-offload-large-web-fetch".to_string(),
            title: "Large Web Fetch".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
            "session-offload-large-web-fetch",
            "fetch the large readable web page",
            10,
        )
        .expect("execute chat turn");
    let _first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");
    handle.join().expect("join server");
    web_handle.join().expect("join web server");

    assert_eq!(report.output_text, "Large web fetch result was offloaded.");

    let snapshot = ContextOffloadSnapshot::try_from(
        store
            .get_context_offload("session-offload-large-web-fetch")
            .expect("get context offload")
            .expect("context offload exists"),
    )
    .expect("restore offload snapshot");
    assert_eq!(snapshot.refs.len(), 1);
    assert!(snapshot.refs[0].label.contains("web_fetch"));
    assert!(snapshot.refs[0].summary.contains(&web_base));

    let payload = store
        .get_context_offload_payload(&snapshot.refs[0].artifact_id)
        .expect("get payload")
        .expect("payload exists");
    let payload_text = String::from_utf8_lossy(&payload.bytes);
    assert!(payload_text.contains("# Readable web page"));
    assert!(payload_text.contains("Readable web page"));
    assert!(payload_text.contains("WEB-OFFLOAD-MARKER"));
    assert!(!payload_text.contains("<html"));
    assert!(!payload_text.contains("console.log"));

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

    assert_eq!(tool_output_json["tool"], serde_json::json!("web_fetch"));
    assert_eq!(tool_output_json["offloaded"], serde_json::json!(true));
    assert_eq!(tool_output_json["url"], serde_json::json!(web_base));
    assert_eq!(
        tool_output_json["title"],
        serde_json::json!("Readable web page")
    );
    assert_eq!(
        tool_output_json["extracted_from_html"],
        serde_json::json!(true)
    );
    assert!(tool_output_json.get("artifact_id").is_some());
    assert!(!tool_output.contains("<html"));
    assert!(!tool_output.contains("console.log"));
}

#[test]
fn execute_chat_turn_prunes_stale_offload_refs_when_a_payload_file_is_missing() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_offload_stale_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_fs_read_old",
                        "type":"function_call",
                        "call_id":"call_fs_read_old",
                        "name":"fs_read_text",
                        "arguments":"{\"path\":\"docs/old.txt\"}"
                    }
                ],
                "usage":{"input_tokens":40,"output_tokens":12,"total_tokens":52}
            }"#
        .to_string(),
        r#"{
                "id":"resp_offload_stale_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_offload_stale_1",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"First file was offloaded."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":6,"total_tokens":24}
            }"#
        .to_string(),
        r#"{
                "id":"resp_offload_stale_3",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_fs_read_new",
                        "type":"function_call",
                        "call_id":"call_fs_read_new",
                        "name":"fs_read_text",
                        "arguments":"{\"path\":\"docs/new.txt\"}"
                    }
                ],
                "usage":{"input_tokens":40,"output_tokens":12,"total_tokens":52}
            }"#
        .to_string(),
        r#"{
                "id":"resp_offload_stale_4",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_offload_stale_2",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Second file was offloaded after pruning stale refs."
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
    let large_old = format!(
        "{}\n{}\n{}\n",
        "OFFLOAD-MARKER-OLD-BLOCK".repeat(120),
        "second-line".repeat(80),
        "third-line".repeat(80)
    );
    let large_new = format!(
        "{}\n{}\n{}\n",
        "OFFLOAD-MARKER-NEW-BLOCK".repeat(120),
        "second-line".repeat(80),
        "third-line".repeat(80)
    );
    fs::write(workspace_root.join("docs/old.txt"), &large_old).expect("write old file");
    fs::write(workspace_root.join("docs/new.txt"), &large_new).expect("write new file");
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
            id: "session-offload-stale-prune".to_string(),
            title: "Stale Offload".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let first = app
        .execute_chat_turn("session-offload-stale-prune", "read the old big file", 10)
        .expect("execute first chat turn");
    assert_eq!(first.output_text, "First file was offloaded.");
    let _ = requests.recv().expect("first provider request");
    let _ = requests.recv().expect("second provider request");

    let first_snapshot = ContextOffloadSnapshot::try_from(
        store
            .get_context_offload("session-offload-stale-prune")
            .expect("get first offload")
            .expect("first offload exists"),
    )
    .expect("restore first snapshot");
    assert_eq!(first_snapshot.refs.len(), 1);
    let stale_artifact_id = first_snapshot.refs[0].artifact_id.clone();
    let stale_path = app
        .persistence
        .stores
        .artifacts_dir
        .join(format!("{stale_artifact_id}.bin"));
    fs::remove_file(&stale_path).expect("remove stale payload");

    let second = app
        .execute_chat_turn("session-offload-stale-prune", "read the new big file", 20)
        .expect("execute second chat turn");
    assert_eq!(
        second.output_text,
        "Second file was offloaded after pruning stale refs."
    );
    let _ = requests.recv().expect("third provider request");
    let second_request = requests.recv().expect("fourth provider request");
    handle.join().expect("join server");

    let snapshot = ContextOffloadSnapshot::try_from(
        store
            .get_context_offload("session-offload-stale-prune")
            .expect("get latest offload")
            .expect("latest offload exists"),
    )
    .expect("restore latest snapshot");
    assert_eq!(snapshot.refs.len(), 1);
    assert!(snapshot.refs[0].summary.contains("docs/new.txt"));
    assert_ne!(snapshot.refs[0].artifact_id, stale_artifact_id);

    let payload = store
        .get_context_offload_payload(&snapshot.refs[0].artifact_id)
        .expect("get latest payload")
        .expect("latest payload exists");
    assert!(String::from_utf8_lossy(&payload.bytes).contains("OFFLOAD-MARKER-NEW-BLOCK"));

    let second_request_body = second_request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("extract second turn provider request body");
    let second_request_json: serde_json::Value =
        serde_json::from_str(second_request_body).expect("parse second turn provider request");
    let tool_output = second_request_json["input"][0]["output"]
        .as_str()
        .expect("tool output string");
    let tool_output_json: serde_json::Value =
        serde_json::from_str(tool_output).expect("parse compact tool output");

    assert_eq!(tool_output_json["offloaded"], serde_json::json!(true));
    assert_eq!(tool_output_json["path"], serde_json::json!("docs/new.txt"));
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
fn execute_chat_turn_can_read_and_update_prompt_budget_policy() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_prompt_budget_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_prompt_budget_update",
                        "type":"function_call",
                        "call_id":"call_prompt_budget_update",
                        "name":"prompt_budget_update",
                        "arguments":"{\"percentages\":{\"system\":5,\"agents\":7,\"active_skills\":12,\"session_head\":5,\"autonomy_state\":5,\"plan\":8,\"context_summary\":15,\"offload_refs\":15,\"recent_tool_activity\":8,\"transcript_tail\":20},\"reason\":\"give tool activity a little more room\"}"
                    }
                ],
                "usage":{"input_tokens":30,"output_tokens":10,"total_tokens":40}
            }"#
        .to_string(),
        r#"{
                "id":"resp_prompt_budget_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_prompt_budget_read",
                        "type":"function_call",
                        "call_id":"call_prompt_budget_read",
                        "name":"prompt_budget_read",
                        "arguments":"{}"
                    }
                ],
                "usage":{"input_tokens":24,"output_tokens":8,"total_tokens":32}
            }"#
        .to_string(),
        r#"{
                "id":"resp_prompt_budget_3",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_prompt_budget",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Prompt budget updated and read back."
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
        context: agent_persistence::config::ContextConfig {
            auto_compaction_trigger_ratio: 0.8,
            context_window_tokens_override: Some(10_000),
            ..agent_persistence::config::ContextConfig::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-prompt-budget".to_string(),
            title: "Prompt Budget".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
        .execute_chat_turn("session-prompt-budget", "adjust prompt budget", 10)
        .expect("execute chat turn");
    let _first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");
    let third_request = requests.recv().expect("third provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_prompt_budget_3");
    assert_eq!(report.output_text, "Prompt budget updated and read back.");

    let updated = Session::try_from(
        store
            .get_session("session-prompt-budget")
            .expect("get session")
            .expect("session exists"),
    )
    .expect("restore session");
    assert_eq!(updated.settings.prompt_budget.agents, 7);
    assert_eq!(updated.settings.prompt_budget.recent_tool_activity, 8);

    let second_request_body = second_request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("extract second provider request body");
    let second_request_json: serde_json::Value =
        serde_json::from_str(second_request_body).expect("parse second provider request");
    let update_tool_output = second_request_json["input"][0]["output"]
        .as_str()
        .expect("update tool output string");
    let update_tool_output_json: serde_json::Value =
        serde_json::from_str(update_tool_output).expect("parse update tool output");

    assert_eq!(
        update_tool_output_json["tool"],
        serde_json::json!("prompt_budget_update")
    );
    assert_eq!(
        update_tool_output_json["usable_context_tokens"],
        serde_json::json!(8000)
    );
    assert_eq!(
        update_tool_output_json["source"],
        serde_json::json!("session_override")
    );

    let third_request_body = third_request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("extract third provider request body");
    let third_request_json: serde_json::Value =
        serde_json::from_str(third_request_body).expect("parse third provider request");
    let read_tool_output = third_request_json["input"][0]["output"]
        .as_str()
        .expect("read tool output string");
    let read_tool_output_json: serde_json::Value =
        serde_json::from_str(read_tool_output).expect("parse read tool output");
    let recent_tool_activity_layer = read_tool_output_json["layers"]
        .as_array()
        .expect("layers array")
        .iter()
        .find(|layer| layer["layer"] == serde_json::json!("recent_tool_activity"))
        .expect("recent tool activity layer");

    assert_eq!(
        read_tool_output_json["tool"],
        serde_json::json!("prompt_budget_read")
    );
    assert_eq!(
        recent_tool_activity_layer["target_tokens"],
        serde_json::json!(640)
    );
}

#[test]
fn execute_chat_turn_applies_next_turn_prompt_budget_once_without_persisting_session_policy() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_next_turn_budget_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_prompt_budget_next_turn",
                        "type":"function_call",
                        "call_id":"call_prompt_budget_next_turn",
                        "name":"prompt_budget_update",
                        "arguments":"{\"scope\":\"next_turn\",\"percentages\":{\"context_summary\":34,\"transcript_tail\":1},\"reason\":\"trim only the next full prompt\"}"
                    }
                ],
                "usage":{"input_tokens":30,"output_tokens":10,"total_tokens":40}
            }"#
        .to_string(),
        r#"{
                "id":"resp_next_turn_budget_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_prompt_budget_next_turn",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Next turn budget override queued."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":22,"output_tokens":6,"total_tokens":28}
            }"#
        .to_string(),
        r#"{
                "id":"resp_next_turn_budget_3",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_prompt_budget_next_turn_applied",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"One-shot budget applied."
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
        context: agent_persistence::config::ContextConfig {
            auto_compaction_trigger_ratio: 1.0,
            context_window_tokens_override: Some(10_000),
            ..agent_persistence::config::ContextConfig::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-prompt-budget-next-turn".to_string(),
            title: "Prompt Budget Next Turn".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let first_report = app
        .execute_chat_turn(
            "session-prompt-budget-next-turn",
            "queue one-shot budget",
            10,
        )
        .expect("execute first chat turn");
    let _first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");

    assert_eq!(
        first_report.output_text,
        "Next turn budget override queued."
    );
    let queued = Session::try_from(
        store
            .get_session("session-prompt-budget-next-turn")
            .expect("get queued session")
            .expect("queued session exists"),
    )
    .expect("restore queued session");
    assert_eq!(
        queued.settings.prompt_budget,
        agent_runtime::session::PromptBudgetPolicy::default()
    );
    assert_eq!(
        queued
            .settings
            .next_prompt_budget_override
            .as_ref()
            .expect("next-turn override")
            .transcript_tail,
        1
    );

    let second_request_body = second_request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("extract second provider request body");
    let second_request_json: serde_json::Value =
        serde_json::from_str(second_request_body).expect("parse second provider request");
    let update_tool_output = second_request_json["input"][0]["output"]
        .as_str()
        .expect("update tool output string");
    let update_tool_output_json: serde_json::Value =
        serde_json::from_str(update_tool_output).expect("parse update tool output");
    assert_eq!(
        update_tool_output_json["scope"],
        serde_json::json!("next_turn")
    );
    assert_eq!(
        update_tool_output_json["source"],
        serde_json::json!("next_turn_override")
    );

    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "transcript-next-turn-old-user".to_string(),
            session_id: "session-prompt-budget-next-turn".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: format!(
                "{}OLD_PROMPT_BUDGET_NEXT_TURN_MARKER",
                "old user filler ".repeat(400)
            ),
            created_at: 2,
        })
        .expect("put old transcript");

    let second_report = app
        .execute_chat_turn(
            "session-prompt-budget-next-turn",
            "this turn should use the one-shot budget",
            10,
        )
        .expect("execute second chat turn");
    let third_request = requests.recv().expect("third provider request");
    handle.join().expect("join server");

    assert_eq!(second_report.output_text, "One-shot budget applied.");
    assert!(third_request.contains("Prompt Budget Truncation:"));
    assert!(third_request.contains("layer=transcript_tail"));
    assert!(!third_request.contains("OLD_PROMPT_BUDGET_NEXT_TURN_MARKER"));

    let consumed = Session::try_from(
        store
            .get_session("session-prompt-budget-next-turn")
            .expect("get consumed session")
            .expect("consumed session exists"),
    )
    .expect("restore consumed session");
    assert_eq!(
        consumed.settings.prompt_budget,
        agent_runtime::session::PromptBudgetPolicy::default()
    );
    assert!(consumed.settings.next_prompt_budget_override.is_none());
}

#[test]
fn execute_chat_turn_applies_prompt_budget_to_transcript_tail() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_prompt_budget_truncation",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_prompt_budget_truncation",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Budgeted prompt received."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":30,"output_tokens":6,"total_tokens":36}
            }"#
        .to_string(),
    ]);
    let temp = tempfile::tempdir().expect("tempdir");
    let settings = SessionSettings {
        prompt_budget: agent_runtime::session::PromptBudgetPolicy {
            context_summary: 34,
            transcript_tail: 1,
            ..agent_runtime::session::PromptBudgetPolicy::default()
        },
        ..SessionSettings::default()
    };
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
        context: agent_persistence::config::ContextConfig {
            auto_compaction_trigger_ratio: 1.0,
            context_window_tokens_override: Some(10_000),
            ..agent_persistence::config::ContextConfig::default()
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-prompt-budget-truncate".to_string(),
            title: "Prompt Budget Truncation".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&settings).expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
            id: "transcript-budget-old-user".to_string(),
            session_id: "session-prompt-budget-truncate".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: format!("{}OLD_TRANSCRIPT_MARKER", "old user filler ".repeat(200)),
            created_at: 2,
        })
        .expect("put old transcript");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "transcript-budget-old-assistant".to_string(),
            session_id: "session-prompt-budget-truncate".to_string(),
            run_id: None,
            kind: "assistant".to_string(),
            content: format!(
                "{}OLD_ASSISTANT_MARKER",
                "old assistant filler ".repeat(200)
            ),
            created_at: 3,
        })
        .expect("put old assistant transcript");

    let report = app
        .execute_chat_turn(
            "session-prompt-budget-truncate",
            "latest short budget question",
            10,
        )
        .expect("execute chat turn");
    let provider_request = requests.recv().expect("provider request");
    handle.join().expect("join server");

    assert_eq!(report.output_text, "Budgeted prompt received.");
    assert!(provider_request.contains("Prompt Budget Truncation:"));
    assert!(provider_request.contains("layer=transcript_tail"));
    assert!(provider_request.contains("hidden_messages=2"));
    assert!(provider_request.contains("latest short budget question"));
    assert!(!provider_request.contains("OLD_TRANSCRIPT_MARKER"));
    assert!(!provider_request.contains("OLD_ASSISTANT_MARKER"));
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
                    pinned: false,
                    explicit_read_count: 0,
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

    let updated_offload = ContextOffloadSnapshot::try_from(
        store
            .get_context_offload("session-offload-tools")
            .expect("get updated offload")
            .expect("updated offload exists"),
    )
    .expect("restore updated offload");
    assert_eq!(updated_offload.refs[0].explicit_read_count, 1);
}

#[test]
fn execute_chat_turn_can_pin_and_unpin_offloaded_context_refs() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_offload_pin_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_artifact_pin",
                        "type":"function_call",
                        "call_id":"call_artifact_pin",
                        "name":"artifact_pin",
                        "arguments":"{\"artifact_id\":\"artifact-offload-pin\"}"
                    }
                ],
                "usage":{"input_tokens":40,"output_tokens":12,"total_tokens":52}
            }"#
        .to_string(),
        r#"{
                "id":"resp_offload_pin_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_artifact_unpin",
                        "type":"function_call",
                        "call_id":"call_artifact_unpin",
                        "name":"artifact_unpin",
                        "arguments":"{\"artifact_id\":\"artifact-offload-pin\"}"
                    }
                ],
                "usage":{"input_tokens":24,"output_tokens":6,"total_tokens":30}
            }"#
        .to_string(),
        r#"{
                "id":"resp_offload_pin_3",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_offload_pin",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Pinned and unpinned offloaded context."
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
            id: "session-offload-pin".to_string(),
            title: "Offload Pin".to_string(),
            prompt_override: Some("Manage offloaded context refs when useful.".to_string()),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
                session_id: "session-offload-pin".to_string(),
                refs: vec![ContextOffloadRef {
                    id: "offload-pin".to_string(),
                    label: "Important diagnostics".to_string(),
                    summary: "Diagnostics that may matter again".to_string(),
                    artifact_id: "artifact-offload-pin".to_string(),
                    token_estimate: 120,
                    message_count: 4,
                    created_at: 2,
                    pinned: false,
                    explicit_read_count: 2,
                }],
                updated_at: 3,
            })
            .expect("offload record"),
            &[ContextOffloadPayload {
                artifact_id: "artifact-offload-pin".to_string(),
                bytes: b"pin me if useful\n".to_vec(),
            }],
        )
        .expect("put context offload");

    let report = app
        .execute_chat_turn("session-offload-pin", "Pin then unpin the diagnostics", 10)
        .expect("execute chat turn");
    let first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");
    let third_request = requests.recv().expect("third provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_offload_pin_3");
    assert_eq!(report.output_text, "Pinned and unpinned offloaded context.");

    let normalized_first = first_request.to_ascii_lowercase();
    assert!(normalized_first.contains("\"name\":\"artifact_pin\""));
    assert!(normalized_first.contains("\"name\":\"artifact_unpin\""));
    assert!(normalized_first.contains("artifact-offload-pin"));

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"call_id\":\"call_artifact_pin\""));
    assert!(normalized_second.contains("\\\"pin_status\\\":\\\"manual\\\""));
    assert!(normalized_second.contains("\\\"pinned\\\":true"));

    let normalized_third = third_request.to_ascii_lowercase();
    assert!(normalized_third.contains("\"call_id\":\"call_artifact_unpin\""));
    assert!(normalized_third.contains("\\\"pin_status\\\":\\\"none\\\""));
    assert!(normalized_third.contains("\\\"pinned\\\":false"));

    let updated_offload = ContextOffloadSnapshot::try_from(
        store
            .get_context_offload("session-offload-pin")
            .expect("get updated offload")
            .expect("updated offload exists"),
    )
    .expect("restore updated offload");
    assert!(!updated_offload.refs[0].pinned);
    assert_eq!(updated_offload.refs[0].explicit_read_count, 2);
    assert_eq!(updated_offload.refs[0].pin_status(), "none");
}

#[test]
fn execute_chat_turn_recovers_when_artifact_read_payload_is_missing() {
    let (api_base, requests, handle) = spawn_json_server_sequence(vec![
        r#"{
                "id":"resp_offload_missing_1",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"fc_artifact_read_missing",
                        "type":"function_call",
                        "call_id":"call_artifact_read_missing",
                        "name":"artifact_read",
                        "arguments":"{\"artifact_id\":\"artifact-offload-missing\"}"
                    }
                ],
                "usage":{"input_tokens":40,"output_tokens":12,"total_tokens":52}
            }"#
        .to_string(),
        r#"{
                "id":"resp_offload_missing_2",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_offload_missing",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Handled missing offloaded context."
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
            id: "session-offload-missing".to_string(),
            title: "Missing Offload".to_string(),
            prompt_override: Some(
                "Recover gracefully when an offload artifact is missing.".to_string(),
            ),
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
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
                session_id: "session-offload-missing".to_string(),
                refs: vec![ContextOffloadRef {
                    id: "offload-missing".to_string(),
                    label: "Broken artifact".to_string(),
                    summary: "This payload file is gone".to_string(),
                    artifact_id: "artifact-offload-missing".to_string(),
                    token_estimate: 120,
                    message_count: 4,
                    created_at: 2,
                    pinned: false,
                    explicit_read_count: 0,
                }],
                updated_at: 3,
            })
            .expect("offload record"),
            &[ContextOffloadPayload {
                artifact_id: "artifact-offload-missing".to_string(),
                bytes: b"stale bytes".to_vec(),
            }],
        )
        .expect("put context offload");
    let missing_path = app
        .persistence
        .stores
        .artifacts_dir
        .join("artifact-offload-missing.bin");
    fs::remove_file(&missing_path).expect("remove payload file");

    let report = app
        .execute_chat_turn("session-offload-missing", "Recover the broken offload", 10)
        .expect("execute chat turn");
    let _first_request = requests.recv().expect("first provider request");
    let second_request = requests.recv().expect("second provider request");
    handle.join().expect("join server");

    assert_eq!(report.response_id, "resp_offload_missing_2");
    assert_eq!(report.output_text, "Handled missing offloaded context.");

    let normalized_second = second_request.to_ascii_lowercase();
    assert!(normalized_second.contains("\"call_id\":\"call_artifact_read_missing\""));
    assert!(
        normalized_second.contains("artifact_offload_missing")
            || normalized_second.contains("artifact-offload-missing")
    );
    assert!(normalized_second.contains("missing from context offload storage"));
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
                workspace_root: app.runtime.workspace.root.display().to_string(),
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

#[test]
fn provider_request_preview_merges_dynamic_mcp_tools_for_default_agents_only() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut app = build_from_config(AppConfig {
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
    app.mcp = SharedMcpRegistry::with_mock_connectors(vec![MockMcpConnectorRuntime {
        id: "docs".to_string(),
        tools: vec![McpDiscoveredTool {
            exposed_name: "mcp__docs__search_code".to_string(),
            remote_name: "search_code".to_string(),
            title: Some("Search code".to_string()),
            description: Some("Search the docs code index".to_string()),
            input_schema: serde_json::json!({
                "type":"object",
                "properties":{"query":{"type":"string"}},
                "required":["query"],
                "additionalProperties": false
            }),
            read_only: true,
            destructive: false,
        }],
        resources: vec![McpDiscoveredResource {
            connector_id: "docs".to_string(),
            uri: "file:///guides/onboarding.md".to_string(),
            name: "onboarding".to_string(),
            title: Some("Onboarding".to_string()),
            description: Some("Operator onboarding guide".to_string()),
            mime_type: Some("text/markdown".to_string()),
        }],
        prompts: vec![McpDiscoveredPrompt {
            connector_id: "docs".to_string(),
            name: "incident_triage".to_string(),
            title: Some("Incident triage".to_string()),
            description: Some("Triage incidents from docs".to_string()),
            arguments: vec![McpDiscoveredPromptArgument {
                name: "service".to_string(),
                description: Some("Service name".to_string()),
                required: false,
            }],
        }],
        tool_results: std::collections::BTreeMap::new(),
        resource_reads: std::collections::BTreeMap::new(),
        prompt_gets: std::collections::BTreeMap::new(),
    }]);
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    for (session_id, title, agent_profile_id) in [
        ("session-preview-mcp-default", "Default Preview", "default"),
        ("session-preview-mcp-judge", "Judge Preview", "judge"),
    ] {
        store
            .put_session(&SessionRecord {
                id: session_id.to_string(),
                title: title.to_string(),
                prompt_override: None,
                settings_json: serde_json::to_string(&SessionSettings::default())
                    .expect("serialize settings"),
                workspace_root: app.runtime.workspace.root.display().to_string(),
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
        .render_provider_request_preview("session-preview-mcp-default")
        .expect("default provider preview");
    let judge_preview = app
        .render_provider_request_preview("session-preview-mcp-judge")
        .expect("judge provider preview");

    assert!(default_preview.contains("\"name\": \"mcp__docs__search_code\""));
    assert!(default_preview.contains("\"name\": \"mcp_search_resources\""));
    assert!(default_preview.contains("\"name\": \"mcp_get_prompt\""));

    assert!(!judge_preview.contains("\"name\": \"mcp__docs__search_code\""));
    assert!(!judge_preview.contains("\"name\": \"mcp_search_resources\""));
    assert!(!judge_preview.contains("\"name\": \"mcp_get_prompt\""));
}
