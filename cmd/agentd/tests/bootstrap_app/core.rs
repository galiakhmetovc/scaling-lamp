use super::support::*;

#[test]
fn build_from_config_creates_runtime_layout_from_one_root() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");
    let config = AppConfig {
        data_dir: data_dir.clone(),
        ..AppConfig::default()
    };

    let app = build_from_config(config.clone()).expect("build app");

    assert_eq!(app.config, config);
    assert_eq!(app.persistence.config, config);
    assert!(app.persistence.stores.artifacts_dir.is_dir());
    assert!(app.persistence.stores.runs_dir.is_dir());
    assert!(app.persistence.stores.transcripts_dir.is_dir());
    assert!(app.persistence.audit.path.parent().is_some());
}

#[test]
fn runtime_store_open_does_not_reconcile_orphan_payloads_on_request_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let orphan_path = app
        .persistence
        .stores
        .transcripts_dir
        .join("orphan-transcript.txt");
    fs::write(&orphan_path, "orphan payload").expect("write orphan payload");

    let _store = app.store().expect("open runtime store");

    assert!(
        orphan_path.exists(),
        "request-scoped store opens should not reconcile orphan payloads"
    );
}

#[test]
fn runtime_status_snapshot_reports_counts_without_loading_full_execution_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let connection =
        rusqlite::Connection::open(&app.persistence.stores.metadata_db).expect("open sqlite");
    connection
        .execute(
            "INSERT INTO sessions (
                id, title, prompt_override, settings_json, agent_profile_id,
                active_mission_id, parent_session_id, parent_job_id, delegation_label,
                created_at, updated_at
            ) VALUES (?1, ?2, NULL, ?3, ?4, NULL, NULL, NULL, NULL, ?5, ?6)",
            rusqlite::params![
                "session-status",
                "Status Session",
                serde_json::to_string(&SessionSettings::default()).expect("serialize settings"),
                "default",
                10i64,
                10i64
            ],
        )
        .expect("insert session");
    connection
        .execute(
            "INSERT INTO missions (
                id, session_id, objective, status, execution_intent, schedule_json,
                acceptance_json, created_at, updated_at, completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
            rusqlite::params![
                "mission-status",
                "session-status",
                "Count rows only",
                MissionStatus::Ready.as_str(),
                MissionExecutionIntent::Autonomous.as_str(),
                serde_json::to_string(&MissionSchedule::once()).expect("serialize schedule"),
                "[]",
                11i64,
                11i64
            ],
        )
        .expect("insert mission");
    connection
        .execute(
            "INSERT INTO runs (
                id, session_id, mission_id, status, error, result, provider_usage_json,
                active_processes_json, recent_steps_json, evidence_refs_json,
                pending_approvals_json, provider_loop_json, delegate_runs_json,
                started_at, updated_at, finished_at
            ) VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, NULL)",
            rusqlite::params![
                "run-status",
                "session-status",
                "mission-status",
                RunStatus::Running.as_str(),
                "null",
                "[]",
                "[]",
                "[]",
                "[]",
                "{not-json",
                "[]",
                12i64,
                12i64
            ],
        )
        .expect("insert run");
    connection
        .execute(
            "INSERT INTO jobs (
                id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
                result_json, error, created_at, updated_at, started_at, finished_at,
                attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
                cancel_requested_at, last_progress_message, callback_json, callback_sent_at
            ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, NULL, NULL, ?8, ?9, NULL, NULL, 0, 3, NULL, NULL, NULL, NULL, NULL, ?10, NULL)",
            rusqlite::params![
                "job-status",
                "session-status",
                "mission-status",
                "run-status",
                agent_runtime::mission::JobKind::ChatTurn.as_str(),
                agent_runtime::mission::JobStatus::Queued.as_str(),
                r#"{"message":"status"}"#,
                13i64,
                13i64,
                "{not-json"
            ],
        )
        .expect("insert job");

    let status = app
        .runtime_status_snapshot()
        .expect("status snapshot should use cheap row counts");

    assert_eq!(status.session_count, 1);
    assert_eq!(status.mission_count, 1);
    assert_eq!(status.run_count, 1);
    assert_eq!(status.job_count, 1);
    assert_eq!(status.data_dir, app.config.data_dir.display().to_string());
}

#[test]
fn build_from_config_rejects_invalid_paths_before_side_effects() {
    let temp = tempfile::tempdir().expect("tempdir");
    let occupied_path = temp.path().join("occupied");
    fs::write(&occupied_path, "not a directory").expect("write marker");

    let error = build_from_config(AppConfig {
        data_dir: occupied_path.clone(),
        ..AppConfig::default()
    })
    .expect_err("invalid data dir must fail");

    assert!(matches!(
        error,
        BootstrapError::Config(ConfigError::InvalidDataDir { .. })
    ));
    assert!(!occupied_path.join("artifacts").exists());
}

#[test]
fn write_debug_bundle_persists_session_snapshot_into_runtime_audit_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    app.runtime.workspace = WorkspaceRef::new(temp.path().join("workspace-without-write-contract"));

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    store
        .put_session(&SessionRecord {
            id: "session-debug".to_string(),
            title: "Debug Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("put session");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "tx-debug-1".to_string(),
            session_id: "session-debug".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "debug me".to_string(),
            created_at: 11,
        })
        .expect("put transcript");

    let path = app
        .write_debug_bundle("session-debug")
        .expect("write debug bundle");

    assert!(
        path.starts_with(
            temp.path()
                .join("state-root")
                .join("audit")
                .join("debug-bundles")
        )
    );
    assert!(path.is_file());

    let bundle = fs::read_to_string(&path).expect("read bundle");
    assert!(bundle.contains("Debug Bundle"));
    assert!(bundle.contains("session_id=session-debug"));
    assert!(bundle.contains("Context:"));
    assert!(bundle.contains("Plan:"));
    assert!(bundle.contains("Transcript Tail:"));
}

#[test]
fn run_with_args_creates_and_shows_sessions_and_missions() {
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

    let created_session = app
        .run_with_args([
            "session",
            "create",
            "session-1",
            "Autonomous",
            "CLI",
            "session",
        ])
        .expect("create session");
    assert!(created_session.contains("created session session-1"));

    let shown_session = app
        .run_with_args(["session", "show", "session-1"])
        .expect("show session");
    assert!(shown_session.contains("session-1"));
    assert!(shown_session.contains("Autonomous CLI session"));

    let created_mission = app
        .run_with_args([
            "mission",
            "create",
            "mission-1",
            "session-1",
            "Ship",
            "the",
            "autonomous",
            "supervisor",
        ])
        .expect("create mission");
    assert!(created_mission.contains("created mission mission-1"));

    let shown_mission = app
        .run_with_args(["mission", "show", "mission-1"])
        .expect("show mission");
    assert!(shown_mission.contains("mission-1"));
    assert!(shown_mission.contains("session-1"));
    assert!(shown_mission.contains("Ship the autonomous supervisor"));

    let status = app.run_with_args(["status"]).expect("status");
    assert!(status.contains("permission_mode=default"));
    assert!(status.contains("sessions=1"));
    assert!(status.contains("missions=1"));

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    assert!(
        store
            .get_session("session-1")
            .expect("load session")
            .is_some()
    );
    assert!(
        store
            .get_mission("mission-1")
            .expect("load mission")
            .is_some()
    );
}

