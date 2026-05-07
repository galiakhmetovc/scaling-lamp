use crate::bootstrap::App;
use crate::delivery_worker::{DeliverySender, DeliveryWorkerReport, deliver_session_output_event};
use crate::diagnostics::DiagnosticEventBuilder;
use crate::event_bus::EventEnvelope;
use crate::http::client::{DaemonClient, DaemonConnectOptions};
use crate::nats::NatsEventBus;
use crate::router_worker::{RouteDecision, route_inbound_event};
use crate::session_worker::{SessionWorkerReport, execute_routed_session_event};
use crate::task_worker::{TaskWorkerReport, execute_task_event_envelope};
use crate::telegram::backend::DaemonTelegramBackend;
use crate::telegram::client::{TelegramClient, TelegramClientConfig};
use crate::telegram::event_delivery::TelegramEventDeliverySender;
use crate::telegram::router::TelegramWorker;
use agent_persistence::{
    DeliveryRepository, DeliveryTargetRecord, EventRepository, PersistenceStore, RouterRepository,
    RouterRuleRecord, SessionOutputRouteRecord, StoreError, TelegramRepository,
    audit::AuditLogConfig,
};
use async_nats::jetstream::Message as JetStreamMessage;
use futures_util::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use teloxide::types::{Message, Update, UpdateId, UpdateKind};

const OUTBOX_PUBLISH_BATCH: i64 = 64;
const OUTBOX_ACTIVE_PUBLISH_INTERVAL: Duration = Duration::from_millis(50);
const OUTBOX_IDLE_PUBLISH_INTERVAL: Duration = Duration::from_secs(1);
const CONSUMER_IDLE_TIMEOUT: Duration = Duration::from_millis(500);
// Preserve the original durable name so webhook-mode upgrades do not replay the
// whole Telegram input stream from the beginning.
const ROUTER_CONSUMER: &str = "teamd-telegram-webhook";
const SESSION_CONSUMER: &str = "teamd-session-worker";
const DELIVERY_CONSUMER: &str = "teamd-delivery-worker";
const TASK_CONSUMER: &str = "teamd-task-worker";

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

pub fn route_input_event_envelope(
    app: &App,
    envelope: EventEnvelope,
    now: i64,
) -> Result<RouteDecision, String> {
    if envelope.payload_ref.table != "inbound_events" {
        return Err(format!(
            "input event {} has unsupported payload_ref table {}",
            envelope.event_id, envelope.payload_ref.table
        ));
    }
    let inbound_event_id = envelope.payload_ref.id.clone();
    if let Ok(store) = app.store()
        && let Ok(Some(inbound)) = store.get_inbound_event(&inbound_event_id)
        && inbound.source_kind == "telegram"
    {
        ensure_telegram_binding_route(app, &inbound.payload_json, &inbound.source_id, now)?;
    }
    let result = route_inbound_event(app, &inbound_event_id, now).map_err(|error| {
        if let Ok(store) = app.store() {
            let _ = store.mark_inbound_event_status(
                &inbound_event_id,
                "failed",
                Some(&error.to_string()),
            );
        }
        error.to_string()
    })?;
    let store = app.store().map_err(|error| error.to_string())?;
    store
        .mark_inbound_event_status(&inbound_event_id, "processed", None)
        .map_err(|error| error.to_string())?;
    Ok(result)
}

pub fn execute_session_input_event_envelope(
    app: &App,
    envelope: EventEnvelope,
    now: i64,
) -> Result<SessionWorkerReport, String> {
    if envelope.event_type != "session.input.routed" {
        return Err(format!(
            "session input event {} has unsupported event_type {}",
            envelope.event_id, envelope.event_type
        ));
    }
    if envelope.payload_ref.table != "routed_events" {
        return Err(format!(
            "session input event {} has unsupported payload_ref table {}",
            envelope.event_id, envelope.payload_ref.table
        ));
    }
    execute_routed_session_event(app, &envelope.payload_ref.id, now)
        .map_err(|error| error.to_string())
}

pub fn execute_task_event_runtime_envelope(
    app: &App,
    envelope: EventEnvelope,
    now: i64,
) -> Result<TaskWorkerReport, String> {
    execute_task_event_envelope(app, envelope, now).map_err(|error| error.to_string())
}

pub fn deliver_session_output_event_envelope<S>(
    app: &App,
    sender: &S,
    envelope: EventEnvelope,
    now: i64,
) -> Result<DeliveryWorkerReport, String>
where
    S: DeliverySender,
{
    if envelope.event_type != "session.output.created" {
        return Err(format!(
            "session output event {} has unsupported event_type {}",
            envelope.event_id, envelope.event_type
        ));
    }
    let outbox_id = format!("outbox-{}", envelope.event_id);
    deliver_session_output_event(app, sender, &outbox_id, now).map_err(|error| error.to_string())
}

