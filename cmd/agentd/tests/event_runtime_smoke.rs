use agent_persistence::{
    AppConfig, DeliveryRepository, DeliveryTargetRecord, EventRepository, PersistenceStore,
    RouterRepository, RouterRuleRecord, SessionOutputRouteRecord, SessionRecord, SessionRepository,
    TaskRegistryRepository, TelegramChatBindingRecord, TelegramRepository,
    TelegramUserPairingRecord, TranscriptRepository,
};
use agent_runtime::provider::{ConfiguredProvider, ProviderKind};
use agent_runtime::session::SessionSettings;
use agentd::bootstrap;
use agentd::delivery_worker::{DeliverySendError, DeliverySender, deliver_session_output_event};
use agentd::event_runtime_runner::{
    deliver_session_output_event_envelope, execute_session_input_event_envelope,
    route_input_event_envelope,
};
use agentd::router_worker::route_inbound_event;
use agentd::session_worker::execute_routed_session_event;
use agentd::telegram::webhook::handle_webhook_update;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, Default)]
struct RecordingSender {
    sent: Arc<Mutex<Vec<(String, String)>>>,
}

impl DeliverySender for RecordingSender {
    fn send_text(
        &self,
        target: &DeliveryTargetRecord,
        text: &str,
    ) -> Result<(), DeliverySendError> {
        self.sent
            .lock()
            .expect("sent")
            .push((target.target_id.clone(), text.to_string()));
        Ok(())
    }
}

#[test]
fn telegram_webhook_event_runtime_happy_path_smoke() {
    let (provider_api_base, provider_handle) =
        spawn_json_server(openai_message_response_json("resp_e2e", "e2e response"));
    let (_temp, app) = test_app(&provider_api_base);
    let store = store(&app);
    seed_session_route_and_delivery(&store);

    let webhook = handle_webhook_update(&app, "secret", &telegram_update(900, "e2e hello"), 200)
        .expect("webhook");
    assert_eq!(webhook.event_id, "telegram-update-900");

    let route = route_inbound_event(&app, &webhook.event_id, 201).expect("route");
    assert_eq!(route.matched_rule_id.as_deref(), Some("rule-e2e"));

    let session = execute_routed_session_event(&app, "routed-telegram-update-900", 202)
        .expect("session worker");
    let run_id = session.run_id.clone().expect("run id");

    let sender = RecordingSender::default();
    let delivery = deliver_session_output_event(
        &app,
        &sender,
        "outbox-output-routed-telegram-update-900",
        203,
    )
    .expect("delivery worker");
    assert_eq!(delivery.delivered, 1);

    let transcripts = store
        .list_transcripts_for_session("session-e2e")
        .expect("transcripts");
    assert!(
        transcripts
            .iter()
            .any(|entry| entry.kind == "user" && entry.content == "e2e hello")
    );
    assert!(
        transcripts
            .iter()
            .any(|entry| entry.kind == "assistant" && entry.content == "e2e response")
    );
    assert_eq!(
        sender.sent.lock().expect("sent").as_slice(),
        &[("telegram-main".to_string(), "e2e response".to_string())]
    );

    let task = store
        .get_task_registry("task-routed-telegram-update-900")
        .expect("task")
        .expect("task exists");
    assert_eq!(task.status, "completed");
    assert!(task.result_ref_json.unwrap().contains(&run_id));

    let route_outbox = store
        .get_event_outbox("outbox-routed-telegram-update-900")
        .expect("route outbox")
        .expect("route outbox exists");
    let route_envelope: serde_json::Value =
        serde_json::from_str(&route_outbox.payload_json).expect("route envelope");
    assert_eq!(route_envelope["trace_id"], "trace-telegram-update-900");

    let output_outbox = store
        .get_event_outbox("outbox-output-routed-telegram-update-900")
        .expect("output outbox")
        .expect("output outbox exists");
    let output_envelope: serde_json::Value =
        serde_json::from_str(&output_outbox.payload_json).expect("output envelope");
    assert_eq!(output_envelope["payload_ref"]["id"], run_id);
    assert_eq!(output_envelope["trace_id"], "trace-telegram-update-900");

    let delivery_record = store
        .get_event_delivery("delivery-output-routed-telegram-update-900-telegram-main")
        .expect("delivery")
        .expect("delivery exists");
    assert_eq!(delivery_record.status, "delivered");
    assert_eq!(count_rows(&app, "event_deliveries"), 1);
    assert_eq!(count_rows(&app, "routed_events"), 1);

    let duplicate = handle_webhook_update(&app, "secret", &telegram_update(900, "duplicate"), 204)
        .expect("duplicate webhook");
    assert!(duplicate.duplicate);
    assert_eq!(count_rows(&app, "runs"), 1);
    assert_eq!(count_rows(&app, "event_outbox"), 3);
    assert_eq!(
        count_rows_where(&app, "event_outbox", "subject = 'teamd.dlq'"),
        0
    );

    provider_handle.join().expect("provider thread");
}