#[test]
fn run_with_args_inspects_and_updates_runs_jobs_approvals_and_delegates() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-ops".to_string(),
            title: "Operator session".to_string(),
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
        .put_mission(&MissionRecord {
            id: "mission-ops".to_string(),
            session_id: "session-ops".to_string(),
            objective: "Handle operator flows".to_string(),
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

    let mut approval_run = RunEngine::new("run-approval", "session-ops", Some("mission-ops"), 3);
    approval_run.start(4).expect("start run");
    approval_run
        .wait_for_approval(
            ApprovalRequest::new("approval-1", "tool-call-1", "allow exec", 5),
            5,
        )
        .expect("wait for approval");
    let mut evidence = EvidenceBundle::new("bundle-1", "run-approval", 6);
    evidence
        .record_check("fmt", CheckOutcome::Passed, Some("clean"), 6)
        .expect("record fmt");
    approval_run
        .record_evidence(&evidence, 6)
        .expect("record evidence");
    store
        .put_run(&RunRecord::try_from(approval_run.snapshot()).expect("run record"))
        .expect("put approval run");

    let mut delegate_run = RunEngine::new("run-delegate", "session-ops", Some("mission-ops"), 7);
    delegate_run.start(8).expect("start delegate run");
    delegate_run
        .wait_for_delegate(DelegateRun::new("delegate-1", "worker-a", 9), 9)
        .expect("wait for delegate");
    store
        .put_run(&RunRecord::try_from(delegate_run.snapshot()).expect("delegate record"))
        .expect("put delegate run");

    let job = JobSpec::mission_turn(
        "job-1",
        "session-ops",
        "mission-ops",
        Some("run-approval"),
        None,
        "Handle operator flows",
        10,
    );
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");

    let run_show = app
        .run_with_args(["run", "show", "run-approval"])
        .expect("show run");
    assert!(run_show.contains("run-approval"));
    assert!(run_show.contains("waiting_approval"));
    assert!(run_show.contains("pending_approvals=1"));

    let approval_list = app
        .run_with_args(["approval", "list", "run-approval"])
        .expect("list approvals");
    assert!(approval_list.contains("approval-1"));
    assert!(approval_list.contains("tool-call-1"));

    let verification_show = app
        .run_with_args(["verification", "show", "run-approval"])
        .expect("show verification");
    assert!(verification_show.contains("bundle:bundle-1"));
    assert!(verification_show.contains("check:fmt"));

    let delegate_list = app
        .run_with_args(["delegate", "list", "run-delegate"])
        .expect("list delegates");
    assert!(delegate_list.contains("delegate-1"));
    assert!(delegate_list.contains("worker-a"));

    let job_show = app
        .run_with_args(["job", "show", "job-1"])
        .expect("show job");
    assert!(job_show.contains("job-1"));
    assert!(job_show.contains("mission_turn"));

    let approval_update = app
        .run_with_args(["approval", "approve", "run-approval", "approval-1"])
        .expect("approve");
    assert!(approval_update.contains("approved approval-1"));

    let updated_run = app
        .run_with_args(["run", "show", "run-approval"])
        .expect("show updated run");
    assert!(updated_run.contains("status=resuming"));
    assert!(updated_run.contains("pending_approvals=0"));

    let persisted = store
        .get_run("run-approval")
        .expect("get updated run")
        .expect("run record exists");
    let snapshot = RunSnapshot::try_from(persisted).expect("snapshot");
    assert_eq!(snapshot.status, RunStatus::Resuming);
    assert!(snapshot.pending_approvals.is_empty());
}

#[test]
fn session_summary_ignores_corrupt_unrelated_session_records() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-good".to_string(),
            title: "Good Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("put good session");

    let connection =
        rusqlite::Connection::open(&app.persistence.stores.metadata_db).expect("open sqlite");
    connection
        .execute(
            "INSERT INTO sessions (
                id, title, prompt_override, settings_json, agent_profile_id,
                active_mission_id, parent_session_id, parent_job_id, delegation_label,
                created_at, updated_at
            ) VALUES (?1, ?2, NULL, ?3, ?4, NULL, NULL, NULL, NULL, ?5, ?6)",
            rusqlite::params![
                "session-bad",
                "Broken Session",
                "{not-json",
                "default",
                11i64,
                11i64
            ],
        )
        .expect("insert corrupt session row");

    let summary = app
        .session_summary("session-good")
        .expect("session summary should ignore unrelated corrupt rows");

    assert_eq!(summary.id, "session-good");
    assert_eq!(summary.title, "Good Session");
}

#[test]
fn session_summary_does_not_require_all_transcript_payloads_for_same_session() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-good".to_string(),
            title: "Good Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("put good session");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "transcript-old".to_string(),
            session_id: "session-good".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "first".to_string(),
            created_at: 11,
        })
        .expect("put old transcript");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "transcript-new".to_string(),
            session_id: "session-good".to_string(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "second".to_string(),
            created_at: 12,
        })
        .expect("put new transcript");

    std::fs::remove_file(
        app.persistence
            .stores
            .transcripts_dir
            .join("session-good")
            .join("transcript-old.txt"),
    )
    .expect("remove old transcript payload");

    let summary = app
        .session_summary("session-good")
        .expect("session summary should not require every transcript payload");

    assert_eq!(summary.id, "session-good");
    assert_eq!(summary.title, "Good Session");
    assert_eq!(summary.message_count, 2);
    assert_eq!(summary.last_message_preview, None);
}

#[test]
fn create_session_succeeds_even_with_corrupt_unrelated_session_records() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");

    let connection =
        rusqlite::Connection::open(&app.persistence.stores.metadata_db).expect("open sqlite");
    connection
        .execute(
            "INSERT INTO sessions (
                id, title, prompt_override, settings_json, agent_profile_id,
                active_mission_id, parent_session_id, parent_job_id, delegation_label,
                created_at, updated_at
            ) VALUES (?1, ?2, NULL, ?3, ?4, NULL, NULL, NULL, NULL, ?5, ?6)",
            rusqlite::params![
                "session-bad",
                "Broken Session",
                "{not-json",
                "default",
                11i64,
                11i64
            ],
        )
        .expect("insert corrupt session row");

    let summary = app
        .create_session("session-new", "Fresh Session")
        .expect("create session should not depend on unrelated corrupt rows");

    assert_eq!(summary.id, "session-new");
    assert_eq!(summary.title, "Fresh Session");
}

