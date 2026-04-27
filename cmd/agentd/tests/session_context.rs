use agent_persistence::{
    AgentRepository, AgentScheduleRecord, AppConfig, ContextOffloadRepository,
    ContextSummaryRepository, JobRecord, JobRepository, MissionRecord, MissionRepository,
    PersistenceStore, RunRecord, RunRepository, SessionInboxRepository, SessionRecord,
    SessionRepository, ToolCallRecord, ToolCallRepository, TranscriptRepository,
};
use agent_runtime::agent::{
    AgentSchedule, AgentScheduleDeliveryMode, AgentScheduleInit, AgentScheduleMode,
};
use agent_runtime::context::{ContextOffloadPayload, ContextOffloadRef, ContextOffloadSnapshot};
use agent_runtime::inbox::SessionInboxEvent;
use agent_runtime::interagent::{AgentChainState, AgentMessageChain, DEFAULT_MAX_HOPS};
use agent_runtime::mission::{JobSpec, JobStatus, MissionSpec, MissionStatus};
use agent_runtime::run::{ActiveProcess, RunEngine};
use agent_runtime::session::{Session, SessionSettings};
use agent_runtime::tool::{ExecStartInput, ProcessOutputStream, ToolCall, ToolRuntime};
use agent_runtime::workspace::WorkspaceRef;
use agentd::bootstrap::{SessionSummary, build_from_config};
use std::fs;
#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[test]
fn render_context_state_explains_usage_summary_and_compaction_policy() {
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
    assert!(rendered.contains("usage=<нет>; approx_ctx="));
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
fn render_context_state_includes_offload_snapshot_details() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("Offload Context Session"))
        .expect("create session");

    store
        .put_context_offload(
            &agent_persistence::ContextOffloadRecord::try_from(&ContextOffloadSnapshot {
                session_id: session.id.clone(),
                refs: vec![
                    ContextOffloadRef {
                        id: "offload-1".to_string(),
                        label: "Large fs_read_text".to_string(),
                        summary: "Big document offloaded".to_string(),
                        artifact_id: "artifact-offload-1".to_string(),
                        token_estimate: 120,
                        message_count: 3,
                        created_at: 77,
                        pinned: false,
                        explicit_read_count: 0,
                    },
                    ContextOffloadRef {
                        id: "offload-2".to_string(),
                        label: "Second payload".to_string(),
                        summary: "Another large block".to_string(),
                        artifact_id: "artifact-offload-2".to_string(),
                        token_estimate: 80,
                        message_count: 2,
                        created_at: 88,
                        pinned: false,
                        explicit_read_count: 0,
                    },
                ],
                updated_at: 99,
            })
            .expect("offload record"),
            &[
                ContextOffloadPayload {
                    artifact_id: "artifact-offload-1".to_string(),
                    bytes: b"payload-1".to_vec(),
                },
                ContextOffloadPayload {
                    artifact_id: "artifact-offload-2".to_string(),
                    bytes: b"payload-2".to_vec(),
                },
            ],
        )
        .expect("put context offload");

    let rendered = app
        .render_context_state(&session.id)
        .expect("render context state");

    assert!(rendered.contains("offload_tokens=200"));
    assert!(rendered.contains("offload_refs=2"));
    assert!(rendered.contains("offload_messages=5"));
    assert!(rendered.contains("offload_updated_at=99"));
    assert!(rendered.contains("Offload:"));
    assert!(rendered.contains("artifact-offload-1"));
    assert!(rendered.contains("Large fs_read_text"));
    assert!(rendered.contains("Big document offloaded"));
}

#[test]
fn session_head_prefers_latest_provider_usage_input_tokens_for_ctx() {
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
    let summary: SessionSummary = app
        .session_summary("session-usage")
        .expect("session summary");

    assert_eq!(head.context_tokens, 123);
    assert_eq!(summary.context_tokens, 123);
    assert_eq!(summary.usage_input_tokens, Some(123));
    assert_eq!(summary.usage_output_tokens, Some(7));
    assert_eq!(summary.usage_total_tokens, Some(130));
}