#[test]
fn telegram_webhook_envelopes_flow_through_router_session_and_delivery_helpers() {
    let (provider_api_base, provider_handle) = spawn_json_server(openai_message_response_json(
        "resp_runner",
        "runner response",
    ));
    let (_temp, app) = test_app(&provider_api_base);
    let store = store(&app);
    seed_session_route_and_delivery(&store);

    let webhook = handle_webhook_update(&app, "secret", &telegram_update(901, "runner hello"), 300)
        .expect("webhook");
    let input_outbox = store
        .get_event_outbox(webhook.outbox_id.as_deref().expect("input outbox id"))
        .expect("input outbox")
        .expect("input outbox exists");
    let input_envelope = serde_json::from_str(&input_outbox.payload_json).expect("input envelope");

    let route = route_input_event_envelope(&app, input_envelope, 301).expect("route envelope");
    assert_eq!(route.session_id, "session-e2e");

    let session_input_outbox = store
        .get_event_outbox("outbox-routed-telegram-update-901")
        .expect("session input outbox")
        .expect("session input outbox exists");
    let session_input_envelope =
        serde_json::from_str(&session_input_outbox.payload_json).expect("session input envelope");

    let session =
        execute_session_input_event_envelope(&app, session_input_envelope, 302).expect("session");
    assert_eq!(session.session_id, "session-e2e");

    let output_outbox = store
        .get_event_outbox("outbox-output-routed-telegram-update-901")
        .expect("output outbox")
        .expect("output outbox exists");
    let output_envelope = serde_json::from_str(&output_outbox.payload_json).expect("output event");
    let sender = RecordingSender::default();

    let delivery = deliver_session_output_event_envelope(&app, &sender, output_envelope, 303)
        .expect("delivery");
    assert_eq!(delivery.delivered, 1);
    assert_eq!(
        sender.sent.lock().expect("sent").as_slice(),
        &[("telegram-main".to_string(), "runner response".to_string())]
    );

    provider_handle.join().expect("provider thread");
}

#[test]
fn telegram_binding_materializes_compat_route_when_router_rule_is_missing() {
    let (_temp, app) = test_app("http://127.0.0.1:9");
    let store = store(&app);
    seed_session_and_telegram_binding(&store);

    let webhook =
        handle_webhook_update(&app, "secret", &telegram_update(902, "binding hello"), 400)
            .expect("webhook");
    let input_outbox = store
        .get_event_outbox(webhook.outbox_id.as_deref().expect("input outbox id"))
        .expect("input outbox")
        .expect("input outbox exists");
    let input_envelope = serde_json::from_str(&input_outbox.payload_json).expect("input envelope");

    let route = route_input_event_envelope(&app, input_envelope, 401).expect("route envelope");

    assert_eq!(route.session_id, "session-e2e");
    assert_eq!(route.agent_id, "default");
    assert_eq!(route.output_targets, vec!["telegram-42"]);
    assert_eq!(
        route.matched_rule_id.as_deref(),
        Some("rule-telegram-binding-42")
    );
    assert!(
        store
            .get_delivery_target("telegram-42")
            .expect("target")
            .is_some()
    );
    assert!(
        store
            .get_session_output_route("route-session-e2e-telegram-42")
            .expect("output route")
            .is_some()
    );
}