#[test]
fn pending_approvals_ignores_corrupt_unrelated_run_records() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-good".to_string(),
            title: "Good Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("put good session");

    let mut run = RunEngine::new("run-good", "session-good", None, 10);
    run.start(10).expect("start run");
    let approval = ApprovalRequest::new(
        "approval-good".to_string(),
        "tool-call-1".to_string(),
        "tool call approval".to_string(),
        11,
    );
    run.wait_for_approval(approval.clone(), 11)
        .expect("waiting approval");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put good run");

    let connection =
        rusqlite::Connection::open(&app.persistence.stores.metadata_db).expect("open sqlite");
    connection
        .execute(
            "INSERT INTO sessions (
                id, title, prompt_override, settings_json, agent_profile_id,
                active_mission_id, parent_session_id, parent_job_id, delegation_label,
                created_at, updated_at
            ) VALUES (?1, ?2, NULL, ?3, ?4, NULL, NULL, NULL, NULL, ?5, ?6)",
            rusqlite::params![
                "session-bad",
                "Broken Session",
                serde_json::to_string(&SessionSettings::default()).expect("serialize settings"),
                "default",
                11i64,
                11i64
            ],
        )
        .expect("insert unrelated session");
    connection
        .execute(
            "INSERT INTO runs (
                id, session_id, mission_id, status, error, result, provider_usage_json,
                active_processes_json, recent_steps_json, evidence_refs_json,
                pending_approvals_json, provider_loop_json, delegate_runs_json,
                started_at, updated_at, finished_at
            ) VALUES (?1, ?2, NULL, ?3, NULL, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL)",
            rusqlite::params![
                "run-bad",
                "session-bad",
                "waiting_approval",
                "null",
                "[]",
                "[]",
                "[]",
                "{not-json",
                "null",
                "[]",
                12i64,
                12i64
            ],
        )
        .expect("insert corrupt unrelated run row");

    let pending = app
        .pending_approvals("session-good")
        .expect("pending approvals should ignore unrelated corrupt runs");

    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].run_id, "run-good");
    assert_eq!(pending[0].approval_id, "approval-good");
}

#[test]
fn list_session_summaries_ignores_corrupt_unrelated_session_records() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-good".to_string(),
            title: "Good Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("put good session");

    let connection =
        rusqlite::Connection::open(&app.persistence.stores.metadata_db).expect("open sqlite");
    connection
        .execute(
            "INSERT INTO sessions (
                id, title, prompt_override, settings_json, agent_profile_id,
                active_mission_id, parent_session_id, parent_job_id, delegation_label,
                created_at, updated_at
            ) VALUES (?1, ?2, NULL, ?3, ?4, NULL, NULL, NULL, NULL, ?5, ?6)",
            rusqlite::params![
                "session-bad",
                "Broken Session",
                "{not-json",
                "default",
                11i64,
                11i64
            ],
        )
        .expect("insert corrupt session row");

    let summaries = app
        .list_session_summaries()
        .expect("session list should ignore unrelated corrupt rows");

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "session-good");
    assert_eq!(summaries[0].title, "Good Session");
}

#[test]
fn list_session_summaries_does_not_require_transcript_payloads_for_same_session() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-good".to_string(),
            title: "Good Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("put good session");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "transcript-old".to_string(),
            session_id: "session-good".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "first".to_string(),
            created_at: 11,
        })
        .expect("put old transcript");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "transcript-new".to_string(),
            session_id: "session-good".to_string(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "second".to_string(),
            created_at: 12,
        })
        .expect("put new transcript");

    std::fs::remove_file(
        app.persistence
            .stores
            .transcripts_dir
            .join("session-good")
            .join("transcript-old.txt"),
    )
    .expect("remove old transcript payload");

    let summaries = app
        .list_session_summaries()
        .expect("session list should not require every transcript payload");

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "session-good");
    assert_eq!(summaries[0].title, "Good Session");
    assert_eq!(summaries[0].message_count, 2);
    assert_eq!(summaries[0].last_message_preview, None);
}

#[test]
fn list_session_summaries_uses_run_rollups_without_full_run_record_conversion() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-good".to_string(),
            title: "Good Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("put good session");

    let connection =
        rusqlite::Connection::open(&app.persistence.stores.metadata_db).expect("open sqlite");
    connection
        .execute(
            "INSERT INTO runs (
                id, session_id, mission_id, status, error, result, provider_usage_json,
                active_processes_json, recent_steps_json, evidence_refs_json,
                pending_approvals_json, provider_loop_json, delegate_runs_json,
                started_at, updated_at, finished_at
            ) VALUES (?1, ?2, NULL, ?3, NULL, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL)",
            rusqlite::params![
                "run-heavy",
                "session-good",
                agent_runtime::run::RunStatus::Running.as_str(),
                r#"{"input_tokens":123,"output_tokens":9,"total_tokens":132}"#,
                "[]",
                "[]",
                "[]",
                r#"[{"id":"approval-1","tool_call_id":"tool-1","summary":"need approval","requested_at":12}]"#,
                "{not-json",
                "[]",
                11i64,
                12i64
            ],
        )
        .expect("insert partially corrupt run row");

    let summaries = app
        .list_session_summaries()
        .expect("session list should not require full run conversion");

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "session-good");
    assert_eq!(summaries[0].usage_input_tokens, Some(123));
    assert!(summaries[0].has_pending_approval);
    assert!(summaries[0].updated_at >= 12);
}

#[test]
fn list_session_summaries_uses_job_counts_without_full_job_record_conversion() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-good".to_string(),
            title: "Good Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("put good session");

    let connection =
        rusqlite::Connection::open(&app.persistence.stores.metadata_db).expect("open sqlite");
    connection
        .execute(
            "INSERT INTO jobs (
                id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
                result_json, error, created_at, updated_at, started_at, finished_at,
                attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
                cancel_requested_at, last_progress_message, callback_json, callback_sent_at
            ) VALUES (?1, ?2, NULL, NULL, NULL, ?3, ?4, ?5, NULL, NULL, ?6, ?7, ?8, NULL, 0, 3, NULL, NULL, NULL, NULL, NULL, ?9, NULL)",
            rusqlite::params![
                "job-heavy",
                "session-good",
                agent_runtime::mission::JobKind::ChatTurn.as_str(),
                agent_runtime::mission::JobStatus::Running.as_str(),
                r#"{"message":"hello"}"#,
                11i64,
                12i64,
                12i64,
                "{not-json"
            ],
        )
        .expect("insert partially corrupt job row");

    let summaries = app
        .list_session_summaries()
        .expect("session list should not require full job conversion");

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "session-good");
    assert_eq!(summaries[0].background_job_count, 1);
    assert_eq!(summaries[0].running_background_job_count, 1);
    assert_eq!(summaries[0].queued_background_job_count, 0);
}

#[test]
fn session_summary_uses_run_rollups_without_full_run_record_conversion() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-good".to_string(),
            title: "Good Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("put good session");

    let connection =
        rusqlite::Connection::open(&app.persistence.stores.metadata_db).expect("open sqlite");
    connection
        .execute(
            "INSERT INTO runs (
                id, session_id, mission_id, status, error, result, provider_usage_json,
                active_processes_json, recent_steps_json, evidence_refs_json,
                pending_approvals_json, provider_loop_json, delegate_runs_json,
                started_at, updated_at, finished_at
            ) VALUES (?1, ?2, NULL, ?3, NULL, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL)",
            rusqlite::params![
                "run-heavy",
                "session-good",
                agent_runtime::run::RunStatus::Running.as_str(),
                r#"{"input_tokens":123,"output_tokens":9,"total_tokens":132}"#,
                "[]",
                "[]",
                "[]",
                r#"[{"id":"approval-1","tool_call_id":"tool-1","summary":"need approval","requested_at":12}]"#,
                "{not-json",
                "[]",
                11i64,
                12i64
            ],
        )
        .expect("insert partially corrupt run row");

    let summary = app
        .session_summary("session-good")
        .expect("single session summary should not require full run conversion");

    assert_eq!(summary.id, "session-good");
    assert_eq!(summary.usage_input_tokens, Some(123));
    assert!(summary.has_pending_approval);
    assert!(summary.updated_at >= 12);
}

