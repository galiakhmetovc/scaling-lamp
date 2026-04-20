use agent_persistence::AppConfig;
use agentd::bootstrap;
use agentd::daemon;
use agentd::http::client::{DaemonClient, DaemonConnectOptions, connect_or_autospawn};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn test_config() -> (tempfile::TempDir, AppConfig) {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("state-root"),
        ..AppConfig::default()
    };
    config.daemon.bind_host = "127.0.0.1".to_string();
    config.daemon.bind_port = free_port();
    config.daemon.bearer_token = Some("secret-token".to_string());
    (temp, config)
}

#[test]
fn daemon_client_can_talk_to_running_daemon() {
    let (_temp, config) = test_config();
    let app = bootstrap::build_from_config(config.clone()).expect("build app");
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");

    let client = DaemonClient::new(&config, &DaemonConnectOptions::default());
    let status = client.status().expect("status");
    assert!(status.ok);

    let created = client
        .create_session_auto(Some("Remote Session"))
        .expect("create session");
    assert_eq!(created.title, "Remote Session");
    let sessions = client.list_session_summaries().expect("list sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, created.id);

    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_client_autospawns_local_daemon_when_missing() {
    let (_temp, config) = test_config();
    let handle_cell = Arc::new(Mutex::new(None));
    let handle_cell_clone = handle_cell.clone();
    let config_clone = config.clone();

    let client = connect_or_autospawn(&config, &DaemonConnectOptions::default(), move || {
        let app = bootstrap::build_from_config(config_clone).expect("build spawned app");
        let handle = daemon::spawn_for_test(app).expect("spawn daemon");
        *handle_cell_clone.lock().expect("lock handle") = Some(handle);
        Ok(())
    })
    .expect("connect or spawn");

    let status = client.status().expect("status");
    assert!(status.ok);

    handle_cell
        .lock()
        .expect("lock handle")
        .take()
        .expect("spawned handle")
        .stop()
        .expect("stop daemon");
}

#[test]
fn daemon_client_does_not_autospawn_for_explicit_remote_target() {
    let (_temp, config) = test_config();
    let spawn_count = Arc::new(Mutex::new(0usize));
    let spawn_count_clone = spawn_count.clone();

    let error = connect_or_autospawn(
        &config,
        &DaemonConnectOptions {
            host: Some("10.6.5.3".to_string()),
            port: Some(5140),
        },
        move || {
            *spawn_count_clone.lock().expect("lock count") += 1;
            Ok(())
        },
    )
    .expect_err("remote target should not autospawn");

    assert!(error.to_string().contains("daemon"));
    assert_eq!(*spawn_count.lock().expect("lock count"), 0);
}

#[test]
fn daemon_client_uses_loopback_when_daemon_binds_all_interfaces() {
    let (_temp, mut config) = test_config();
    config.daemon.bind_host = "0.0.0.0".to_string();
    let app = bootstrap::build_from_config(config.clone()).expect("build app");
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");

    let client = DaemonClient::new(&config, &DaemonConnectOptions::default());
    let status = client.status().expect("status over wildcard bind");
    assert!(status.ok);
    assert_eq!(status.bind_host, "0.0.0.0");

    let session = client
        .create_session_auto(Some("Wildcard Bind Session"))
        .expect("create session over loopback");
    assert_eq!(session.title, "Wildcard Bind Session");

    handle.stop().expect("stop daemon");
}
