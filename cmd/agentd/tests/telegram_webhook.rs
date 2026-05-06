use agent_persistence::{AppConfig, EventRepository, PersistenceStore, RunRepository};
use agentd::bootstrap;
use agentd::daemon;
use agentd::telegram::webhook::{TelegramWebhookErrorKind, handle_webhook_update};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde_json::json;
use std::net::TcpListener;
use std::time::Duration;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn test_app() -> (tempfile::TempDir, bootstrap::App, String) {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("teamd-state"),
        ..AppConfig::default()
    };
    config.daemon.bind_host = "127.0.0.1".to_string();
    config.daemon.bind_port = free_port();
    config.daemon.bearer_token = Some("daemon-token".to_string());
    config.telegram.enabled = true;
    config.telegram.bot_token = Some("test-token".to_string());
    config.telegram.mode = "webhook".to_string();
    config.telegram.webhook_public_url =
        Some("https://teamd.example/v1/telegram/webhook/webhook-secret".to_string());
    config.telegram.webhook_secret = Some("webhook-secret".to_string());
    let base_url = format!(
        "http://{}:{}",
        config.daemon.bind_host, config.daemon.bind_port
    );
    let app = bootstrap::build_from_config(config).expect("build app");
    (temp, app, base_url)
}

fn telegram_update(update_id: i64, text: &str) -> String {
    json!({
        "update_id": update_id,
        "message": {
            "message_id": 55,
            "message_thread_id": 77,
            "date": 1770000200,
            "chat": {
                "id": 42,
                "type": "private",
                "username": "operator"
            },
            "from": {
                "id": 7,
                "is_bot": false,
                "username": "operator",
                "first_name": "Operator"
            },
            "text": text
        }
    })
    .to_string()
}

fn count_rows(app: &bootstrap::App, table: &str) -> i64 {
    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let query = format!("SELECT COUNT(*) FROM {table}");
    store
        .with_postgres_client(|client| {
            client
                .query_one(&query, &[])
                .map(|row| row.get::<_, i64>(0))
                .map_err(agent_persistence::StoreError::from)
        })
        .expect("count rows")
}

#[test]
fn wrong_webhook_secret_is_rejected_without_persisting_event() {
    let (_temp, app, _base_url) = test_app();

    let error = handle_webhook_update(
        &app,
        "wrong-secret",
        &telegram_update(100, "hello"),
        1770000201,
    )
    .expect_err("wrong secret must fail");

    assert_eq!(error.kind(), TelegramWebhookErrorKind::Unauthorized);
    assert_eq!(count_rows(&app, "inbound_events"), 0);
    assert_eq!(count_rows(&app, "event_outbox"), 0);
}

#[test]
fn valid_update_stores_inbound_event_and_outbox_without_running_chat_turn() {
    let (_temp, app, _base_url) = test_app();

    let outcome = handle_webhook_update(
        &app,
        "webhook-secret",
        &telegram_update(101, "hello"),
        1770000202,
    )
    .expect("valid webhook update");

    assert_eq!(outcome.event_id, "telegram-update-101");
    assert_eq!(
        outcome.outbox_id.as_deref(),
        Some("outbox-telegram-update-101")
    );
    assert!(!outcome.duplicate);

    let store = PersistenceStore::open(&app.persistence).expect("open store");
    let inbound = store
        .get_inbound_event("telegram-update-101")
        .expect("get inbound")
        .expect("inbound exists");
    assert_eq!(inbound.dedupe_key, "telegram:update:101");
    assert_eq!(inbound.source_kind, "telegram");
    assert_eq!(inbound.source_id, "telegram-chat-42");
    assert_eq!(inbound.operator_id.as_deref(), Some("telegram-user-7"));
    assert_eq!(inbound.status, "pending");

    let metadata: serde_json::Value =
        serde_json::from_str(&inbound.metadata_json).expect("metadata json");
    assert_eq!(metadata["trace_id"], "trace-telegram-update-101");

    let payload: serde_json::Value =
        serde_json::from_str(&inbound.payload_json).expect("payload json");
    assert_eq!(payload["text"], "hello");
    assert_eq!(payload["chat_id"], 42);
    assert_eq!(payload["message_thread_id"], 77);

    let outbox = store
        .get_event_outbox("outbox-telegram-update-101")
        .expect("get outbox")
        .expect("outbox exists");
    assert_eq!(outbox.subject, "teamd.input.telegram");
    assert_eq!(outbox.status, "pending");

    let envelope: serde_json::Value =
        serde_json::from_str(&outbox.payload_json).expect("outbox envelope");
    assert_eq!(envelope["event_id"], "telegram-update-101");
    assert_eq!(envelope["event_type"], "telegram.message.received");
    assert_eq!(envelope["source_kind"], "telegram");
    assert_eq!(envelope["payload_ref"]["table"], "inbound_events");
    assert_eq!(envelope["payload_ref"]["id"], "telegram-update-101");

    assert!(store.list_runs().expect("runs").is_empty());
    assert_eq!(count_rows(&app, "transcripts"), 0);
}

#[test]
fn duplicate_telegram_update_is_deduped_without_second_outbox() {
    let (_temp, app, _base_url) = test_app();

    let first = handle_webhook_update(
        &app,
        "webhook-secret",
        &telegram_update(102, "hello"),
        1770000203,
    )
    .expect("first update");
    let second = handle_webhook_update(
        &app,
        "webhook-secret",
        &telegram_update(102, "hello again"),
        1770000204,
    )
    .expect("duplicate update");

    assert!(!first.duplicate);
    assert!(second.duplicate);
    assert_eq!(count_rows(&app, "inbound_events"), 1);
    assert_eq!(count_rows(&app, "event_outbox"), 1);
    assert_eq!(count_rows(&app, "routed_events"), 0);
}

#[test]
fn http_webhook_route_uses_webhook_secret_instead_of_daemon_bearer_token() {
    let (_temp, app, base_url) = test_app();
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .expect("http client");

    let wrong = client
        .post(format!("{base_url}/v1/telegram/webhook/wrong-secret"))
        .body(telegram_update(103, "hello"))
        .send()
        .expect("wrong webhook response");
    assert_eq!(wrong.status(), StatusCode::FORBIDDEN);

    let valid = client
        .post(format!("{base_url}/v1/telegram/webhook/webhook-secret"))
        .body(telegram_update(103, "hello"))
        .send()
        .expect("valid webhook response");
    assert_eq!(valid.status(), StatusCode::OK);

    handle.stop().expect("stop daemon");
}
