use crate::bootstrap::App;
use crate::diagnostics::DiagnosticEventBuilder;
use crate::event_bus::EventEnvelope;
use crate::http::client::{DaemonClient, DaemonConnectOptions};
use crate::nats::NatsEventBus;
use crate::telegram::backend::DaemonTelegramBackend;
use crate::telegram::client::{TelegramClient, TelegramClientConfig};
use crate::telegram::router::TelegramWorker;
use agent_persistence::{EventRepository, audit::AuditLogConfig};
use async_nats::jetstream::Message as JetStreamMessage;
use futures_util::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use teloxide::types::Update;

const TELEGRAM_WEBHOOK_CONSUMER: &str = "teamd-telegram-webhook";
const OUTBOX_PUBLISH_BATCH: i64 = 64;
const OUTBOX_PUBLISH_INTERVAL: Duration = Duration::from_millis(100);
const CONSUMER_IDLE_TIMEOUT: Duration = Duration::from_millis(500);

pub fn spawn_event_runtime(app: App, shutdown: Arc<AtomicBool>) -> Option<JoinHandle<()>> {
    if !event_runtime_enabled(&app) {
        return None;
    }

    Some(thread::spawn(move || {
        let audit = AuditLogConfig::from_config(&app.config);
        DiagnosticEventBuilder::new(
            &app.config,
            "info",
            "event_runtime",
            "runtime.start",
            "event runtime starting",
        )
        .field("telegram_mode", app.config.telegram.mode.as_str())
        .field("nats_url", app.config.event_bus.nats_url.clone())
        .emit(&audit);

        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                DiagnosticEventBuilder::new(
                    &app.config,
                    "error",
                    "event_runtime",
                    "runtime.start_failed",
                    "event runtime failed to build tokio runtime",
                )
                .error(error.to_string())
                .emit(&audit);
                return;
            }
        };

        if let Err(error) = runtime.block_on(run_event_runtime(app.clone(), shutdown.clone())) {
            DiagnosticEventBuilder::new(
                &app.config,
                "error",
                "event_runtime",
                "runtime.error",
                "event runtime stopped with error",
            )
            .error(error)
            .emit(&audit);
        }
    }))
}

fn event_runtime_enabled(app: &App) -> bool {
    app.config.telegram.enabled
        && app.config.telegram.mode == "webhook"
        && app.config.event_bus.required
}

async fn run_event_runtime(app: App, shutdown: Arc<AtomicBool>) -> Result<(), String> {
    let bus = NatsEventBus::connect(&app.config.event_bus)
        .await
        .map_err(|error| error.to_string())?;

    let publisher_app = app.clone();
    let publisher_bus = bus.clone();
    let publisher_shutdown = shutdown.clone();
    let publisher = tokio::spawn(async move {
        outbox_publisher_loop(publisher_app, publisher_bus, publisher_shutdown).await
    });

    let telegram =
        tokio::spawn(async move { telegram_webhook_consumer_loop(app, bus, shutdown).await });

    let (publisher_result, telegram_result) = tokio::join!(publisher, telegram);
    publisher_result.map_err(|error| format!("outbox publisher task join error: {error}"))??;
    telegram_result.map_err(|error| format!("telegram consumer task join error: {error}"))??;
    Ok(())
}

async fn outbox_publisher_loop(
    app: App,
    bus: NatsEventBus,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    while !shutdown.load(Ordering::Relaxed) {
        publish_pending_outbox_once(&app, &bus, unix_timestamp()).await?;
        tokio::time::sleep(OUTBOX_PUBLISH_INTERVAL).await;
    }
    Ok(())
}

async fn publish_pending_outbox_once(
    app: &App,
    bus: &NatsEventBus,
    now: i64,
) -> Result<(), String> {
    let store = app.store().map_err(|error| error.to_string())?;
    let outboxes = store
        .claim_pending_event_outbox(OUTBOX_PUBLISH_BATCH, now)
        .map_err(|error| error.to_string())?;
    for outbox in outboxes {
        match bus
            .publish_json(&outbox.subject, &outbox.payload_json)
            .await
        {
            Ok(()) => store
                .mark_event_outbox_published(&outbox.outbox_id, now)
                .map_err(|error| error.to_string())?,
            Err(error) => {
                let retry_at = now + retry_delay_seconds(outbox.attempt_count);
                store
                    .mark_event_outbox_pending_retry(
                        &outbox.outbox_id,
                        retry_at,
                        &error.to_string(),
                    )
                    .map_err(|store_error| store_error.to_string())?;
            }
        }
    }
    Ok(())
}

