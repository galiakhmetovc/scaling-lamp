use super::support::*;
use agent_persistence::{EventRepository, TaskRegistryRecord, TaskRegistryRepository};
use agentd::bootstrap::{DeliveryTargetCreateOptions, SessionOutputRouteCreateOptions};

#[test]
fn app_manages_delivery_targets_and_session_output_routes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let session = app
        .create_session_auto(Some("Server Watcher"))
        .expect("create session");
    app.store()
        .expect("open store")
        .put_transcript(&agent_persistence::TranscriptRecord {
            id: "transcript-before-route".to_string(),
            session_id: session.id.clone(),
            run_id: None,
            kind: "assistant".to_string(),
            content: "existing answer".to_string(),
            created_at: 100,
        })
        .expect("put existing transcript");

    let target = app
        .create_delivery_target(
            "ops-status",
            DeliveryTargetCreateOptions {
                kind: "telegram".to_string(),
                address: "-100100200300".to_string(),
                scope: "group".to_string(),
                owner_user_id: Some("telegram:42".to_string()),
                allowed_agent_ids: vec!["default".to_string()],
                allowed_session_ids: vec![session.id.clone()],
                send_policy_json: r#"{"quiet_hours":false}"#.to_string(),
                format_policy: "summary".to_string(),
            },
        )
        .expect("create delivery target");

    assert_eq!(target.id, "ops-status");
    assert_eq!(target.kind, "telegram");
    assert_eq!(target.address, "-100100200300");
    assert_eq!(target.allowed_agent_ids, vec!["default"]);
    assert_eq!(target.allowed_session_ids, vec![session.id.clone()]);

    let route = app
        .attach_session_output_route(
            &session.id,
            "ops-status",
            SessionOutputRouteCreateOptions {
                route_id: Some("route-server-watcher-ops-status".to_string()),
                filter_json: r#"{"kind":"assistant"}"#.to_string(),
                format_policy: "summary".to_string(),
                enabled: true,
            },
        )
        .expect("attach output route");

    assert_eq!(route.session_id, session.id);
    assert_eq!(route.target_id, "ops-status");
    assert_eq!(route.format_policy, "summary");
    assert!(route.enabled);
    assert_eq!(route.last_delivered_transcript_created_at, Some(100));
    assert_eq!(
        route.last_delivered_transcript_id.as_deref(),
        Some("transcript-before-route")
    );

    let targets = app.list_delivery_targets().expect("list targets");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].id, "ops-status");

    let routes = app
        .list_enabled_session_output_routes(&session.id)
        .expect("list enabled routes");
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].id, "route-server-watcher-ops-status");
}

#[test]
fn app_manages_task_followers_and_task_cancellation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app = build_from_config(AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    })
    .expect("build app");
    let store = app.store().expect("open store");
    store
        .put_task_registry(&TaskRegistryRecord {
            task_id: "task-agent-1".to_string(),
            kind: "agent_task".to_string(),
            source_session_id: None,
            owner_agent_id: Some("default".to_string()),
            executor_agent_id: Some("judge".to_string()),
            parent_task_id: None,
            status: "running".to_string(),
            dependency_json: "[]".to_string(),
            context_ref_json: r#"{"goal":"review"}"#.to_string(),
            result_ref_json: None,
            retry_policy_json: "{}".to_string(),
            attempt_count: 0,
            max_attempts: 1,
            timeout_at: None,
            chain_id: None,
            hop_count: None,
            max_hops: None,
            trace_id: Some("trace-1".to_string()),
            created_at: 100,
            updated_at: 100,
            started_at: Some(101),
            finished_at: None,
            error: None,
        })
        .expect("put task");
    app.create_delivery_target(
        "ops-status",
        DeliveryTargetCreateOptions {
            kind: "telegram".to_string(),
            address: "-100100200300".to_string(),
            scope: "group".to_string(),
            owner_user_id: Some("telegram:42".to_string()),
            allowed_agent_ids: Vec::new(),
            allowed_session_ids: Vec::new(),
            send_policy_json: "null".to_string(),
            format_policy: "full_text".to_string(),
        },
    )
    .expect("create target");

    let follower = app
        .follow_task("task-agent-1", "ops-status", Some("telegram:42"))
        .expect("follow task");
    assert_eq!(follower.task_id, "task-agent-1");
    assert_eq!(follower.target_id, "ops-status");
    assert!(follower.enabled);

    let rendered = app.render_task("task-agent-1").expect("render task");
    assert!(rendered.contains("followers:"));
    assert!(rendered.contains("ops-status"));

    let cancelled = app.cancel_task("task-agent-1").expect("cancel task");
    assert!(cancelled.contains("cancelled task-agent-1"));
    let outbox = store
        .get_event_outbox("outbox-task-result-task-agent-1")
        .expect("get task result outbox")
        .expect("task result outbox exists");
    assert!(outbox.payload_json.contains("agent_task.failed"));
    let task = store
        .get_task_registry("task-agent-1")
        .expect("get task")
        .expect("task exists");
    assert_eq!(task.status, "cancelled");
    assert!(task.finished_at.is_some());

    let disabled = app
        .unfollow_task("task-agent-1", "ops-status")
        .expect("unfollow task");
    assert!(!disabled.enabled);
}
