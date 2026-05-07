use agentd::event_bus::{EventEnvelope, EventPayloadRef};
use agentd::event_errors::{EventErrorKind, EventRetryPolicy, EventRuntimeError};
use serde_json::json;

fn original_event() -> EventEnvelope {
    EventEnvelope {
        event_id: "event-err-1".to_string(),
        event_type: "telegram.message.received".to_string(),
        trace_id: Some("trace-err-1".to_string()),
        source_kind: "telegram".to_string(),
        source_id: "chat-100".to_string(),
        subject: "teamd.input.telegram".to_string(),
        payload_ref: EventPayloadRef {
            table: "inbound_events".to_string(),
            id: "inbound-err-1".to_string(),
        },
        created_at: 1770000100,
        metadata: json!({"dedupe_key": "telegram:update:200"}),
    }
}

#[test]
fn transient_infrastructure_errors_are_retryable() {
    for error in [
        EventRuntimeError::new(EventErrorKind::Nats, "nats connection reset"),
        EventRuntimeError::new(EventErrorKind::Telegram, "telegram rate limited"),
        EventRuntimeError::new(EventErrorKind::Postgres, "database is locked"),
    ] {
        assert!(error.is_retryable(), "{error:?}");
        assert!(!error.is_terminal(), "{error:?}");
    }
}

#[test]
fn invalid_or_unauthorized_input_errors_are_not_retryable() {
    for error in [
        EventRuntimeError::new(EventErrorKind::InvalidWebhookSecret, "bad secret"),
        EventRuntimeError::new(EventErrorKind::InvalidPayload, "missing message"),
        EventRuntimeError::new(EventErrorKind::UnauthorizedSource, "chat is not paired"),
    ] {
        assert!(!error.is_retryable(), "{error:?}");
        assert!(error.is_terminal(), "{error:?}");
    }
}

#[test]
fn retry_policy_dead_letters_when_max_attempts_are_exceeded() {
    let policy = EventRetryPolicy { max_attempts: 3 };
    let error = EventRuntimeError::new(EventErrorKind::Nats, "nats publish failed");

    assert!(policy.should_retry(&error, 2));
    assert!(!policy.should_dead_letter(&error, 2));
    assert!(!policy.should_retry(&error, 3));
    assert!(policy.should_dead_letter(&error, 3));
}

#[test]
fn non_retryable_errors_dead_letter_immediately() {
    let policy = EventRetryPolicy { max_attempts: 3 };
    let error = EventRuntimeError::new(EventErrorKind::RouteNotFound, "no matching route");

    assert!(!policy.should_retry(&error, 0));
    assert!(policy.should_dead_letter(&error, 0));
}

#[test]
fn dlq_envelope_preserves_original_event_trace_source_payload_and_reason() {
    let policy = EventRetryPolicy { max_attempts: 3 };
    let error = EventRuntimeError::new(EventErrorKind::UnauthorizedSource, "chat is not paired");

    let dlq = policy
        .dead_letter_envelope(
            original_event(),
            &error,
            "teamd.dlq".to_string(),
            1770000110,
        )
        .expect("dead letter envelope");

    assert_eq!(dlq["event_id"], "dlq-event-err-1");
    assert_eq!(dlq["event_type"], "event.dead_letter");
    assert_eq!(dlq["trace_id"], "trace-err-1");
    assert_eq!(dlq["original_event"]["event_id"], "event-err-1");
    assert_eq!(dlq["original_event"]["source_kind"], "telegram");
    assert_eq!(dlq["original_event"]["source_id"], "chat-100");
    assert_eq!(dlq["original_event"]["payload_ref"]["id"], "inbound-err-1");
    assert_eq!(dlq["reason"]["code"], "unauthorized_source");
    assert_eq!(dlq["reason"]["retryable"], false);
}
