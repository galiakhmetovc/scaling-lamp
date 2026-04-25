use agent_persistence::{AppConfig, TranscriptRepository};
use agent_runtime::permission::{
    PermissionAction, PermissionConfig, PermissionMode, PermissionRule,
};
use agent_runtime::provider::{ConfiguredProvider, ProviderKind};
use agentd::bootstrap;
use agentd::bootstrap::BootstrapError;
use agentd::daemon;
use agentd::execution::{ChatExecutionEvent, ExecutionError, ToolExecutionStatus};
use agentd::http::client::{
    DaemonClient, DaemonConnectOptions, connect_or_autospawn, connect_or_autospawn_detailed,
};
use agentd::http::types::{DaemonStopResponse, StatusResponse};
use agentd::tui::app::TuiAppState;
use agentd::tui::events::TuiAction;
use agentd::tui::timeline::{Timeline, TimelineEntryKind};
use agentd::tui::{dispatch_action, pump_background};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Response, Server, StatusCode};

const TEST_SERVER_READ_TIMEOUT: Duration = Duration::from_secs(15);

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

fn spawn_delayed_sse_server_sequence(
    responses: Vec<(Duration, String)>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for (delay, body) in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(TEST_SERVER_READ_TIMEOUT))
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
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
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
                .set_read_timeout(Some(TEST_SERVER_READ_TIMEOUT))
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

fn spawn_sse_server_sequence(
    bodies: Vec<String>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(TEST_SERVER_READ_TIMEOUT))
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

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
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

fn spawn_streaming_sse_server_sequence(
    responses: Vec<Vec<(Duration, String)>>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for chunks in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(TEST_SERVER_READ_TIMEOUT))
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

            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n",
                )
                .expect("write headers");
            stream.flush().expect("flush headers");

            for (delay, chunk) in chunks {
                thread::sleep(delay);
                stream.write_all(chunk.as_bytes()).expect("write chunk");
                stream.flush().expect("flush chunk");
            }
        }
    });

    (format!("http://{address}"), receiver, handle)
}

fn render_transcript_dump(label: &str, transcript: &bootstrap::SessionTranscriptView) -> String {
    let mut rendered = format!("=== {label} ({}) ===\n", transcript.session_id);
    for entry in &transcript.entries {
        rendered.push_str(&format!("[{}] {}\n", entry.role, entry.content));
    }
    rendered
}

