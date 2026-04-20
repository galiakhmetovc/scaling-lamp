use agent_persistence::AppConfig;
use agentd::bootstrap;
use agentd::daemon;
use agentd::http::types::{
    CreateSessionRequest, ErrorResponse, SessionSummaryResponse, StatusResponse,
};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use std::net::TcpListener;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn test_app(token: Option<&str>) -> (tempfile::TempDir, bootstrap::App, String) {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("teamd-state"),
        ..AppConfig::default()
    };
    config.daemon.bind_host = "127.0.0.1".to_string();
    config.daemon.bind_port = free_port();
    config.daemon.bearer_token = token.map(str::to_string);
    let base_url = format!(
        "http://{}:{}",
        config.daemon.bind_host, config.daemon.bind_port
    );
    let app = bootstrap::build_from_config(config).expect("build app");
    (temp, app, base_url)
}

#[test]
fn daemon_http_status_is_public_when_no_token_is_configured() {
    let (_temp, app, base_url) = test_app(None);
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let response = client
        .get(format!("{base_url}/v1/status"))
        .send()
        .expect("status request");

    assert_eq!(response.status(), StatusCode::OK);
    let body: StatusResponse = response.json().expect("status json");
    assert!(body.ok);
    assert_eq!(body.bind_host, "127.0.0.1");

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_requires_bearer_token_when_configured() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let unauthorized = client
        .get(format!("{base_url}/v1/status"))
        .send()
        .expect("unauthorized response");

    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let error: ErrorResponse = unauthorized.json().expect("error json");
    assert!(error.error.contains("authorization"));

    let authorized = client
        .get(format!("{base_url}/v1/status"))
        .bearer_auth("secret-token")
        .send()
        .expect("authorized response");

    assert_eq!(authorized.status(), StatusCode::OK);

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_can_create_a_session_over_json() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let response = client
        .post(format!("{base_url}/v1/sessions"))
        .bearer_auth("secret-token")
        .json(&CreateSessionRequest {
            title: Some("Daemon Session".to_string()),
        })
        .send()
        .expect("create session");

    assert_eq!(response.status(), StatusCode::CREATED);
    let session: SessionSummaryResponse = response.json().expect("session json");
    assert_eq!(session.title, "Daemon Session");
    assert_eq!(session.message_count, 0);
    assert!(!session.id.is_empty());

    handle.stop().expect("stop daemon");
}
