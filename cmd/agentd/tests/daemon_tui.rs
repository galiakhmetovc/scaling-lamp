use agent_persistence::AppConfig;
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
use agentd::tui::app::TuiAppState;
use agentd::tui::events::TuiAction;
use agentd::tui::timeline::{Timeline, TimelineEntryKind};
use agentd::tui::{dispatch_action, pump_background};
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
            tool_name,
            status: ToolExecutionStatus::Requested,
            ..
        } if tool_name == "web_fetch"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        ChatExecutionEvent::ToolStatus {
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
        first_event_at < Duration::from_millis(500),
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

fn wait_for_daemon_tui_idle(
    client: &DaemonClient,
    state: &mut TuiAppState,
    redraw: &mut dyn FnMut(&TuiAppState) -> Result<(), BootstrapError>,
) {
    for _ in 0..100 {
        pump_background(client, state, redraw).expect("pump background");
        if !state.has_active_run() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("daemon-backed tui did not become idle in time");
}