fn write_test_artifact(name: &str, content: &str) -> PathBuf {
    let dir = PathBuf::from("target/test-artifacts");
    fs::create_dir_all(&dir).expect("create test artifact dir");
    let path = dir.join(name);
    fs::write(&path, content).expect("write test artifact");
    path
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
    let (provider_api_base, provider_requests, provider_handle) = spawn_delayed_sse_server_sequence(
        vec![(
            Duration::from_secs(6),
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_slow\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"slow hello\"}]}],\"usage\":{\"input_tokens\":12,\"output_tokens\":4,\"total_tokens\":16}}}\n\n".to_string(),
        )],
    );
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

#[test]
fn daemon_client_compact_session_waits_for_slow_daemon_responses() {
    let (_temp, mut config) = test_config();
    let (provider_api_base, provider_requests, provider_handle) =
        spawn_delayed_json_server_sequence(vec![(
            Duration::from_secs(6),
            r#"{
                "id":"resp_compact_slow",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_compact_slow",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Сжатое раннее состояние."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":42,"output_tokens":9,"total_tokens":51}
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
    let store = agent_persistence::PersistenceStore::open(&app.persistence).expect("open store");
    let session = app
        .create_session_auto(Some("Slow Compact Session"))
        .expect("create session");
    for (index, (kind, content)) in [
        ("user", "covered user one"),
        ("assistant", "covered assistant one"),
        ("user", "recent user one"),
        ("assistant", "recent assistant one"),
        ("user", "recent user two"),
        ("assistant", "recent assistant two"),
        ("user", "recent user three"),
        ("assistant", "recent assistant three"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .put_transcript(&agent_persistence::TranscriptRecord {
                id: format!("compact-transcript-{index}"),
                session_id: session.id.clone(),
                run_id: None,
                kind: kind.to_string(),
                content: content.to_string(),
                created_at: 10 + index as i64,
            })
            .expect("put transcript");
    }

    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = DaemonClient::new(&config, &DaemonConnectOptions::default());

    let summary = client
        .compact_session(&session.id)
        .expect("compact session should wait for slow daemon response");
    let provider_request = provider_requests.recv().expect("provider request");
    provider_handle.join().expect("join provider");
    handle.stop().expect("stop daemon");

    assert_eq!(summary.compactifications, 1);
    assert!(
        provider_request
            .to_ascii_lowercase()
            .contains("/v1/responses")
    );
}

#[test]
fn daemon_client_streams_reasoning_and_text_events_from_chat_turn() {
    let (_temp, mut config) = test_config();
    let stream = "data: {\"type\":\"response.reasoning_summary_text.delta\",\"item_id\":\"rs_1\",\"output_index\":0,\"summary_index\":0,\"delta\":\"Compare context. \"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_1\",\"output_index\":1,\"content_index\":0,\"delta\":\"hello \"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_1\",\"output_index\":1,\"content_index\":0,\"delta\":\"from daemon\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_stream_daemon\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"rs_1\",\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"Compare context. \"}]},{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello from daemon\",\"annotations\":[]}]}],\"usage\":{\"input_tokens\":11,\"output_tokens\":7,\"total_tokens\":18}}}\n\n".to_string();
    let (provider_api_base, _provider_requests, provider_handle) =
        spawn_sse_server_sequence(vec![stream]);
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
        .create_session_auto(Some("Streamed Remote Session"))
        .expect("create session");
    let mut events = Vec::new();

    let report = client
        .execute_chat_turn_with_control_and_observer(
            &session.id,
            "Привет",
            10,
            None,
            &mut |event| {
                events.push(event);
            },
        )
        .expect("chat turn should stream intermediate events");

    provider_handle.join().expect("join provider");
    handle.stop().expect("stop daemon");

    assert_eq!(report.output_text, "hello from daemon");
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ChatExecutionEvent::ReasoningDelta(delta) if delta == "Compare context. "))
    );
    assert!(events.iter().any(
        |event| matches!(event, ChatExecutionEvent::AssistantTextDelta(delta) if delta == "hello ")
    ));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ChatExecutionEvent::AssistantTextDelta(delta) if delta == "from daemon"))
    );
}

#[test]
fn daemon_client_streams_tool_status_before_approval_required() {
    let (_temp, mut config) = test_config();
    let stream = "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_tool_daemon\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"fc_1\",\"type\":\"function_call\",\"status\":\"completed\",\"call_id\":\"call_web_fetch\",\"name\":\"web_fetch\",\"arguments\":\"{\\\"url\\\":\\\"https://example.com/weather\\\"}\"}],\"usage\":{\"input_tokens\":19,\"output_tokens\":7,\"total_tokens\":26}}}\n\n".to_string();
    let (provider_api_base, _provider_requests, provider_handle) =
        spawn_sse_server_sequence(vec![stream]);
    config.provider = ConfiguredProvider {
        kind: ProviderKind::OpenAiResponses,
        api_base: Some(format!("{provider_api_base}/v1")),
        api_key: Some("test-key".to_string()),
        default_model: Some("gpt-5.4".to_string()),
        ..ConfiguredProvider::default()
    };
    config.permissions = PermissionConfig {
        mode: PermissionMode::Default,
        rules: vec![PermissionRule {
            action: PermissionAction::Ask,
            tool: Some("web_fetch".to_string()),
            family: None,
            path_prefix: None,
        }],
    };

    let app = bootstrap::build_from_config(config.clone()).expect("build app");
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = DaemonClient::new(&config, &DaemonConnectOptions::default());
    let session = client
        .create_session_auto(Some("Approval Remote Session"))
        .expect("create session");
    let mut events = Vec::new();

    let error = client
        .execute_chat_turn_with_control_and_observer(
            &session.id,
            "Какая погода в Москве?",
            10,
            None,
            &mut |event| events.push(event),
        )
        .expect_err("chat turn should require approval");

    provider_handle.join().expect("join provider");
    handle.stop().expect("stop daemon");

    assert!(matches!(
        error,
        BootstrapError::Execution(ExecutionError::ApprovalRequired { .. })
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        ChatExecutionEvent::ToolStatus {
            tool_call_id: _,
            tool_name,
            status: ToolExecutionStatus::Requested,
            ..
        } if tool_name == "web_fetch"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        ChatExecutionEvent::ToolStatus {
            tool_call_id: _,
            tool_name,
            status: ToolExecutionStatus::WaitingApproval,
            ..
        } if tool_name == "web_fetch"
    )));
}