#[test]
fn session_head_includes_agent_and_schedule_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-scheduled-head".to_string(),
            title: "Scheduled Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "judge".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: Some("agent-schedule:judge-pulse".to_string()),
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_agent_schedule(&AgentScheduleRecord::from(
            &AgentSchedule::new(AgentScheduleInit {
                id: "judge-pulse".to_string(),
                agent_profile_id: "judge".to_string(),
                workspace_root: fs::canonicalize(".").expect("canonical workspace"),
                prompt: "watch for regressions".to_string(),
                mode: AgentScheduleMode::Interval,
                delivery_mode: AgentScheduleDeliveryMode::ExistingSession,
                target_session_id: Some("session-scheduled-head".to_string()),
                interval_seconds: 300,
                next_fire_at: 123,
                enabled: true,
                last_triggered_at: None,
                last_finished_at: None,
                last_session_id: None,
                last_job_id: None,
                last_result: Some("running".to_string()),
                last_error: None,
                created_at: 1,
                updated_at: 2,
            })
            .expect("schedule"),
        ))
        .expect("put schedule");

    let head = app
        .session_head("session-scheduled-head")
        .expect("session head");

    assert_eq!(head.agent_profile_id, "judge");
    assert_eq!(head.agent_name, "Judge");
    let schedule = head.schedule.expect("schedule metadata");
    assert_eq!(schedule.id, "judge-pulse");
    assert_eq!(schedule.mode, AgentScheduleMode::Interval);
    assert_eq!(
        schedule.delivery_mode,
        AgentScheduleDeliveryMode::ExistingSession
    );
    assert_eq!(
        schedule.target_session_id.as_deref(),
        Some("session-scheduled-head")
    );
    assert_eq!(schedule.last_result.as_deref(), Some("running"));
}

#[test]
fn render_active_run_shows_usage_and_active_processes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-run".to_string(),
            title: "Session Run".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: Some("agent-schedule:judge-pulse".to_string()),
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_agent_schedule(&AgentScheduleRecord::from(
            &AgentSchedule::new(AgentScheduleInit {
                id: "judge-pulse".to_string(),
                agent_profile_id: "judge".to_string(),
                workspace_root: fs::canonicalize(".").expect("canonical workspace"),
                prompt: "review the latest change".to_string(),
                mode: AgentScheduleMode::Interval,
                delivery_mode: AgentScheduleDeliveryMode::FreshSession,
                target_session_id: None,
                interval_seconds: 300,
                next_fire_at: 123,
                enabled: true,
                last_triggered_at: None,
                last_finished_at: None,
                last_session_id: None,
                last_job_id: None,
                last_result: Some("running".to_string()),
                last_error: None,
                created_at: 1,
                updated_at: 2,
            })
            .expect("schedule"),
        ))
        .expect("put schedule");

    let mut run = RunEngine::new("run-live", "session-run", None, 10);
    run.start(10).expect("start run");
    run.track_active_process(
        ActiveProcess::new("exec-9", "exec", "pid:4242", 11)
            .with_command_details("curl -fL https://example.test/govc.tar.gz", "/workspace"),
        11,
    )
    .expect("track active process");
    run.record_tool_completion("plan_snapshot -> plan_snapshot items=11", 12)
        .expect("record plan snapshot");
    run.record_system_note(
        "provider retryable error: upstream reset; retrying request (1/3)",
        13,
    )
    .expect("record system note");
    run.record_tool_completion(
        "fs_list path=projects/adqm/infra/ansible recursive=true -> fs_list entries=46959",
        14,
    )
    .expect("record fs_list");
    run.set_latest_provider_usage(
        Some(agent_runtime::provider::ProviderUsage {
            input_tokens: 400,
            output_tokens: 40,
            total_tokens: 440,
        }),
        12,
    )
    .expect("set provider usage");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");

    let rendered = app
        .render_active_run("session-run")
        .expect("render active run");

    assert!(rendered.contains("Ход:"));
    assert!(rendered.contains("сессия: Session Run"));
    assert!(rendered.contains("агент: Ассистент (default)"));
    assert!(rendered.contains("run-live"));
    assert!(rendered.contains("статус: running"));
    assert!(
        rendered
            .contains("расписание: judge-pulse mode=interval delivery=fresh_session enabled=true")
    );
    assert!(rendered.contains("last_result: running"));
    assert!(rendered.contains("usage: input=400 output=40 total=440"));
    assert!(rendered.contains(
        "последний шаг: fs_list path=projects/adqm/infra/ansible recursive=true -> fs_list entries=46959"
    ));
    assert!(rendered.contains("предыдущие шаги:"));
    assert!(rendered.contains("provider retryable error: upstream reset; retrying request (1/3)"));
    assert!(rendered.contains("plan_snapshot -> plan_snapshot items=11"));
    assert!(rendered.contains("exec-9 (exec) pid:4242"));
    assert!(rendered.contains("команда: curl -fL https://example.test/govc.tar.gz"));
    assert!(rendered.contains("cwd: /workspace"));
}

