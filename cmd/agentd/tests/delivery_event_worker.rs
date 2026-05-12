use agent_persistence::{
    AppConfig, DeliveryRepository, DeliveryTargetRecord, EventOutboxRecord, EventRepository,
    PersistenceStore, RunRecord, RunRepository, SessionOutputRouteRecord, SessionRecord,
    SessionRepository, TaskRegistryRecord, TaskRegistryRepository, TranscriptRecord,
    TranscriptRepository,
};
use agent_runtime::session::SessionSettings;
use agentd::bootstrap;
use agentd::delivery_worker::{
    DeliverySendError, DeliverySender, DeliveryWorkerReport, deliver_session_output_event,
    deliver_task_result_event,
};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct RecordingSender {
    state: Arc<Mutex<RecordingSenderState>>,
}

#[derive(Debug, Default)]
struct RecordingSenderState {
    fail_next: Option<String>,
    sent: Vec<(String, String)>,
}

impl RecordingSender {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingSenderState::default())),
        }
    }

    fn fail_next(&self, message: &str) {
        self.state.lock().unwrap().fail_next = Some(message.to_string());
    }

    fn sent(&self) -> Vec<(String, String)> {
        self.state.lock().unwrap().sent.clone()
    }
}

impl DeliverySender for RecordingSender {
    fn send_text(
        &self,
        target: &DeliveryTargetRecord,
        text: &str,
    ) -> Result<(), DeliverySendError> {
        let mut state = self.state.lock().unwrap();
        if let Some(message) = state.fail_next.take() {
            return Err(DeliverySendError::new(message));
        }
        state
            .sent
            .push((target.target_id.clone(), text.to_string()));
        Ok(())
    }
}

fn test_app() -> (tempfile::TempDir, bootstrap::App) {
    let temp = tempfile::tempdir().expect("tempdir");
    let config = AppConfig {
        data_dir: temp.path().join("teamd-state"),
        ..AppConfig::default()
    };
    let app = bootstrap::build_from_config(config).expect("build app");
    (temp, app)
}

fn store(app: &bootstrap::App) -> PersistenceStore {
    PersistenceStore::open(&app.persistence).expect("open store")
}

fn seed_output_fixture(
    store: &PersistenceStore,
    route_format_policy: &str,
    cursor_transcript_id: Option<&str>,
) {
    seed_output_fixture_with_options(
        store,
        route_format_policy,
        cursor_transcript_id,
        None,
        None,
        "completed",
        None,
    )
}

fn seed_output_fixture_with_options(
    store: &PersistenceStore,
    route_format_policy: &str,
    cursor_transcript_id: Option<&str>,
    allowed_agent_ids_json: Option<&str>,
    allowed_session_ids_json: Option<&str>,
    run_status: &str,
    run_error: Option<&str>,
) {
    store
        .put_session(&SessionRecord {
            id: "session-delivery".to_string(),
            title: "session-delivery".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default()).unwrap(),
            workspace_root: ".".to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 100,
            updated_at: 100,
        })
        .expect("put session");
    store
        .put_run(&RunRecord {
            id: "run-delivery".to_string(),
            session_id: "session-delivery".to_string(),
            mission_id: None,
            status: run_status.to_string(),
            error: run_error.map(str::to_string),
            result: Some("assistant full text".to_string()),
            provider_usage_json: "{}".to_string(),
            active_processes_json: "[]".to_string(),
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "{}".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 110,
            updated_at: 120,
            finished_at: Some(120),
        })
        .expect("put run");
    store
        .put_transcript(&TranscriptRecord {
            id: "transcript-assistant".to_string(),
            session_id: "session-delivery".to_string(),
            run_id: Some("run-delivery".to_string()),
            kind: "assistant".to_string(),
            content: "assistant full text".to_string(),
            created_at: 121,
        })
        .expect("put transcript");
    store
        .put_delivery_target(&DeliveryTargetRecord {
            target_id: "telegram-main".to_string(),
            kind: "telegram".to_string(),
            address: "42".to_string(),
            scope: "private".to_string(),
            owner_user_id: Some("telegram-user-7".to_string()),
            allowed_agent_ids_json: allowed_agent_ids_json.unwrap_or("[]").to_string(),
            allowed_session_ids_json: allowed_session_ids_json.unwrap_or("[]").to_string(),
            send_policy_json: "{}".to_string(),
            format_policy: "full_text".to_string(),
            created_at: 100,
            updated_at: 100,
        })
        .expect("put target");
    store
        .put_session_output_route(&SessionOutputRouteRecord {
            route_id: "route-delivery".to_string(),
            session_id: "session-delivery".to_string(),
            target_id: "telegram-main".to_string(),
            filter_json: "{}".to_string(),
            format_policy: route_format_policy.to_string(),
            enabled: true,
            last_delivered_transcript_created_at: cursor_transcript_id.map(|_| 121),
            last_delivered_transcript_id: cursor_transcript_id.map(str::to_string),
            created_at: 100,
            updated_at: 100,
        })
        .expect("put route");
    store
        .put_event_outbox(&EventOutboxRecord {
            outbox_id: "outbox-output-routed-delivery".to_string(),
            subject: "teamd.session.session-delivery.output".to_string(),
            payload_json: json!({
                "event_id": "output-routed-delivery",
                "event_type": "session.output.created",
                "trace_id": "trace-delivery",
                "source_kind": "session_worker",
                "source_id": "routed-delivery",
                "subject": "teamd.session.session-delivery.output",
                "payload_ref": {"table": "runs", "id": "run-delivery"},
                "created_at": 122,
                "metadata": {"session_id": "session-delivery", "routed_event_id": "routed-delivery"}
            })
            .to_string(),
            status: "pending".to_string(),
            attempt_count: 0,
            next_attempt_at: 122,
            created_at: 122,
            published_at: None,
            last_error: None,
        })
        .expect("put output outbox");
}