#[test]
fn daemon_client_delivers_stream_events_before_the_final_outcome() {
    let (_temp, mut config) = test_config();
    let chunks = vec![
        (
            Duration::from_millis(50),
            "data: {\"type\":\"response.reasoning_summary_text.delta\",\"item_id\":\"rs_timed\",\"output_index\":0,\"summary_index\":0,\"delta\":\"step one \"}\n\n".to_string(),
        ),
        (
            Duration::from_millis(700),
            "data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_timed\",\"output_index\":1,\"content_index\":0,\"delta\":\"hello \"}\n\n".to_string(),
        ),
        (
            Duration::from_millis(700),
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_timed\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"rs_timed\",\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"step one \"}]},{\"id\":\"msg_timed\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello daemon\",\"annotations\":[]}]}],\"usage\":{\"input_tokens\":11,\"output_tokens\":7,\"total_tokens\":18}}}\n\n".to_string(),
        ),
    ];
    let (provider_api_base, _provider_requests, provider_handle) =
        spawn_streaming_sse_server_sequence(vec![chunks]);
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
        .create_session_auto(Some("Timed Stream Session"))
        .expect("create session");

    let started = std::time::Instant::now();
    let mut first_event_at = None;
    let report = client
        .execute_chat_turn_with_control_and_observer(
            &session.id,
            "Привет",
            10,
            None,
            &mut |event| {
                if first_event_at.is_none()
                    && matches!(
                        event,
                        ChatExecutionEvent::ReasoningDelta(_)
                            | ChatExecutionEvent::AssistantTextDelta(_)
                    )
                {
                    first_event_at = Some(started.elapsed());
                }
            },
        )
        .expect("chat turn should succeed");
    let total_elapsed = started.elapsed();

    provider_handle.join().expect("join provider");
    handle.stop().expect("stop daemon");

    assert_eq!(report.output_text, "hello daemon");
    let first_event_at = first_event_at.expect("expected at least one streamed event");
    assert!(
        first_event_at < Duration::from_secs(3),
        "first stream event arrived too late: {:?}",
        first_event_at
    );
    assert!(
        total_elapsed > Duration::from_millis(1200),
        "total elapsed too short to prove staged streaming: {:?}",
        total_elapsed
    );
}

