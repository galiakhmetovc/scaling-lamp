use agent_persistence::{
    AppConfig, EventRepository, InboundEventRecord, PersistenceStore, RoutedEventRecord,
    SessionRecord, SessionRepository, TaskRegistryRepository, TranscriptRepository,
};
use agent_runtime::provider::{ConfiguredProvider, ProviderKind};
use agent_runtime::session::SessionSettings;
use agentd::bootstrap;
use agentd::session_worker::{SessionWorkerStatus, execute_routed_session_event};
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

fn test_app(provider_api_base: &str) -> (tempfile::TempDir, bootstrap::App) {
    let temp = tempfile::tempdir().expect("tempdir");
    let config = AppConfig {
        data_dir: temp.path().join("teamd-state"),
        provider: ConfiguredProvider {
            kind: ProviderKind::OpenAiResponses,
            api_base: Some(provider_api_base.to_string()),
            api_key: Some("test-key".to_string()),
            default_model: Some("gpt-test".to_string()),
            ..ConfiguredProvider::default()
        },
        ..AppConfig::default()
    };
    let app = bootstrap::build_from_config(config).expect("build app");
    (temp, app)
}

fn store(app: &bootstrap::App) -> PersistenceStore {
    PersistenceStore::open(&app.persistence).expect("open store")
}

fn put_session(store: &PersistenceStore) {
    store
        .put_session(&SessionRecord {
            id: "session-worker".to_string(),
            title: "session-worker".to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default())
                .expect("settings json"),
            workspace_root: ".".to_string(),
            agent_profile_id: "default".to_string(),
            active_mission_id: None,
            parent_session_id: None,
            parent_job_id: None,
            delegation_label: None,
            created_at: 100,
            updated_at: 100,
        })
        .expect("put session");
}

fn put_inbound_and_routed(store: &PersistenceStore, routed_event_id: &str, metadata_json: &str) {
    store
        .put_inbound_event(&InboundEventRecord {
            event_id: "event-worker-inbound".to_string(),
            dedupe_key: "dedupe:event-worker-inbound".to_string(),
            source_kind: "telegram".to_string(),
            source_id: "telegram-chat-42".to_string(),
            operator_id: Some("telegram-user-7".to_string()),
            payload_json: json!({"text": "hello from event", "chat_id": 42}).to_string(),
            metadata_json: json!({"trace_id": "trace-session-worker"}).to_string(),
            status: "pending".to_string(),
            received_at: 110,
            published_at: None,
            error: None,
        })
        .expect("put inbound");
    store
        .put_routed_event(&RoutedEventRecord {
            routed_event_id: routed_event_id.to_string(),
            inbound_event_id: "event-worker-inbound".to_string(),
            rule_id: None,
            session_id: "session-worker".to_string(),
            agent_id: "default".to_string(),
            queue_policy: "fifo".to_string(),
            priority: 10,
            payload_json: json!({"text": "hello from event", "chat_id": 42}).to_string(),
            metadata_json: metadata_json.to_string(),
            status: "pending".to_string(),
            routed_at: 120,
            published_at: None,
            error: None,
        })
        .expect("put routed");
}

#[test]
fn routed_session_input_executes_canonical_chat_and_publishes_output_event() {
    let (provider_api_base, provider_requests, provider_handle) =
        spawn_json_server(openai_message_response_json("resp_worker", "worker done"));
    let (_temp, app) = test_app(&provider_api_base);
    let store = store(&app);
    put_session(&store);
    put_inbound_and_routed(&store, "routed-worker-1", r#"{"labels":["worker-test"]}"#);

    let report =
        execute_routed_session_event(&app, "routed-worker-1", 130).expect("execute routed event");

    assert_eq!(report.status, SessionWorkerStatus::Completed);
    assert_eq!(report.session_id, "session-worker");
    assert!(
        report
            .run_id
            .as_deref()
            .is_some_and(|run_id| !run_id.is_empty())
    );

    let transcripts = store
        .list_transcripts_for_session("session-worker")
        .expect("session transcripts");
    assert!(
        transcripts
            .iter()
            .any(|entry| entry.kind == "user" && entry.content == "hello from event")
    );
    assert!(
        transcripts
            .iter()
            .any(|entry| entry.kind == "assistant" && entry.content == "worker done")
    );

    let task = store
        .get_task_registry("task-routed-worker-1")
        .expect("get task")
        .expect("task exists");
    assert_eq!(task.status, "completed");
    assert_eq!(task.executor_agent_id.as_deref(), Some("default"));
    assert!(
        task.result_ref_json
            .as_deref()
            .unwrap_or("")
            .contains("run_id")
    );

    let outbox = store
        .get_event_outbox("outbox-output-routed-worker-1")
        .expect("get output outbox")
        .expect("output outbox exists");
    assert_eq!(outbox.subject, "teamd.session.session-worker.output");
    let envelope: serde_json::Value =
        serde_json::from_str(&outbox.payload_json).expect("output envelope");
    assert_eq!(envelope["event_type"], "session.output.created");
    assert_eq!(envelope["payload_ref"]["table"], "runs");
    assert_eq!(envelope["payload_ref"]["id"], report.run_id.unwrap());

    let provider_request = provider_requests
        .recv_timeout(Duration::from_secs(2))
        .expect("provider request");
    assert!(provider_request.contains("hello from event"));
    assert!(
        !provider_request.contains("chat_id"),
        "session worker must pass normalized message text, not Telegram transport metadata"
    );
    provider_handle.join().expect("provider thread");
}

#[test]
fn routed_session_input_with_dependencies_waits_without_running_provider() {
    let (_temp, app) = test_app("http://127.0.0.1:9");
    let store = store(&app);
    put_session(&store);
    put_inbound_and_routed(
        &store,
        "routed-waiting-1",
        r#"{"dependencies":["task-parent"]}"#,
    );

    let report =
        execute_routed_session_event(&app, "routed-waiting-1", 130).expect("execute routed event");

    assert_eq!(report.status, SessionWorkerStatus::WaitingDependency);
    assert_eq!(
        store
            .count_transcripts_for_session("session-worker")
            .unwrap(),
        0
    );
    let task = store
        .get_task_registry("task-routed-waiting-1")
        .expect("get task")
        .expect("task exists");
    assert_eq!(task.status, "waiting_dependency");
    assert!(task.dependency_json.contains("task-parent"));
}

fn openai_message_response_json(response_id: &str, text: &str) -> String {
    let text = serde_json::to_string(text).expect("serialize text");
    format!(
        "{{\"id\":\"{response_id}\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":{text}}}]}}],\"usage\":{{\"input_tokens\":16,\"output_tokens\":3,\"total_tokens\":19}}}}"
    )
}

fn spawn_json_server(body: String) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut raw_request = String::new();
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("read line");
            if line.eq("\r\n") || line.is_empty() {
                break;
            }
            if line.to_ascii_lowercase().starts_with("content-length:")
                && let Some((_, value)) = line.split_once(':')
            {
                content_length = value.trim().parse::<usize>().unwrap_or(0);
            }
            raw_request.push_str(&line);
        }
        if content_length > 0 {
            let mut body_bytes = vec![0; content_length];
            reader.read_exact(&mut body_bytes).expect("read body");
            raw_request.push_str(&String::from_utf8_lossy(&body_bytes));
        }
        let _ = sender.send(raw_request);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    });
    (format!("http://{address}"), receiver, handle)
}
