use agent_persistence::{
    AppConfig, EventRepository, InboundEventRecord, PersistenceStore, RouterRepository,
    RouterRuleRecord, SessionRecord, SessionRepository,
};
use agent_runtime::session::SessionSettings;
use agentd::bootstrap;
use agentd::router_worker::{RouteErrorKind, route_inbound_event, sort_inbound_events_for_routing};
use serde_json::json;

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

fn put_session(store: &PersistenceStore, session_id: &str) {
    store
        .put_session(&SessionRecord {
            id: session_id.to_string(),
            title: session_id.to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("settings json"),
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
}

fn put_rule(
    store: &PersistenceStore,
    rule_id: &str,
    priority: i64,
    enabled: bool,
    source_filter_json: &str,
    operator_filter_json: &str,
    route_policy_json: &str,
) {
    store
        .put_router_rule(&RouterRuleRecord {
            rule_id: rule_id.to_string(),
            priority,
            enabled,
            source_filter_json: source_filter_json.to_string(),
            operator_filter_json: operator_filter_json.to_string(),
            condition_json: "{}".to_string(),
            route_policy_json: route_policy_json.to_string(),
            created_at: 100 + priority,
            updated_at: 100 + priority,
        })
        .expect("put router rule");
}

fn put_inbound(store: &PersistenceStore, event_id: &str, source_id: &str, operator_id: &str) {
    store
        .put_inbound_event(&InboundEventRecord {
            event_id: event_id.to_string(),
            dedupe_key: format!("dedupe:{event_id}"),
            source_kind: "telegram".to_string(),
            source_id: source_id.to_string(),
            operator_id: Some(operator_id.to_string()),
            payload_json: json!({"text": "hello"}).to_string(),
            metadata_json: json!({"trace_id": "trace-router"}).to_string(),
            status: "pending".to_string(),
            received_at: 110,
            published_at: None,
            error: None,
        })
        .expect("put inbound");
}

fn count_rows(app: &bootstrap::App, table: &str) -> i64 {
    let query = format!("SELECT COUNT(*) FROM {table}");
    store(app)
        .with_postgres_client(|client| {
            client
                .query_one(&query, &[])
                .map(|row| row.get::<_, i64>(0))
                .map_err(agent_persistence::StoreError::from)
        })
        .expect("count rows")
}

#[test]
fn exact_chat_rule_routes_and_persists_routed_event_before_session_outbox() {
    let (_temp, app) = test_app();
    let store = store(&app);
    put_session(&store, "session-chat-42");
    put_inbound(
        &store,
        "event-chat-42",
        "telegram-chat-42",
        "telegram-user-7",
    );
    put_rule(
        &store,
        "rule-chat-42",
        10,
        true,
        r#"{"source_id":"telegram-chat-42"}"#,
        "{}",
        r#"{"session_id":"session-chat-42","agent_id":"default","queue_policy":"fifo","output_targets":["telegram-main"],"format_policy":"full","labels":["chat-exact"]}"#,
    );

    let decision = route_inbound_event(&app, "event-chat-42", 120).expect("route inbound");

    assert_eq!(decision.matched_rule_id.as_deref(), Some("rule-chat-42"));
    assert_eq!(decision.session_id, "session-chat-42");
    assert_eq!(decision.agent_id, "default");
    assert_eq!(decision.queue_policy, "fifo");
    assert_eq!(decision.output_targets, vec!["telegram-main"]);
    assert_eq!(decision.labels, vec!["chat-exact"]);

    let routed = store
        .get_routed_event("routed-event-chat-42")
        .expect("get routed")
        .expect("routed exists");
    assert_eq!(routed.inbound_event_id, "event-chat-42");
    assert_eq!(routed.rule_id.as_deref(), Some("rule-chat-42"));
    assert_eq!(routed.status, "pending");

    let outbox = store
        .get_event_outbox("outbox-routed-event-chat-42")
        .expect("get outbox")
        .expect("outbox exists");
    assert_eq!(outbox.subject, "teamd.session.session-chat-42.input");
    let envelope: serde_json::Value =
        serde_json::from_str(&outbox.payload_json).expect("outbox envelope");
    assert_eq!(envelope["payload_ref"]["table"], "routed_events");
    assert_eq!(envelope["payload_ref"]["id"], "routed-event-chat-42");
}

#[test]
fn operator_rule_applies_when_no_chat_rule_matches() {
    let (_temp, app) = test_app();
    let store = store(&app);
    put_session(&store, "session-operator");
    put_session(&store, "session-default");
    put_inbound(
        &store,
        "event-operator",
        "telegram-chat-42",
        "telegram-user-7",
    );
    put_rule(
        &store,
        "rule-other-chat",
        10,
        true,
        r#"{"source_id":"telegram-chat-99"}"#,
        "{}",
        r#"{"session_id":"session-default","agent_id":"default"}"#,
    );
    put_rule(
        &store,
        "rule-operator",
        20,
        true,
        "{}",
        r#"{"operator_id":"telegram-user-7"}"#,
        r#"{"session_id":"session-operator","agent_id":"default"}"#,
    );

    let decision = route_inbound_event(&app, "event-operator", 120).expect("route inbound");

    assert_eq!(decision.matched_rule_id.as_deref(), Some("rule-operator"));
    assert_eq!(decision.session_id, "session-operator");
}

#[test]
fn global_default_applies_last_and_disabled_rules_are_ignored() {
    let (_temp, app) = test_app();
    let store = store(&app);
    put_session(&store, "session-disabled");
    put_session(&store, "session-default");
    put_inbound(
        &store,
        "event-default",
        "telegram-chat-42",
        "telegram-user-7",
    );
    put_rule(
        &store,
        "rule-disabled-exact",
        1,
        false,
        r#"{"source_id":"telegram-chat-42"}"#,
        "{}",
        r#"{"session_id":"session-disabled","agent_id":"default"}"#,
    );
    put_rule(
        &store,
        "rule-global-default",
        1000,
        true,
        "{}",
        "{}",
        r#"{"session_id":"session-default","agent_id":"default","queue_policy":"priority"}"#,
    );

    let decision = route_inbound_event(&app, "event-default", 120).expect("route inbound");

    assert_eq!(
        decision.matched_rule_id.as_deref(),
        Some("rule-global-default")
    );
    assert_eq!(decision.session_id, "session-default");
    assert_eq!(decision.queue_policy, "priority");
}

#[test]
fn no_matching_route_writes_non_retryable_dlq_event() {
    let (_temp, app) = test_app();
    let store = store(&app);
    put_inbound(
        &store,
        "event-unmatched",
        "telegram-chat-42",
        "telegram-user-7",
    );

    let error = route_inbound_event(&app, "event-unmatched", 120).expect_err("route failure");

    assert_eq!(error.kind(), RouteErrorKind::RouteNotFound);
    assert_eq!(count_rows(&app, "routed_events"), 0);
    let outbox = store
        .get_event_outbox("dlq-event-unmatched")
        .expect("get dlq outbox")
        .expect("dlq outbox exists");
    assert_eq!(outbox.subject, "teamd.dlq");
    let envelope: serde_json::Value =
        serde_json::from_str(&outbox.payload_json).expect("dlq envelope");
    assert_eq!(envelope["reason"]["code"], "route_not_found");
    assert_eq!(envelope["reason"]["retryable"], false);
    assert_eq!(
        envelope["original_event"]["payload_ref"]["id"],
        "event-unmatched"
    );
}

#[test]
fn pending_events_are_ordered_by_received_at_then_event_id() {
    let mut events = vec![
        InboundEventRecord {
            event_id: "event-b".to_string(),
            received_at: 200,
            dedupe_key: "b".to_string(),
            source_kind: "telegram".to_string(),
            source_id: "telegram-chat-1".to_string(),
            operator_id: None,
            payload_json: "{}".to_string(),
            metadata_json: "{}".to_string(),
            status: "pending".to_string(),
            published_at: None,
            error: None,
        },
        InboundEventRecord {
            event_id: "event-a".to_string(),
            received_at: 200,
            dedupe_key: "a".to_string(),
            source_kind: "telegram".to_string(),
            source_id: "telegram-chat-1".to_string(),
            operator_id: None,
            payload_json: "{}".to_string(),
            metadata_json: "{}".to_string(),
            status: "pending".to_string(),
            published_at: None,
            error: None,
        },
        InboundEventRecord {
            event_id: "event-c".to_string(),
            received_at: 100,
            dedupe_key: "c".to_string(),
            source_kind: "telegram".to_string(),
            source_id: "telegram-chat-1".to_string(),
            operator_id: None,
            payload_json: "{}".to_string(),
            metadata_json: "{}".to_string(),
            status: "pending".to_string(),
            published_at: None,
            error: None,
        },
    ];

    sort_inbound_events_for_routing(&mut events);

    assert_eq!(
        events
            .into_iter()
            .map(|event| event.event_id)
            .collect::<Vec<_>>(),
        vec!["event-c", "event-a", "event-b"]
    );
}