#[test]
fn build_from_config_interrupts_unrecoverable_runs_but_keeps_approvals_pending() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");
    let app = build_from_config(AppConfig {
        data_dir: data_dir.clone(),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-recovery".to_string(),
            title: "Recovery session".to_string(),
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
        .put_mission(&MissionRecord {
            id: "mission-recovery".to_string(),
            session_id: "session-recovery".to_string(),
            objective: "Recover autonomous work".to_string(),
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

    for record in [
        RunRecord {
            id: "run-running".to_string(),
            session_id: "session-recovery".to_string(),
            mission_id: Some("mission-recovery".to_string()),
            status: RunStatus::Running.as_str().to_string(),
            error: None,
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 3,
            updated_at: 4,
            finished_at: None,
        },
        RunRecord {
            id: "run-resuming".to_string(),
            session_id: "session-recovery".to_string(),
            mission_id: Some("mission-recovery".to_string()),
            status: RunStatus::Resuming.as_str().to_string(),
            error: None,
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 5,
            updated_at: 6,
            finished_at: None,
        },
        RunRecord {
            id: "run-process".to_string(),
            session_id: "session-recovery".to_string(),
            mission_id: Some("mission-recovery".to_string()),
            status: RunStatus::WaitingProcess.as_str().to_string(),
            error: None,
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 7,
            updated_at: 8,
            finished_at: None,
        },
        RunRecord {
            id: "run-delegate".to_string(),
            session_id: "session-recovery".to_string(),
            mission_id: Some("mission-recovery".to_string()),
            status: RunStatus::WaitingDelegate.as_str().to_string(),
            error: None,
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: serde_json::to_string(&vec![DelegateRun::new(
                "delegate-1",
                "worker-a",
                9,
            )])
            .expect("serialize delegates"),
            started_at: 9,
            updated_at: 10,
            finished_at: None,
        },
        RunRecord {
            id: "run-approval".to_string(),
            session_id: "session-recovery".to_string(),
            mission_id: Some("mission-recovery".to_string()),
            status: RunStatus::WaitingApproval.as_str().to_string(),
            error: None,
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: serde_json::to_string(&vec![ApprovalRequest::new(
                "approval-1",
                "tool-call-1",
                "allow exec",
                11,
            )])
            .expect("serialize approvals"),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 11,
            updated_at: 12,
            finished_at: None,
        },
    ] {
        store.put_run(&record).expect("put run");
    }

    drop(store);
    drop(app);

    let reopened = build_from_config(AppConfig {
        data_dir,
        ..AppConfig::default()
    })
    .expect("reopen app");
    let reopened_store = PersistenceStore::open(&reopened.persistence).expect("reopen store");

    for run_id in ["run-running", "run-resuming", "run-process", "run-delegate"] {
        let interrupted = RunSnapshot::try_from(
            reopened_store
                .get_run(run_id)
                .expect("get interrupted run")
                .expect("interrupted run exists"),
        )
        .expect("interrupted snapshot");
        assert_eq!(interrupted.status, RunStatus::Interrupted);
        assert_eq!(
            interrupted.error.as_deref(),
            Some("runtime restart interrupted a non-recoverable run state")
        );
    }

    let pending = RunSnapshot::try_from(
        reopened_store
            .get_run("run-approval")
            .expect("get approval run")
            .expect("approval run exists"),
    )
    .expect("approval snapshot");
    assert_eq!(pending.status, RunStatus::WaitingApproval);
    assert_eq!(pending.pending_approvals.len(), 1);
}

#[test]
fn client_build_from_config_does_not_interrupt_active_runs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");
    let app = build_from_config(AppConfig {
        data_dir: data_dir.clone(),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-client-recovery".to_string(),
            title: "Client recovery session".to_string(),
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
        .put_run(&RunRecord {
            id: "run-client-active".to_string(),
            session_id: "session-client-recovery".to_string(),
            mission_id: None,
            status: RunStatus::Running.as_str().to_string(),
            error: None,
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 3,
            updated_at: 4,
            finished_at: None,
        })
        .expect("put run");

    drop(store);
    drop(app);

    let client_app = build_from_config_without_recovery(AppConfig {
        data_dir,
        ..AppConfig::default()
    })
    .expect("build client app");
    let client_store = PersistenceStore::open_runtime(&client_app.persistence).expect("open store");
    let active = RunSnapshot::try_from(
        client_store
            .get_run("run-client-active")
            .expect("get active run")
            .expect("active run exists"),
    )
    .expect("active snapshot");

    assert_eq!(active.status, RunStatus::Running);
    assert_eq!(active.error, None);
    assert_eq!(active.finished_at, None);
}

#[test]
fn client_build_from_config_without_recovery_bootstraps_empty_store() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = temp.path().join("state-root");

    let app = build_from_config_without_recovery(AppConfig {
        data_dir,
        ..AppConfig::default()
    })
    .expect("build client app");
    let store = PersistenceStore::open_runtime(&app.persistence).expect("open runtime store");

    assert!(
        store
            .get_agent_profile("default")
            .expect("get default profile")
            .is_some()
    );
}

#[test]
fn run_show_surfaces_error_details_for_interrupted_runs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-show".to_string(),
            title: "Show session".to_string(),
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
        .put_run(&RunRecord {
            id: "run-interrupted".to_string(),
            session_id: "session-show".to_string(),
            mission_id: None,
            status: RunStatus::Interrupted.as_str().to_string(),
            error: Some("runtime restart interrupted a non-recoverable run state".to_string()),
            result: None,
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 3,
            updated_at: 4,
            finished_at: Some(4),
        })
        .expect("put run");

    let shown = app
        .run_with_args(["run", "show", "run-interrupted"])
        .expect("show run");
    assert!(shown.contains("status=interrupted"));
    assert!(shown.contains("error=runtime restart interrupted a non-recoverable run state"));
}

#[test]
fn run_with_args_provider_smoke_uses_the_configured_driver() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_123",
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
                                "text":"hello world"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":11,"output_tokens":7,"total_tokens":18}
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

    let output = app
        .run_with_args(["provider", "smoke", "Say", "hi"])
        .expect("provider smoke");
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert!(output.contains("provider name=openai-responses"));
    assert!(output.contains("response_id=resp_123"));
    assert!(output.contains("model=gpt-5.4"));
    assert!(output.contains("output=hello world"));

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/v1/responses"));
    assert!(normalized_request.contains("\"text\":\"say hi\""));
}

#[test]
fn execute_chat_turn_uses_the_session_model_override() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_model_override",
                "model":"glm-5-air",
                "output":[
                    {
                        "id":"msg_1",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"model override ok"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":5,"output_tokens":4,"total_tokens":9}
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
    let session = app
        .create_session_auto(Some("Model override session"))
        .expect("create session");
    app.update_session_preferences(
        &session.id,
        SessionPreferencesPatch {
            model: Some(Some("glm-5-air".to_string())),
            ..SessionPreferencesPatch::default()
        },
    )
    .expect("update model");

    let report = app
        .execute_chat_turn(&session.id, "hello model override", 100)
        .expect("chat turn");
    let raw_request = requests.recv().expect("provider request");
    handle.join().expect("join provider");

    assert_eq!(report.output_text, "model override ok");
    assert!(raw_request.contains("\"model\":\"glm-5-air\""));
}

#[test]
fn supervisor_tick_queues_due_mission_turn_jobs_from_persisted_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-queue".to_string(),
            title: "Queue session".to_string(),
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
        .put_mission(&MissionRecord {
            id: "mission-queue".to_string(),
            session_id: "session-queue".to_string(),
            objective: "Queue a mission turn".to_string(),
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

    let report = app.supervisor_tick(60, &[]).expect("run supervisor tick");

    assert_eq!(
        report.actions,
        vec![SupervisorAction::QueueJob(Box::new(JobSpec::mission_turn(
            "mission-queue-mission-turn-60",
            "session-queue",
            "mission-queue",
            None,
            None,
            "Queue a mission turn",
            60,
        )))]
    );

    let queued_job = store
        .get_job("mission-queue-mission-turn-60")
        .expect("get queued job")
        .expect("queued job exists");
    assert_eq!(queued_job.status, "queued");
    assert_eq!(queued_job.created_at, 60);
}

#[test]
fn supervisor_tick_dispatches_queued_jobs_and_completes_verified_missions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-ops".to_string(),
            title: "Execution session".to_string(),
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
        .put_mission(&MissionRecord {
            id: "mission-ready".to_string(),
            session_id: "session-ops".to_string(),
            objective: "Dispatch work".to_string(),
            status: MissionStatus::Ready.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 2,
            completed_at: None,
        })
        .expect("put ready mission");
    store
        .put_job(
            &JobRecord::try_from(&JobSpec::mission_turn(
                "job-dispatch",
                "session-ops",
                "mission-ready",
                None,
                None,
                "Dispatch work",
                10,
            ))
            .expect("job record"),
        )
        .expect("put queued job");

    store
        .put_mission(&MissionRecord {
            id: "mission-done".to_string(),
            session_id: "session-ops".to_string(),
            objective: "Complete work".to_string(),
            status: MissionStatus::Running.as_str().to_string(),
            execution_intent: MissionExecutionIntent::Autonomous.as_str().to_string(),
            schedule_json: serde_json::to_string(&MissionSchedule::once())
                .expect("serialize schedule"),
            acceptance_json: "[]".to_string(),
            created_at: 3,
            updated_at: 3,
            completed_at: None,
        })
        .expect("put running mission");
    store
        .put_run(&RunRecord {
            id: "run-done".to_string(),
            session_id: "session-ops".to_string(),
            mission_id: Some("mission-done".to_string()),
            status: RunStatus::Completed.as_str().to_string(),
            error: None,
            result: Some("done".to_string()),
            provider_usage_json: "null".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 20,
            updated_at: 21,
            finished_at: Some(21),
        })
        .expect("put completed run");

    let report = app
        .supervisor_tick(
            90,
            &[MissionVerificationSummary {
                mission_id: "mission-done".to_string(),
                status: VerificationStatus::Passed,
                missing_required_checks: Vec::new(),
                open_risks: Vec::new(),
            }],
        )
        .expect("run supervisor tick");

    assert!(report.actions.contains(&SupervisorAction::DispatchJob {
        job_id: "job-dispatch".to_string(),
        kind: agent_runtime::mission::JobKind::MissionTurn,
    }));
    assert!(report.actions.contains(&SupervisorAction::CompleteMission {
        mission_id: "mission-done".to_string(),
    }));

    let dispatched_job = store
        .get_job("job-dispatch")
        .expect("get dispatched job")
        .expect("dispatched job exists");
    assert_eq!(dispatched_job.status, "running");
    assert_eq!(dispatched_job.started_at, Some(90));

    let completed_mission = store
        .get_mission("mission-done")
        .expect("get completed mission")
        .expect("completed mission exists");
    assert_eq!(completed_mission.status, "completed");
    assert_eq!(completed_mission.completed_at, Some(90));
}

#[test]
fn execute_mission_turn_job_creates_a_run_calls_provider_and_persists_transcript() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_456",
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
                                "text":"Mission result"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":15,"output_tokens":5,"total_tokens":20}
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
            id: "session-turn".to_string(),
            title: "Mission turn session".to_string(),
            prompt_override: Some("Reply tersely.".to_string()),
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
        .put_mission(&MissionRecord {
            id: "mission-turn".to_string(),
            session_id: "session-turn".to_string(),
            objective: "Ship one provider-backed mission turn".to_string(),
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
                "job-turn",
                "session-turn",
                "mission-turn",
                None,
                None,
                "Draft a short mission update",
                3,
            ))
            .expect("job record"),
        )
        .expect("put job");

    let report = app
        .execute_mission_turn_job("job-turn", 10)
        .expect("execute mission turn");
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert_eq!(report.run_id, "run-job-turn");
    assert_eq!(report.response_id, "resp_456");
    assert_eq!(report.output_text, "Mission result");

    let run = store
        .get_run("run-job-turn")
        .expect("get run")
        .expect("run exists");
    assert_eq!(run.status, "completed");
    assert_eq!(run.result.as_deref(), Some("Mission result"));

    let job = store
        .get_job("job-turn")
        .expect("get job")
        .expect("job exists");
    assert_eq!(job.status, "completed");
    assert_eq!(job.run_id.as_deref(), Some("run-job-turn"));
    assert_eq!(job.finished_at, Some(10));

    let mission = store
        .get_mission("mission-turn")
        .expect("get mission")
        .expect("mission exists");
    assert_eq!(mission.status, "running");

    let transcripts = store
        .list_transcripts_for_session("session-turn")
        .expect("list transcripts");
    assert_eq!(transcripts.len(), 2);
    assert_eq!(transcripts[0].kind, "user");
    assert_eq!(transcripts[0].content, "Draft a short mission update");
    assert_eq!(transcripts[1].kind, "assistant");
    assert_eq!(transcripts[1].content, "Mission result");

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/v1/responses"));
    assert!(normalized_request.contains("\"instructions\":\"reply tersely.\""));
    assert!(normalized_request.contains("\"text\":\"draft a short mission update\""));
}