#[test]
fn output_event_sends_assistant_text_to_configured_target_and_updates_cursor() {
    let (_temp, app) = test_app();
    let store = store(&app);
    seed_output_fixture(&store, "full_text", None);
    let sender = RecordingSender::new();

    let report = deliver_session_output_event(&app, &sender, "outbox-output-routed-delivery", 130)
        .expect("deliver output");

    assert_eq!(report.delivered, 1);
    assert_eq!(
        sender.sent(),
        vec![(
            "telegram-main".to_string(),
            "assistant full text".to_string()
        )]
    );
    let delivery = store
        .get_event_delivery("delivery-output-routed-delivery-telegram-main")
        .expect("get delivery")
        .expect("delivery exists");
    assert_eq!(delivery.status, "delivered");
    let route = store
        .get_session_output_route("route-delivery")
        .expect("get route")
        .expect("route exists");
    assert_eq!(
        route.last_delivered_transcript_id.as_deref(),
        Some("transcript-assistant")
    );
}

#[test]
fn route_cursor_prevents_duplicate_sends() {
    let (_temp, app) = test_app();
    let store = store(&app);
    seed_output_fixture(&store, "full_text", Some("transcript-assistant"));
    let sender = RecordingSender::new();

    let report = deliver_session_output_event(&app, &sender, "outbox-output-routed-delivery", 130)
        .expect("deliver output");

    assert_eq!(report.delivered, 0);
    assert_eq!(report.skipped, 1);
    assert!(sender.sent().is_empty());
}

#[test]
fn delivery_failure_is_persisted_without_rolling_back_run() {
    let (_temp, app) = test_app();
    let store = store(&app);
    seed_output_fixture(&store, "full_text", None);
    let sender = RecordingSender::new();
    sender.fail_next("telegram unavailable");

    let report = deliver_session_output_event(&app, &sender, "outbox-output-routed-delivery", 130)
        .expect("delivery worker should persist failure and continue");

    assert_eq!(report.failed, 1);
    assert_eq!(
        store.get_run("run-delivery").unwrap().unwrap().status,
        "completed"
    );
    let delivery = store
        .get_event_delivery("delivery-output-routed-delivery-telegram-main")
        .expect("get delivery")
        .expect("delivery exists");
    assert_eq!(delivery.status, "failed");
    assert_eq!(delivery.last_error.as_deref(), Some("telegram unavailable"));
}

#[test]
fn status_only_format_policy_does_not_send_full_text() {
    let (_temp, app) = test_app();
    let store = store(&app);
    seed_output_fixture(&store, "status_only", None);
    let sender = RecordingSender::new();

    let DeliveryWorkerReport { delivered, .. } =
        deliver_session_output_event(&app, &sender, "outbox-output-routed-delivery", 130)
            .expect("deliver status only");

    assert_eq!(delivered, 1);
    let sent = sender.sent();
    assert_eq!(sent.len(), 1);
    assert!(!sent[0].1.contains("assistant full text"));
    assert!(sent[0].1.contains("session-delivery"));
}

