use crate::bootstrap::App;
use crate::event_bus::{EventEnvelope, EventPayloadRef, EventSubjects, build_event_envelope};
use agent_persistence::{EventOutboxRecord, EventRepository, InboundEventRecord};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramWebhookErrorKind {
    Unauthorized,
    InvalidPayload,
    Config,
    Store,
    Encode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramWebhookError {
    kind: TelegramWebhookErrorKind,
    message: String,
}

impl TelegramWebhookError {
    pub fn kind(&self) -> TelegramWebhookErrorKind {
        self.kind
    }

    fn new(kind: TelegramWebhookErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for TelegramWebhookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "telegram webhook error: {}", self.message)
    }
}

impl std::error::Error for TelegramWebhookError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramWebhookOutcome {
    pub event_id: String,
    pub duplicate: bool,
    pub outbox_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    update_id: i64,
    #[serde(default)]
    message: Option<TelegramMessage>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    message_id: i64,
    #[serde(default)]
    message_thread_id: Option<i64>,
    chat: TelegramChat,
    #[serde(default)]
    from: Option<TelegramUser>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    caption: Option<String>,
    #[serde(default)]
    document: Option<TelegramDocument>,
}

#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: i64,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramUser {
    id: i64,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    first_name: Option<String>,
    #[serde(default)]
    last_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramDocument {
    file_id: String,
    #[serde(default)]
    file_unique_id: Option<String>,
    #[serde(default)]
    file_name: Option<String>,
    #[serde(default)]
    mime_type: Option<String>,
    #[serde(default)]
    file_size: Option<i64>,
}

struct NormalizedTelegramUpdate {
    update_id: i64,
    event_id: String,
    outbox_id: String,
    dedupe_key: String,
    source_id: String,
    operator_id: Option<String>,
    payload_json: String,
    metadata_json: String,
}

pub fn handle_webhook_update(
    app: &App,
    provided_secret: &str,
    body: &str,
    now: i64,
) -> Result<TelegramWebhookOutcome, TelegramWebhookError> {
    let expected_secret = app
        .config
        .telegram
        .webhook_secret
        .as_deref()
        .ok_or_else(|| {
            TelegramWebhookError::new(
                TelegramWebhookErrorKind::Config,
                "telegram.webhook_secret is not configured",
            )
        })?;
    if provided_secret != expected_secret {
        return Err(TelegramWebhookError::new(
            TelegramWebhookErrorKind::Unauthorized,
            "invalid telegram webhook secret",
        ));
    }

    let raw_update: Value = serde_json::from_str(body).map_err(|error| {
        TelegramWebhookError::new(
            TelegramWebhookErrorKind::InvalidPayload,
            format!("invalid telegram update json: {error}"),
        )
    })?;
    let update: TelegramUpdate = serde_json::from_value(raw_update.clone()).map_err(|error| {
        TelegramWebhookError::new(
            TelegramWebhookErrorKind::InvalidPayload,
            format!("invalid telegram update json: {error}"),
        )
    })?;
    let normalized = normalize_update(update, raw_update)?;
    let store = app.store().map_err(|error| {
        TelegramWebhookError::new(TelegramWebhookErrorKind::Store, error.to_string())
    })?;
    let duplicate = store
        .get_inbound_event(&normalized.event_id)
        .map_err(|error| {
            TelegramWebhookError::new(TelegramWebhookErrorKind::Store, error.to_string())
        })?
        .is_some();

    let inbound = InboundEventRecord {
        event_id: normalized.event_id.clone(),
        dedupe_key: normalized.dedupe_key,
        source_kind: "telegram".to_string(),
        source_id: normalized.source_id.clone(),
        operator_id: normalized.operator_id.clone(),
        payload_json: normalized.payload_json,
        metadata_json: normalized.metadata_json,
        status: "pending".to_string(),
        received_at: now,
        published_at: None,
        error: None,
    };
    let inbound = store.put_inbound_event(&inbound).map_err(|error| {
        TelegramWebhookError::new(TelegramWebhookErrorKind::Store, error.to_string())
    })?;

    let mut outbox_id = None;
    if !duplicate {
        let subjects = EventSubjects::from_config(&app.config.event_bus);
        let subject = subjects.input("telegram");
        let envelope = build_event_envelope(EventEnvelope {
            event_id: inbound.event_id.clone(),
            event_type: "telegram.message.received".to_string(),
            trace_id: Some(format!("trace-telegram-update-{}", normalized.update_id)),
            source_kind: inbound.source_kind.clone(),
            source_id: inbound.source_id.clone(),
            subject: subject.clone(),
            payload_ref: EventPayloadRef {
                table: "inbound_events".to_string(),
                id: inbound.event_id.clone(),
            },
            created_at: now,
            metadata: json!({
                "dedupe_key": inbound.dedupe_key,
                "operator_id": inbound.operator_id,
            }),
        })
        .map_err(|error| {
            TelegramWebhookError::new(TelegramWebhookErrorKind::Encode, error.to_string())
        })?;
        let envelope_json = serde_json::to_string(&envelope).map_err(|error| {
            TelegramWebhookError::new(TelegramWebhookErrorKind::Encode, error.to_string())
        })?;
        let outbox = EventOutboxRecord {
            outbox_id: normalized.outbox_id.clone(),
            subject,
            payload_json: envelope_json,
            status: "pending".to_string(),
            attempt_count: 0,
            next_attempt_at: now,
            created_at: now,
            published_at: None,
            last_error: None,
        };
        store.put_event_outbox(&outbox).map_err(|error| {
            TelegramWebhookError::new(TelegramWebhookErrorKind::Store, error.to_string())
        })?;
        outbox_id = Some(outbox.outbox_id);
    }

    Ok(TelegramWebhookOutcome {
        event_id: inbound.event_id,
        duplicate,
        outbox_id,
    })
}

