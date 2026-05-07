use crate::bootstrap::App;
use crate::event_bus::{EventEnvelope, EventPayloadRef, EventSubjects, build_event_envelope};
use agent_persistence::{
    EventOutboxRecord, EventRepository, TaskRegistryRecord, TaskRegistryRepository,
};
use agent_runtime::mission::{JobSpec, JobStatus};
use serde_json::{Value, json};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskWorkerStatus {
    Completed,
    Failed,
    Blocked,
    Running,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskWorkerReport {
    pub task_id: String,
    pub job_id: Option<String>,
    pub status: TaskWorkerStatus,
    pub run_id: Option<String>,
    pub result_outbox_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskWorkerErrorKind {
    MissingTask,
    InvalidPayload,
    Store,
    Execution,
    Encode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskWorkerError {
    kind: TaskWorkerErrorKind,
    message: String,
}

impl TaskWorkerError {
    pub fn kind(&self) -> TaskWorkerErrorKind {
        self.kind
    }

    fn new(kind: TaskWorkerErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for TaskWorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "task worker error: {}", self.message)
    }
}

impl std::error::Error for TaskWorkerError {}

pub fn execute_task_event_envelope(
    app: &App,
    envelope: EventEnvelope,
    now: i64,
) -> Result<TaskWorkerReport, TaskWorkerError> {
    if envelope.event_type != "agent_task.created" && envelope.event_type != "delegate.created" {
        return Ok(TaskWorkerReport {
            task_id: envelope.payload_ref.id,
            job_id: None,
            status: TaskWorkerStatus::Ignored,
            run_id: None,
            result_outbox_id: None,
        });
    }
    if envelope.payload_ref.table != "task_registry" {
        return Err(TaskWorkerError::new(
            TaskWorkerErrorKind::InvalidPayload,
            format!(
                "task event {} has unsupported payload_ref table {}",
                envelope.event_id, envelope.payload_ref.table
            ),
        ));
    }

    let store = app
        .store()
        .map_err(|error| TaskWorkerError::new(TaskWorkerErrorKind::Store, error.to_string()))?;
    let task_id = envelope.payload_ref.id.clone();
    let mut task = store
        .get_task_registry(&task_id)
        .map_err(|error| TaskWorkerError::new(TaskWorkerErrorKind::Store, error.to_string()))?
        .ok_or_else(|| {
            TaskWorkerError::new(
                TaskWorkerErrorKind::MissingTask,
                format!("task {task_id} not found"),
            )
        })?;

    if let Some(report) = terminal_report_from_task(&task) {
        return Ok(report);
    }

    let job_id = job_id_from_context_refs(&task.context_ref_json).ok_or_else(|| {
        TaskWorkerError::new(
            TaskWorkerErrorKind::InvalidPayload,
            format!("task {task_id} context refs do not contain a jobs ref"),
        )
    })?;

    task.status = "running".to_string();
    task.started_at = task.started_at.or(Some(now));
    task.updated_at = now;
    task.attempt_count = task.attempt_count.saturating_add(1);
    store
        .put_task_registry(&task)
        .map_err(|error| TaskWorkerError::new(TaskWorkerErrorKind::Store, error.to_string()))?;

    let provider = app
        .provider_driver()
        .map_err(|error| TaskWorkerError::new(TaskWorkerErrorKind::Execution, error.to_string()))?;
    let execution_service = app.execution_service();
    let job =
        match execution_service.execute_task_backed_job(&store, provider.as_ref(), &job_id, now) {
            Ok(job) => job,
            Err(error) => {
                let mut failed = task;
                failed.status = "failed".to_string();
                failed.updated_at = now;
                failed.finished_at = Some(now);
                failed.error = Some(error.to_string());
                store.put_task_registry(&failed).map_err(|store_error| {
                    TaskWorkerError::new(TaskWorkerErrorKind::Store, store_error.to_string())
                })?;
                return Err(TaskWorkerError::new(
                    TaskWorkerErrorKind::Execution,
                    error.to_string(),
                ));
            }
        };

    let (status, task_status, finished_at, error) = status_from_job(&job);
    let result_ref_json = Some(
        json!({
            "table": "jobs",
            "job_id": job.id,
            "session_id": job.session_id,
            "run_id": job.run_id,
            "status": job.status.as_str(),
        })
        .to_string(),
    );
    let mut updated_task = task;
    updated_task.status = task_status.to_string();
    updated_task.result_ref_json = result_ref_json;
    updated_task.updated_at = now;
    updated_task.finished_at = finished_at;
    updated_task.error = error;
    store
        .put_task_registry(&updated_task)
        .map_err(|error| TaskWorkerError::new(TaskWorkerErrorKind::Store, error.to_string()))?;

    let result_outbox_id = if finished_at.is_some() {
        Some(persist_task_result_event(
            app,
            &updated_task,
            &job,
            status,
            now,
        )?)
    } else {
        None
    };

    Ok(TaskWorkerReport {
        task_id,
        job_id: Some(job_id),
        status,
        run_id: job.run_id,
        result_outbox_id,
    })
}

fn terminal_report_from_task(task: &TaskRegistryRecord) -> Option<TaskWorkerReport> {
    let status = match task.status.as_str() {
        "completed" => TaskWorkerStatus::Completed,
        "failed" | "cancelled" => TaskWorkerStatus::Failed,
        "blocked" => TaskWorkerStatus::Blocked,
        _ => return None,
    };
    Some(TaskWorkerReport {
        task_id: task.task_id.clone(),
        job_id: job_id_from_context_refs(&task.context_ref_json),
        status,
        run_id: run_id_from_result_ref(task.result_ref_json.as_deref()),
        result_outbox_id: None,
    })
}

fn job_id_from_context_refs(context_ref_json: &str) -> Option<String> {
    let refs: Value = serde_json::from_str(context_ref_json).ok()?;
    refs.as_array()?.iter().find_map(|item| {
        let table = item.get("table").and_then(Value::as_str)?;
        if table != "jobs" {
            return None;
        }
        item.get("id").and_then(Value::as_str).map(str::to_string)
    })
}

fn run_id_from_result_ref(result_ref_json: Option<&str>) -> Option<String> {
    let value: Value = serde_json::from_str(result_ref_json?).ok()?;
    value
        .get("run_id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn status_from_job(job: &JobSpec) -> (TaskWorkerStatus, &'static str, Option<i64>, Option<String>) {
    match job.status {
        JobStatus::Completed => (
            TaskWorkerStatus::Completed,
            "completed",
            job.finished_at,
            None,
        ),
        JobStatus::Failed => (
            TaskWorkerStatus::Failed,
            "failed",
            job.finished_at,
            job.error.clone(),
        ),
        JobStatus::Cancelled => (
            TaskWorkerStatus::Failed,
            "cancelled",
            job.finished_at,
            job.error.clone(),
        ),
        JobStatus::Blocked => (
            TaskWorkerStatus::Blocked,
            "blocked",
            job.finished_at.or(Some(job.updated_at)),
            job.error.clone(),
        ),
        _ => (TaskWorkerStatus::Running, "running", None, None),
    }
}

fn persist_task_result_event(
    app: &App,
    task: &TaskRegistryRecord,
    job: &JobSpec,
    status: TaskWorkerStatus,
    now: i64,
) -> Result<String, TaskWorkerError> {
    let store = app
        .store()
        .map_err(|error| TaskWorkerError::new(TaskWorkerErrorKind::Store, error.to_string()))?;
    let subjects = EventSubjects::from_config(&app.config.event_bus);
    let subject = subjects.task(&task.task_id);
    let event_type = match status {
        TaskWorkerStatus::Completed => "agent_task.completed",
        TaskWorkerStatus::Failed => "agent_task.failed",
        TaskWorkerStatus::Blocked => "agent_task.blocked",
        TaskWorkerStatus::Running | TaskWorkerStatus::Ignored => "agent_task.updated",
    };
    let envelope = build_event_envelope(EventEnvelope {
        event_id: format!("task-result-{}", task.task_id),
        event_type: event_type.to_string(),
        trace_id: task.trace_id.clone(),
        source_kind: "task_worker".to_string(),
        source_id: task.task_id.clone(),
        subject: subject.clone(),
        payload_ref: EventPayloadRef {
            table: "task_registry".to_string(),
            id: task.task_id.clone(),
        },
        created_at: now,
        metadata: json!({
            "task_id": task.task_id,
            "job_id": job.id,
            "session_id": job.session_id,
            "run_id": job.run_id,
            "status": job.status.as_str(),
        }),
    })
    .map_err(|error| TaskWorkerError::new(TaskWorkerErrorKind::Encode, error.to_string()))?;
    let outbox_id = format!("outbox-task-result-{}", task.task_id);
    let outbox = EventOutboxRecord {
        outbox_id: outbox_id.clone(),
        subject,
        payload_json: serde_json::to_string(&envelope).map_err(|error| {
            TaskWorkerError::new(TaskWorkerErrorKind::Encode, error.to_string())
        })?,
        status: "pending".to_string(),
        attempt_count: 0,
        next_attempt_at: now,
        created_at: now,
        published_at: None,
        last_error: None,
    };
    store
        .put_event_outbox(&outbox)
        .map_err(|error| TaskWorkerError::new(TaskWorkerErrorKind::Store, error.to_string()))?;
    Ok(outbox_id)
}