#[test]
fn output_route_does_not_send_when_target_disallows_session_or_agent() {
    let (_temp, app) = test_app();
    let store = store(&app);
    seed_output_fixture_with_options(
        &store,
        "full_text",
        None,
        Some(r#"["judge"]"#),
        Some(r#"["session-other"]"#),
        "completed",
        None,
    );
    let sender = RecordingSender::new();

    let report = deliver_session_output_event(&app, &sender, "outbox-output-routed-delivery", 130)
        .expect("delivery worker should record authorization failure");

    assert_eq!(report.delivered, 0);
    assert_eq!(report.failed, 1);
    assert!(sender.sent().is_empty());
    let delivery = store
        .get_event_delivery("delivery-output-routed-delivery-telegram-main")
        .expect("get delivery")
        .expect("delivery exists");
    assert_eq!(delivery.status, "failed");
    assert!(
        delivery
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("not allowed")
    );
    let route = store
        .get_session_output_route("route-delivery")
        .expect("get route")
        .expect("route exists");
    assert_eq!(route.last_delivered_transcript_id, None);
}

#[test]
fn errors_only_route_skips_success_and_delivers_failed_run_error() {
    let (_temp, app) = test_app();
    let success_store = store(&app);
    seed_output_fixture(&success_store, "errors_only", None);
    let sender = RecordingSender::new();

    let success_report =
        deliver_session_output_event(&app, &sender, "outbox-output-routed-delivery", 130)
            .expect("deliver errors-only success");

    assert_eq!(success_report.delivered, 0);
    assert_eq!(success_report.skipped, 1);
    assert!(sender.sent().is_empty());

    let (_temp, app) = test_app();
    let failed_store = store(&app);
    seed_output_fixture_with_options(
        &failed_store,
        "errors_only",
        None,
        None,
        None,
        "failed",
        Some("provider failed"),
    );
    let sender = RecordingSender::new();

    let failed_report =
        deliver_session_output_event(&app, &sender, "outbox-output-routed-delivery", 130)
            .expect("deliver errors-only failure");

    assert_eq!(failed_report.delivered, 1);
    let sent = sender.sent();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].1.contains("session-delivery"));
    assert!(sent[0].1.contains("provider failed"));
    assert!(!sent[0].1.contains("assistant full text"));
}

#[test]
fn task_result_event_sends_status_to_task_followers() {
    let (_temp, app) = test_app();
    let store = store(&app);
    seed_task_result_fixture(&app, &store, "completed", None, "[]", "[]", "full_text");
    let sender = RecordingSender::new();

    let report = deliver_task_result_event(&app, &sender, "outbox-task-result-task-agent-1", 130)
        .expect("deliver task result");

    assert_eq!(report.delivered, 1);
    let sent = sender.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "telegram-main");
    assert!(sent[0].1.contains("task-agent-1"));
    assert!(sent[0].1.contains("completed"));
    assert!(sent[0].1.contains("approved"));
    let delivery = store
        .get_event_delivery("delivery-task-result-task-agent-1-telegram-main")
        .expect("get delivery")
        .expect("delivery exists");
    assert_eq!(delivery.status, "delivered");
    let followers = store
        .list_task_followers("task-agent-1")
        .expect("followers");
    assert_eq!(followers[0].delivered_at, Some(130));
}

#[test]
fn task_result_delivery_respects_target_agent_and_session_acl() {
    let (_temp, app) = test_app();
    let store = store(&app);
    seed_task_result_fixture(
        &app,
        &store,
        "completed",
        None,
        r#"["other-agent"]"#,
        r#"["session-other"]"#,
        "full_text",
    );
    let sender = RecordingSender::new();

    let report = deliver_task_result_event(&app, &sender, "outbox-task-result-task-agent-1", 130)
        .expect("delivery worker should record authorization failure");

    assert_eq!(report.delivered, 0);
    assert_eq!(report.failed, 1);
    assert!(sender.sent().is_empty());
    let delivery = store
        .get_event_delivery("delivery-task-result-task-agent-1-telegram-main")
        .expect("get delivery")
        .expect("delivery exists");
    assert_eq!(delivery.status, "failed");
    assert!(
        delivery
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("not allowed")
    );
}