fn normalize_update(
    update: TelegramUpdate,
    raw_update: Value,
) -> Result<NormalizedTelegramUpdate, TelegramWebhookError> {
    let message = update.message.ok_or_else(|| {
        TelegramWebhookError::new(
            TelegramWebhookErrorKind::InvalidPayload,
            "telegram update does not contain message",
        )
    })?;
    let event_id = format!("telegram-update-{}", update.update_id);
    let outbox_id = format!("outbox-{event_id}");
    let source_id = source_id_for_chat(message.chat.id);
    let operator_id = message.from.as_ref().map(|user| user_id(user.id));
    let payload = json!({
        "raw_update": raw_update,
        "update_id": update.update_id,
        "message_id": message.message_id,
        "message_thread_id": message.message_thread_id,
        "chat_id": message.chat.id,
        "chat_type": message.chat.r#type,
        "chat_username": message.chat.username,
        "chat_title": message.chat.title,
        "telegram_user_id": message.from.as_ref().map(|user| user.id),
        "telegram_username": message.from.as_ref().and_then(|user| user.username.clone()),
        "telegram_first_name": message.from.as_ref().and_then(|user| user.first_name.clone()),
        "telegram_last_name": message.from.as_ref().and_then(|user| user.last_name.clone()),
        "text": message.text,
        "caption": message.caption,
        "document": document_payload(message.document),
    });
    let metadata = json!({
        "trace_id": format!("trace-telegram-update-{}", update.update_id),
        "telegram_update_id": update.update_id,
        "telegram_chat_id": message.chat.id,
        "telegram_message_id": message.message_id,
        "telegram_message_thread_id": message.message_thread_id,
    });
    Ok(NormalizedTelegramUpdate {
        update_id: update.update_id,
        event_id,
        outbox_id,
        dedupe_key: format!("telegram:update:{}", update.update_id),
        source_id,
        operator_id,
        payload_json: payload.to_string(),
        metadata_json: metadata.to_string(),
    })
}

fn document_payload(document: Option<TelegramDocument>) -> Value {
    match document {
        Some(document) => json!({
            "file_id": document.file_id,
            "file_unique_id": document.file_unique_id,
            "file_name": document.file_name,
            "mime_type": document.mime_type,
            "file_size": document.file_size,
        }),
        None => Value::Null,
    }
}

fn source_id_for_chat(chat_id: i64) -> String {
    format!("telegram-chat-{}", numeric_id_token(chat_id))
}

fn user_id(user_id: i64) -> String {
    format!("telegram-user-{}", numeric_id_token(user_id))
}

fn numeric_id_token(id: i64) -> String {
    if id < 0 {
        format!("n{}", id.unsigned_abs())
    } else {
        id.to_string()
    }
}