fn retry_delay_seconds(attempt_count: i64) -> i64 {
    let bounded = attempt_count.clamp(1, 6);
    2_i64.pow(u32::try_from(bounded - 1).unwrap_or(0)).min(60)
}

async fn telegram_webhook_consumer_loop(
    app: App,
    bus: NatsEventBus,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    let token = app
        .config
        .telegram
        .bot_token
        .clone()
        .ok_or_else(|| "telegram.bot_token is not configured".to_string())?;
    let telegram_client = TelegramClient::new(TelegramClientConfig {
        token,
        api_url: None,
        poll_request_timeout_seconds: app.config.telegram.poll_request_timeout_seconds,
    })
    .map_err(|error| error.to_string())?;
    let daemon_client = DaemonClient::new(&app.config, &DaemonConnectOptions::default());
    let backend = DaemonTelegramBackend::new(daemon_client);
    let worker = TelegramWorker::new(app.clone(), backend, telegram_client);
    worker
        .register_commands()
        .await
        .map_err(|error| error.to_string())?;

    let consumer = bus
        .pull_consumer(
            &app.config.event_bus.input_stream,
            TELEGRAM_WEBHOOK_CONSUMER,
            "teamd.input.telegram",
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut messages = consumer
        .messages()
        .await
        .map_err(|error| error.to_string())?;

    while !shutdown.load(Ordering::Relaxed) {
        let next = tokio::time::timeout(CONSUMER_IDLE_TIMEOUT, messages.next()).await;
        let Some(message) = next.ok().flatten() else {
            continue;
        };
        let message = message.map_err(|error| error.to_string())?;
        if let Err(error) = handle_telegram_webhook_message(&app, &worker, &message).await {
            log_event_runtime_error(&app, "telegram.consumer.error", &error);
        }
        message.ack().await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn handle_telegram_webhook_message<B>(
    app: &App,
    worker: &TelegramWorker<B>,
    message: &JetStreamMessage,
) -> Result<(), String>
where
    B: crate::telegram::backend::TelegramBackend,
{
    let envelope: EventEnvelope =
        serde_json::from_slice(&message.message.payload).map_err(|error| error.to_string())?;
    if envelope.event_type != "telegram.message.received" {
        return Ok(());
    }
    if envelope.payload_ref.table != "inbound_events" {
        return Err(format!(
            "telegram event {} has unsupported payload_ref table {}",
            envelope.event_id, envelope.payload_ref.table
        ));
    }

    let store = app.store().map_err(|error| error.to_string())?;
    let Some(inbound) = store
        .get_inbound_event(&envelope.payload_ref.id)
        .map_err(|error| error.to_string())?
    else {
        return Err(format!(
            "inbound event {} not found",
            envelope.payload_ref.id
        ));
    };
    if inbound.status == "processed" {
        return Ok(());
    }
    let update = telegram_update_from_payload(&inbound.payload_json)?;
    match worker.handle_update(update).await {
        Ok(()) => store
            .mark_inbound_event_status(&inbound.event_id, "processed", None)
            .map_err(|error| error.to_string()),
        Err(error) => {
            let message = error.to_string();
            let _ = store.mark_inbound_event_status(&inbound.event_id, "failed", Some(&message));
            Err(message)
        }
    }
}

pub fn telegram_update_from_payload(payload_json: &str) -> Result<Update, String> {
    let payload: Value = serde_json::from_str(payload_json).map_err(|error| error.to_string())?;
    let raw_update = payload
        .get("raw_update")
        .cloned()
        .ok_or_else(|| "telegram inbound payload missing raw_update".to_string())?;
    serde_json::from_value(raw_update).map_err(|error| error.to_string())
}

fn log_event_runtime_error(app: &App, op: &str, error: &str) {
    DiagnosticEventBuilder::new(
        &app.config,
        "error",
        "event_runtime",
        op,
        "event runtime worker error",
    )
    .error(error.to_string())
    .emit(&AuditLogConfig::from_config(&app.config));
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
