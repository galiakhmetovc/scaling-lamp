use crate::repository::RunRepository;
use crate::store::{PersistenceStore, StoreError};
use agent_runtime::run::RunStatus;
use std::error::Error;
use std::fmt;

const NON_RECOVERABLE_RUN_REASON: &str = "runtime restart interrupted a non-recoverable run state";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecoveryMode {
    #[default]
    Reconcile,
    MarkInterrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecoveryPolicy {
    pub mode: RecoveryMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecoveryReport {
    pub scanned_runs: usize,
    pub interrupted_runs: usize,
}

#[derive(Debug)]
pub enum RecoveryError {
    Store(StoreError),
}

pub fn reconcile_runs(
    store: &PersistenceStore,
    policy: RecoveryPolicy,
    at: i64,
) -> Result<RecoveryReport, RecoveryError> {
    let runs = store.list_runs().map_err(RecoveryError::Store)?;
    let mut report = RecoveryReport {
        scanned_runs: runs.len(),
        interrupted_runs: 0,
    };

    for record in runs {
        if should_interrupt_run(record.status.as_str(), policy.mode) {
            let mut updated = record;
            updated.status = RunStatus::Interrupted.as_str().to_string();
            updated.error = Some(NON_RECOVERABLE_RUN_REASON.to_string());
            updated.updated_at = at;
            updated.finished_at = Some(at);
            store.put_run(&updated).map_err(RecoveryError::Store)?;
            report.interrupted_runs += 1;
        }
    }

    Ok(report)
}

fn should_interrupt_run(status: &str, mode: RecoveryMode) -> bool {
    match mode {
        RecoveryMode::MarkInterrupted => !matches!(
            status,
            "queued" | "completed" | "failed" | "cancelled" | "interrupted"
        ),
        RecoveryMode::Reconcile => matches!(
            status,
            "running" | "waiting_process" | "waiting_delegate" | "resuming"
        ),
    }
}

impl fmt::Display for RecoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(source) => write!(formatter, "recovery store error: {source}"),
        }
    }
}

impl Error for RecoveryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(source) => Some(source),
        }
    }
}
