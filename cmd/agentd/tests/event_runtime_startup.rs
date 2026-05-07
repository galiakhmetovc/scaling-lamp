use agent_persistence::AppConfig;
use agentd::bootstrap;
use agentd::daemon;
use agentd::event_runtime::{EventRuntimeWorker, build_event_runtime_plan};
use agentd::http::types::StatusResponse;
use reqwest::blocking::Client;
use std::net::TcpListener;
use std::time::Duration;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

#[test]
fn webhook_mode_runtime_plan_starts_event_workers_and_disables_polling() {
    let mut config = AppConfig::default();
    config.telegram.enabled = true;
    config.telegram.mode = "webhook".to_string();
    config.telegram.webhook_public_url =
        Some("https://teamd.example/v1/telegram/webhook/secret".to_string());
    config.telegram.webhook_secret = Some("secret".to_string());
    config.event_bus.required = true;
    config.event_bus.nats_url = Some("nats://127.0.0.1:4222".to_string());

    let plan = build_event_runtime_plan(&config).expect("event runtime plan");

    assert!(!plan.starts_telegram_polling);
    assert_eq!(
        plan.workers,
        vec![
            EventRuntimeWorker::NatsJetStream,
            EventRuntimeWorker::TelegramWebhook,
            EventRuntimeWorker::Router,
            EventRuntimeWorker::Session,
            EventRuntimeWorker::Delivery,
            EventRuntimeWorker::Task,
            EventRuntimeWorker::OutboxPublisher,
        ]
    );
}

#[test]
fn polling_mode_runtime_plan_keeps_legacy_polling_without_event_workers() {
    let mut config = AppConfig::default();
    config.telegram.enabled = true;
    config.telegram.mode = "polling".to_string();

    let plan = build_event_runtime_plan(&config).expect("polling runtime plan");

    assert!(plan.starts_telegram_polling);
    assert!(plan.workers.is_empty());
}

#[test]
fn webhook_mode_runtime_plan_requires_nats_url() {
    let mut config = AppConfig::default();
    config.telegram.enabled = true;
    config.telegram.mode = "webhook".to_string();
    config.telegram.webhook_public_url =
        Some("https://teamd.example/v1/telegram/webhook/secret".to_string());
    config.telegram.webhook_secret = Some("secret".to_string());
    config.event_bus.required = true;
    config.event_bus.nats_url = None;

    let error = build_event_runtime_plan(&config).expect_err("missing nats must fail");

    assert!(error.to_string().contains("event_bus.nats_url"));
}

#[test]
fn status_endpoint_reports_event_bus_configuration() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut config = AppConfig {
        data_dir: temp.path().join("teamd-state"),
        ..AppConfig::default()
    };
    config.daemon.bind_host = "127.0.0.1".to_string();
    config.daemon.bind_port = free_port();
    config.telegram.enabled = true;
    config.telegram.bot_token = Some("test-token".to_string());
    config.telegram.mode = "webhook".to_string();
    config.telegram.webhook_public_url =
        Some("https://teamd.example/v1/telegram/webhook/secret".to_string());
    config.telegram.webhook_secret = Some("secret".to_string());
    config.event_bus.required = true;
    config.event_bus.nats_url = Some("nats://127.0.0.1:4222".to_string());
    let base_url = format!(
        "http://{}:{}",
        config.daemon.bind_host, config.daemon.bind_port
    );
    let app = bootstrap::build_from_config(config).expect("build app");
    let handle = daemon::spawn_for_test(app).expect("spawn daemon");
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("http client");

    let status: StatusResponse = client
        .get(format!("{base_url}/v1/status"))
        .send()
        .expect("status response")
        .json()
        .expect("status json");

    assert_eq!(status.telegram_mode, "webhook");
    assert!(status.event_bus_required);
    assert_eq!(status.event_bus_backend, "nats_jetstream");
    assert!(status.event_bus_nats_configured);

    handle.stop().expect("stop daemon");
}
