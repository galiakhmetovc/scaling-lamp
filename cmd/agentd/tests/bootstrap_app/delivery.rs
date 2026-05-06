use super::support::*;
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

    let targets = app.list_delivery_targets().expect("list targets");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].id, "ops-status");

    let routes = app
        .list_enabled_session_output_routes(&session.id)
        .expect("list enabled routes");
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].id, "route-server-watcher-ops-status");
}