#[test]
fn tool_execution_pauses_for_approval_then_resumes_and_records_evidence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-tool".to_string(),
            title: "Tool session".to_string(),
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
        .put_mission(&MissionRecord {
            id: "mission-tool".to_string(),
            session_id: "session-tool".to_string(),
            objective: "Drive an approval-gated tool".to_string(),
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
        "job-tool",
        "session-tool",
        "mission-tool",
        Some("run-tool"),
        None,
        "Write a mission artifact",
        3,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.started_at = Some(4);
    job.updated_at = 4;

    let mut run = RunEngine::new("run-tool", "session-tool", Some("mission-tool"), 4);
    run.start(4).expect("start run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");

    let tool_call = ToolCall::FsWrite(FsWriteInput {
        path: "notes/out.txt".to_string(),
        content: "tool output\n".to_string(),
    });

    let approval = app
        .request_tool_approval("job-tool", "run-tool", &tool_call, 20)
        .expect("request approval");
    assert_eq!(approval.run_status, RunStatus::WaitingApproval);
    assert_eq!(
        approval.approval_id.as_deref(),
        Some("approval-job-tool-fs_write")
    );

    let waiting_run = RunSnapshot::try_from(
        store
            .get_run("run-tool")
            .expect("get waiting run")
            .expect("waiting run exists"),
    )
    .expect("waiting snapshot");
    assert_eq!(waiting_run.status, RunStatus::WaitingApproval);
    assert_eq!(waiting_run.pending_approvals.len(), 1);

    let blocked_job = store
        .get_job("job-tool")
        .expect("get blocked job")
        .expect("blocked job exists");
    assert_eq!(blocked_job.status, "blocked");
    assert!(!workspace_root.join("notes/out.txt").exists());

    let mut evidence = EvidenceBundle::new("bundle-tool", "run-tool", 21);
    evidence
        .record_check("fmt", CheckOutcome::Passed, Some("clean"), 21)
        .expect("record fmt");
    evidence
        .record_check("clippy", CheckOutcome::Passed, Some("clean"), 21)
        .expect("record clippy");
    evidence
        .record_check("test", CheckOutcome::Passed, Some("green"), 21)
        .expect("record test");
    evidence.add_artifact_ref("artifact:notes/out.txt");

    let resumed = app
        .resume_tool_call(execution::ToolResumeRequest {
            job_id: "job-tool",
            run_id: "run-tool",
            approval_id: approval.approval_id.as_deref().expect("approval id"),
            tool_call: &tool_call,
            workspace_root: &workspace_root,
            evidence: Some(&evidence),
            now: 21,
        })
        .expect("resume tool call");
    assert_eq!(resumed.run_status, RunStatus::Completed);
    assert_eq!(
        resumed.output_summary.as_deref(),
        Some("fs_write path=notes/out.txt bytes=12")
    );
    assert!(
        resumed
            .evidence_refs
            .contains(&"bundle:bundle-tool".to_string())
    );
    assert!(resumed.evidence_refs.contains(&"check:fmt".to_string()));
    assert!(resumed.evidence_refs.contains(&"check:clippy".to_string()));
    assert!(resumed.evidence_refs.contains(&"check:test".to_string()));
    assert!(
        resumed
            .evidence_refs
            .contains(&"artifact:notes/out.txt".to_string())
    );

    assert_eq!(
        fs::read_to_string(workspace_root.join("notes/out.txt")).expect("read workspace file"),
        "tool output\n"
    );

    let completed_run = RunSnapshot::try_from(
        store
            .get_run("run-tool")
            .expect("get completed run")
            .expect("completed run exists"),
    )
    .expect("completed snapshot");
    assert_eq!(completed_run.status, RunStatus::Completed);
    assert!(completed_run.pending_approvals.is_empty());
    assert_eq!(
        completed_run.result.as_deref(),
        Some("fs_write path=notes/out.txt bytes=12")
    );
    assert!(
        completed_run
            .evidence_refs
            .contains(&"bundle:bundle-tool".to_string())
    );

    let completed_job = store
        .get_job("job-tool")
        .expect("get completed job")
        .expect("completed job exists");
    assert_eq!(completed_job.status, "completed");
    assert_eq!(
        completed_job.result_json.as_deref(),
        Some(r#"{"Summary":{"outcome":"fs_write path=notes/out.txt bytes=12"}}"#)
    );

    let mission = store
        .get_mission("mission-tool")
        .expect("get mission")
        .expect("mission exists");
    assert_eq!(mission.status, "running");
    assert_eq!(mission.updated_at, 21);
}

#[test]
fn accept_edits_mode_skips_approval_for_filesystem_edits() {
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

    store
        .put_session(&SessionRecord {
            id: "session-allow".to_string(),
            title: "Allow session".to_string(),
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
        .put_mission(&MissionRecord {
            id: "mission-allow".to_string(),
            session_id: "session-allow".to_string(),
            objective: "Allow edit".to_string(),
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
        "job-allow",
        "session-allow",
        "mission-allow",
        Some("run-allow"),
        None,
        "Write without approval",
        3,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.started_at = Some(4);
    job.updated_at = 4;

    let mut run = RunEngine::new("run-allow", "session-allow", Some("mission-allow"), 4);
    run.start(4).expect("start run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");

    let tool_call = ToolCall::FsWrite(FsWriteInput {
        path: "notes/out.txt".to_string(),
        content: "allowed\n".to_string(),
    });

    let report = app
        .request_tool_approval("job-allow", "run-allow", &tool_call, 20)
        .expect("request tool gate");
    assert_eq!(report.run_status, RunStatus::Running);
    assert_eq!(report.approval_id, None);

    let run_snapshot = RunSnapshot::try_from(
        store
            .get_run("run-allow")
            .expect("get run")
            .expect("run exists"),
    )
    .expect("run snapshot");
    assert_eq!(run_snapshot.status, RunStatus::Running);
    assert!(run_snapshot.pending_approvals.is_empty());
}

#[test]
fn deny_rule_fails_tool_execution_before_approval_is_created() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        permissions: PermissionConfig {
            mode: PermissionMode::AcceptEdits,
            rules: vec![PermissionRule {
                action: PermissionAction::Deny,
                tool: Some("fs_write".to_string()),
                family: None,
                path_prefix: Some("secrets/".to_string()),
            }],
        },
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-deny".to_string(),
            title: "Deny session".to_string(),
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
        .put_mission(&MissionRecord {
            id: "mission-deny".to_string(),
            session_id: "session-deny".to_string(),
            objective: "Deny edit".to_string(),
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
        "job-deny",
        "session-deny",
        "mission-deny",
        Some("run-deny"),
        None,
        "Write forbidden file",
        3,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.started_at = Some(4);
    job.updated_at = 4;

    let mut run = RunEngine::new("run-deny", "session-deny", Some("mission-deny"), 4);
    run.start(4).expect("start run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");

    let tool_call = ToolCall::FsWrite(FsWriteInput {
        path: "secrets/out.txt".to_string(),
        content: "denied\n".to_string(),
    });

    let error = app
        .request_tool_approval("job-deny", "run-deny", &tool_call, 20)
        .expect_err("deny rule must fail");
    assert!(error.to_string().contains("permission denied"));

    let run_snapshot = RunSnapshot::try_from(
        store
            .get_run("run-deny")
            .expect("get run")
            .expect("run exists"),
    )
    .expect("run snapshot");
    assert_eq!(run_snapshot.status, RunStatus::Failed);
    assert!(run_snapshot.pending_approvals.is_empty());

    let failed_job = store
        .get_job("job-deny")
        .expect("get job")
        .expect("job exists");
    assert_eq!(failed_job.status, "failed");
    assert!(
        failed_job
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("secrets/")
    );
}

#[test]
fn judge_agent_profile_denies_exec_start_during_direct_tool_execution() {
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

    store
        .put_session(&SessionRecord {
            id: "session-judge-deny-exec".to_string(),
            title: "Judge deny exec".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "judge".to_string(),
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
            id: "mission-judge-deny-exec".to_string(),
            session_id: "session-judge-deny-exec".to_string(),
            objective: "Judge exec deny".to_string(),
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
        "job-judge-deny-exec",
        "session-judge-deny-exec",
        "mission-judge-deny-exec",
        Some("run-judge-deny-exec"),
        None,
        "Forbidden exec",
        3,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.started_at = Some(4);
    job.updated_at = 4;

    let mut run = RunEngine::new(
        "run-judge-deny-exec",
        "session-judge-deny-exec",
        Some("mission-judge-deny-exec"),
        4,
    );
    run.start(4).expect("start run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");

    let tool_call = ToolCall::ExecStart(agent_runtime::tool::ExecStartInput {
        executable: "echo".to_string(),
        args: vec!["hi".to_string()],
        cwd: None,
    });

    let error = app
        .request_tool_approval("job-judge-deny-exec", "run-judge-deny-exec", &tool_call, 20)
        .expect_err("judge exec_start must be denied");
    assert!(
        error
            .to_string()
            .contains("tool exec_start is not allowed by agent profile Judge (judge)")
    );

    let run_snapshot = RunSnapshot::try_from(
        store
            .get_run("run-judge-deny-exec")
            .expect("get run")
            .expect("run exists"),
    )
    .expect("run snapshot");
    assert_eq!(run_snapshot.status, RunStatus::Failed);
}

#[test]
fn judge_agent_profile_denies_fs_write_text_during_direct_tool_execution() {
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

    store
        .put_session(&SessionRecord {
            id: "session-judge-deny-write".to_string(),
            title: "Judge deny write".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "judge".to_string(),
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
            id: "mission-judge-deny-write".to_string(),
            session_id: "session-judge-deny-write".to_string(),
            objective: "Judge write deny".to_string(),
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
        "job-judge-deny-write",
        "session-judge-deny-write",
        "mission-judge-deny-write",
        Some("run-judge-deny-write"),
        None,
        "Forbidden write",
        3,
    );
    job.status = agent_runtime::mission::JobStatus::Running;
    job.started_at = Some(4);
    job.updated_at = 4;

    let mut run = RunEngine::new(
        "run-judge-deny-write",
        "session-judge-deny-write",
        Some("mission-judge-deny-write"),
        4,
    );
    run.start(4).expect("start run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");
    store
        .put_job(&JobRecord::try_from(&job).expect("job record"))
        .expect("put job");

    let tool_call = ToolCall::FsWriteText(agent_runtime::tool::FsWriteTextInput {
        path: "notes/out.txt".to_string(),
        content: "denied\n".to_string(),
        mode: agent_runtime::tool::FsWriteMode::Upsert,
    });

    let error = app
        .request_tool_approval(
            "job-judge-deny-write",
            "run-judge-deny-write",
            &tool_call,
            20,
        )
        .expect_err("judge fs_write_text must be denied");
    assert!(
        error
            .to_string()
            .contains("tool fs_write_text is not allowed by agent profile Judge (judge)")
    );

    let run_snapshot = RunSnapshot::try_from(
        store
            .get_run("run-judge-deny-write")
            .expect("get run")
            .expect("run exists"),
    )
    .expect("run snapshot");
    assert_eq!(run_snapshot.status, RunStatus::Failed);
}

#[test]
fn run_with_args_executes_mission_ticks_and_jobs() {
    let (api_base, requests, handle) = spawn_json_server(
        r#"{
                "id":"resp_cli",
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
                                "text":"CLI mission result"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":12,"output_tokens":4,"total_tokens":16}
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
            id: "session-cli".to_string(),
            title: "CLI session".to_string(),
            prompt_override: Some("Reply tersely.".to_string()),
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
        .put_mission(&MissionRecord {
            id: "mission-cli".to_string(),
            session_id: "session-cli".to_string(),
            objective: "Drive one CLI mission".to_string(),
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

    let tick = app
        .run_with_args(["mission", "tick", "60"])
        .expect("mission tick");
    assert!(tick.contains("queued_jobs=1"));
    assert!(tick.contains("mission-cli-mission-turn-60"));

    let executed = app
        .run_with_args(["job", "execute", "mission-cli-mission-turn-60", "61"])
        .expect("job execute");
    let raw_request = requests.recv().expect("raw request");
    handle.join().expect("join server");

    assert!(executed.contains("job execute id=mission-cli-mission-turn-60"));
    assert!(executed.contains("run_id=run-mission-cli-mission-turn-60"));
    assert!(executed.contains("response_id=resp_cli"));
    assert!(executed.contains("output=CLI mission result"));

    let executed_job = store
        .get_job("mission-cli-mission-turn-60")
        .expect("get executed job")
        .expect("job exists");
    assert_eq!(executed_job.status, "completed");

    let executed_run = store
        .get_run("run-mission-cli-mission-turn-60")
        .expect("get executed run")
        .expect("run exists");
    assert_eq!(executed_run.status, "completed");
    assert_eq!(executed_run.result.as_deref(), Some("CLI mission result"));

    let transcripts = store
        .list_transcripts_for_session("session-cli")
        .expect("list transcripts");
    assert_eq!(transcripts.len(), 2);

    let normalized_request = raw_request.to_ascii_lowercase();
    assert!(normalized_request.contains("/v1/responses"));
    assert!(normalized_request.contains("\"instructions\":\"reply tersely.\""));
    assert!(normalized_request.contains("\"text\":\"drive one cli mission\""));
}

#[test]
fn session_transcript_view_renders_entries_in_chronological_order() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-chat".to_string(),
            title: "Chat session".to_string(),
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
            id: "msg-2".to_string(),
            session_id: "session-chat".to_string(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "Hi".to_string(),
            created_at: 11,
        })
        .expect("put assistant transcript");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-1".to_string(),
            session_id: "session-chat".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "Hello".to_string(),
            created_at: 10,
        })
        .expect("put user transcript");

    let transcript = app
        .session_transcript("session-chat")
        .expect("load transcript view");

    assert_eq!(transcript.session_id, "session-chat");
    assert_eq!(transcript.entries.len(), 2);
    assert_eq!(transcript.entries[0].role, "user");
    assert_eq!(transcript.entries[0].content, "Hello");
    assert_eq!(transcript.entries[1].role, "assistant");
    assert_eq!(transcript.entries[1].content, "Hi");
    assert_eq!(transcript.render(), "[10] user: Hello\n[11] assistant: Hi");
}

#[test]
fn session_transcript_ignores_corrupt_unrelated_run_records() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-good".to_string(),
            title: "Good Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("put good session");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-1".to_string(),
            session_id: "session-good".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "Hello".to_string(),
            created_at: 10,
        })
        .expect("put user transcript");

    let connection =
        rusqlite::Connection::open(&app.persistence.stores.metadata_db).expect("open sqlite");
    connection
        .execute(
            "INSERT INTO sessions (
                id, title, prompt_override, settings_json, agent_profile_id,
                active_mission_id, parent_session_id, parent_job_id, delegation_label,
                created_at, updated_at
            ) VALUES (?1, ?2, NULL, ?3, ?4, NULL, NULL, NULL, NULL, ?5, ?6)",
            rusqlite::params![
                "session-bad",
                "Broken Session",
                serde_json::to_string(&SessionSettings::default()).expect("serialize settings"),
                "default",
                11i64,
                11i64
            ],
        )
        .expect("insert unrelated session");
    connection
        .execute(
            "INSERT INTO runs (
                id, session_id, mission_id, status, error, result, provider_usage_json,
                active_processes_json, recent_steps_json, evidence_refs_json,
                pending_approvals_json, provider_loop_json, delegate_runs_json,
                started_at, updated_at, finished_at
            ) VALUES (?1, ?2, NULL, ?3, NULL, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL)",
            rusqlite::params![
                "run-bad",
                "session-bad",
                "running",
                "null",
                "[]",
                "[]",
                "[]",
                "[]",
                "{not-json",
                "[]",
                12i64,
                12i64
            ],
        )
        .expect("insert corrupt unrelated run row");

    let transcript = app
        .session_transcript("session-good")
        .expect("session transcript should ignore unrelated corrupt runs");

    assert_eq!(transcript.session_id, "session-good");
    assert_eq!(transcript.entries.len(), 1);
    assert_eq!(transcript.entries[0].content, "Hello");
}

#[test]
fn session_transcript_tail_limits_entries_and_ignores_corrupt_unrelated_run_records() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-good".to_string(),
            title: "Good Session".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("serialize settings"),
            workspace_root: app.runtime.workspace.root.display().to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 10,
            updated_at: 10,
        })
        .expect("put good session");
    for (id, kind, content, created_at) in [
        ("msg-1", "user", "Hello", 10),
        ("msg-2", "assistant", "Hi", 11),
        ("msg-3", "user", "Need status", 12),
    ] {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: id.to_string(),
                session_id: "session-good".to_string(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at,
            })
            .expect("put transcript");
    }

    let connection =
        rusqlite::Connection::open(&app.persistence.stores.metadata_db).expect("open sqlite");
    connection
        .execute(
            "INSERT INTO sessions (
                id, title, prompt_override, settings_json, agent_profile_id,
                active_mission_id, parent_session_id, parent_job_id, delegation_label,
                created_at, updated_at
            ) VALUES (?1, ?2, NULL, ?3, ?4, NULL, NULL, NULL, NULL, ?5, ?6)",
            rusqlite::params![
                "session-bad",
                "Broken Session",
                serde_json::to_string(&SessionSettings::default()).expect("serialize settings"),
                "default",
                11i64,
                11i64
            ],
        )
        .expect("insert unrelated session");
    connection
        .execute(
            "INSERT INTO runs (
                id, session_id, mission_id, status, error, result, provider_usage_json,
                active_processes_json, recent_steps_json, evidence_refs_json,
                pending_approvals_json, provider_loop_json, delegate_runs_json,
                started_at, updated_at, finished_at
            ) VALUES (?1, ?2, NULL, ?3, NULL, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL)",
            rusqlite::params![
                "run-bad",
                "session-bad",
                "running",
                "null",
                "[]",
                "[]",
                "[]",
                "[]",
                "{not-json",
                "[]",
                12i64,
                12i64
            ],
        )
        .expect("insert corrupt unrelated run row");

    let transcript = app
        .session_transcript_tail("session-good", 2)
        .expect("session transcript tail should stay bounded and ignore unrelated corrupt runs");

    assert_eq!(transcript.session_id, "session-good");
    assert_eq!(transcript.entries.len(), 2);
    assert_eq!(transcript.entries[0].content, "Hi");
    assert_eq!(transcript.entries[1].content, "Need status");
}

