use crate::event_bus::{DeadLetterReason, EventEnvelope, PublishError, build_dead_letter_envelope};
use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventErrorKind {
    Nats,
    Telegram,
    Postgres,
    InvalidWebhookSecret,
    InvalidPayload,
    UnauthorizedSource,
    RouteNotFound,
    InvalidPolicy,
    WorkerTimeout,
    Internal,
}

impl EventErrorKind {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Nats => "nats",
            Self::Telegram => "telegram",
            Self::Postgres => "postgres",
            Self::InvalidWebhookSecret => "invalid_webhook_secret",
            Self::InvalidPayload => "invalid_payload",
            Self::UnauthorizedSource => "unauthorized_source",
            Self::RouteNotFound => "route_not_found",
            Self::InvalidPolicy => "invalid_policy",
            Self::WorkerTimeout => "worker_timeout",
            Self::Internal => "internal",
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Nats | Self::Telegram | Self::Postgres | Self::WorkerTimeout | Self::Internal
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRuntimeError {
    pub kind: EventErrorKind,
    pub message: String,
}

impl EventRuntimeError {
    pub fn new(kind: EventErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn is_retryable(&self) -> bool {
        self.kind.is_retryable()
    }

    pub fn is_terminal(&self) -> bool {
        !self.is_retryable()
    }

    pub fn dead_letter_reason(&self) -> DeadLetterReason {
        DeadLetterReason {
            code: self.kind.code().to_string(),
            message: self.message.clone(),
            retryable: self.is_retryable(),
        }
    }
}

impl fmt::Display for EventRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.code(), self.message)
    }
}

impl std::error::Error for EventRuntimeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventRetryPolicy {
    pub max_attempts: i64,
}

impl Default for EventRetryPolicy {
    fn default() -> Self {
        Self { max_attempts: 5 }
    }
}

impl EventRetryPolicy {
    pub fn should_retry(&self, error: &EventRuntimeError, attempt_count: i64) -> bool {
        error.is_retryable() && attempt_count < self.max_attempts
    }

    pub fn should_dead_letter(&self, error: &EventRuntimeError, attempt_count: i64) -> bool {
        !self.should_retry(error, attempt_count)
    }

    pub fn dead_letter_envelope(
        &self,
        original: EventEnvelope,
        error: &EventRuntimeError,
        subject: String,
        created_at: i64,
    ) -> Result<Value, PublishError> {
        build_dead_letter_envelope(original, error.dead_letter_reason(), subject, created_at)
    }
}
