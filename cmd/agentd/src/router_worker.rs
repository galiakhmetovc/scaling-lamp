use crate::bootstrap::App;
use crate::event_bus::{EventEnvelope, EventPayloadRef, EventSubjects, build_event_envelope};
use crate::event_errors::{EventErrorKind, EventRetryPolicy, EventRuntimeError};
use agent_persistence::{
    DeliveryRepository, EventOutboxRecord, EventRepository, InboundEventRecord, PersistenceStore,
    RoutedEventRecord, RouterRepository, RouterRuleRecord, SessionOutputRouteRecord,
    TranscriptRepository,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteErrorKind {
    MissingInboundEvent,
    RouteNotFound,
    InvalidPolicy,
    Store,
    Encode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteError {
    kind: RouteErrorKind,
    message: String,
}

impl RouteError {
    pub fn kind(&self) -> RouteErrorKind {
        self.kind
    }

    fn new(kind: RouteErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "route error: {}", self.message)
    }
}

impl std::error::Error for RouteError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDecision {
    pub session_id: String,
    pub agent_id: String,
    pub queue_policy: String,
    pub priority: i64,
    pub output_targets: Vec<String>,
    pub format_policy: String,
    pub tool_policy: Value,
    pub retry_policy: Value,
    pub labels: Vec<String>,
    pub matched_rule_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RoutePolicy {
    session_id: Option<String>,
    agent_id: Option<String>,
    #[serde(default)]
    queue_policy: Option<String>,
    #[serde(default)]
    output_targets: Option<Vec<String>>,
    #[serde(default)]
    format_policy: Option<String>,
    #[serde(default)]
    tool_policy: Option<Value>,
    #[serde(default)]
    retry_policy: Option<Value>,
    #[serde(default)]
    labels: Option<Vec<String>>,
}

pub fn route_inbound_event(
    app: &App,
    inbound_event_id: &str,
    now: i64,
) -> Result<RouteDecision, RouteError> {
    let store = app
        .store()
        .map_err(|error| RouteError::new(RouteErrorKind::Store, error.to_string()))?;
    let inbound = store
        .get_inbound_event(inbound_event_id)
        .map_err(|error| RouteError::new(RouteErrorKind::Store, error.to_string()))?
        .ok_or_else(|| {
            RouteError::new(
                RouteErrorKind::MissingInboundEvent,
                format!("inbound event {inbound_event_id} not found"),
            )
        })?;

    let rules = store
        .list_enabled_router_rules()
        .map_err(|error| RouteError::new(RouteErrorKind::Store, error.to_string()))?;
    let Some((rule, decision)) = resolve_route(&inbound, &rules)? else {
        persist_route_failure_dlq(app, &inbound, now)?;
        return Err(RouteError::new(
            RouteErrorKind::RouteNotFound,
            format!("no route matched inbound event {}", inbound.event_id),
        ));
    };
    materialize_output_routes(&store, &inbound, &rule, &decision, now)?;

    let routed_event_id = format!("routed-{}", inbound.event_id);
    let subjects = EventSubjects::from_config(&app.config.event_bus);
    let subject = subjects.session_input(&decision.session_id);
    let trace_id = trace_id_from_metadata(&inbound.metadata_json);
    let route_metadata = json!({
        "trace_id": trace_id,
        "matched_rule_id": decision.matched_rule_id,
        "output_targets": decision.output_targets,
        "format_policy": decision.format_policy,
        "tool_policy": decision.tool_policy,
        "retry_policy": decision.retry_policy,
        "labels": decision.labels,
    });
    let routed = RoutedEventRecord {
        routed_event_id: routed_event_id.clone(),
        inbound_event_id: inbound.event_id.clone(),
        rule_id: Some(rule.rule_id.clone()),
        session_id: decision.session_id.clone(),
        agent_id: decision.agent_id.clone(),
        queue_policy: decision.queue_policy.clone(),
        priority: decision.priority,
        payload_json: inbound.payload_json.clone(),
        metadata_json: route_metadata.to_string(),
        status: "pending".to_string(),
        routed_at: now,
        published_at: None,
        error: None,
    };
    store
        .put_routed_event(&routed)
        .map_err(|error| RouteError::new(RouteErrorKind::Store, error.to_string()))?;

    let envelope = build_event_envelope(EventEnvelope {
        event_id: routed_event_id.clone(),
        event_type: "session.input.routed".to_string(),
        trace_id,
        source_kind: "router".to_string(),
        source_id: rule.rule_id.clone(),
        subject: subject.clone(),
        payload_ref: EventPayloadRef {
            table: "routed_events".to_string(),
            id: routed_event_id.clone(),
        },
        created_at: now,
        metadata: route_metadata,
    })
    .map_err(|error| RouteError::new(RouteErrorKind::Encode, error.to_string()))?;
    let outbox = EventOutboxRecord {
        outbox_id: format!("outbox-{routed_event_id}"),
        subject,
        payload_json: serde_json::to_string(&envelope)
            .map_err(|error| RouteError::new(RouteErrorKind::Encode, error.to_string()))?,
        status: "pending".to_string(),
        attempt_count: 0,
        next_attempt_at: now,
        created_at: now,
        published_at: None,
        last_error: None,
    };
    store
        .put_event_outbox(&outbox)
        .map_err(|error| RouteError::new(RouteErrorKind::Store, error.to_string()))?;

    Ok(decision)
}

pub fn sort_inbound_events_for_routing(events: &mut [InboundEventRecord]) {
    events.sort_by(|left, right| {
        left.received_at
            .cmp(&right.received_at)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
}

fn resolve_route(
    inbound: &InboundEventRecord,
    rules: &[RouterRuleRecord],
) -> Result<Option<(RouterRuleRecord, RouteDecision)>, RouteError> {
    for rule in rules {
        if !filter_matches(&rule.source_filter_json, inbound, FilterKind::Source)? {
            continue;
        }
        if !filter_matches(&rule.operator_filter_json, inbound, FilterKind::Operator)? {
            continue;
        }
        if !condition_matches(&rule.condition_json, inbound)? {
            continue;
        }
        let decision = route_decision_from_rule(rule)?;
        return Ok(Some((rule.clone(), decision)));
    }
    Ok(None)
}

fn route_decision_from_rule(rule: &RouterRuleRecord) -> Result<RouteDecision, RouteError> {
    let policy: RoutePolicy = serde_json::from_str(&rule.route_policy_json).map_err(|error| {
        RouteError::new(
            RouteErrorKind::InvalidPolicy,
            format!("invalid route_policy_json for {}: {error}", rule.rule_id),
        )
    })?;
    let session_id = required_policy_field(policy.session_id, &rule.rule_id, "session_id")?;
    let agent_id = required_policy_field(policy.agent_id, &rule.rule_id, "agent_id")?;
    Ok(RouteDecision {
        session_id,
        agent_id,
        queue_policy: policy.queue_policy.unwrap_or_else(|| "fifo".to_string()),
        priority: rule.priority,
        output_targets: policy.output_targets.unwrap_or_default(),
        format_policy: normalize_format_policy(policy.format_policy.as_deref(), &rule.rule_id)?,
        tool_policy: policy.tool_policy.unwrap_or_else(|| json!({})),
        retry_policy: policy.retry_policy.unwrap_or_else(|| json!({})),
        labels: policy.labels.unwrap_or_default(),
        matched_rule_id: Some(rule.rule_id.clone()),
    })
}

fn normalize_format_policy(value: Option<&str>, rule_id: &str) -> Result<String, RouteError> {
    match value.unwrap_or("full_text").trim() {
        "full" | "full_text" => Ok("full_text".to_string()),
        "summary" => Ok("summary".to_string()),
        "status_only" => Ok("status_only".to_string()),
        "errors_only" => Ok("errors_only".to_string()),
        other => Err(RouteError::new(
            RouteErrorKind::InvalidPolicy,
            format!(
                "route rule {rule_id} has unsupported format_policy {other:?}; expected full_text|summary|status_only|errors_only"
            ),
        )),
    }
}

fn required_policy_field(
    value: Option<String>,
    rule_id: &str,
    field: &'static str,
) -> Result<String, RouteError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            RouteError::new(
                RouteErrorKind::InvalidPolicy,
                format!("route rule {rule_id} is missing {field}"),
            )
        })
}

fn materialize_output_routes(
    store: &PersistenceStore,
    inbound: &InboundEventRecord,
    rule: &RouterRuleRecord,
    decision: &RouteDecision,
    now: i64,
) -> Result<(), RouteError> {
    if decision.output_targets.is_empty() {
        return Ok(());
    }
    let latest_transcript = store
        .get_latest_transcript_for_session(&decision.session_id)
        .map_err(|error| RouteError::new(RouteErrorKind::Store, error.to_string()))?;
    let latest_created_at = latest_transcript
        .as_ref()
        .map(|transcript| transcript.created_at);
    let latest_id = latest_transcript
        .as_ref()
        .map(|transcript| transcript.id.clone());

    for target_id in &decision.output_targets {
        if store
            .get_delivery_target(target_id)
            .map_err(|error| RouteError::new(RouteErrorKind::Store, error.to_string()))?
            .is_none()
        {
            return Err(RouteError::new(
                RouteErrorKind::InvalidPolicy,
                format!(
                    "route rule {} references missing delivery target {}",
                    rule.rule_id, target_id
                ),
            ));
        }
        let canonical_route_id = format!("route-{}-{}", decision.session_id, target_id);
        let existing = find_existing_output_route(
            store,
            &decision.session_id,
            target_id,
            &canonical_route_id,
        )?;
        let route_id = existing
            .as_ref()
            .map(|route| route.route_id.clone())
            .unwrap_or(canonical_route_id);
        let record = SessionOutputRouteRecord {
            route_id,
            session_id: decision.session_id.clone(),
            target_id: target_id.clone(),
            filter_json: json!({
                "source": "router_rule",
                "rule_id": rule.rule_id,
                "inbound_event_id": inbound.event_id,
            })
            .to_string(),
            format_policy: decision.format_policy.clone(),
            enabled: true,
            last_delivered_transcript_created_at: existing
                .as_ref()
                .and_then(|route| route.last_delivered_transcript_created_at)
                .or(latest_created_at)
                .or(Some(0)),
            last_delivered_transcript_id: existing
                .as_ref()
                .and_then(|route| route.last_delivered_transcript_id.clone())
                .or_else(|| latest_id.clone())
                .or_else(|| Some(String::new())),
            created_at: existing
                .as_ref()
                .map(|route| route.created_at)
                .unwrap_or(now),
            updated_at: now,
        };
        store
            .put_session_output_route(&record)
            .map_err(|error| RouteError::new(RouteErrorKind::Store, error.to_string()))?;
    }
    Ok(())
}

fn find_existing_output_route(
    store: &PersistenceStore,
    session_id: &str,
    target_id: &str,
    canonical_route_id: &str,
) -> Result<Option<SessionOutputRouteRecord>, RouteError> {
    if let Some(route) = store
        .get_session_output_route(canonical_route_id)
        .map_err(|error| RouteError::new(RouteErrorKind::Store, error.to_string()))?
    {
        return Ok(Some(route));
    }
    let routes = store
        .list_enabled_session_output_routes(session_id)
        .map_err(|error| RouteError::new(RouteErrorKind::Store, error.to_string()))?;
    Ok(routes
        .into_iter()
        .find(|route| route.target_id == target_id))
}

#[derive(Debug, Clone, Copy)]
enum FilterKind {
    Source,
    Operator,
}

fn filter_matches(
    filter_json: &str,
    inbound: &InboundEventRecord,
    kind: FilterKind,
) -> Result<bool, RouteError> {
    let filter: Value = parse_json_object(filter_json, "router filter")?;
    let Some(object) = filter.as_object() else {
        return Ok(false);
    };
    if object.is_empty() {
        return Ok(true);
    }

    match kind {
        FilterKind::Source => {
            if let Some(expected) = object.get("source_id").and_then(Value::as_str)
                && inbound.source_id != expected
            {
                return Ok(false);
            }
            if let Some(expected) = object.get("source_kind").and_then(Value::as_str)
                && inbound.source_kind != expected
            {
                return Ok(false);
            }
        }
        FilterKind::Operator => {
            if let Some(expected) = object.get("operator_id").and_then(Value::as_str)
                && inbound.operator_id.as_deref() != Some(expected)
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn condition_matches(
    condition_json: &str,
    inbound: &InboundEventRecord,
) -> Result<bool, RouteError> {
    let condition: Value = parse_json_object(condition_json, "router condition")?;
    let Some(object) = condition.as_object() else {
        return Ok(false);
    };
    if object.is_empty() {
        return Ok(true);
    }
    let payload: Value = serde_json::from_str(&inbound.payload_json).unwrap_or_else(|_| json!({}));
    if let Some(expected) = object.get("text_contains").and_then(Value::as_str)
        && !payload
            .get("text")
            .and_then(Value::as_str)
            .map(|text| text.contains(expected))
            .unwrap_or(false)
    {
        return Ok(false);
    }
    if let Some(expected_payload) = object.get("payload") {
        let Some(expected_object) = expected_payload.as_object() else {
            return Err(RouteError::new(
                RouteErrorKind::InvalidPolicy,
                "router condition payload must be an object",
            ));
        };
        for (key, expected_value) in expected_object {
            if payload.get(key) != Some(expected_value) {
                return Ok(false);
            }
        }
    }
    for key in object.keys() {
        if key != "text_contains" && key != "payload" {
            return Err(RouteError::new(
                RouteErrorKind::InvalidPolicy,
                format!("unsupported router condition key {key:?}"),
            ));
        }
    }
    Ok(true)
}

fn parse_json_object(value: &str, label: &'static str) -> Result<Value, RouteError> {
    serde_json::from_str(value).map_err(|error| {
        RouteError::new(
            RouteErrorKind::InvalidPolicy,
            format!("invalid {label}: {error}"),
        )
    })
}

fn persist_route_failure_dlq(
    app: &App,
    inbound: &InboundEventRecord,
    now: i64,
) -> Result<(), RouteError> {
    let store = app
        .store()
        .map_err(|error| RouteError::new(RouteErrorKind::Store, error.to_string()))?;
    let subjects = EventSubjects::from_config(&app.config.event_bus);
    let subject = subjects.dead_letter();
    let original = EventEnvelope {
        event_id: inbound.event_id.clone(),
        event_type: format!("{}.received", inbound.source_kind),
        trace_id: trace_id_from_metadata(&inbound.metadata_json),
        source_kind: inbound.source_kind.clone(),
        source_id: inbound.source_id.clone(),
        subject: subjects.input(&inbound.source_kind),
        payload_ref: EventPayloadRef {
            table: "inbound_events".to_string(),
            id: inbound.event_id.clone(),
        },
        created_at: inbound.received_at,
        metadata: serde_json::from_str(&inbound.metadata_json).unwrap_or_else(|_| json!({})),
    };
    let error = EventRuntimeError::new(
        EventErrorKind::RouteNotFound,
        format!("no route matched inbound event {}", inbound.event_id),
    );
    let envelope = EventRetryPolicy::default()
        .dead_letter_envelope(original, &error, subject.clone(), now)
        .map_err(|error| RouteError::new(RouteErrorKind::Encode, error.to_string()))?;
    let outbox = EventOutboxRecord {
        outbox_id: format!("dlq-{}", inbound.event_id),
        subject,
        payload_json: serde_json::to_string(&envelope)
            .map_err(|error| RouteError::new(RouteErrorKind::Encode, error.to_string()))?,
        status: "pending".to_string(),
        attempt_count: 0,
        next_attempt_at: now,
        created_at: now,
        published_at: None,
        last_error: None,
    };
    store
        .put_event_outbox(&outbox)
        .map_err(|error| RouteError::new(RouteErrorKind::Store, error.to_string()))
}

fn trace_id_from_metadata(metadata_json: &str) -> Option<String> {
    serde_json::from_str::<Value>(metadata_json)
        .ok()
        .and_then(|metadata| {
            metadata
                .get("trace_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}