async fn run_event_runtime(app: App, shutdown: Arc<AtomicBool>) -> Result<(), String> {
    let bus = NatsEventBus::connect(&app.config.event_bus)
        .await
        .map_err(|error| error.to_string())?;
    let store = open_runtime_store(&app).await?;

    let publisher_bus = bus.clone();
    let publisher_store = store.clone();
    let publisher_shutdown = shutdown.clone();
    let publisher = tokio::spawn(async move {
        outbox_publisher_loop(publisher_bus, publisher_store, publisher_shutdown).await
    });

    let router_app = app.clone();
    let router_bus = bus.clone();
    let router_store = store.clone();
    let router_shutdown = shutdown.clone();
    let router = tokio::spawn(async move {
        telegram_webhook_consumer_loop(router_app, router_bus, router_store, router_shutdown).await
    });

    let session_app = app.clone();
    let session_bus = bus.clone();
    let session_shutdown = shutdown.clone();
    let session = tokio::spawn(async move {
        session_input_consumer_loop(session_app, session_bus, session_shutdown).await
    });

    let delivery_app = app.clone();
    let delivery_bus = bus.clone();
    let delivery_shutdown = shutdown.clone();
    let delivery = tokio::spawn(async move {
        session_output_consumer_loop(delivery_app, delivery_bus, delivery_shutdown).await
    });

    let task_app = app.clone();
    let task_bus = bus.clone();
    let task_shutdown = shutdown.clone();
    let task =
        tokio::spawn(async move { task_consumer_loop(task_app, task_bus, task_shutdown).await });

    let (publisher_result, router_result, session_result, delivery_result, task_result) =
        tokio::join!(publisher, router, session, delivery, task);
    publisher_result.map_err(|error| format!("outbox publisher task join error: {error}"))??;
    router_result
        .map_err(|error| format!("telegram/router consumer task join error: {error}"))??;
    session_result.map_err(|error| format!("session consumer task join error: {error}"))??;
    delivery_result.map_err(|error| format!("delivery consumer task join error: {error}"))??;
    task_result.map_err(|error| format!("task consumer task join error: {error}"))??;
    Ok(())
}

async fn outbox_publisher_loop(
    bus: NatsEventBus,
    store: Arc<PersistenceStore>,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    while !shutdown.load(Ordering::Relaxed) {
        let published_count =
            publish_pending_outbox_once(&bus, store.clone(), unix_timestamp()).await?;
        let sleep_for = if published_count == 0 {
            OUTBOX_IDLE_PUBLISH_INTERVAL
        } else {
            OUTBOX_ACTIVE_PUBLISH_INTERVAL
        };
        tokio::time::sleep(sleep_for).await;
    }
    Ok(())
}

async fn publish_pending_outbox_once(
    bus: &NatsEventBus,
    store: Arc<PersistenceStore>,
    now: i64,
) -> Result<usize, String> {
    let outboxes = with_store(store.clone(), move |store| {
        store.claim_pending_event_outbox(OUTBOX_PUBLISH_BATCH, now)
    })
    .await?;
    let claimed_count = outboxes.len();
    for outbox in outboxes {
        match bus
            .publish_json(&outbox.subject, &outbox.payload_json)
            .await
        {
            Ok(()) => {
                let outbox_id = outbox.outbox_id.clone();
                with_store(store.clone(), move |store| {
                    store.mark_event_outbox_published(&outbox_id, now)
                })
                .await?;
            }
            Err(error) => {
                let retry_at = now + retry_delay_seconds(outbox.attempt_count);
                let outbox_id = outbox.outbox_id.clone();
                let error = error.to_string();
                with_store(store.clone(), move |store| {
                    store.mark_event_outbox_pending_retry(&outbox_id, retry_at, &error)
                })
                .await?;
            }
        }
    }
    Ok(claimed_count)
}

fn retry_delay_seconds(attempt_count: i64) -> i64 {
    let bounded = attempt_count.clamp(1, 6);
    2_i64.pow(u32::try_from(bounded - 1).unwrap_or(0)).min(60)
}