#[test]
fn render_active_run_includes_interagent_chain_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-run-chain".to_string(),
            title: "Session Run Chain".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "judge".to_string(),
            active_mission_id: None,
            parent_session_id: Some("session-origin".to_string()),
            parent_job_id: None,
            delegation_label: Some("agent-chain:chain-status".to_string()),
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_transcript(&agent_persistence::TranscriptRecord::from(
            &agent_runtime::session::TranscriptEntry::system(
                "transcript-run-chain",
                "session-run-chain",
                None,
                AgentMessageChain::new(
                    "chain-status",
                    "session-origin",
                    "default",
                    DEFAULT_MAX_HOPS,
                    DEFAULT_MAX_HOPS,
                    Some("session-parent".to_string()),
                    AgentChainState::BlockedMaxHops,
                )
                .expect("blocked chain")
                .to_transcript_metadata(),
                10,
            ),
        ))
        .expect("put chain transcript");
    app.grant_session_chain_continuation(
        "session-run-chain",
        "chain-status",
        "Нужен ещё один hop.",
        11,
    )
    .expect("grant continuation");

    let mut run = RunEngine::new("run-chain", "session-run-chain", None, 10);
    run.start(10).expect("start run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");

    let rendered = app
        .render_active_run("session-run-chain")
        .expect("render active run");

    assert!(rendered.contains("межагент: chain_id=chain-status state=blocked_max_hops"));
    assert!(rendered.contains(&format!("hop={}/{}", DEFAULT_MAX_HOPS, DEFAULT_MAX_HOPS)));
    assert!(rendered.contains("origin_session: session-origin"));
    assert!(rendered.contains("origin_agent: default"));
    assert!(rendered.contains("parent_interagent_session: session-parent"));
    assert!(rendered.contains("parent_session: session-origin"));
    assert!(rendered.contains("continuation_grant: pending"));
}