#[test]
fn daemon_client_can_query_pending_approvals_while_chat_turn_is_in_flight() {
    let (_temp, mut config) = test_config();
    let (provider_api_base, _provider_requests, provider_handle) = spawn_delayed_sse_server_sequence(
        vec![(
            Duration::from_secs(2),
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_inflight\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"msg_inflight\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"done\"}]}],\"usage\":{\"input_tokens\":12,\"output_tokens\":4,\"total_tokens\":16}}}\n\n".to_string(),
        )],
    );
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
        .create_session_auto(Some("Inflight Session"))
        .expect("create session");

    let client_for_chat = client.clone();
    let session_id = session.id.clone();
    let chat_thread = thread::spawn(move || {
        client_for_chat.execute_chat_turn_with_control_and_observer(
            &session_id,
            "Привет",
            10,
            None,
            &mut |_| {},
        )
    });

    thread::sleep(Duration::from_millis(200));
    let started = std::time::Instant::now();
    let approvals = client
        .pending_approvals(&session.id)
        .expect("query approvals while chat turn is in flight");
    let elapsed = started.elapsed();

    assert!(approvals.is_empty());
    assert!(
        elapsed < Duration::from_secs(1),
        "pending approvals request should not block on the in-flight chat turn, got {:?}",
        elapsed
    );

    let report = chat_thread
        .join()
        .expect("join chat thread")
        .expect("chat turn");
    assert_eq!(report.output_text, "done");
    provider_handle.join().expect("join provider");
    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_backed_tui_shows_pending_approval_in_timeline() {
    let (_temp, mut config) = test_config();
    let stream = "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_tool_daemon_tui\",\"model\":\"gpt-5.4\",\"output\":[{\"id\":\"fc_1\",\"type\":\"function_call\",\"status\":\"completed\",\"call_id\":\"call_web_fetch\",\"name\":\"web_fetch\",\"arguments\":\"{\\\"url\\\":\\\"https://example.com/weather\\\"}\"}],\"usage\":{\"input_tokens\":19,\"output_tokens\":7,\"total_tokens\":26}}}\n\n".to_string();
    let (provider_api_base, _provider_requests, provider_handle) =
        spawn_sse_server_sequence(vec![stream]);
    config.provider = ConfiguredProvider {
        kind: ProviderKind::OpenAiResponses,
        api_base: Some(format!("{provider_api_base}/v1")),
        api_key: Some("test-key".to_string()),
        default_model: Some("gpt-5.4".to_string()),
        ..ConfiguredProvider::default()
    };
    config.permissions = PermissionConfig {
        mode: PermissionMode::Default,
        rules: vec![PermissionRule {
            action: PermissionAction::Ask,
            tool: Some("web_fetch".to_string()),
            family: None,
            path_prefix: None,
        }],
    };

    let app = bootstrap::build_from_config(config.clone()).expect("build app");
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = DaemonClient::new(&config, &DaemonConnectOptions::default());
    let session = client
        .create_session_auto(Some("Approval TUI Session"))
        .expect("create session");

    let mut state = TuiAppState::new(
        client.list_session_summaries().expect("list sessions"),
        Some(session.id.clone()),
    );
    state.set_current_session(
        client
            .session_summary(&session.id)
            .expect("session summary"),
        Timeline::default(),
    );
    let mut redraw = |_state: &TuiAppState| Ok::<_, BootstrapError>(());

    dispatch_action(
        &client,
        &mut state,
        TuiAction::SubmitChatInput("Нужна погода".to_string()),
        &mut redraw,
    )
    .expect("dispatch chat");
    wait_for_daemon_tui_idle(&client, &mut state, &mut redraw);

    assert!(
        state
            .timeline()
            .entries(true)
            .iter()
            .any(|entry| matches!(entry.kind, TimelineEntryKind::Approval { .. })),
        "daemon-backed TUI should show the pending approval in the timeline"
    );

    provider_handle.join().expect("join provider");
    handle.stop().expect("stop daemon");
}

#[test]
fn daemon_backed_tui_can_send_judge_message_and_observe_child_reply() {
    let (_temp, mut config) = test_config();
    let (provider_api_base, provider_requests, provider_handle) =
        spawn_delayed_json_server_sequence(vec![
            (
                Duration::from_millis(50),
                r#"{
                "id":"resp_judge_tui",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_judge_tui",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Я Judge. Проверяю результаты и выношу вердикт."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":18,"output_tokens":10,"total_tokens":28}
            }"#
                .to_string(),
            ),
            (
                Duration::from_millis(50),
                r#"{
                "id":"resp_origin_tui",
                "model":"gpt-5.4",
                "output":[
                    {
                        "id":"msg_origin_tui",
                        "type":"message",
                        "status":"completed",
                        "role":"assistant",
                        "content":[
                            {
                                "type":"output_text",
                                "text":"Я получил ответ Judge и продолжаю."
                            }
                        ]
                    }
                ],
                "usage":{"input_tokens":16,"output_tokens":8,"total_tokens":24}
            }"#
                .to_string(),
            ),
        ]);
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
    let origin = client
        .create_session_auto(Some("Interagent TUI Session"))
        .expect("create session");

    let mut state = TuiAppState::new(
        client.list_session_summaries().expect("list sessions"),
        Some(origin.id.clone()),
    );
    state.set_current_session(
        client.session_summary(&origin.id).expect("session summary"),
        Timeline::default(),
    );
    let mut redraw = |_state: &TuiAppState| Ok::<_, BootstrapError>(());

    dispatch_action(&client, &mut state, TuiAction::OpenJudgeDialog, &mut redraw)
        .expect("open judge dialog");
    state.dialog_next_field();
    state.set_dialog_input("Кто ты?".to_string());
    dispatch_action(&client, &mut state, TuiAction::ConfirmDialog, &mut redraw)
        .expect("send judge message");

    assert!(state.timeline().entries(true).iter().any(|entry| {
        matches!(entry.kind, TimelineEntryKind::System)
            && entry.content.contains("сообщение отправлено агенту judge")
    }));

    let started = Instant::now();
    let child = loop {
        let sessions = client.list_session_summaries().expect("list sessions");
        state.sync_sessions(sessions.clone());
        if let Some(summary) = sessions
            .into_iter()
            .find(|summary| summary.id != origin.id && summary.agent_profile_id == "judge")
        {
            let child_transcript = client
                .session_transcript(&summary.id)
                .expect("child transcript");
            let origin_transcript = client
                .session_transcript(&origin.id)
                .expect("origin transcript");
            if child_transcript
                .entries
                .iter()
                .any(|entry| entry.content == "Я Judge. Проверяю результаты и выношу вердикт.")
                && origin_transcript
                    .entries
                    .iter()
                    .any(|entry| entry.content == "Я получил ответ Judge и продолжаю.")
            {
                break summary;
            }
        }
        if started.elapsed() > Duration::from_secs(5) {
            panic!("daemon-backed TUI did not observe judge reply in time");
        }
        thread::sleep(Duration::from_millis(50));
    };

    let origin_transcript = client
        .session_transcript(&origin.id)
        .expect("origin transcript");
    let child_transcript = client
        .session_transcript(&child.id)
        .expect("child transcript");
    let mut transcript_dump = String::from("=== origin tui timeline ===\n");
    for entry in state.timeline().entries(true) {
        transcript_dump.push_str(&format!("[{:?}] {}\n", entry.kind, entry.content));
    }
    transcript_dump.push('\n');
    transcript_dump.push_str(&render_transcript_dump(
        "origin transcript",
        &origin_transcript,
    ));
    transcript_dump.push('\n');
    transcript_dump.push_str(&render_transcript_dump(
        "judge child transcript",
        &child_transcript,
    ));
    let artifact_path = write_test_artifact(
        "daemon-tui-interagent-judge-chat.log",
        transcript_dump.as_str(),
    );
    eprintln!(
        "saved daemon-backed TUI interagent transcript: {}\n{}",
        artifact_path.display(),
        transcript_dump
    );

    let child_request = provider_requests.recv().expect("judge request");
    let wake_request = provider_requests.recv().expect("origin wake request");
    provider_handle.join().expect("join provider");
    handle.stop().expect("stop daemon");

    assert_eq!(child.agent_profile_id, "judge");
    assert!(child.title.contains("Judge"));
    assert_eq!(state.sessions().len(), 2);
    assert!(
        state
            .sessions()
            .iter()
            .any(|summary| summary.id == child.id)
    );
    assert!(child_request.contains("[agent:Ассистент]"));
    assert!(child_request.contains("Кто ты?"));
    assert!(wake_request.to_ascii_lowercase().contains("[agent:judge]"));
}

