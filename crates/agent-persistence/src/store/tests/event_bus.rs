use super::*;
use crate::{
    EventDeliveryRecord, EventOutboxRecord, EventRepository, EventSourceRecord, InboundEventRecord,
    RoutedEventRecord, RouterRepository, RouterRuleRecord, TaskRegistryRecord,
    TaskRegistryRepository,
};

#[test]
fn event_bus_repository_round_trips_events_routes_outbox_and_tasks() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-router".to_string(),
        title: "Router".to_string(),
        prompt_override: None,
        settings_json: "{}".to_string(),
        workspace_root: ".".to_string(),
        agent_profile_id: "default".to_string(),
        active_mission_id: None,
        parent_session_id: None,
        parent_job_id: None,
        delegation_label: None,
        created_at: 90,
        updated_at: 90,
    };
    store.put_session(&session).expect("put session");

    let source = EventSourceRecord {
        source_id: "telegram-private-42".to_string(),
        kind: "telegram_private".to_string(),
        address: "42".to_string(),
        display_name: Some("Anton".to_string()),
        owner_user_id: Some("telegram:42".to_string()),
        auth_policy_json: r#"{"paired":true}"#.to_string(),
        default_route_policy_json: r#"{"agent_id":"default"}"#.to_string(),
        enabled: true,
        created_at: 100,
        updated_at: 101,
    };
    store.put_event_source(&source).expect("put source");
    assert_eq!(
        store
            .get_event_source("telegram-private-42")
            .expect("get source"),
        Some(source.clone())
    );

    let low_priority_rule = RouterRuleRecord {
        rule_id: "rule-default".to_string(),
        priority: 100,
        enabled: true,
        source_filter_json: "{}".to_string(),
        operator_filter_json: "{}".to_string(),
        condition_json: "{}".to_string(),
        route_policy_json: r#"{"agent_id":"default"}"#.to_string(),
        created_at: 100,
        updated_at: 100,
    };
    let high_priority_rule = RouterRuleRecord {
        rule_id: "rule-chat-42".to_string(),
        priority: 10,
        enabled: true,
        source_filter_json: r#"{"source_id":"telegram-private-42"}"#.to_string(),
        operator_filter_json: r#"{"operator_id":"telegram:42"}"#.to_string(),
        condition_json: "{}".to_string(),
        route_policy_json: r#"{"agent_id":"default","session_strategy":"per_private_chat"}"#
            .to_string(),
        created_at: 101,
        updated_at: 101,
    };
    store
        .put_router_rule(&low_priority_rule)
        .expect("put low rule");
    store
        .put_router_rule(&high_priority_rule)
        .expect("put high rule");
    assert_eq!(
        store
            .list_enabled_router_rules()
            .expect("list rules")
            .into_iter()
            .map(|rule| rule.rule_id)
            .collect::<Vec<_>>(),
        vec!["rule-chat-42".to_string(), "rule-default".to_string()]
    );

    let inbound = InboundEventRecord {
        event_id: "evt-telegram-1".to_string(),
        dedupe_key: "telegram:update:1001".to_string(),
        source_kind: "telegram_private".to_string(),
        source_id: "telegram-private-42".to_string(),
        operator_id: Some("telegram:42".to_string()),
        payload_json: r#"{"text":"hello"}"#.to_string(),
        metadata_json: r#"{"update_id":1001}"#.to_string(),
        status: "received".to_string(),
        received_at: 110,
        published_at: None,
        error: None,
    };
    let stored_inbound = store
        .put_inbound_event(&inbound)
        .expect("put inbound event");
    assert_eq!(stored_inbound, inbound);

    let duplicate = InboundEventRecord {
        event_id: "evt-telegram-duplicate".to_string(),
        payload_json: r#"{"text":"duplicate"}"#.to_string(),
        ..inbound.clone()
    };
    assert_eq!(
        store
            .put_inbound_event(&duplicate)
            .expect("dedupe inbound event"),
        inbound,
        "dedupe_key must make inbound insert idempotent"
    );

    let routed = RoutedEventRecord {
        routed_event_id: "routed-telegram-1".to_string(),
        inbound_event_id: "evt-telegram-1".to_string(),
        rule_id: Some("rule-chat-42".to_string()),
        session_id: "session-router".to_string(),
        agent_id: "default".to_string(),
        queue_policy: "priority".to_string(),
        priority: 10,
        payload_json: r#"{"text":"hello"}"#.to_string(),
        metadata_json: r#"{"matched":true}"#.to_string(),
        status: "routed".to_string(),
        routed_at: 111,
        published_at: None,
        error: None,
    };
    store.put_routed_event(&routed).expect("put routed");
    assert_eq!(
        store
            .get_routed_event("routed-telegram-1")
            .expect("get routed"),
        Some(routed)
    );

    let outbox = EventOutboxRecord {
        outbox_id: "outbox-1".to_string(),
        subject: "teamd.input.telegram".to_string(),
        payload_json: r#"{"event_id":"evt-telegram-1"}"#.to_string(),
        status: "pending".to_string(),
        attempt_count: 0,
        next_attempt_at: 120,
        created_at: 119,
        published_at: None,
        last_error: None,
    };
    store.put_event_outbox(&outbox).expect("put outbox");
    assert_eq!(
        store
            .claim_pending_event_outbox(1, 120)
            .expect("claim outbox")
            .into_iter()
            .map(|record| record.outbox_id)
            .collect::<Vec<_>>(),
        vec!["outbox-1".to_string()]
    );
    store
        .mark_event_outbox_published("outbox-1", 121)
        .expect("mark published");
    assert_eq!(
        store
            .get_event_outbox("outbox-1")
            .expect("get outbox")
            .expect("outbox exists")
            .status,
        "published"
    );

    let delivery = EventDeliveryRecord {
        delivery_event_id: "delivery-1".to_string(),
        source_event_id: "routed-telegram-1".to_string(),
        target_id: "telegram-private-42".to_string(),
        status: "pending".to_string(),
        attempt_count: 0,
        created_at: 130,
        updated_at: 130,
        delivered_at: None,
        last_error: None,
    };
    store.put_event_delivery(&delivery).expect("put delivery");
    store
        .put_event_delivery(&EventDeliveryRecord {
            status: "failed".to_string(),
            attempt_count: 1,
            updated_at: 131,
            last_error: Some("telegram timeout".to_string()),
            ..delivery.clone()
        })
        .expect("update delivery");
    assert_eq!(
        store
            .get_event_delivery("delivery-1")
            .expect("get delivery")
            .expect("delivery exists")
            .last_error
            .as_deref(),
        Some("telegram timeout")
    );

    let task = TaskRegistryRecord {
        task_id: "task-agent-1".to_string(),
        kind: "agent_task".to_string(),
        source_session_id: Some("session-router".to_string()),
        owner_agent_id: Some("default".to_string()),
        executor_agent_id: Some("judge".to_string()),
        parent_task_id: None,
        status: "queued".to_string(),
        dependency_json: r#"["task-parent"]"#.to_string(),
        context_ref_json: r#"["artifact-1"]"#.to_string(),
        result_ref_json: None,
        retry_policy_json: r#"{"max_attempts":3}"#.to_string(),
        attempt_count: 0,
        max_attempts: 3,
        timeout_at: Some(999),
        chain_id: Some("chain-1".to_string()),
        hop_count: Some(1),
        max_hops: Some(3),
        trace_id: Some("trace-1".to_string()),
        created_at: 140,
        updated_at: 140,
        started_at: None,
        finished_at: None,
        error: None,
    };
    store.put_task_registry(&task).expect("put task");
    assert_eq!(
        store.get_task_registry("task-agent-1").expect("get task"),
        Some(task.clone())
    );

    let second_task = TaskRegistryRecord {
        task_id: "task-agent-2".to_string(),
        status: "completed".to_string(),
        updated_at: 150,
        finished_at: Some(150),
        result_ref_json: Some(r#"{"kind":"event","id":"result-1"}"#.to_string()),
        ..task.clone()
    };
    store
        .put_task_registry(&second_task)
        .expect("put second task");

    let listed = store
        .list_task_registry_for_session("session-router")
        .expect("list session tasks");
    assert_eq!(
        listed
            .iter()
            .map(|task| task.task_id.as_str())
            .collect::<Vec<_>>(),
        vec!["task-agent-2", "task-agent-1"]
    );
}
