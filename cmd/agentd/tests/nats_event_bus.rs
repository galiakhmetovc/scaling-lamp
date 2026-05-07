use agent_persistence::{EventBusConfig, EventOutboxRecord};
use agentd::event_bus::{
    DeadLetterReason, EventEnvelope, EventPayloadRef, EventPublishOutcome, EventPublisher,
    EventSubjects, JsonEventPublisher, PublishError, build_dead_letter_envelope,
    build_event_envelope, publish_outbox_event,
};
use agentd::nats::NatsEventBus;
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct RecordingPublisher {
    state: Arc<Mutex<RecordingPublisherState>>,
}

#[derive(Debug, Default)]
struct RecordingPublisherState {
    fail_next: Option<PublishError>,
    published: Vec<(String, String)>,
}

impl RecordingPublisher {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingPublisherState::default())),
        }
    }

    fn fail_next(&self, error: PublishError) {
        self.state.lock().expect("publisher state").fail_next = Some(error);
    }

    fn published(&self) -> Vec<(String, String)> {
        self.state
            .lock()
            .expect("publisher state")
            .published
            .clone()
    }
}

impl EventPublisher for RecordingPublisher {
    fn publish_json(&self, subject: &str, body: &str) -> Result<(), PublishError> {
        let mut state = self.state.lock().expect("publisher state");
        if let Some(error) = state.fail_next.take() {
            return Err(error);
        }
        state
            .published
            .push((subject.to_string(), body.to_string()));
        Ok(())
    }
}

fn test_config() -> EventBusConfig {
    EventBusConfig {
        required: true,
        backend: "nats_jetstream".to_string(),
        nats_url: Some("nats://127.0.0.1:4222".to_string()),
        input_stream: "TEAMD_INPUT".to_string(),
        session_stream: "TEAMD_SESSION".to_string(),
        delivery_stream: "TEAMD_DELIVERY".to_string(),
        task_stream: "TEAMD_TASKS".to_string(),
        dlq_stream: "TEAMD_DLQ".to_string(),
    }
}

fn payload_ref() -> EventPayloadRef {
    EventPayloadRef {
        table: "inbound_events".to_string(),
        id: "inbound-1".to_string(),
    }
}

#[test]
fn event_subjects_are_computed_from_runtime_config() {
    let subjects = EventSubjects::from_config(&test_config());

    assert_eq!(subjects.input("telegram"), "teamd.input.telegram");
    assert_eq!(
        subjects.session_input("session-123"),
        "teamd.session.session-123.input"
    );
    assert_eq!(
        subjects.session_output("session-123"),
        "teamd.session.session-123.output"
    );
    assert_eq!(
        subjects.delivery("telegram-main"),
        "teamd.delivery.telegram-main"
    );
    assert_eq!(subjects.task("task-7"), "teamd.task.task-7");
    assert_eq!(subjects.dead_letter(), "teamd.dlq");

    assert_eq!(
        subjects.stream_subjects("TEAMD_SESSION"),
        vec!["teamd.session.*.input", "teamd.session.*.output"]
    );
}

#[test]
fn event_envelope_contains_trace_source_and_payload_ref() {
    let envelope = build_event_envelope(EventEnvelope {
        event_id: "event-1".to_string(),
        event_type: "telegram.message.received".to_string(),
        trace_id: Some("trace-1".to_string()),
        source_kind: "telegram".to_string(),
        source_id: "chat-42".to_string(),
        subject: "teamd.input.telegram".to_string(),
        payload_ref: payload_ref(),
        created_at: 1770000000,
        metadata: json!({"priority": "normal"}),
    })
    .expect("event envelope");

    assert_eq!(envelope["event_id"], "event-1");
    assert_eq!(envelope["event_type"], "telegram.message.received");
    assert_eq!(envelope["trace_id"], "trace-1");
    assert_eq!(envelope["source_kind"], "telegram");
    assert_eq!(envelope["source_id"], "chat-42");
    assert_eq!(envelope["subject"], "teamd.input.telegram");
    assert_eq!(envelope["payload_ref"]["table"], "inbound_events");
    assert_eq!(envelope["payload_ref"]["id"], "inbound-1");
    assert_eq!(envelope["metadata"]["priority"], "normal");
}