#[test]
fn detailed_daemon_connection_can_shutdown_autospawned_local_daemon() {
    let (_temp, config) = test_config();
    let handle_cell = Arc::new(Mutex::new(None));
    let handle_cell_clone = handle_cell.clone();
    let config_clone = config.clone();

    let connection =
        connect_or_autospawn_detailed(&config, &DaemonConnectOptions::default(), move || {
            let app = bootstrap::build_from_config(config_clone).expect("build spawned app");
            let handle = daemon::spawn_for_test(app).expect("spawn daemon");
            *handle_cell_clone.lock().expect("lock handle") = Some(handle);
            Ok(())
        })
        .expect("connect or spawn");

    assert!(connection.was_autospawned());
    assert!(
        connection
            .client()
            .status()
            .expect("status before shutdown")
            .ok
    );

    connection
        .shutdown_if_autospawned()
        .expect("shutdown owned daemon");
    thread::sleep(Duration::from_millis(250));

    connection
        .client()
        .status()
        .expect_err("daemon should stop");

    handle_cell
        .lock()
        .expect("lock handle")
        .take()
        .expect("spawned handle")
        .stop()
        .expect("join stopped daemon");
}

#[test]
fn daemon_client_restarts_incompatible_local_daemon_build() {
    let (_temp, config) = test_config();
    let current_version = env!("CARGO_PKG_VERSION");
    let current_commit = option_env!("AGENTD_GIT_COMMIT").unwrap_or("unknown");
    let bind = format!("{}:{}", config.daemon.bind_host, config.daemon.bind_port);
    let old_status_port = config.daemon.bind_port;
    let old_data_dir = config.data_dir.display().to_string();
    let old_daemon_running = Arc::new(AtomicBool::new(true));
    let old_daemon_flag = old_daemon_running.clone();

    let old_thread = thread::spawn(move || {
        let server = Server::http(&bind).expect("bind fake old daemon");
        while old_daemon_flag.load(Ordering::Relaxed) {
            let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(100)) else {
                continue;
            };
            match (request.method(), request.url()) {
                (&Method::Get, "/v1/status") => {
                    let payload = serde_json::to_string(&StatusResponse {
                        ok: true,
                        version: Some("1.0.0".to_string()),
                        commit: Some("oldbeef".to_string()),
                        tree_state: Some("clean".to_string()),
                        build_id: Some("old-build".to_string()),
                        bind_host: "127.0.0.1".to_string(),
                        bind_port: old_status_port,
                        permission_mode: "default".to_string(),
                        session_count: 0,
                        mission_count: 0,
                        run_count: 0,
                        job_count: 0,
                        components: 0,
                        data_dir: old_data_dir.clone(),
                        state_db: format!("{old_data_dir}/state.sqlite"),
                    })
                    .expect("serialize status");
                    let response = Response::from_string(payload)
                        .with_status_code(StatusCode(200))
                        .with_header(
                            Header::from_bytes(&b"content-type"[..], &b"application/json"[..])
                                .expect("content-type header"),
                        );
                    request.respond(response).expect("respond status");
                }
                (&Method::Post, "/v1/daemon/stop") => {
                    old_daemon_flag.store(false, Ordering::Relaxed);
                    let payload = serde_json::to_string(&DaemonStopResponse { stopping: true })
                        .expect("serialize stop");
                    let response = Response::from_string(payload)
                        .with_status_code(StatusCode(200))
                        .with_header(
                            Header::from_bytes(&b"content-type"[..], &b"application/json"[..])
                                .expect("content-type header"),
                        );
                    request.respond(response).expect("respond stop");
                }
                _ => {
                    request
                        .respond(Response::empty(StatusCode(404)))
                        .expect("respond 404");
                }
            }
        }
    });

    let handle_cell = Arc::new(Mutex::new(None));
    let handle_cell_clone = handle_cell.clone();
    let config_clone = config.clone();
    let connection =
        connect_or_autospawn_detailed(&config, &DaemonConnectOptions::default(), move || {
            let app = bootstrap::build_from_config(config_clone).expect("build spawned app");
            let handle = daemon::spawn_for_test(app).expect("spawn daemon");
            *handle_cell_clone.lock().expect("lock handle") = Some(handle);
            Ok(())
        })
        .expect("restart incompatible daemon");

    assert!(connection.was_autospawned());
    let status = connection.client().status().expect("status");
    assert_eq!(status.version.as_deref(), Some(current_version));
    assert_eq!(status.commit.as_deref(), Some(current_commit));

    old_thread.join().expect("join old daemon");
    handle_cell
        .lock()
        .expect("lock handle")
        .take()
        .expect("spawned handle")
        .stop()
        .expect("stop daemon");
}

