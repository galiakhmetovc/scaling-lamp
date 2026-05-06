use crate::bootstrap::App;
use agent_persistence::{
    DeliveryRepository, DeliveryTargetRecord, EventDeliveryRecord, EventOutboxRecord,
    EventRepository, SessionOutputRouteRecord, TranscriptRecord, TranscriptRepository,
};
use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliverySendError {
    message: String,
}

impl DeliverySendError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for DeliverySendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DeliverySendError {}

pub trait DeliverySender {
    fn send_text(&self, target: &DeliveryTargetRecord, text: &str)
    -> Result<(), DeliverySendError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryWorkerReport {
    pub outbox_id: String,
    pub delivered: usize,
    pub failed: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryWorkerErrorKind {
    MissingOutbox,
    InvalidEnvelope,
    MissingTranscript,
    Store,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryWorkerError {
    kind: DeliveryWorkerErrorKind,
    message: String,
}

impl DeliveryWorkerError {
    pub fn kind(&self) -> DeliveryWorkerErrorKind {
        self.kind
    }

    fn new(kind: DeliveryWorkerErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for DeliveryWorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "delivery worker error: {}", self.message)
    }
}

impl std::error::Error for DeliveryWorkerError {}

pub fn deliver_session_output_event<S>(
    app: &App,
    sender: &S,
    outbox_id: &str,
    now: i64,
) -> Result<DeliveryWorkerReport, DeliveryWorkerError>
where
    S: DeliverySender,
{
    let store = app.store().map_err(|error| {
        DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
    })?;
    let outbox = store
        .get_event_outbox(outbox_id)
        .map_err(|error| {
            DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
        })?
        .ok_or_else(|| {
            DeliveryWorkerError::new(
                DeliveryWorkerErrorKind::MissingOutbox,
                format!("event outbox {outbox_id} not found"),
            )
        })?;
    let envelope = OutputEnvelope::from_outbox(&outbox)?;
    let transcript =
        latest_assistant_transcript_for_run(&store, &envelope.session_id, &envelope.run_id)?;
    let routes = store
        .list_enabled_session_output_routes(&envelope.session_id)
        .map_err(|error| {
            DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
        })?;

    let mut report = DeliveryWorkerReport {
        outbox_id: outbox_id.to_string(),
        delivered: 0,
        failed: 0,
        skipped: 0,
    };

    for route in routes {
        if route_already_delivered(&route, &transcript) {
            report.skipped += 1;
            continue;
        }
        let Some(target) = store
            .get_delivery_target(&route.target_id)
            .map_err(|error| {
                DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
            })?
        else {
            persist_delivery(
                &store,
                &envelope,
                &route.target_id,
                "failed",
                Some("delivery target not found".to_string()),
                now,
            )?;
            report.failed += 1;
            continue;
        };
        let text = render_delivery_text(&envelope.session_id, &route, &target, &transcript);
        match sender.send_text(&target, &text) {
            Ok(()) => {
                persist_delivery(&store, &envelope, &target.target_id, "delivered", None, now)?;
                let mut updated_route = route.clone();
                updated_route.last_delivered_transcript_created_at = Some(transcript.created_at);
                updated_route.last_delivered_transcript_id = Some(transcript.id.clone());
                updated_route.updated_at = now;
                store
                    .put_session_output_route(&updated_route)
                    .map_err(|error| {
                        DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
                    })?;
                report.delivered += 1;
            }
            Err(error) => {
                persist_delivery(
                    &store,
                    &envelope,
                    &target.target_id,
                    "failed",
                    Some(error.to_string()),
                    now,
                )?;
                report.failed += 1;
            }
        }
    }

    Ok(report)
}

#[derive(Debug)]
struct OutputEnvelope {
    event_id: String,
    session_id: String,
    run_id: String,
}

impl OutputEnvelope {
    fn from_outbox(outbox: &EventOutboxRecord) -> Result<Self, DeliveryWorkerError> {
        let value: Value = serde_json::from_str(&outbox.payload_json).map_err(|error| {
            DeliveryWorkerError::new(
                DeliveryWorkerErrorKind::InvalidEnvelope,
                format!("invalid output envelope json: {error}"),
            )
        })?;
        let event_type = value
            .get("event_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if event_type != "session.output.created" {
            return Err(DeliveryWorkerError::new(
                DeliveryWorkerErrorKind::InvalidEnvelope,
                format!("unsupported output event_type {event_type}"),
            ));
        }
        let event_id = required_string(&value, "event_id")?;
        let run_id = value
            .get("payload_ref")
            .and_then(|payload_ref| payload_ref.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                DeliveryWorkerError::new(
                    DeliveryWorkerErrorKind::InvalidEnvelope,
                    "output envelope missing payload_ref.id",
                )
            })?;
        let session_id = value
            .get("metadata")
            .and_then(|metadata| metadata.get("session_id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                DeliveryWorkerError::new(
                    DeliveryWorkerErrorKind::InvalidEnvelope,
                    "output envelope missing metadata.session_id",
                )
            })?;
        Ok(Self {
            event_id,
            session_id,
            run_id,
        })
    }
}

fn latest_assistant_transcript_for_run(
    store: &agent_persistence::PersistenceStore,
    session_id: &str,
    run_id: &str,
) -> Result<TranscriptRecord, DeliveryWorkerError> {
    store
        .list_transcripts_for_session(session_id)
        .map_err(|error| {
            DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
        })?
        .into_iter()
        .filter(|transcript| {
            transcript.kind == "assistant" && transcript.run_id.as_deref() == Some(run_id)
        })
        .max_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        })
        .ok_or_else(|| {
            DeliveryWorkerError::new(
                DeliveryWorkerErrorKind::MissingTranscript,
                format!("assistant transcript for run {run_id} not found"),
            )
        })
}

fn route_already_delivered(
    route: &SessionOutputRouteRecord,
    transcript: &TranscriptRecord,
) -> bool {
    route.last_delivered_transcript_id.as_deref() == Some(transcript.id.as_str())
        || route
            .last_delivered_transcript_created_at
            .map(|created_at| created_at >= transcript.created_at)
            .unwrap_or(false)
}

fn render_delivery_text(
    session_id: &str,
    route: &SessionOutputRouteRecord,
    target: &DeliveryTargetRecord,
    transcript: &TranscriptRecord,
) -> String {
    let policy = if route.format_policy.trim().is_empty() {
        target.format_policy.as_str()
    } else {
        route.format_policy.as_str()
    };
    match policy {
        "status_only" => format!("Session {session_id} produced a new assistant response."),
        _ => transcript.content.clone(),
    }
}

fn persist_delivery(
    store: &agent_persistence::PersistenceStore,
    envelope: &OutputEnvelope,
    target_id: &str,
    status: &str,
    error: Option<String>,
    now: i64,
) -> Result<(), DeliveryWorkerError> {
    store
        .put_event_delivery(&EventDeliveryRecord {
            delivery_event_id: format!("delivery-{}-{target_id}", envelope.event_id),
            source_event_id: envelope.event_id.clone(),
            target_id: target_id.to_string(),
            status: status.to_string(),
            attempt_count: 1,
            created_at: now,
            updated_at: now,
            delivered_at: if status == "delivered" {
                Some(now)
            } else {
                None
            },
            last_error: error,
        })
        .map_err(|error| {
            DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
        })
}

fn required_string(value: &Value, field: &'static str) -> Result<String, DeliveryWorkerError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            DeliveryWorkerError::new(
                DeliveryWorkerErrorKind::InvalidEnvelope,
                format!("output envelope missing {field}"),
            )
        })
}