#[test]
fn telegram_binding_refreshes_compat_route_when_selected_session_changes() {
    let (_temp, app) = test_app("http://127.0.0.1:9");
    let store = store(&app);
    seed_session_and_telegram_binding(&store);

    let first_webhook =
        handle_webhook_update(&app, "secret", &telegram_update(904, "old session"), 600)
            .expect("first webhook");
    let first_outbox = store
        .get_event_outbox(first_webhook.outbox_id.as_deref().expect("first outbox id"))
        .expect("first input outbox")
        .expect("first input outbox exists");
    let first_envelope = serde_json::from_str(&first_outbox.payload_json).expect("first envelope");
    let first_route = route_input_event_envelope(&app, first_envelope, 601).expect("first route");
    assert_eq!(first_route.session_id, "session-e2e");

    seed_session_with_id(&store, "session-new");
    let mut binding = store
        .get_telegram_chat_binding(42)
        .expect("binding")
        .expect("binding exists");
    binding.selected_session_id = Some("session-new".to_string());
    binding.updated_at = 602;
    store
        .put_telegram_chat_binding(&binding)
        .expect("update binding");

    let second_webhook =
        handle_webhook_update(&app, "secret", &telegram_update(905, "new session"), 603)
            .expect("second webhook");
    let second_outbox = store
        .get_event_outbox(
            second_webhook
                .outbox_id
                .as_deref()
                .expect("second outbox id"),
        )
        .expect("second input outbox")
        .expect("second input outbox exists");
    let second_envelope =
        serde_json::from_str(&second_outbox.payload_json).expect("second envelope");
    let second_route =
        route_input_event_envelope(&app, second_envelope, 604).expect("second route");

    assert_eq!(second_route.session_id, "session-new");
    let target = store
        .get_delivery_target("telegram-42")
        .expect("target")
        .expect("target exists");
    let allowed_sessions: Vec<String> =
        serde_json::from_str(&target.allowed_session_ids_json).expect("allowed sessions");
    assert!(allowed_sessions.contains(&"session-e2e".to_string()));
    assert!(allowed_sessions.contains(&"session-new".to_string()));
    assert!(
        store
            .get_session_output_route("route-session-new-telegram-42")
            .expect("new output route")
            .is_some()
    );
    let rule = store
        .get_router_rule("rule-telegram-binding-42")
        .expect("rule")
        .expect("rule exists");
    let route_policy: serde_json::Value =
        serde_json::from_str(&rule.route_policy_json).expect("route policy");
    assert_eq!(route_policy["session_id"], "session-new");
}

#[test]
fn telegram_private_inbound_without_binding_bootstraps_session_and_route() {
    let (_temp, app) = test_app("http://127.0.0.1:9");
    let store = store(&app);
    seed_activated_pairing(&store);

    let webhook = handle_webhook_update(&app, "secret", &telegram_update(903, "fresh hello"), 500)
        .expect("webhook");
    let input_outbox = store
        .get_event_outbox(webhook.outbox_id.as_deref().expect("input outbox id"))
        .expect("input outbox")
        .expect("input outbox exists");
    let input_envelope = serde_json::from_str(&input_outbox.payload_json).expect("input envelope");

    let route = route_input_event_envelope(&app, input_envelope, 501).expect("route envelope");

    assert_eq!(route.agent_id, "default");
    assert_eq!(route.output_targets, vec!["telegram-42"]);
    assert_eq!(count_rows(&app, "sessions"), 1);

    let binding = store
        .get_telegram_chat_binding(42)
        .expect("binding")
        .expect("binding exists");
    assert_eq!(binding.owner_telegram_user_id, Some(7));
    assert_eq!(
        binding.selected_session_id.as_deref(),
        Some(route.session_id.as_str())
    );

    assert!(
        store
            .get_delivery_target("telegram-42")
            .expect("target")
            .is_some()
    );
    assert!(
        store
            .get_session_output_route(&format!("route-{}-telegram-42", route.session_id))
            .expect("output route")
            .is_some()
    );
    assert!(
        store
            .get_router_rule("rule-telegram-binding-42")
            .expect("router rule")
            .is_some()
    );
}

fn test_app(provider_api_base: &str) -> (tempfile::TempDir, bootstrap::App) {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
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
    config.telegram.enabled = true;
    config.telegram.mode = "webhook".to_string();
    config.telegram.bot_token = Some("test-token".to_string());
    config.telegram.webhook_public_url =
        Some("https://teamd.example/v1/telegram/webhook/secret".to_string());
    config.telegram.webhook_secret = Some("secret".to_string());
    config.event_bus.required = true;
    config.event_bus.nats_url = Some("nats://127.0.0.1:4222".to_string());
    let app = bootstrap::build_from_config(config).expect("build app");
    (temp, app)
}

fn store(app: &bootstrap::App) -> PersistenceStore {
    PersistenceStore::open(&app.persistence).expect("open store")
}

