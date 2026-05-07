use agent_persistence::{EventBusConfig, EventOutboxRecord};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventPayloadRef {
    pub table: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: String,
    pub event_type: String,
    pub trace_id: Option<String>,
    pub source_kind: String,
    pub source_id: String,
    pub subject: String,
    pub payload_ref: EventPayloadRef,
    pub created_at: i64,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadLetterReason {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishError {
    Transient(String),
    Permanent(String),
}

impl fmt::Display for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient(message) => write!(f, "transient publish error: {message}"),
            Self::Permanent(message) => write!(f, "permanent publish error: {message}"),
        }
    }
}

impl std::error::Error for PublishError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventPublishOutcome {
    Failed {
        should_mark_published: bool,
        error: PublishError,
    },
}

pub trait EventPublisher {
    fn publish_json(&self, subject: &str, body: &str) -> Result<(), PublishError>;
}

#[derive(Debug, Clone)]
pub struct JsonEventPublisher<P> {
    publisher: P,
}

impl<P> JsonEventPublisher<P> {
    pub fn new(publisher: P) -> Self {
        Self { publisher }
    }
}

impl<P> JsonEventPublisher<P>
where
    P: EventPublisher,
{
    pub fn publish_event(&self, envelope: EventEnvelope) -> Result<(), PublishError> {
        let subject = envelope.subject.clone();
        let body = serde_json::to_string(&build_event_envelope(envelope)?)
            .map_err(|err| PublishError::Permanent(err.to_string()))?;
        self.publisher.publish_json(&subject, &body)
    }

    pub fn publish_dead_letter(&self, body: Value) -> Result<(), PublishError> {
        let subject = body
            .get("subject")
            .and_then(Value::as_str)
            .ok_or_else(|| PublishError::Permanent("dead letter subject is missing".to_string()))?
            .to_string();
        let body =
            serde_json::to_string(&body).map_err(|err| PublishError::Permanent(err.to_string()))?;
        self.publisher.publish_json(&subject, &body)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventSubjects {
    input_stream: String,
    session_stream: String,
    delivery_stream: String,
    task_stream: String,
    dlq_stream: String,
}

impl EventSubjects {
    pub fn from_config(config: &EventBusConfig) -> Self {
        Self {
            input_stream: config.input_stream.clone(),
            session_stream: config.session_stream.clone(),
            delivery_stream: config.delivery_stream.clone(),
            task_stream: config.task_stream.clone(),
            dlq_stream: config.dlq_stream.clone(),
        }
    }

    pub fn input(&self, source_kind: &str) -> String {
        format!("teamd.input.{}", normalize_subject_token(source_kind))
    }

    pub fn session_input(&self, session_id: &str) -> String {
        format!(
            "teamd.session.{}.input",
            normalize_subject_token(session_id)
        )
    }

    pub fn session_output(&self, session_id: &str) -> String {
        format!(
            "teamd.session.{}.output",
            normalize_subject_token(session_id)
        )
    }

    pub fn delivery(&self, target_id: &str) -> String {
        format!("teamd.delivery.{}", normalize_subject_token(target_id))
    }

    pub fn task(&self, task_id: &str) -> String {
        format!("teamd.task.{}", normalize_subject_token(task_id))
    }

    pub fn dead_letter(&self) -> String {
        "teamd.dlq".to_string()
    }

    pub fn stream_subjects(&self, stream_name: &str) -> Vec<String> {
        if stream_name == self.input_stream {
            return vec!["teamd.input.*".to_string()];
        }
        if stream_name == self.session_stream {
            return vec![
                "teamd.session.*.input".to_string(),
                "teamd.session.*.output".to_string(),
            ];
        }
        if stream_name == self.delivery_stream {
            return vec!["teamd.delivery.*".to_string()];
        }
        if stream_name == self.task_stream {
            return vec!["teamd.task.*".to_string()];
        }
        if stream_name == self.dlq_stream {
            return vec!["teamd.dlq".to_string()];
        }
        Vec::new()
    }

    pub fn stream_configs(&self) -> Vec<(&str, Vec<String>)> {
        vec![
            (
                self.input_stream.as_str(),
                self.stream_subjects(&self.input_stream),
            ),
            (
                self.session_stream.as_str(),
                self.stream_subjects(&self.session_stream),
            ),
            (
                self.delivery_stream.as_str(),
                self.stream_subjects(&self.delivery_stream),
            ),
            (
                self.task_stream.as_str(),
                self.stream_subjects(&self.task_stream),
            ),
            (
                self.dlq_stream.as_str(),
                self.stream_subjects(&self.dlq_stream),
            ),
        ]
    }
}

pub fn build_event_envelope(envelope: EventEnvelope) -> Result<Value, PublishError> {
    serde_json::to_value(envelope).map_err(|err| PublishError::Permanent(err.to_string()))
}

pub fn build_dead_letter_envelope(
    original: EventEnvelope,
    reason: DeadLetterReason,
    subject: String,
    created_at: i64,
) -> Result<Value, PublishError> {
    let original = build_event_envelope(original)?;
    let original_event_id = original
        .get("event_id")
        .and_then(Value::as_str)
        .ok_or_else(|| PublishError::Permanent("original event_id is missing".to_string()))?;
    Ok(json!({
        "event_id": format!("dlq-{original_event_id}"),
        "event_type": "event.dead_letter",
        "trace_id": original.get("trace_id").cloned().unwrap_or(Value::Null),
        "subject": subject,
        "original_event": original,
        "reason": reason,
        "created_at": created_at,
    }))
}

pub fn publish_outbox_event<P>(
    publisher: &P,
    outbox: &EventOutboxRecord,
) -> Result<(), EventPublishOutcome>
where
    P: EventPublisher,
{
    publisher
        .publish_json(&outbox.subject, &outbox.payload_json)
        .map_err(|error| EventPublishOutcome::Failed {
            should_mark_published: false,
            error,
        })
}

fn normalize_subject_token(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            normalized.push(ch);
        } else {
            normalized.push('_');
        }
    }
    if normalized.is_empty() {
        "_".to_string()
    } else {
        normalized
    }
}