#[test]
fn session_transcript_places_persisted_reasoning_before_final_assistant_for_same_run() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-reasoning".to_string(),
            title: "Reasoning session".to_string(),
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

    let mut run = RunEngine::new("run-reasoning", "session-reasoning", None, 10);
    run.start(10).expect("start run");
    run.push_provider_reasoning("I should inspect the Timeweb profile first.", 10)
        .expect("persist reasoning");
    run.complete("Done", 10).expect("complete run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");

    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-user".to_string(),
            session_id: "session-reasoning".to_string(),
            run_id: Some("run-reasoning".to_string()),
            kind: "user".to_string(),
            content: "show timeweb servers".to_string(),
            created_at: 10,
        })
        .expect("put user transcript");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-assistant".to_string(),
            session_id: "session-reasoning".to_string(),
            run_id: Some("run-reasoning".to_string()),
            kind: "assistant".to_string(),
            content: "Done".to_string(),
            created_at: 10,
        })
        .expect("put assistant transcript");

    let transcript = app
        .session_transcript("session-reasoning")
        .expect("load transcript view");

    let roles = transcript
        .entries
        .iter()
        .map(|entry| entry.role.as_str())
        .collect::<Vec<_>>();
    assert_eq!(roles, vec!["user", "reasoning", "assistant"]);
    assert_eq!(
        transcript.entries[1].content,
        "I should inspect the Timeweb profile first."
    );
}

