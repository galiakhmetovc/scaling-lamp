use super::*;
use crate::{
    DeliveryRepository, DeliveryTargetRecord, SessionOutputRouteRecord, TaskFollowerRecord,
    TaskRegistryRecord, TaskRegistryRepository,
};

#[test]
fn delivery_repository_round_trips_targets_and_session_routes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::super::PersistenceStore::open(&scaffold).expect("open store");

    let session = SessionRecord {
        id: "session-monitor".to_string(),
        title: "Monitor".to_string(),
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

    let target = DeliveryTargetRecord {
        target_id: "ops-status".to_string(),
        kind: "telegram".to_string(),
        address: "-100100200300".to_string(),
        scope: "group".to_string(),
        owner_user_id: Some("telegram:42".to_string()),
        allowed_agent_ids_json: r#"["default"]"#.to_string(),
        allowed_session_ids_json: r#"["session-monitor"]"#.to_string(),
        send_policy_json: r#"{"retry":"default"}"#.to_string(),
        format_policy: "full_text".to_string(),
        created_at: 100,
        updated_at: 100,
    };
    store
        .put_delivery_target(&target)
        .expect("put delivery target");
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
            context_ref_json: "{}".to_string(),
            result_ref_json: None,
            retry_policy_json: "{}".to_string(),
            attempt_count: 0,
            max_attempts: 1,
            timeout_at: None,
            chain_id: None,
            hop_count: None,
            max_hops: None,
            trace_id: None,
            created_at: 100,
            updated_at: 100,
            started_at: None,
            finished_at: None,
            error: None,
        })
        .expect("put task");

    let route = SessionOutputRouteRecord {
        route_id: "route-session-monitor-ops-status".to_string(),
        session_id: "session-monitor".to_string(),
        target_id: "ops-status".to_string(),
        filter_json: "null".to_string(),
        format_policy: "summary".to_string(),
        enabled: true,
        last_delivered_transcript_created_at: Some(101),
        last_delivered_transcript_id: Some("transcript-101".to_string()),
        created_at: 110,
        updated_at: 120,
    };
    store
        .put_session_output_route(&route)
        .expect("put output route");

    assert_eq!(
        store.get_delivery_target("ops-status").expect("get target"),
        Some(target.clone())
    );
    assert_eq!(
        store.list_delivery_targets().expect("list targets"),
        vec![target]
    );
    assert_eq!(
        store
            .get_session_output_route("route-session-monitor-ops-status")
            .expect("get route"),
        Some(route.clone())
    );
    assert_eq!(
        store
            .list_enabled_session_output_routes("session-monitor")
            .expect("list enabled routes"),
        vec![route.clone()]
    );

    let disabled = SessionOutputRouteRecord {
        route_id: "route-session-monitor-muted".to_string(),
        target_id: "ops-status".to_string(),
        enabled: false,
        created_at: 130,
        updated_at: 130,
        ..route
    };
    store
        .put_session_output_route(&disabled)
        .expect("put disabled route");
    assert_eq!(
        store
            .list_enabled_session_output_routes("session-monitor")
            .expect("list enabled routes after disabled insert")
            .len(),
        1
    );
    assert_eq!(
        store
            .list_enabled_session_output_routes_for_target_kind("telegram")
            .expect("list enabled telegram routes")
            .into_iter()
            .map(|route| route.route_id)
            .collect::<Vec<_>>(),
        vec!["route-session-monitor-ops-status".to_string()]
    );
}

#[test]
fn delivery_repository_round_trips_task_followers() {
    let temp = tempfile::tempdir().expect("tempdir");
    let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
        data_dir: temp.path().join("state-root"),
        ..crate::AppConfig::default()
    });
    let store = super::super::PersistenceStore::open(&scaffold).expect("open store");

    let target = DeliveryTargetRecord {
        target_id: "ops-status".to_string(),
        kind: "telegram".to_string(),
        address: "-100100200300".to_string(),
        scope: "group".to_string(),
        owner_user_id: Some("telegram:42".to_string()),
        allowed_agent_ids_json: "[]".to_string(),
        allowed_session_ids_json: "[]".to_string(),
        send_policy_json: "{}".to_string(),
        format_policy: "full_text".to_string(),
        created_at: 100,
        updated_at: 100,
    };
    store
        .put_delivery_target(&target)
        .expect("put delivery target");
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
            context_ref_json: "{}".to_string(),
            result_ref_json: None,
            retry_policy_json: "{}".to_string(),
            attempt_count: 0,
            max_attempts: 1,
            timeout_at: None,
            chain_id: None,
            hop_count: None,
            max_hops: None,
            trace_id: None,
            created_at: 100,
            updated_at: 100,
            started_at: None,
            finished_at: None,
            error: None,
        })
        .expect("put task");

    let follower = TaskFollowerRecord {
        follower_id: "follow-task-agent-1-ops-status".to_string(),
        task_id: "task-agent-1".to_string(),
        target_id: "ops-status".to_string(),
        enabled: true,
        created_by_user_id: Some("telegram:42".to_string()),
        created_at: 110,
        updated_at: 120,
        delivered_at: None,
        last_error: None,
    };
    store
        .put_task_follower(&follower)
        .expect("put task follower");

    assert_eq!(
        store
            .get_task_follower("follow-task-agent-1-ops-status")
            .expect("get follower"),
        Some(follower.clone())
    );
    assert_eq!(
        store
            .list_enabled_task_followers("task-agent-1")
            .expect("list enabled followers"),
        vec![follower.clone()]
    );

    let disabled = TaskFollowerRecord {
        enabled: false,
        updated_at: 130,
        ..follower
    };
    store
        .put_task_follower(&disabled)
        .expect("disable task follower");

    assert_eq!(
        store
            .list_enabled_task_followers("task-agent-1")
            .expect("list enabled followers after disable"),
        Vec::<TaskFollowerRecord>::new()
    );
    assert_eq!(
        store
            .list_task_followers("task-agent-1")
            .expect("list all followers"),
        vec![disabled]
    );
}