#[test]
fn publish_errors_are_reported_without_marking_outbox_published() {
    let publisher = RecordingPublisher::new();
    publisher.fail_next(PublishError::Transient("nats unavailable".to_string()));

    let outbox = EventOutboxRecord {
        outbox_id: "outbox-1".to_string(),
        subject: "teamd.input.telegram".to_string(),
        payload_json: json!({"table": "inbound_events", "id": "inbound-1"}).to_string(),
        status: "pending".to_string(),
        attempt_count: 0,
        next_attempt_at: 1770000000,
        created_at: 1770000000,
        published_at: None,
        last_error: None,
    };

    let outcome =
        publish_outbox_event(&publisher, &outbox).expect_err("publish should surface error");

    assert_eq!(
        outcome,
        EventPublishOutcome::Failed {
            should_mark_published: false,
            error: PublishError::Transient("nats unavailable".to_string())
        }
    );
    assert!(publisher.published().is_empty());
}

#[test]
fn json_publisher_serializes_and_publishes_event_envelopes() {
    let publisher = RecordingPublisher::new();
    let json_publisher = JsonEventPublisher::new(publisher.clone());

    json_publisher
        .publish_event(EventEnvelope {
            event_id: "event-2".to_string(),
            event_type: "session.input.created".to_string(),
            trace_id: Some("trace-2".to_string()),
            source_kind: "router".to_string(),
            source_id: "rule-default".to_string(),
            subject: "teamd.session.session-1.input".to_string(),
            payload_ref: payload_ref(),
            created_at: 1770000001,
            metadata: json!({}),
        })
        .expect("publish event");

    let published = publisher.published();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].0, "teamd.session.session-1.input");

    let body: serde_json::Value = serde_json::from_str(&published[0].1).expect("json body");
    assert_eq!(body["event_id"], "event-2");
    assert_eq!(body["payload_ref"]["id"], "inbound-1");
}

#[test]
fn dead_letter_envelope_preserves_original_event_and_reason() {
    let dlq = build_dead_letter_envelope(
        EventEnvelope {
            event_id: "event-3".to_string(),
            event_type: "telegram.message.received".to_string(),
            trace_id: Some("trace-3".to_string()),
            source_kind: "telegram".to_string(),
            source_id: "chat-42".to_string(),
            subject: "teamd.input.telegram".to_string(),
            payload_ref: payload_ref(),
            created_at: 1770000002,
            metadata: json!({"dedupe_key": "telegram:update:100"}),
        },
        DeadLetterReason {
            code: "route_not_found".to_string(),
            message: "no enabled router rule matched".to_string(),
            retryable: false,
        },
        "teamd.dlq".to_string(),
        1770000010,
    )
    .expect("dead letter envelope");

    assert_eq!(dlq["event_id"], "dlq-event-3");
    assert_eq!(dlq["event_type"], "event.dead_letter");
    assert_eq!(dlq["original_event"]["event_id"], "event-3");
    assert_eq!(dlq["original_event"]["trace_id"], "trace-3");
    assert_eq!(dlq["reason"]["code"], "route_not_found");
    assert_eq!(dlq["reason"]["retryable"], false);
    assert_eq!(dlq["subject"], "teamd.dlq");
}

#[test]
fn nats_jetstream_integration() {
    let Ok(url) = std::env::var("TEAMD_TEST_NATS_URL") else {
        eprintln!("skipping real NATS integration test: TEAMD_TEST_NATS_URL is not set");
        return;
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    runtime.block_on(async {
        let mut config = test_config();
        config.nats_url = Some(url);
        config.input_stream = "TEAMD_TEST_INPUT".to_string();
        config.session_stream = "TEAMD_TEST_SESSION".to_string();
        config.delivery_stream = "TEAMD_TEST_DELIVERY".to_string();
        config.task_stream = "TEAMD_TEST_TASKS".to_string();
        config.dlq_stream = "TEAMD_TEST_DLQ".to_string();

        let bus = NatsEventBus::connect(&config).await.expect("connect nats");
        bus.publish_event(EventEnvelope {
            event_id: "event-integration-1".to_string(),
            event_type: "integration.test".to_string(),
            trace_id: Some("trace-integration-1".to_string()),
            source_kind: "test".to_string(),
            source_id: "nats_event_bus".to_string(),
            subject: bus.subjects().input("integration"),
            payload_ref: payload_ref(),
            created_at: 1770000011,
            metadata: json!({"integration": true}),
        })
        .await
        .expect("publish integration event");
    });
}