#[test]
fn session_transcript_coalesces_reasoning_deltas_into_one_line() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = PersistenceStore::open(&app.persistence).expect("open store");

    store
        .put_session(&SessionRecord {
            id: "session-reasoning-deltas".to_string(),
            title: "Reasoning delta session".to_string(),
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

    let mut run = RunEngine::new("run-reasoning-deltas", "session-reasoning-deltas", None, 10);
    run.start(10).expect("start run");
    run.push_provider_reasoning("Let ", 10)
        .expect("reasoning delta one");
    run.push_provider_reasoning("me ", 10)
        .expect("reasoning delta two");
    run.push_provider_reasoning("check", 10)
        .expect("reasoning delta three");
    run.complete("Done", 11).expect("complete run");
    store
        .put_run(&RunRecord::try_from(run.snapshot()).expect("run record"))
        .expect("put run");

    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-user-deltas".to_string(),
            session_id: "session-reasoning-deltas".to_string(),
            run_id: Some("run-reasoning-deltas".to_string()),
            kind: "user".to_string(),
            content: "show me".to_string(),
            created_at: 10,
        })
        .expect("put user transcript");
    store
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "msg-assistant-deltas".to_string(),
            session_id: "session-reasoning-deltas".to_string(),
            run_id: Some("run-reasoning-deltas".to_string()),
            kind: "assistant".to_string(),
            content: "Done".to_string(),
            created_at: 11,
        })
        .expect("put assistant transcript");

    let transcript = app
        .session_transcript("session-reasoning-deltas")
        .expect("load transcript view");
    let reasoning_entries = transcript
        .entries
        .iter()
        .filter(|entry| entry.role == "reasoning")
        .collect::<Vec<_>>();

    assert_eq!(reasoning_entries.len(), 1);
    assert_eq!(reasoning_entries[0].content, "Let me check");
}
