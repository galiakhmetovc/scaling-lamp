use crate::bootstrap::{App, BootstrapError};
use agent_persistence::{
    JobRepository, MissionRecord, MissionRepository, PersistenceStore, RunRecord, RunRepository,
    SessionRecord, SessionRepository,
};
use agent_runtime::mission::{MissionExecutionIntent, MissionSchedule, MissionSpec, MissionStatus};
use agent_runtime::run::{RunEngine, RunSnapshot};
use agent_runtime::session::{Session, SessionSettings};
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Status,
    SessionCreate {
        id: String,
        title: String,
    },
    SessionShow {
        id: String,
    },
    MissionCreate {
        id: String,
        session_id: String,
        objective: String,
    },
    MissionShow {
        id: String,
    },
    RunShow {
        id: String,
    },
    JobShow {
        id: String,
    },
    ApprovalList {
        run_id: String,
    },
    ApprovalApprove {
        run_id: String,
        approval_id: String,
    },
    DelegateList {
        run_id: String,
    },
    VerificationShow {
        run_id: String,
    },
}

pub fn execute<I, S>(app: &App, args: I) -> Result<String, BootstrapError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let command = Command::parse(args)?;
    let store = app.store()?;

    match command {
        Command::Status => render_status(app),
        Command::SessionCreate { id, title } => create_session(&store, &id, &title),
        Command::SessionShow { id } => show_session(&store, &id),
        Command::MissionCreate {
            id,
            session_id,
            objective,
        } => create_mission(&store, &id, &session_id, &objective),
        Command::MissionShow { id } => show_mission(&store, &id),
        Command::RunShow { id } => show_run(&store, &id),
        Command::JobShow { id } => show_job(&store, &id),
        Command::ApprovalList { run_id } => list_approvals(&store, &run_id),
        Command::ApprovalApprove {
            run_id,
            approval_id,
        } => approve_run(&store, &run_id, &approval_id),
        Command::DelegateList { run_id } => list_delegates(&store, &run_id),
        Command::VerificationShow { run_id } => show_verification(&store, &run_id),
    }
}

impl Command {
    fn parse<I, S>(args: I) -> Result<Self, BootstrapError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let args = args
            .into_iter()
            .map(|value| value.as_ref().to_string())
            .collect::<Vec<_>>();

        match args.as_slice() {
            [] => Ok(Self::Status),
            [status] if status == "status" => Ok(Self::Status),
            [scope, action, id, title @ ..] if scope == "session" && action == "create" => {
                let title = join_required(title, "session title")?;
                Ok(Self::SessionCreate {
                    id: id.clone(),
                    title,
                })
            }
            [scope, action, id] if scope == "session" && action == "show" => {
                Ok(Self::SessionShow { id: id.clone() })
            }
            [scope, action, id, session_id, objective @ ..]
                if scope == "mission" && action == "create" =>
            {
                let objective = join_required(objective, "mission objective")?;
                Ok(Self::MissionCreate {
                    id: id.clone(),
                    session_id: session_id.clone(),
                    objective,
                })
            }
            [scope, action, id] if scope == "mission" && action == "show" => {
                Ok(Self::MissionShow { id: id.clone() })
            }
            [scope, action, id] if scope == "run" && action == "show" => {
                Ok(Self::RunShow { id: id.clone() })
            }
            [scope, action, id] if scope == "job" && action == "show" => {
                Ok(Self::JobShow { id: id.clone() })
            }
            [scope, action, run_id] if scope == "approval" && action == "list" => {
                Ok(Self::ApprovalList {
                    run_id: run_id.clone(),
                })
            }
            [scope, action, run_id, approval_id]
                if scope == "approval" && action == "approve" =>
            {
                Ok(Self::ApprovalApprove {
                    run_id: run_id.clone(),
                    approval_id: approval_id.clone(),
                })
            }
            [scope, action, run_id] if scope == "delegate" && action == "list" => {
                Ok(Self::DelegateList {
                    run_id: run_id.clone(),
                })
            }
            [scope, action, run_id] if scope == "verification" && action == "show" => {
                Ok(Self::VerificationShow {
                    run_id: run_id.clone(),
                })
            }
            _ => Err(BootstrapError::Usage {
                reason: "expected one of: status | session create/show | mission create/show | run show | job show | approval list/approve | delegate list | verification show".to_string(),
            }),
        }
    }
}

fn create_session(
    store: &PersistenceStore,
    id: &str,
    title: &str,
) -> Result<String, BootstrapError> {
    let now = unix_timestamp()?;
    let session = Session {
        id: id.to_string(),
        title: title.to_string(),
        prompt_override: None,
        settings: SessionSettings::default(),
        active_mission_id: None,
        created_at: now,
        updated_at: now,
    };
    let record = SessionRecord::try_from(&session).map_err(BootstrapError::RecordConversion)?;
    store.put_session(&record)?;
    Ok(format!(
        "created session {} title={}",
        record.id, record.title
    ))
}

fn show_session(store: &PersistenceStore, id: &str) -> Result<String, BootstrapError> {
    let record = store
        .get_session(id)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "session",
            id: id.to_string(),
        })?;

    Ok(format!(
        "session id={} title={} active_mission_id={} settings={}",
        record.id,
        record.title,
        record.active_mission_id.as_deref().unwrap_or("<none>"),
        record.settings_json
    ))
}

