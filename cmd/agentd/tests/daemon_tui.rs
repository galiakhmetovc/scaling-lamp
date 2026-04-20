use agent_persistence::AppConfig;
use agent_runtime::provider::{ConfiguredProvider, ProviderKind};
use agentd::bootstrap;
use agentd::daemon;
use agentd::http::client::{DaemonClient, DaemonConnectOptions, connect_or_autospawn};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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

fn spawn_delayed_json_server_sequence(
    responses: Vec<(Duration, String)>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for (delay, body) in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");

            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut raw_request = String::new();
            let mut content_length = 0usize;

            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request line");
                raw_request.push_str(&line);

                if line == "\r\n" {
                    break;
                }

                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = value.trim().parse().expect("parse content length");
                }
            }

            let mut body_buf = vec![0u8; content_length];
            reader.read_exact(&mut body_buf).expect("read request body");
            raw_request.push_str(std::str::from_utf8(&body_buf).expect("utf8 body"));
            sender.send(raw_request).expect("send request");

            thread::sleep(delay);

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        }
    });

    (format!("http://{address}"), receiver, handle)
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

#[test]
fn daemon_client_chat_turn_waits_for_slow_daemon_responses() {
    let (_temp, mut config) = test_config();
    let (provider_api_base, provider_requests, provider_handle) =
        spawn_delayed_json_server_sequence(vec![(
            Duration::from_secs(6),
            r#"{
                "id":"resp_slow",
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
                                "text":"slow hello"
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":12,"output_tokens":4,"total_tokens":16}
            }"#
            .to_string(),
        )]);
    config.provider = ConfiguredProvider {
        kind: ProviderKind::OpenAiResponses,
        api_base: Some(format!("{provider_api_base}/v1")),
        api_key: Some("test-key".to_string()),
        default_model: Some("gpt-5.4".to_string()),
        ..ConfiguredProvider::default()
    };

    let app = bootstrap::build_from_config(config.clone()).expect("build app");
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = DaemonClient::new(&config, &DaemonConnectOptions::default());
    let session = client
        .create_session_auto(Some("Slow Remote Session"))
        .expect("create session");

    let report = client
        .execute_chat_turn_with_control_and_observer(&session.id, "Привет", 10, None, &mut |_| {})
        .expect("chat turn should wait for slow daemon response");
    let provider_request = provider_requests.recv().expect("provider request");
    provider_handle.join().expect("join provider");

    assert_eq!(report.output_text, "slow hello");
    assert!(
        provider_request
            .to_ascii_lowercase()
            .contains("/v1/responses")
    );

    handle.stop().expect("stop daemon");
}
