use agent_persistence::{
    AppConfig, ContextSummaryRepository, PersistenceStore, RunRecord, RunRepository, SessionRecord,
    SessionRepository, TranscriptRepository,
};
use agent_runtime::run::{ActiveProcess, RunEngine};
use agent_runtime::session::SessionSettings;
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
    assert!(rendered.contains("compaction_manual=true"));
    assert!(rendered.contains("threshold_messages=12"));
    assert!(rendered.contains("keep_tail=4"));
    assert!(rendered.contains("summary_covers_messages=2"));
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
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 1,
            updated_at: 1,
        })
        .expect("put session");

    let mut run = RunEngine::new("run-live", "session-run", None, 10);
    run.start(10).expect("start run");
    run.track_active_process(
        ActiveProcess::new("exec-9", "exec", "pid:4242", 11)
            .with_command_details("curl -fL https://example.test/govc.tar.gz", "/workspace"),
        11,
    )
    .expect("track active process");
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
    assert!(rendered.contains("run-live"));
    assert!(rendered.contains("статус: running"));
    assert!(rendered.contains("usage: input=400 output=40 total=440"));
    assert!(rendered.contains("exec-9 (exec) pid:4242"));
    assert!(rendered.contains("команда: curl -fL https://example.test/govc.tar.gz"));
    assert!(rendered.contains("cwd: /workspace"));
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