fn create_mission(
    store: &PersistenceStore,
    id: &str,
    session_id: &str,
    objective: &str,
) -> Result<String, BootstrapError> {
    if store.get_session(session_id)?.is_none() {
        return Err(BootstrapError::MissingRecord {
            kind: "session",
            id: session_id.to_string(),
        });
    }

    let now = unix_timestamp()?;
    let mission = MissionSpec {
        id: id.to_string(),
        session_id: session_id.to_string(),
        objective: objective.to_string(),
        status: MissionStatus::Ready,
        execution_intent: MissionExecutionIntent::Autonomous,
        schedule: MissionSchedule::once(),
        acceptance_criteria: Vec::new(),
        created_at: now,
        updated_at: now,
        completed_at: None,
    };
    let record = MissionRecord::try_from(&mission).map_err(BootstrapError::RecordConversion)?;
    store.put_mission(&record)?;
    Ok(format!(
        "created mission {} session_id={} objective={}",
        record.id, record.session_id, record.objective
    ))
}

fn show_mission(store: &PersistenceStore, id: &str) -> Result<String, BootstrapError> {
    let record = store
        .get_mission(id)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "mission",
            id: id.to_string(),
        })?;

    Ok(format!(
        "mission id={} session_id={} status={} execution_intent={} objective={} schedule={} acceptance={}",
        record.id,
        record.session_id,
        record.status,
        record.execution_intent,
        record.objective,
        record.schedule_json,
        record.acceptance_json
    ))
}

fn show_run(store: &PersistenceStore, id: &str) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, id)?;
    Ok(format!(
        "run id={} session_id={} mission_id={} status={} pending_approvals={} delegates={} evidence_refs={}",
        snapshot.id,
        snapshot.session_id,
        snapshot.mission_id.as_deref().unwrap_or("<none>"),
        snapshot.status.as_str(),
        snapshot.pending_approvals.len(),
        snapshot.delegate_runs.len(),
        snapshot.evidence_refs.len()
    ))
}

fn show_job(store: &PersistenceStore, id: &str) -> Result<String, BootstrapError> {
    let record = store
        .get_job(id)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "job",
            id: id.to_string(),
        })?;

    Ok(format!(
        "job id={} mission_id={} run_id={} kind={} status={} input={} result={}",
        record.id,
        record.mission_id,
        record.run_id.as_deref().unwrap_or("<none>"),
        record.kind,
        record.status,
        record.input_json.as_deref().unwrap_or("<none>"),
        record.result_json.as_deref().unwrap_or("<none>")
    ))
}

fn list_approvals(store: &PersistenceStore, run_id: &str) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, run_id)?;
    if snapshot.pending_approvals.is_empty() {
        return Ok(format!("approval run_id={} none", run_id));
    }

    let approvals = snapshot
        .pending_approvals
        .iter()
        .map(|approval| {
            format!(
                "{} tool_call_id={} reason={}",
                approval.id, approval.tool_call_id, approval.reason
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");
    Ok(format!("approval run_id={} {}", run_id, approvals))
}

fn approve_run(
    store: &PersistenceStore,
    run_id: &str,
    approval_id: &str,
) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, run_id)?;
    let mut engine = RunEngine::from_snapshot(snapshot);
    engine
        .resolve_approval(approval_id, unix_timestamp()?)
        .map_err(BootstrapError::RunTransition)?;
    let record =
        RunRecord::try_from(engine.snapshot()).map_err(BootstrapError::RecordConversion)?;
    store.put_run(&record)?;
    Ok(format!("approved {} on run {}", approval_id, run_id))
}

fn list_delegates(store: &PersistenceStore, run_id: &str) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, run_id)?;
    if snapshot.delegate_runs.is_empty() {
        return Ok(format!("delegate run_id={} none", run_id));
    }

    let delegates = snapshot
        .delegate_runs
        .iter()
        .map(|delegate| format!("{} label={}", delegate.id, delegate.label))
        .collect::<Vec<_>>()
        .join(" | ");
    Ok(format!("delegate run_id={} {}", run_id, delegates))
}

fn show_verification(store: &PersistenceStore, run_id: &str) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, run_id)?;
    let refs = if snapshot.evidence_refs.is_empty() {
        "<none>".to_string()
    } else {
        snapshot.evidence_refs.join(",")
    };
    Ok(format!("verification run_id={} refs={}", run_id, refs))
}

fn render_status(app: &App) -> Result<String, BootstrapError> {
    let connection = Connection::open(&app.persistence.stores.metadata_db)?;
    let session_count = count_rows(&connection, "sessions")?;
    let mission_count = count_rows(&connection, "missions")?;
    let run_count = count_rows(&connection, "runs")?;
    let job_count = count_rows(&connection, "jobs")?;

    Ok(format!(
        "status data_dir={} sessions={} missions={} runs={} jobs={} components={} state_db={}",
        app.config.data_dir.display(),
        session_count,
        mission_count,
        run_count,
        job_count,
        app.runtime.component_count(),
        app.persistence.stores.metadata_db.display()
    ))
}

fn count_rows(connection: &Connection, table: &str) -> Result<i64, BootstrapError> {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(BootstrapError::Sqlite)
}

fn load_run_snapshot(
    store: &PersistenceStore,
    run_id: &str,
) -> Result<RunSnapshot, BootstrapError> {
    let record = store
        .get_run(run_id)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "run",
            id: run_id.to_string(),
        })?;
    RunSnapshot::try_from(record).map_err(BootstrapError::RecordConversion)
}

fn join_required(parts: &[String], label: &'static str) -> Result<String, BootstrapError> {
    let joined = parts.join(" ");
    if joined.trim().is_empty() {
        return Err(BootstrapError::Usage {
            reason: format!("{label} must not be empty"),
        });
    }

    Ok(joined)
}

fn unix_timestamp() -> Result<i64, BootstrapError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(BootstrapError::Clock)?
        .as_secs() as i64)
}
