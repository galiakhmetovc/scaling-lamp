use crate::bootstrap::App;
use crate::event_bus::{EventEnvelope, EventPayloadRef, EventSubjects, build_event_envelope};
use agent_persistence::{
    EventOutboxRecord, EventRepository, RoutedEventRecord, TaskRegistryRecord,
    TaskRegistryRepository,
};
use serde_json::{Value, json};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionWorkerStatus {
    Completed,
    WaitingDependency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWorkerReport {
    pub routed_event_id: String,
    pub session_id: String,
    pub status: SessionWorkerStatus,
    pub run_id: Option<String>,
    pub output_outbox_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionWorkerErrorKind {
    MissingRoutedEvent,
    InvalidPayload,
    Store,
    Execution,
    Encode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWorkerError {
    kind: SessionWorkerErrorKind,
    message: String,
}

impl SessionWorkerError {
    pub fn kind(&self) -> SessionWorkerErrorKind {
        self.kind
    }

    fn new(kind: SessionWorkerErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for SessionWorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "session worker error: {}", self.message)
    }
}

impl std::error::Error for SessionWorkerError {}

pub fn execute_routed_session_event(
    app: &App,
    routed_event_id: &str,
    now: i64,
) -> Result<SessionWorkerReport, SessionWorkerError> {
    let store = app.store().map_err(|error| {
        SessionWorkerError::new(SessionWorkerErrorKind::Store, error.to_string())
    })?;
    let routed = store
        .get_routed_event(routed_event_id)
        .map_err(|error| SessionWorkerError::new(SessionWorkerErrorKind::Store, error.to_string()))?
        .ok_or_else(|| {
            SessionWorkerError::new(
                SessionWorkerErrorKind::MissingRoutedEvent,
                format!("routed event {routed_event_id} not found"),
            )
        })?;
    let metadata = parse_json(&routed.metadata_json).unwrap_or_else(|_| json!({}));
    let dependencies = dependencies_from_metadata(&metadata);
    let task_id = format!("task-{routed_event_id}");

    if !dependencies.is_empty() {
        store
            .put_task_registry(&task_record(
                &task_id,
                &routed,
                "waiting_dependency",
                json!(dependencies).to_string(),
                None,
                now,
                None,
            ))
            .map_err(|error| {
                SessionWorkerError::new(SessionWorkerErrorKind::Store, error.to_string())
            })?;
        return Ok(SessionWorkerReport {
            routed_event_id: routed_event_id.to_string(),
            session_id: routed.session_id,
            status: SessionWorkerStatus::WaitingDependency,
            run_id: None,
            output_outbox_id: None,
        });
    }

    store
        .put_task_registry(&task_record(
            &task_id,
            &routed,
            "running",
            "[]".to_string(),
            None,
            now,
            None,
        ))
        .map_err(|error| {
            SessionWorkerError::new(SessionWorkerErrorKind::Store, error.to_string())
        })?;

    let message = message_text_from_routed(&routed)?;
    let report = app
        .execute_chat_turn(&routed.session_id, &message, now)
        .map_err(|error| {
            SessionWorkerError::new(SessionWorkerErrorKind::Execution, error.to_string())
        })?;

    let output_outbox_id = persist_output_event(app, &routed, &report.run_id, now)?;
    let mut completed = routed.clone();
    completed.status = "completed".to_string();
    completed.published_at = Some(now);
    store.put_routed_event(&completed).map_err(|error| {
        SessionWorkerError::new(SessionWorkerErrorKind::Store, error.to_string())
    })?;
    store
        .put_task_registry(&task_record(
            &task_id,
            &routed,
            "completed",
            "[]".to_string(),
            Some(
                json!({
                    "run_id": report.run_id,
                    "response_id": report.response_id,
                    "routed_event_id": routed.routed_event_id,
                })
                .to_string(),
            ),
            now,
            Some(now),
        ))
        .map_err(|error| {
            SessionWorkerError::new(SessionWorkerErrorKind::Store, error.to_string())
        })?;

    Ok(SessionWorkerReport {
        routed_event_id: routed_event_id.to_string(),
        session_id: report.session_id,
        status: SessionWorkerStatus::Completed,
        run_id: Some(report.run_id),
        output_outbox_id: Some(output_outbox_id),
    })
}

fn persist_output_event(
    app: &App,
    routed: &RoutedEventRecord,
    run_id: &str,
    now: i64,
) -> Result<String, SessionWorkerError> {
    let store = app.store().map_err(|error| {
        SessionWorkerError::new(SessionWorkerErrorKind::Store, error.to_string())
    })?;
    let subjects = EventSubjects::from_config(&app.config.event_bus);
    let subject = subjects.session_output(&routed.session_id);
    let outbox_id = format!("outbox-output-{}", routed.routed_event_id);
    let envelope = build_event_envelope(EventEnvelope {
        event_id: format!("output-{}", routed.routed_event_id),
        event_type: "session.output.created".to_string(),
        trace_id: trace_id_from_metadata(&routed.metadata_json),
        source_kind: "session_worker".to_string(),
        source_id: routed.routed_event_id.clone(),
        subject: subject.clone(),
        payload_ref: EventPayloadRef {
            table: "runs".to_string(),
            id: run_id.to_string(),
        },
        created_at: now,
        metadata: json!({
            "routed_event_id": routed.routed_event_id,
            "session_id": routed.session_id,
            "agent_id": routed.agent_id,
        }),
    })
    .map_err(|error| SessionWorkerError::new(SessionWorkerErrorKind::Encode, error.to_string()))?;
    let outbox = EventOutboxRecord {
        outbox_id: outbox_id.clone(),
        subject,
        payload_json: serde_json::to_string(&envelope).map_err(|error| {
            SessionWorkerError::new(SessionWorkerErrorKind::Encode, error.to_string())
        })?,
        status: "pending".to_string(),
        attempt_count: 0,
        next_attempt_at: now,
        created_at: now,
        published_at: None,
        last_error: None,
    };
    store.put_event_outbox(&outbox).map_err(|error| {
        SessionWorkerError::new(SessionWorkerErrorKind::Store, error.to_string())
    })?;
    Ok(outbox_id)
}

fn message_text_from_routed(routed: &RoutedEventRecord) -> Result<String, SessionWorkerError> {
    let payload = parse_json(&routed.payload_json)?;
    payload
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| {
            SessionWorkerError::new(
                SessionWorkerErrorKind::InvalidPayload,
                format!(
                    "routed event {} payload does not contain text",
                    routed.routed_event_id
                ),
            )
        })
}

fn dependencies_from_metadata(metadata: &Value) -> Vec<String> {
    metadata
        .get("dependencies")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn task_record(
    task_id: &str,
    routed: &RoutedEventRecord,
    status: &str,
    dependency_json: String,
    result_ref_json: Option<String>,
    now: i64,
    finished_at: Option<i64>,
) -> TaskRegistryRecord {
    TaskRegistryRecord {
        task_id: task_id.to_string(),
        kind: "session_input".to_string(),
        source_session_id: Some(routed.session_id.clone()),
        owner_agent_id: Some(routed.agent_id.clone()),
        executor_agent_id: Some(routed.agent_id.clone()),
        parent_task_id: None,
        status: status.to_string(),
        dependency_json,
        context_ref_json: json!({
            "routed_event_id": routed.routed_event_id,
            "inbound_event_id": routed.inbound_event_id,
        })
        .to_string(),
        result_ref_json,
        retry_policy_json: "{}".to_string(),
        attempt_count: if status == "waiting_dependency" { 0 } else { 1 },
        max_attempts: 1,
        timeout_at: None,
        chain_id: None,
        hop_count: None,
        max_hops: None,
        trace_id: trace_id_from_metadata(&routed.metadata_json),
        created_at: now,
        updated_at: now,
        started_at: if status == "waiting_dependency" {
            None
        } else {
            Some(now)
        },
        finished_at,
        error: None,
    }
}

fn parse_json(value: &str) -> Result<Value, SessionWorkerError> {
    serde_json::from_str(value).map_err(|error| {
        SessionWorkerError::new(
            SessionWorkerErrorKind::InvalidPayload,
            format!("invalid routed event json: {error}"),
        )
    })
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