fn seed_session_route_and_delivery(store: &PersistenceStore) {
    seed_session(store);
    store
        .put_router_rule(&RouterRuleRecord {
            rule_id: "rule-e2e".to_string(),
            priority: 1,
            enabled: true,
            source_filter_json: r#"{"source_id":"telegram-chat-42"}"#.to_string(),
            operator_filter_json: r#"{"operator_id":"telegram-user-7"}"#.to_string(),
            condition_json: "{}".to_string(),
            route_policy_json: r#"{"session_id":"session-e2e","agent_id":"default","queue_policy":"fifo","output_targets":["telegram-main"],"format_policy":"full_text"}"#.to_string(),
            created_at: 100,
            updated_at: 100,
        })
        .expect("put rule");
    store
        .put_delivery_target(&DeliveryTargetRecord {
            target_id: "telegram-main".to_string(),
            kind: "telegram".to_string(),
            address: "42".to_string(),
            scope: "private".to_string(),
            owner_user_id: Some("telegram-user-7".to_string()),
            allowed_agent_ids_json: "[]".to_string(),
            allowed_session_ids_json: "[]".to_string(),
            send_policy_json: "{}".to_string(),
            format_policy: "full_text".to_string(),
            created_at: 100,
            updated_at: 100,
        })
        .expect("put target");
    store
        .put_session_output_route(&SessionOutputRouteRecord {
            route_id: "route-e2e".to_string(),
            session_id: "session-e2e".to_string(),
            target_id: "telegram-main".to_string(),
            filter_json: "{}".to_string(),
            format_policy: "full_text".to_string(),
            enabled: true,
            last_delivered_transcript_created_at: None,
            last_delivered_transcript_id: None,
            created_at: 100,
            updated_at: 100,
        })
        .expect("put output route");
}

fn seed_session_and_telegram_binding(store: &PersistenceStore) {
    seed_session(store);
    store
        .put_telegram_chat_binding(&TelegramChatBindingRecord {
            telegram_chat_id: 42,
            scope: "private".to_string(),
            owner_telegram_user_id: Some(7),
            selected_session_id: Some("session-e2e".to_string()),
            default_agent_profile_id: Some("default".to_string()),
            last_delivered_transcript_created_at: None,
            last_delivered_transcript_id: None,
            inbound_queue_mode: "queue".to_string(),
            inbound_coalesce_window_ms: Some(5000),
            created_at: 100,
            updated_at: 100,
        })
        .expect("put binding");
}

fn seed_activated_pairing(store: &PersistenceStore) {
    store
        .put_telegram_user_pairing(&TelegramUserPairingRecord {
            token: "pair-activated".to_string(),
            telegram_user_id: 7,
            telegram_chat_id: 42,
            telegram_username: Some("operator".to_string()),
            telegram_display_name: "Operator".to_string(),
            status: "activated".to_string(),
            created_at: 100,
            expires_at: 200,
            activated_at: Some(101),
        })
        .expect("put pairing");
}

fn seed_session(store: &PersistenceStore) {
    seed_session_with_id(store, "session-e2e");
}

fn seed_session_with_id(store: &PersistenceStore, session_id: &str) {
    store
        .put_session(&SessionRecord {
            id: session_id.to_string(),
            title: session_id.to_string(),
            prompt_override: None,
            settings_json: serde_json::to_string(&SessionSettings::default()).unwrap(),
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

fn telegram_update(update_id: i64, text: &str) -> String {
    json!({
        "update_id": update_id,
        "message": {
            "message_id": update_id + 10,
            "date": 200,
            "chat": {"id": 42, "type": "private"},
            "from": {"id": 7, "is_bot": false, "first_name": "Operator"},
            "text": text
        }
    })
    .to_string()
}

fn count_rows(app: &bootstrap::App, table: &str) -> i64 {
    count_rows_where(app, table, "TRUE")
}

fn count_rows_where(app: &bootstrap::App, table: &str, where_clause: &str) -> i64 {
    let query = format!("SELECT COUNT(*) FROM {table} WHERE {where_clause}");
    store(app)
        .with_postgres_client(|client| {
            client
                .query_one(&query, &[])
                .map(|row| row.get::<_, i64>(0))
                .map_err(agent_persistence::StoreError::from)
        })
        .expect("count rows")
}

fn openai_message_response_json(response_id: &str, text: &str) -> String {
    let text = serde_json::to_string(text).expect("serialize text");
    format!(
        "{{\"id\":\"{response_id}\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":{text}}}]}}],\"usage\":{{\"input_tokens\":16,\"output_tokens\":3,\"total_tokens\":19}}}}"
    )
}

fn spawn_json_server(body: String) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
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
        }
        if content_length > 0 {
            let mut body_bytes = vec![0; content_length];
            reader.read_exact(&mut body_bytes).expect("read body");
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    });
    (format!("http://{address}"), handle)
}