#[test]
fn daemon_client_restarts_local_daemon_when_data_dir_mismatches() {
    let (_temp, config) = test_config();
    let current_version = env!("CARGO_PKG_VERSION");
    let current_commit = option_env!("AGENTD_GIT_COMMIT").unwrap_or("unknown");
    let bind = format!("{}:{}", config.daemon.bind_host, config.daemon.bind_port);
    let old_status_port = config.daemon.bind_port;
    let old_data_dir = config
        .data_dir
        .parent()
        .expect("parent")
        .join("other-state-root")
        .display()
        .to_string();
    let old_daemon_running = Arc::new(AtomicBool::new(true));
    let old_daemon_flag = old_daemon_running.clone();

    let old_thread = thread::spawn(move || {
        let server = Server::http(&bind).expect("bind fake old daemon");
        while old_daemon_flag.load(Ordering::Relaxed) {
            let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(100)) else {
                continue;
            };
            match (request.method(), request.url()) {
                (&Method::Get, "/v1/status") => {
                    let payload = serde_json::to_string(&StatusResponse {
                        ok: true,
                        version: Some(current_version.to_string()),
                        commit: Some(current_commit.to_string()),
                        tree_state: Some(
                            option_env!("AGENTD_GIT_TREE_STATE")
                                .unwrap_or("unknown")
                                .to_string(),
                        ),
                        build_id: Some("other-build".to_string()),
                        bind_host: "127.0.0.1".to_string(),
                        bind_port: old_status_port,
                        permission_mode: "default".to_string(),
                        session_count: 0,
                        mission_count: 0,
                        run_count: 0,
                        job_count: 0,
                        components: 0,
                        data_dir: old_data_dir.clone(),
                        state_db: format!("{old_data_dir}/state.sqlite"),
                    })
                    .expect("serialize status");
                    let response = Response::from_string(payload)
                        .with_status_code(StatusCode(200))
                        .with_header(
                            Header::from_bytes(&b"content-type"[..], &b"application/json"[..])
                                .expect("content-type header"),
                        );
                    request.respond(response).expect("respond status");
                }
                (&Method::Post, "/v1/daemon/stop") => {
                    old_daemon_flag.store(false, Ordering::Relaxed);
                    let payload = serde_json::to_string(&DaemonStopResponse { stopping: true })
                        .expect("serialize stop");
                    let response = Response::from_string(payload)
                        .with_status_code(StatusCode(200))
                        .with_header(
                            Header::from_bytes(&b"content-type"[..], &b"application/json"[..])
                                .expect("content-type header"),
                        );
                    request.respond(response).expect("respond stop");
                }
                _ => {
                    request
                        .respond(Response::empty(StatusCode(404)))
                        .expect("respond 404");
                }
            }
        }
    });

    let handle_cell = Arc::new(Mutex::new(None));
    let handle_cell_clone = handle_cell.clone();
    let config_clone = config.clone();
    let connection =
        connect_or_autospawn_detailed(&config, &DaemonConnectOptions::default(), move || {
            let app = bootstrap::build_from_config(config_clone).expect("build spawned app");
            let handle = daemon::spawn_for_test(app).expect("spawn daemon");
            *handle_cell_clone.lock().expect("lock handle") = Some(handle);
            Ok(())
        })
        .expect("restart mismatched data dir daemon");

    assert!(connection.was_autospawned());
    let status = connection.client().status().expect("status");
    assert_eq!(status.version.as_deref(), Some(current_version));
    assert_eq!(status.commit.as_deref(), Some(current_commit));
    assert_eq!(status.data_dir, config.data_dir.display().to_string());

    old_thread.join().expect("join old daemon");
    handle_cell
        .lock()
        .expect("lock handle")
        .take()
        .expect("spawned handle")
        .stop()
        .expect("stop daemon");
}

fn wait_for_daemon_tui_idle(
    client: &DaemonClient,
    state: &mut TuiAppState,
    redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), BootstrapError>,
) {
    let runtime_timing = agent_persistence::RuntimeTimingConfig::default();
    for _ in 0..500 {
        pump_background(client, state, redraw, &runtime_timing).expect("pump background");
        if !state.has_active_run() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("daemon-backed tui did not become idle in time");
}
