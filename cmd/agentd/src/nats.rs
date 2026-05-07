use crate::event_bus::{
    DeadLetterReason, EventEnvelope, EventSubjects, PublishError, build_dead_letter_envelope,
    build_event_envelope,
};
use agent_persistence::EventBusConfig;
use async_nats::jetstream;
use async_nats::jetstream::consumer;
use async_nats::jetstream::consumer::Consumer;
use async_nats::jetstream::consumer::pull;
use async_nats::jetstream::stream;
use std::fmt;

#[derive(Debug)]
pub enum NatsEventBusError {
    MissingUrl,
    Connect(String),
    Stream(String),
    Publish(String),
    Consumer(String),
    Encode(PublishError),
}

impl fmt::Display for NatsEventBusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingUrl => write!(f, "event_bus.nats_url is required"),
            Self::Connect(message) => write!(f, "nats connect error: {message}"),
            Self::Stream(message) => write!(f, "nats stream error: {message}"),
            Self::Publish(message) => write!(f, "nats publish error: {message}"),
            Self::Consumer(message) => write!(f, "nats consumer error: {message}"),
            Self::Encode(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for NatsEventBusError {}

#[derive(Clone)]
pub struct NatsEventBus {
    context: jetstream::Context,
    subjects: EventSubjects,
}

impl NatsEventBus {
    pub async fn connect(config: &EventBusConfig) -> Result<Self, NatsEventBusError> {
        let url = config
            .nats_url
            .as_deref()
            .ok_or(NatsEventBusError::MissingUrl)?;
        let client = async_nats::connect(url)
            .await
            .map_err(|err| NatsEventBusError::Connect(err.to_string()))?;
        let context = jetstream::new(client);
        let bus = Self {
            context,
            subjects: EventSubjects::from_config(config),
        };
        bus.ensure_streams().await?;
        Ok(bus)
    }

    pub fn subjects(&self) -> &EventSubjects {
        &self.subjects
    }

    pub async fn ensure_streams(&self) -> Result<(), NatsEventBusError> {
        for (name, subjects) in self.subjects.stream_configs() {
            self.context
                .get_or_create_stream(stream::Config {
                    name: name.to_string(),
                    subjects,
                    ..Default::default()
                })
                .await
                .map_err(|err| NatsEventBusError::Stream(err.to_string()))?;
        }
        Ok(())
    }

    pub async fn publish_json(&self, subject: &str, body: &str) -> Result<(), NatsEventBusError> {
        let ack = self
            .context
            .publish(subject.to_string(), body.to_string().into())
            .await
            .map_err(|err| NatsEventBusError::Publish(err.to_string()))?;
        ack.await
            .map_err(|err| NatsEventBusError::Publish(err.to_string()))?;
        Ok(())
    }

    pub async fn publish_event(&self, envelope: EventEnvelope) -> Result<(), NatsEventBusError> {
        let subject = envelope.subject.clone();
        let body = serde_json::to_string(
            &build_event_envelope(envelope).map_err(NatsEventBusError::Encode)?,
        )
        .map_err(|err| NatsEventBusError::Publish(err.to_string()))?;
        self.publish_json(&subject, &body).await
    }

    pub async fn publish_dead_letter(
        &self,
        original: EventEnvelope,
        reason: DeadLetterReason,
        created_at: i64,
    ) -> Result<(), NatsEventBusError> {
        let subject = self.subjects.dead_letter();
        let body = build_dead_letter_envelope(original, reason, subject.clone(), created_at)
            .map_err(NatsEventBusError::Encode)?;
        let body = serde_json::to_string(&body)
            .map_err(|err| NatsEventBusError::Publish(err.to_string()))?;
        self.publish_json(&subject, &body).await
    }

    pub async fn pull_consumer(
        &self,
        stream_name: &str,
        consumer_name: &str,
        filter_subject: &str,
    ) -> Result<Consumer<pull::Config>, NatsEventBusError> {
        let stream = self
            .context
            .get_stream(stream_name)
            .await
            .map_err(|err| NatsEventBusError::Stream(err.to_string()))?;
        stream
            .get_or_create_consumer(
                consumer_name,
                pull::Config {
                    durable_name: Some(consumer_name.to_string()),
                    name: Some(consumer_name.to_string()),
                    filter_subject: filter_subject.to_string(),
                    ack_policy: consumer::AckPolicy::Explicit,
                    ..Default::default()
                },
            )
            .await
            .map_err(|err| NatsEventBusError::Consumer(err.to_string()))
    }
}
