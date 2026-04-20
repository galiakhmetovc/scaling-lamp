use agent_persistence::AppConfig;
use agentd::bootstrap;
use agentd::daemon;
use agentd::http::types::{
    CreateSessionRequest, DaemonStopResponse, ErrorResponse, SessionSummaryResponse, SkillCommandRequest,
    StatusResponse,
};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

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
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("http client");

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
            id: None,
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

#[test]
fn daemon_http_lists_and_updates_session_skills() {
    let temp = tempfile::tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(skills_dir.join("rust-debug")).expect("rust skill dir");
    std::fs::write(
        skills_dir.join("rust-debug").join("SKILL.md"),
        "---\nname: rust-debug\ndescription: Debug Rust compiler errors and cargo regressions.\n---\n\n# rust-debug\n",
    )
    .expect("write skill");

    let mut config = AppConfig {
        data_dir: temp.path().join("teamd-state"),
        ..AppConfig::default()
    };
    config.daemon.bind_host = "127.0.0.1".to_string();
    config.daemon.bind_port = free_port();
    config.daemon.bearer_token = Some("secret-token".to_string());
    config.daemon.skills_dir = skills_dir;
    let base_url = format!(
        "http://{}:{}",
        config.daemon.bind_host, config.daemon.bind_port
    );
    let app = bootstrap::build_from_config(config).expect("build app");
    let session = app
        .create_session_auto(Some("Daemon Skill Session"))
        .expect("create session");
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let listed = client
        .get(format!("{base_url}/v1/sessions/{}/skills", session.id))
        .bearer_auth("secret-token")
        .send()
        .expect("list skills");
    assert_eq!(listed.status(), StatusCode::OK);
    let listed: Vec<bootstrap::SessionSkillStatus> = listed.json().expect("skills json");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].mode, "inactive");

    let enabled = client
        .post(format!(
            "{base_url}/v1/sessions/{}/skills/enable",
            session.id
        ))
        .bearer_auth("secret-token")
        .json(&SkillCommandRequest {
            name: "rust-debug".to_string(),
        })
        .send()
        .expect("enable skill");
    assert_eq!(enabled.status(), StatusCode::OK);
    let enabled: Vec<bootstrap::SessionSkillStatus> = enabled.json().expect("enabled json");
    assert_eq!(enabled[0].mode, "manual");

    let disabled = client
        .post(format!(
            "{base_url}/v1/sessions/{}/skills/disable",
            session.id
        ))
        .bearer_auth("secret-token")
        .json(&SkillCommandRequest {
            name: "rust-debug".to_string(),
        })
        .send()
        .expect("disable skill");
    assert_eq!(disabled.status(), StatusCode::OK);
    let disabled: Vec<bootstrap::SessionSkillStatus> = disabled.json().expect("disabled json");
    assert_eq!(disabled[0].mode, "disabled");

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_http_stop_shuts_down_a_running_server() {
    let (_temp, app, base_url) = test_app(Some("secret-token"));
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::new();

    let response = client
        .post(format!("{base_url}/v1/daemon/stop"))
        .bearer_auth("secret-token")
        .json(&serde_json::json!({}))
        .send()
        .expect("stop request");

    assert_eq!(response.status(), StatusCode::OK);
    let body: DaemonStopResponse = response.json().expect("stop json");
    assert!(body.stopping);

    thread::sleep(Duration::from_millis(250));

    let result = client
        .get(format!("{base_url}/v1/status"))
        .bearer_auth("secret-token")
        .send();
    assert!(result.is_err(), "daemon should stop answering status");

    handle.stop().expect("join stopped daemon");
}