async fn telegram_webhook_consumer_loop(
    app: App,
    bus: NatsEventBus,
    store: Arc<PersistenceStore>,
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
            ROUTER_CONSUMER,
            "teamd.input.telegram",
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut messages = consumer
        .messages()
        .await
        .map_err(|error| error.to_string())?;

    while !shutdown.load(Ordering::Relaxed) {
        if let Err(error) = worker.deliver_pending_session_notifications().await {
            log_event_runtime_error(&app, "telegram.delivery.error", &error.to_string());
        }

        let next = tokio::time::timeout(CONSUMER_IDLE_TIMEOUT, messages.next()).await;
        let Some(message) = next.ok().flatten() else {
            continue;
        };
        let message = message.map_err(|error| error.to_string())?;
        if let Err(error) =
            handle_telegram_webhook_message(&app, &worker, store.clone(), &message).await
        {
            log_event_runtime_error(&app, "telegram.consumer.error", &error);
        }
        message.ack().await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn handle_telegram_webhook_message<B>(
    app: &App,
    worker: &TelegramWorker<B>,
    store: Arc<PersistenceStore>,
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

    let inbound_event_id = envelope.payload_ref.id.clone();
    let Some(inbound) = with_store(store.clone(), move |store| {
        store.get_inbound_event(&inbound_event_id)
    })
    .await?
    else {
        return Err(format!(
            "inbound event {} not found",
            envelope.payload_ref.id
        ));
    };
    if inbound.status == "processed" {
        return Ok(());
    }

    if telegram_inbound_is_control_command(&inbound.payload_json) {
        let update = telegram_update_from_payload(&inbound.payload_json)?;
        match worker.handle_update(update).await {
            Ok(()) => {
                let inbound_event_id = inbound.event_id.clone();
                with_store(store.clone(), move |store| {
                    store.mark_inbound_event_status(&inbound_event_id, "processed", None)
                })
                .await
            }
            Err(error) => {
                let message = error.to_string();
                let inbound_event_id = inbound.event_id.clone();
                let error_message = message.clone();
                let _ = with_store(store.clone(), move |store| {
                    store.mark_inbound_event_status(
                        &inbound_event_id,
                        "failed",
                        Some(&error_message),
                    )
                })
                .await;
                Err(message)
            }
        }
    } else {
        let app = app.clone();
        tokio::task::spawn_blocking(move || {
            route_input_event_envelope(&app, envelope, unix_timestamp()).map(|_| ())
        })
        .await
        .map_err(|error| error.to_string())?
    }
}

async fn session_input_consumer_loop(
    app: App,
    bus: NatsEventBus,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    let consumer = bus
        .pull_consumer(
            &app.config.event_bus.session_stream,
            SESSION_CONSUMER,
            "teamd.session.*.input",
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
        let app_for_event = app.clone();
        let payload = message.message.payload.clone();
        let result = tokio::task::spawn_blocking(move || {
            let envelope: EventEnvelope =
                serde_json::from_slice(&payload).map_err(|error| error.to_string())?;
            execute_session_input_event_envelope(&app_for_event, envelope, unix_timestamp())
        })
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result);
        match result {
            Ok(_) => {}
            Err(error) => log_event_runtime_error(&app, "session.consumer.error", &error),
        }
        message.ack().await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn session_output_consumer_loop(
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
    let sender = TelegramEventDeliverySender::new(app.clone(), telegram_client);
    let consumer = bus
        .pull_consumer(
            &app.config.event_bus.session_stream,
            DELIVERY_CONSUMER,
            "teamd.session.*.output",
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
        let app_for_event = app.clone();
        let sender_for_event = sender.clone();
        let payload = message.message.payload.clone();
        let result = tokio::task::spawn_blocking(move || {
            let envelope: EventEnvelope =
                serde_json::from_slice(&payload).map_err(|error| error.to_string())?;
            deliver_session_output_event_envelope(
                &app_for_event,
                &sender_for_event,
                envelope,
                unix_timestamp(),
            )
        })
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result);
        match result {
            Ok(_) => {}
            Err(error) => log_event_runtime_error(&app, "delivery.consumer.error", &error),
        }
        message.ack().await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn task_consumer_loop(
    app: App,
    bus: NatsEventBus,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    let consumer = bus
        .pull_consumer(
            &app.config.event_bus.task_stream,
            TASK_CONSUMER,
            "teamd.task.*",
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
        let app_for_event = app.clone();
        let payload = message.message.payload.clone();
        let result = tokio::task::spawn_blocking(move || {
            let envelope: EventEnvelope =
                serde_json::from_slice(&payload).map_err(|error| error.to_string())?;
            execute_task_event_runtime_envelope(&app_for_event, envelope, unix_timestamp())
        })
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result);
        match result {
            Ok(_) => {}
            Err(error) => log_event_runtime_error(&app, "task.consumer.error", &error),
        }
        message.ack().await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn telegram_inbound_is_control_command(payload_json: &str) -> bool {
    let Ok(payload) = serde_json::from_str::<Value>(payload_json) else {
        return false;
    };
    payload
        .get("text")
        .or_else(|| payload.get("caption"))
        .and_then(Value::as_str)
        .map(str::trim_start)
        .is_some_and(|text| text.starts_with('/'))
}

fn ensure_telegram_binding_route(
    app: &App,
    payload_json: &str,
    source_id: &str,
    now: i64,
) -> Result<(), String> {
    let payload: Value = serde_json::from_str(payload_json).map_err(|error| error.to_string())?;
    let Some(chat_id) = payload.get("chat_id").and_then(Value::as_i64) else {
        return Ok(());
    };
    let store = app.store().map_err(|error| error.to_string())?;
    let Some(binding) = store
        .get_telegram_chat_binding(chat_id)
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let Some(session_id) = binding.selected_session_id.as_deref() else {
        return Ok(());
    };
    let target_id = format!("telegram-{}", numeric_id_token(chat_id));
    if store
        .get_delivery_target(&target_id)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        store
            .put_delivery_target(&DeliveryTargetRecord {
                target_id: target_id.clone(),
                kind: "telegram".to_string(),
                address: chat_id.to_string(),
                scope: binding.scope.clone(),
                owner_user_id: binding
                    .owner_telegram_user_id
                    .map(|user_id| format!("telegram-user-{}", numeric_id_token(user_id))),
                allowed_agent_ids_json: "[]".to_string(),
                allowed_session_ids_json: serde_json::to_string(&vec![session_id.to_string()])
                    .map_err(|error| error.to_string())?,
                send_policy_json: "{}".to_string(),
                format_policy: "full_text".to_string(),
                created_at: now,
                updated_at: now,
            })
            .map_err(|error| error.to_string())?;
    }

    let route_id = format!("route-{session_id}-{target_id}");
    if store
        .get_session_output_route(&route_id)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        store
            .put_session_output_route(&SessionOutputRouteRecord {
                route_id,
                session_id: session_id.to_string(),
                target_id: target_id.clone(),
                filter_json: "{}".to_string(),
                format_policy: "full_text".to_string(),
                enabled: true,
                last_delivered_transcript_created_at: Some(0),
                last_delivered_transcript_id: Some(String::new()),
                created_at: now,
                updated_at: now,
            })
            .map_err(|error| error.to_string())?;
    }

    let rule_id = format!("rule-telegram-binding-{}", numeric_id_token(chat_id));
    if store
        .get_router_rule(&rule_id)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        store
            .put_router_rule(&RouterRuleRecord {
                rule_id,
                priority: 900_000,
                enabled: true,
                source_filter_json: serde_json::json!({ "source_id": source_id }).to_string(),
                operator_filter_json: "{}".to_string(),
                condition_json: "{}".to_string(),
                route_policy_json: serde_json::json!({
                    "session_id": session_id,
                    "agent_id": binding
                        .default_agent_profile_id
                        .as_deref()
                        .unwrap_or("default"),
                    "queue_policy": binding.inbound_queue_mode,
                    "output_targets": [target_id],
                    "format_policy": "full_text",
                    "labels": ["telegram-binding-compat"]
                })
                .to_string(),
                created_at: now,
                updated_at: now,
            })
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn numeric_id_token(id: i64) -> String {
    if id < 0 {
        format!("n{}", id.unsigned_abs())
    } else {
        id.to_string()
    }
}

pub fn telegram_update_from_payload(payload_json: &str) -> Result<Update, String> {
    let payload: Value = serde_json::from_str(payload_json).map_err(|error| error.to_string())?;
    let raw_update = payload
        .get("raw_update")
        .cloned()
        .ok_or_else(|| "telegram inbound payload missing raw_update".to_string())?;
    telegram_update_from_raw_value(raw_update)
}

fn telegram_update_from_raw_value(raw_update: Value) -> Result<Update, String> {
    let update_id = raw_update
        .get("update_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| "telegram raw update missing update_id".to_string())
        .and_then(|value| {
            u32::try_from(value).map_err(|_| format!("telegram update_id {value} overflows u32"))
        })?;
    if let Some(message) = raw_update.get("message").cloned() {
        return Ok(Update {
            id: UpdateId(update_id),
            kind: UpdateKind::Message(
                serde_json::from_value::<Message>(message).map_err(|error| error.to_string())?,
            ),
        });
    }
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

async fn open_runtime_store(app: &App) -> Result<Arc<PersistenceStore>, String> {
    let persistence = app.persistence.clone();
    tokio::task::spawn_blocking(move || PersistenceStore::open_runtime(&persistence).map(Arc::new))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

async fn with_store<T, F>(store: Arc<PersistenceStore>, operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&PersistenceStore) -> Result<T, StoreError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || operation(&store))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
