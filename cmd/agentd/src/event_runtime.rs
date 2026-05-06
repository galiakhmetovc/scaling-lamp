use agent_persistence::AppConfig;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventRuntimeWorker {
    NatsJetStream,
    TelegramWebhook,
    Router,
    Session,
    Delivery,
    OutboxPublisher,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRuntimePlan {
    pub starts_telegram_polling: bool,
    pub workers: Vec<EventRuntimeWorker>,
    pub nats_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRuntimeStartupError {
    message: String,
}

impl EventRuntimeStartupError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for EventRuntimeStartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "event runtime startup error: {}", self.message)
    }
}

impl std::error::Error for EventRuntimeStartupError {}

pub fn build_event_runtime_plan(
    config: &AppConfig,
) -> Result<EventRuntimePlan, EventRuntimeStartupError> {
    if !config.telegram.enabled {
        return Ok(EventRuntimePlan {
            starts_telegram_polling: false,
            workers: Vec::new(),
            nats_url: config.event_bus.nats_url.clone(),
        });
    }

    match config.telegram.mode.as_str() {
        "polling" => Ok(EventRuntimePlan {
            starts_telegram_polling: true,
            workers: Vec::new(),
            nats_url: config.event_bus.nats_url.clone(),
        }),
        "webhook" => {
            let nats_url = config.event_bus.nats_url.clone().ok_or_else(|| {
                EventRuntimeStartupError::new(
                    "event_bus.nats_url is required for telegram webhook event runtime",
                )
            })?;
            if config.telegram.webhook_public_url.is_none()
                || config.telegram.webhook_secret.is_none()
            {
                return Err(EventRuntimeStartupError::new(
                    "telegram webhook_public_url and webhook_secret are required for webhook mode",
                ));
            }
            Ok(EventRuntimePlan {
                starts_telegram_polling: false,
                workers: vec![
                    EventRuntimeWorker::NatsJetStream,
                    EventRuntimeWorker::TelegramWebhook,
                    EventRuntimeWorker::Router,
                    EventRuntimeWorker::Session,
                    EventRuntimeWorker::Delivery,
                    EventRuntimeWorker::OutboxPublisher,
                ],
                nats_url: Some(nats_url),
            })
        }
        other => Err(EventRuntimeStartupError::new(format!(
            "unsupported telegram.mode {other}"
        ))),
    }
}