#[test]
fn task_result_errors_only_skips_success_and_delivers_failed_task() {
    let (_temp, app) = test_app();
    let success_store = store(&app);
    seed_task_result_fixture(
        &app,
        &success_store,
        "completed",
        None,
        "[]",
        "[]",
        "errors_only",
    );
    let sender = RecordingSender::new();

    let success_report =
        deliver_task_result_event(&app, &sender, "outbox-task-result-task-agent-1", 130)
            .expect("deliver task success");

    assert_eq!(success_report.delivered, 0);
    assert_eq!(success_report.skipped, 1);
    assert!(sender.sent().is_empty());

    let (_temp, app) = test_app();
    let failed_store = store(&app);
    seed_task_result_fixture(
        &app,
        &failed_store,
        "failed",
        Some("child agent failed"),
        "[]",
        "[]",
        "errors_only",
    );
    let sender = RecordingSender::new();

    let failed_report =
        deliver_task_result_event(&app, &sender, "outbox-task-result-task-agent-1", 130)
            .expect("deliver task failure");

    assert_eq!(failed_report.delivered, 1);
    let sent = sender.sent();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].1.contains("task-agent-1"));
    assert!(sent[0].1.contains("child agent failed"));
    assert!(!sent[0].1.contains("result_ref_json"));
}

fn seed_task_result_fixture(
    app: &bootstrap::App,
    store: &PersistenceStore,
    status: &str,
    error: Option<&str>,
    allowed_agent_ids_json: &str,
    allowed_session_ids_json: &str,
    format_policy: &str,
) {
    store
        .put_delivery_target(&DeliveryTargetRecord {
            target_id: "telegram-main".to_string(),
            kind: "telegram".to_string(),
            address: "42".to_string(),
            scope: "private".to_string(),
            owner_user_id: Some("telegram-user-7".to_string()),
            allowed_agent_ids_json: allowed_agent_ids_json.to_string(),
            allowed_session_ids_json: allowed_session_ids_json.to_string(),
            send_policy_json: "{}".to_string(),
            format_policy: format_policy.to_string(),
            created_at: 100,
            updated_at: 100,
        })
        .expect("put target");
    store
        .put_task_registry(&TaskRegistryRecord {
            task_id: "task-agent-1".to_string(),
            kind: "agent_task".to_string(),
            source_session_id: Some("session-parent".to_string()),
            owner_agent_id: Some("default".to_string()),
            executor_agent_id: Some("judge".to_string()),
            parent_task_id: None,
            status: status.to_string(),
            dependency_json: "[]".to_string(),
            context_ref_json: r#"{"goal":"review"}"#.to_string(),
            result_ref_json: Some(r#"{"run_id":"run-child","summary":"approved"}"#.to_string()),
            retry_policy_json: "{}".to_string(),
            attempt_count: 1,
            max_attempts: 1,
            timeout_at: None,
            chain_id: Some("chain-1".to_string()),
            hop_count: Some(1),
            max_hops: Some(3),
            trace_id: Some("trace-1".to_string()),
            created_at: 100,
            updated_at: 120,
            started_at: Some(101),
            finished_at: Some(120),
            error: error.map(str::to_string),
        })
        .expect("put task");
    app.follow_task("task-agent-1", "telegram-main", Some("telegram-user-7"))
        .expect("follow task");
    store
        .put_event_outbox(&EventOutboxRecord {
            outbox_id: "outbox-task-result-task-agent-1".to_string(),
            subject: "teamd.task.task-agent-1".to_string(),
            payload_json: json!({
                "event_id": "task-result-task-agent-1",
                "event_type": if status == "completed" {
                    "agent_task.completed"
                } else {
                    "agent_task.failed"
                },
                "trace_id": "trace-1",
                "source_kind": "task_worker",
                "source_id": "task-agent-1",
                "subject": "teamd.task.task-agent-1",
                "payload_ref": {"table": "task_registry", "id": "task-agent-1"},
                "created_at": 121,
                "metadata": {}
            })
            .to_string(),
            status: "pending".to_string(),
            attempt_count: 0,
            next_attempt_at: 121,
            created_at: 121,
            published_at: None,
            last_error: None,
        })
        .expect("put task result outbox");
}