#[test]
fn render_active_run_shows_live_exec_output_tail() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-run-live-output".to_string(),
            title: "Session Run Live Output".to_string(),
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

    let mut runtime = ToolRuntime::with_shared_process_registry(workspace, app.processes.clone());
    let started = runtime
        .invoke(ToolCall::ExecStart(ExecStartInput {
            executable: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'status-stdout\\n'; printf 'status-stderr\\n' >&2; sleep 5".to_string(),
            ],
            cwd: None,
        }))
        .expect("exec_start")
        .into_process_start()
        .expect("process start");

    let mut run = RunEngine::new("run-live-output", "session-run-live-output", None, 10);
    run.start(10).expect("start run");
    run.track_active_process(
        ActiveProcess::new(&started.process_id, "exec", started.pid_ref.clone(), 11)
            .with_command_details(started.command_display, started.cwd),
        11,
    )
    .expect("track active process");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let rendered = app
            .render_active_run("session-run-live-output")
            .expect("render active run");
        if rendered.contains("status-stdout") && rendered.contains("status-stderr") {
            assert!(rendered.contains("вывод:"));
            break;
        }
        let read = app
            .processes
            .read_exec_output(
                &started.process_id,
                ProcessOutputStream::Merged,
                None,
                Some(1024),
                Some(10),
            )
            .expect("read live output");
        if read.text.contains("status-stdout") && read.text.contains("status-stderr") {
            let rendered = app
                .render_active_run("session-run-live-output")
                .expect("render active run");
            assert!(rendered.contains("status-stdout"));
            assert!(rendered.contains("status-stderr"));
            assert!(rendered.contains("вывод:"));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for live output"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn render_system_blocks_include_interagent_section() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-system-chain".to_string(),
            title: "Session System Chain".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "judge".to_string(),
            active_mission_id: None,
            parent_session_id: Some("session-origin".to_string()),
            parent_job_id: None,
            delegation_label: Some("agent-chain:chain-system".to_string()),
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");
    store
        .put_transcript(&agent_persistence::TranscriptRecord::from(
            &agent_runtime::session::TranscriptEntry::system(
                "transcript-system-chain",
                "session-system-chain",
                None,
                AgentMessageChain::new(
                    "chain-system",
                    "session-origin",
                    "default",
                    1,
                    DEFAULT_MAX_HOPS,
                    Some("session-parent".to_string()),
                    AgentChainState::ContinuedOnce,
                )
                .expect("continued chain")
                .to_transcript_metadata(),
                10,
            ),
        ))
        .expect("put chain transcript");
    let mut run = RunEngine::new("run-system-chain", "session-system-chain", None, 20);
    run.start(20).expect("start run");
    run.complete("done", 21).expect("complete run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");
    store
        .put_tool_call(&ToolCallRecord {
            id: "toolcall-system-chain-1".to_string(),
            session_id: "session-system-chain".to_string(),
            run_id: "run-system-chain".to_string(),
            provider_tool_call_id: "call-system-chain-1".to_string(),
            tool_name: "schedule_create".to_string(),
            arguments_json: r#"{"mode":once}"#.to_string(),
            summary: "schedule_create mode=once".to_string(),
            status: "failed".to_string(),
            error: Some("invalid JSON: mode must be quoted".to_string()),
            result_summary: None,
            result_preview: None,
            result_artifact_id: None,
            result_truncated: false,
            result_byte_len: None,
            requested_at: 30,
            updated_at: 31,
        })
        .expect("put tool call");

    let rendered = app
        .render_system_blocks("session-system-chain")
        .expect("render system blocks");

    assert!(rendered.contains("[InterAgent]"));
    assert!(rendered.contains("chain_id=chain-system state=continued_once hop=1"));
    assert!(rendered.contains("max_hops=3"));
    assert!(rendered.contains("origin_session_id=session-origin"));
    assert!(rendered.contains("origin_agent_id=default"));
    assert!(rendered.contains("parent_interagent_session_id=session-parent"));
    assert!(rendered.contains("parent_session_id=session-origin"));
    assert!(rendered.contains("delegation_label=agent-chain:chain-system"));
    assert!(rendered.contains("continuation_grant_pending=false"));
    assert!(rendered.contains("[AutonomyState]"));
    assert!(rendered.contains("Autonomy State:"));
    assert!(rendered.contains("InterAgent Chain: chain-system state=continued_once"));
    assert!(rendered.contains("[RecentToolActivity]"));
    assert!(rendered.contains("Recent Tool Activity:"));
    assert!(rendered.contains("failed schedule_create"));
    assert!(rendered.contains("mode must be quoted"));
}

#[cfg(unix)]
#[test]
fn cancel_latest_session_run_terminates_a_long_running_process() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("Cancel Session"))
        .expect("create session");

    let mut child = Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");

    let mut run = RunEngine::new("run-cancel", &session.id, None, 10);
    run.start(10).expect("start run");
    run.track_active_process(
        ActiveProcess::new("exec-1", "exec", format!("pid:{}", child.id()), 11),
        11,
    )
    .expect("track process");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");

    let message = app
        .cancel_latest_session_run(&session.id, 12)
        .expect("cancel active run");

    assert!(message.contains("остановлен оператором"));

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            assert!(!status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "sleep process was not terminated in time"
        );
        thread::sleep(Duration::from_millis(20));
    }

    let rendered = app
        .render_active_run(&session.id)
        .expect("render active run");
    assert_eq!(rendered, "Ход: активного выполнения нет");
}

