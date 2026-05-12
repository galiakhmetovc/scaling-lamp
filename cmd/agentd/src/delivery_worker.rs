use crate::bootstrap::App;
use agent_persistence::{
    DeliveryRepository, DeliveryTargetRecord, EventDeliveryRecord, EventOutboxRecord,
    EventRepository, RunRecord, RunRepository, SessionOutputRouteRecord, SessionRecord,
    SessionRepository, TaskRegistryRecord, TaskRegistryRepository, TranscriptRecord,
    TranscriptRepository,
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
    let session = store
        .get_session(&envelope.session_id)
        .map_err(|error| {
            DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
        })?
        .ok_or_else(|| {
            DeliveryWorkerError::new(
                DeliveryWorkerErrorKind::InvalidEnvelope,
                format!("session {} not found", envelope.session_id),
            )
        })?;
    let run = store
        .get_run(&envelope.run_id)
        .map_err(|error| {
            DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
        })?
        .ok_or_else(|| {
            DeliveryWorkerError::new(
                DeliveryWorkerErrorKind::InvalidEnvelope,
                format!("run {} not found", envelope.run_id),
            )
        })?;
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
        if let Err(error) = target_allows_session_output(&target, &session) {
            persist_delivery(
                &store,
                &envelope,
                &target.target_id,
                "failed",
                Some(error),
                now,
            )?;
            report.failed += 1;
            continue;
        }
        let Some(text) =
            render_delivery_text(&envelope.session_id, &route, &target, &transcript, &run)
        else {
            report.skipped += 1;
            continue;
        };
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

pub fn deliver_task_result_event<S>(
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
    let envelope = TaskResultEnvelope::from_outbox(&outbox)?;
    let task = store
        .get_task_registry(&envelope.task_id)
        .map_err(|error| {
            DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
        })?
        .ok_or_else(|| {
            DeliveryWorkerError::new(
                DeliveryWorkerErrorKind::InvalidEnvelope,
                format!("task {} not found", envelope.task_id),
            )
        })?;
    let followers = store
        .list_enabled_task_followers(&envelope.task_id)
        .map_err(|error| {
            DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
        })?;

    let mut report = DeliveryWorkerReport {
        outbox_id: outbox_id.to_string(),
        delivered: 0,
        failed: 0,
        skipped: 0,
    };

    for follower in followers {
        if follower.delivered_at.is_some() {
            report.skipped += 1;
            continue;
        }
        let Some(target) = store
            .get_delivery_target(&follower.target_id)
            .map_err(|error| {
                DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
            })?
        else {
            persist_event_delivery(
                &store,
                &envelope.event_id,
                &follower.target_id,
                "failed",
                Some("delivery target not found".to_string()),
                now,
            )?;
            let mut updated_follower = follower;
            updated_follower.updated_at = now;
            updated_follower.last_error = Some("delivery target not found".to_string());
            store
                .put_task_follower(&updated_follower)
                .map_err(|error| {
                    DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
                })?;
            report.failed += 1;
            continue;
        };
        if let Err(error) = target_allows_task_result(&target, &task) {
            persist_event_delivery(
                &store,
                &envelope.event_id,
                &target.target_id,
                "failed",
                Some(error.clone()),
                now,
            )?;
            let mut updated_follower = follower;
            updated_follower.updated_at = now;
            updated_follower.last_error = Some(error);
            store
                .put_task_follower(&updated_follower)
                .map_err(|error| {
                    DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
                })?;
            report.failed += 1;
            continue;
        }
        let Some(text) = render_task_result_text(&task, &target) else {
            report.skipped += 1;
            continue;
        };
        match sender.send_text(&target, &text) {
            Ok(()) => {
                persist_event_delivery(
                    &store,
                    &envelope.event_id,
                    &target.target_id,
                    "delivered",
                    None,
                    now,
                )?;
                let mut updated_follower = follower;
                updated_follower.updated_at = now;
                updated_follower.delivered_at = Some(now);
                updated_follower.last_error = None;
                store
                    .put_task_follower(&updated_follower)
                    .map_err(|error| {
                        DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
                    })?;
                report.delivered += 1;
            }
            Err(error) => {
                persist_event_delivery(
                    &store,
                    &envelope.event_id,
                    &target.target_id,
                    "failed",
                    Some(error.to_string()),
                    now,
                )?;
                let mut updated_follower = follower;
                updated_follower.updated_at = now;
                updated_follower.last_error = Some(error.to_string());
                store
                    .put_task_follower(&updated_follower)
                    .map_err(|error| {
                        DeliveryWorkerError::new(DeliveryWorkerErrorKind::Store, error.to_string())
                    })?;
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

#[derive(Debug)]
struct TaskResultEnvelope {
    event_id: String,
    task_id: String,
}

impl TaskResultEnvelope {
    fn from_outbox(outbox: &EventOutboxRecord) -> Result<Self, DeliveryWorkerError> {
        let value: Value = serde_json::from_str(&outbox.payload_json).map_err(|error| {
            DeliveryWorkerError::new(
                DeliveryWorkerErrorKind::InvalidEnvelope,
                format!("invalid task result envelope json: {error}"),
            )
        })?;
        let event_type = value
            .get("event_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !matches!(
            event_type,
            "agent_task.completed" | "agent_task.failed" | "agent_task.blocked"
        ) {
            return Err(DeliveryWorkerError::new(
                DeliveryWorkerErrorKind::InvalidEnvelope,
                format!("unsupported task result event_type {event_type}"),
            ));
        }
        let task_id = value
            .get("payload_ref")
            .and_then(|payload_ref| {
                let table = payload_ref.get("table").and_then(Value::as_str)?;
                (table == "task_registry").then(|| payload_ref.get("id"))?
            })
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                DeliveryWorkerError::new(
                    DeliveryWorkerErrorKind::InvalidEnvelope,
                    "task result envelope missing task_registry payload_ref.id",
                )
            })?;
        Ok(Self {
            event_id: required_string(&value, "event_id")?,
            task_id,
        })
    }
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
    run: &RunRecord,
) -> Option<String> {
    let policy = if route.format_policy.trim().is_empty() {
        target.format_policy.as_str()
    } else {
        route.format_policy.as_str()
    };
    match policy {
        "status_only" => Some(format!(
            "Session {session_id} produced a new assistant response."
        )),
        "summary" => Some(format!(
            "Session {session_id} assistant response:\n{}",
            bounded_single_line(&transcript.content, 700)
        )),
        "errors_only" if run.status == "failed" || run.status == "cancelled" => Some(format!(
            "Session {session_id} failed: {}",
            run.error.as_deref().unwrap_or("unknown error")
        )),
        "errors_only" => None,
        _ => Some(transcript.content.clone()),
    }
}

fn target_allows_session_output(
    target: &DeliveryTargetRecord,
    session: &SessionRecord,
) -> Result<(), String> {
    let allowed_sessions =
        parse_string_array(&target.allowed_session_ids_json, "allowed_session_ids_json")?;
    if !allowed_sessions.is_empty() && !allowed_sessions.iter().any(|id| id == &session.id) {
        return Err(format!(
            "delivery target {} is not allowed for session {}",
            target.target_id, session.id
        ));
    }
    let allowed_agents =
        parse_string_array(&target.allowed_agent_ids_json, "allowed_agent_ids_json")?;
    if !allowed_agents.is_empty()
        && !allowed_agents
            .iter()
            .any(|id| id == &session.agent_profile_id)
    {
        return Err(format!(
            "delivery target {} is not allowed for agent {}",
            target.target_id, session.agent_profile_id
        ));
    }
    Ok(())
}

fn target_allows_task_result(
    target: &DeliveryTargetRecord,
    task: &TaskRegistryRecord,
) -> Result<(), String> {
    let allowed_sessions =
        parse_string_array(&target.allowed_session_ids_json, "allowed_session_ids_json")?;
    if !allowed_sessions.is_empty() {
        let Some(source_session_id) = task.source_session_id.as_deref() else {
            return Err(format!(
                "delivery target {} is not allowed for task {} without source session",
                target.target_id, task.task_id
            ));
        };
        if !allowed_sessions.iter().any(|id| id == source_session_id) {
            return Err(format!(
                "delivery target {} is not allowed for session {}",
                target.target_id, source_session_id
            ));
        }
    }

    let allowed_agents =
        parse_string_array(&target.allowed_agent_ids_json, "allowed_agent_ids_json")?;
    if !allowed_agents.is_empty() {
        let owner_allowed = task
            .owner_agent_id
            .as_deref()
            .is_some_and(|agent_id| allowed_agents.iter().any(|id| id == agent_id));
        let executor_allowed = task
            .executor_agent_id
            .as_deref()
            .is_some_and(|agent_id| allowed_agents.iter().any(|id| id == agent_id));
        if !owner_allowed && !executor_allowed {
            return Err(format!(
                "delivery target {} is not allowed for task agents owner={:?} executor={:?}",
                target.target_id, task.owner_agent_id, task.executor_agent_id
            ));
        }
    }

    Ok(())
}

fn parse_string_array(raw: &str, label: &'static str) -> Result<Vec<String>, String> {
    serde_json::from_str::<Vec<String>>(raw)
        .map_err(|error| format!("invalid delivery target {label}: {error}"))
}

fn bounded_single_line(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    let mut trimmed = compact.chars().take(max_chars).collect::<String>();
    trimmed.push_str("...");
    trimmed
}

fn render_task_result_text(
    task: &TaskRegistryRecord,
    target: &DeliveryTargetRecord,
) -> Option<String> {
    match target.format_policy.as_str() {
        "status_only" => Some(format!(
            "Task {} finished with status {}.",
            task.task_id, task.status
        )),
        "summary" => Some(format!(
            "Task {} {} ({})",
            task.task_id, task.status, task.kind
        )),
        "errors_only"
            if task.status == "failed"
                || task.status == "blocked"
                || task.status == "cancelled" =>
        {
            Some(format!(
                "Task {} finished with status {}: {}",
                task.task_id,
                task.status,
                task.error.as_deref().unwrap_or("no error detail")
            ))
        }
        "errors_only" => None,
        _ => {
            let mut lines = vec![
                "Task result:".to_string(),
                format!("- id: {}", task.task_id),
                format!("- status: {}", task.status),
                format!("- kind: {}", task.kind),
            ];
            if let Some(source_session_id) = task.source_session_id.as_deref() {
                lines.push(format!("- source_session_id: {source_session_id}"));
            }
            if let Some(executor_agent_id) = task.executor_agent_id.as_deref() {
                lines.push(format!("- executor_agent_id: {executor_agent_id}"));
            }
            if let Some(error) = task.error.as_deref() {
                lines.push(format!("- error: {error}"));
            }
            if let Some(result_ref_json) = task.result_ref_json.as_deref() {
                lines.push("- result_ref_json:".to_string());
                lines.extend(indent_lines(&pretty_json_str(result_ref_json), "  "));
            }
            Some(lines.join("\n"))
        }
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
    persist_event_delivery(store, &envelope.event_id, target_id, status, error, now)
}

fn persist_event_delivery(
    store: &agent_persistence::PersistenceStore,
    event_id: &str,
    target_id: &str,
    status: &str,
    error: Option<String>,
    now: i64,
) -> Result<(), DeliveryWorkerError> {
    store
        .put_event_delivery(&EventDeliveryRecord {
            delivery_event_id: format!("delivery-{event_id}-{target_id}"),
            source_event_id: event_id.to_string(),
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

fn pretty_json_str(raw: &str) -> String {
    serde_json::from_str::<Value>(raw)
        .map(|value| serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()))
        .unwrap_or_else(|_| raw.to_string())
}

fn indent_lines(value: &str, prefix: &str) -> Vec<String> {
    value
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect()
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