#[test]
fn cancel_all_session_work_cancels_runs_jobs_missions_and_queued_wakeups_for_session_tree() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let root = app
        .create_session_auto(Some("Root Session"))
        .expect("create root session");
    let child_summary = app
        .create_session_auto(Some("Child Session"))
        .expect("create child session");

    let mut child = Session::try_from(
        store
            .get_session(&child_summary.id)
            .expect("get child session")
            .expect("child session exists"),
    )
    .expect("child session");
    child.parent_session_id = Some(root.id.clone());
    child.updated_at = 2;
    store
        .put_session(&SessionRecord::try_from(&child).expect("child session record"))
        .expect("put child session");

    let mut child_process = Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");

    let mut root_run = RunEngine::new("run-root-cancel-all", &root.id, None, 10);
    root_run.start(10).expect("start root run");
    root_run
        .track_active_process(
            ActiveProcess::new(
                "exec-root",
                "exec",
                format!("pid:{}", child_process.id()),
                11,
            ),
            11,
        )
        .expect("track root process");
    store
        .put_run(&RunRecord::try_from(root_run.snapshot()).expect("root run record"))
        .expect("put root run");

    let mut child_run = RunEngine::new("run-child-cancel-all", &child.id, None, 10);
    child_run.start(10).expect("start child run");
    store
        .put_run(&RunRecord::try_from(child_run.snapshot()).expect("child run record"))
        .expect("put child run");

    let mut root_job = JobSpec::chat_turn(
        "job-root",
        &root.id,
        Some("run-root-cancel-all"),
        None,
        "hi",
        10,
    );
    root_job.status = JobStatus::Running;
    root_job.started_at = Some(10);
    store
        .put_job(&JobRecord::try_from(&root_job).expect("root job record"))
        .expect("put root job");

    let mut child_job = JobSpec::chat_turn(
        "job-child",
        &child.id,
        Some("run-child-cancel-all"),
        None,
        "hi",
        10,
    );
    child_job.status = JobStatus::Queued;
    store
        .put_job(&JobRecord::try_from(&child_job).expect("child job record"))
        .expect("put child job");

    let mission = MissionSpec {
        id: "mission-root".to_string(),
        session_id: root.id.clone(),
        status: MissionStatus::Running,
        updated_at: 10,
        ..MissionSpec::default()
    };
    store
        .put_mission(&MissionRecord::try_from(&mission).expect("mission record"))
        .expect("put mission");

    let mut root_session = Session::try_from(
        store
            .get_session(&root.id)
            .expect("get root session")
            .expect("root session exists"),
    )
    .expect("root session");
    root_session.active_mission_id = Some("mission-root".to_string());
    root_session.updated_at = 11;
    store
        .put_session(&SessionRecord::try_from(&root_session).expect("root session record"))
        .expect("put root session");

    let inbox = SessionInboxEvent::job_completed(
        "inbox-root-cancel-all",
        &root.id,
        Some("job-root"),
        "queued wakeup",
        10,
    );
    store
        .put_session_inbox_event(
            &agent_persistence::SessionInboxEventRecord::try_from(&inbox).expect("inbox record"),
        )
        .expect("put inbox event");

    let message = app
        .cancel_all_session_work(&root.id, 20)
        .expect("cancel all session work");

    assert!(message.contains("sessions=2"));
    assert!(message.contains("runs=2"));
    assert!(message.contains("jobs=2"));
    assert!(message.contains("missions=1"));
    assert!(message.contains("inbox_events=1"));

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child_process.try_wait().expect("try_wait") {
            assert!(!status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "sleep process was not terminated in time"
        );
        thread::sleep(Duration::from_millis(20));
    }

    let root_run = agent_runtime::run::RunSnapshot::try_from(
        store
            .get_run("run-root-cancel-all")
            .expect("get root run")
            .expect("root run exists"),
    )
    .expect("root run snapshot");
    assert_eq!(root_run.status, agent_runtime::run::RunStatus::Cancelled);

    let child_run = agent_runtime::run::RunSnapshot::try_from(
        store
            .get_run("run-child-cancel-all")
            .expect("get child run")
            .expect("child run exists"),
    )
    .expect("child run snapshot");
    assert_eq!(child_run.status, agent_runtime::run::RunStatus::Cancelled);

    let root_job = JobSpec::try_from(
        store
            .get_job("job-root")
            .expect("get root job")
            .expect("root job exists"),
    )
    .expect("root job");
    assert_eq!(root_job.status, JobStatus::Cancelled);

    let child_job = JobSpec::try_from(
        store
            .get_job("job-child")
            .expect("get child job")
            .expect("child job exists"),
    )
    .expect("child job");
    assert_eq!(child_job.status, JobStatus::Cancelled);

    let mission = MissionSpec::try_from(
        store
            .get_mission("mission-root")
            .expect("get mission")
            .expect("mission exists"),
    )
    .expect("mission");
    assert_eq!(mission.status, MissionStatus::Cancelled);

    let root_session = Session::try_from(
        store
            .get_session(&root.id)
            .expect("get root session")
            .expect("root session exists"),
    )
    .expect("root session");
    assert_eq!(root_session.active_mission_id, None);

    assert!(
        store
            .list_queued_session_inbox_events_for_session(&root.id)
            .expect("list queued inbox events")
            .is_empty()
    );
}

#[test]
fn session_head_usage_fixture_workspace_tree_still_builds() {
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

    let summary = app
        .create_session_auto(Some("Workspace Fixture"))
        .expect("create session");
    let head = app.session_head(&summary.id).expect("session head");

    assert!(head.render().contains("Workspace Tree:"));
}
