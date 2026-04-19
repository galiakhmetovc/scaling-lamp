use crate::verification::EvidenceBundle;
use std::error::Error;
use std::fmt;

const RECENT_STEP_LIMIT: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunStatus {
    #[default]
    Queued,
    Running,
    WaitingApproval,
    WaitingProcess,
    WaitingDelegate,
    Resuming,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSnapshot {
    pub id: String,
    pub session_id: String,
    pub mission_id: Option<String>,
    pub status: RunStatus,
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
    pub result: Option<String>,
    pub pending_approvals: Vec<ApprovalRequest>,
    pub active_processes: Vec<ActiveProcess>,
    pub recent_steps: Vec<RunStep>,
    pub provider_stream: Option<ProviderStreamState>,
    pub delegate_runs: Vec<DelegateRun>,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub id: String,
    pub tool_call_id: String,
    pub reason: String,
    pub requested_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveProcess {
    pub id: String,
    pub kind: String,
    pub pid_ref: String,
    pub started_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegateRun {
    pub id: String,
    pub label: String,
    pub started_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderStreamState {
    pub response_id: String,
    pub model: String,
    pub output_text: String,
    pub started_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStep {
    pub kind: RunStepKind,
    pub detail: String,
    pub recorded_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStepKind {
    Started,
    ProviderStreamStarted,
    ProviderTextDelta,
    ProviderStreamFinished,
    EvidenceRecorded,
    WaitingApproval,
    ApprovalResolved,
    WaitingProcess,
    ProcessCompleted,
    WaitingDelegate,
    DelegateCompleted,
    Resumed,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug)]
pub enum RunTransitionError {
    InvalidTransition {
        action: &'static str,
        status: RunStatus,
    },
    MissingApproval {
        id: String,
    },
    MissingDelegate {
        id: String,
    },
    MissingProcess {
        id: String,
    },
    ProviderStreamInactive,
}

#[derive(Debug, Clone)]
pub struct RunEngine {
    snapshot: RunSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStatusParseError {
    value: String,
}

impl Default for RunSnapshot {
    fn default() -> Self {
        Self {
            id: String::new(),
            session_id: String::new(),
            mission_id: None,
            status: RunStatus::Queued,
            started_at: 0,
            updated_at: 0,
            finished_at: None,
            error: None,
            result: None,
            pending_approvals: Vec::new(),
            active_processes: Vec::new(),
            recent_steps: Vec::new(),
            provider_stream: None,
            delegate_runs: Vec::new(),
            evidence_refs: Vec::new(),
        }
    }
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingApproval => "waiting_approval",
            Self::WaitingProcess => "waiting_process",
            Self::WaitingDelegate => "waiting_delegate",
            Self::Resuming => "resuming",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

impl TryFrom<&str> for RunStatus {
    type Error = RunStatusParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "waiting_approval" => Ok(Self::WaitingApproval),
            "waiting_process" => Ok(Self::WaitingProcess),
            "waiting_delegate" => Ok(Self::WaitingDelegate),
            "resuming" => Ok(Self::Resuming),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(RunStatusParseError {
                value: value.to_string(),
            }),
        }
    }
}

impl ApprovalRequest {
    pub fn new(
        id: impl Into<String>,
        tool_call_id: impl Into<String>,
        reason: impl Into<String>,
        requested_at: i64,
    ) -> Self {
        Self {
            id: id.into(),
            tool_call_id: tool_call_id.into(),
            reason: reason.into(),
            requested_at,
        }
    }
}

impl ActiveProcess {
    pub fn new(
        id: impl Into<String>,
        kind: impl Into<String>,
        pid_ref: impl Into<String>,
        started_at: i64,
    ) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            pid_ref: pid_ref.into(),
            started_at,
        }
    }
}

impl DelegateRun {
    pub fn new(id: impl Into<String>, label: impl Into<String>, started_at: i64) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            started_at,
        }
    }
}

impl RunEngine {
    pub fn new(
        id: impl Into<String>,
        session_id: impl Into<String>,
        mission_id: Option<&str>,
        started_at: i64,
    ) -> Self {
        Self {
            snapshot: RunSnapshot {
                id: id.into(),
                session_id: session_id.into(),
                mission_id: mission_id.map(str::to_owned),
                status: RunStatus::Queued,
                started_at,
                updated_at: started_at,
                ..RunSnapshot::default()
            },
        }
    }

    pub fn snapshot(&self) -> &RunSnapshot {
        &self.snapshot
    }

    pub fn start(&mut self, at: i64) -> Result<(), RunTransitionError> {
        self.require_status("start", &[RunStatus::Queued])?;
        self.snapshot.status = RunStatus::Running;
        self.touch(at);
        self.push_step(RunStepKind::Started, "run started", at);
        Ok(())
    }

    pub fn resume(&mut self, at: i64) -> Result<(), RunTransitionError> {
        self.require_status("resume", &[RunStatus::Resuming])?;
        self.snapshot.status = RunStatus::Running;
        self.touch(at);
        self.push_step(RunStepKind::Resumed, "run resumed", at);
        Ok(())
    }

    pub fn begin_provider_stream(
        &mut self,
        response_id: impl Into<String>,
        model: impl Into<String>,
        at: i64,
    ) -> Result<(), RunTransitionError> {
        self.require_status("begin_provider_stream", &[RunStatus::Running])?;
        self.snapshot.provider_stream = Some(ProviderStreamState {
            response_id: response_id.into(),
            model: model.into(),
            output_text: String::new(),
            started_at: at,
            updated_at: at,
        });
        self.touch(at);
        self.push_step(
            RunStepKind::ProviderStreamStarted,
            "provider stream started",
            at,
        );
        Ok(())
    }

    pub fn push_provider_text(
        &mut self,
        delta: impl AsRef<str>,
        at: i64,
    ) -> Result<(), RunTransitionError> {
        self.require_status("push_provider_text", &[RunStatus::Running])?;
        let delta = delta.as_ref();
        let stream = self
            .snapshot
            .provider_stream
            .as_mut()
            .ok_or(RunTransitionError::ProviderStreamInactive)?;
        stream.output_text.push_str(delta);
        stream.updated_at = at;
        self.touch(at);
        self.push_step(
            RunStepKind::ProviderTextDelta,
            format!("provider delta: {delta}"),
            at,
        );
        Ok(())
    }

    pub fn finish_provider_stream(&mut self, at: i64) -> Result<(), RunTransitionError> {
        self.require_status("finish_provider_stream", &[RunStatus::Running])?;
        if self.snapshot.provider_stream.take().is_none() {
            return Err(RunTransitionError::ProviderStreamInactive);
        }

        self.touch(at);
        self.push_step(
            RunStepKind::ProviderStreamFinished,
            "provider stream finished",
            at,
        );
        Ok(())
    }

    pub fn wait_for_approval(
        &mut self,
        approval: ApprovalRequest,
        at: i64,
    ) -> Result<(), RunTransitionError> {
        self.require_status("wait_for_approval", &[RunStatus::Running])?;
        let detail = format!("waiting for approval {}", approval.id);
        self.snapshot.pending_approvals.push(approval);
        self.snapshot.status = RunStatus::WaitingApproval;
        self.touch(at);
        self.push_step(RunStepKind::WaitingApproval, detail, at);
        Ok(())
    }

    pub fn record_evidence(
        &mut self,
        bundle: &EvidenceBundle,
        at: i64,
    ) -> Result<(), RunTransitionError> {
        self.require_not_terminal("record_evidence")?;

        for evidence_ref in bundle.refs() {
            if !self.snapshot.evidence_refs.contains(&evidence_ref) {
                self.snapshot.evidence_refs.push(evidence_ref);
            }
        }

        self.touch(at);
        self.push_step(
            RunStepKind::EvidenceRecorded,
            format!("recorded evidence bundle {}", bundle.id),
            at,
        );
        Ok(())
    }

    pub fn resolve_approval(
        &mut self,
        approval_id: &str,
        at: i64,
    ) -> Result<(), RunTransitionError> {
        self.require_status("resolve_approval", &[RunStatus::WaitingApproval])?;
        let removed = remove_by_id(&mut self.snapshot.pending_approvals, approval_id);

        if !removed {
            return Err(RunTransitionError::MissingApproval {
                id: approval_id.to_string(),
            });
        }

        if self.snapshot.pending_approvals.is_empty() {
            self.snapshot.status = RunStatus::Resuming;
        }
        self.touch(at);
        self.push_step(
            RunStepKind::ApprovalResolved,
            format!("approval resolved {approval_id}"),
            at,
        );
        Ok(())
    }

    pub fn wait_for_process(
        &mut self,
        process: ActiveProcess,
        at: i64,
    ) -> Result<(), RunTransitionError> {
        self.require_status("wait_for_process", &[RunStatus::Running])?;
        let detail = format!("waiting for process {}", process.id);
        self.snapshot.active_processes.push(process);
        self.snapshot.status = RunStatus::WaitingProcess;
        self.touch(at);
        self.push_step(RunStepKind::WaitingProcess, detail, at);
        Ok(())
    }

    pub fn complete_process(
        &mut self,
        process_id: &str,
        exit_code: Option<i32>,
        at: i64,
    ) -> Result<(), RunTransitionError> {
        self.require_status("complete_process", &[RunStatus::WaitingProcess])?;
        let removed = remove_by_id(&mut self.snapshot.active_processes, process_id);

        if !removed {
            return Err(RunTransitionError::MissingProcess {
                id: process_id.to_string(),
            });
        }

        if self.snapshot.active_processes.is_empty() {
            self.snapshot.status = RunStatus::Resuming;
        }
        self.touch(at);
        self.push_step(
            RunStepKind::ProcessCompleted,
            format!("process {process_id} completed with {:?}", exit_code),
            at,
        );
        Ok(())
    }

    pub fn wait_for_delegate(
        &mut self,
        delegate: DelegateRun,
        at: i64,
    ) -> Result<(), RunTransitionError> {
        self.require_status("wait_for_delegate", &[RunStatus::Running])?;
        let detail = format!("waiting for delegate {}", delegate.id);
        self.snapshot.delegate_runs.push(delegate);
        self.snapshot.status = RunStatus::WaitingDelegate;
        self.touch(at);
        self.push_step(RunStepKind::WaitingDelegate, detail, at);
        Ok(())
    }

    pub fn complete_delegate(
        &mut self,
        delegate_id: &str,
        at: i64,
    ) -> Result<(), RunTransitionError> {
        self.require_status("complete_delegate", &[RunStatus::WaitingDelegate])?;
        let removed = remove_by_id(&mut self.snapshot.delegate_runs, delegate_id);

        if !removed {
            return Err(RunTransitionError::MissingDelegate {
                id: delegate_id.to_string(),
            });
        }

        if self.snapshot.delegate_runs.is_empty() {
            self.snapshot.status = RunStatus::Resuming;
        }
        self.touch(at);
        self.push_step(
            RunStepKind::DelegateCompleted,
            format!("delegate {delegate_id} completed"),
            at,
        );
        Ok(())
    }

    pub fn complete(
        &mut self,
        result: impl Into<String>,
        at: i64,
    ) -> Result<(), RunTransitionError> {
        self.require_not_terminal("complete")?;
        self.snapshot.status = RunStatus::Completed;
        self.snapshot.result = Some(result.into());
        self.snapshot.finished_at = Some(at);
        self.snapshot.provider_stream = None;
        self.snapshot.pending_approvals.clear();
        self.snapshot.active_processes.clear();
        self.snapshot.delegate_runs.clear();
        self.touch(at);
        self.push_step(RunStepKind::Completed, "run completed", at);
        Ok(())
    }

    pub fn fail(&mut self, error: impl Into<String>, at: i64) -> Result<(), RunTransitionError> {
        self.require_not_terminal("fail")?;
        self.snapshot.status = RunStatus::Failed;
        self.snapshot.error = Some(error.into());
        self.snapshot.finished_at = Some(at);
        self.snapshot.provider_stream = None;
        self.touch(at);
        self.push_step(RunStepKind::Failed, "run failed", at);
        Ok(())
    }

    pub fn cancel(&mut self, reason: impl Into<String>, at: i64) -> Result<(), RunTransitionError> {
        self.require_not_terminal("cancel")?;
        self.snapshot.status = RunStatus::Cancelled;
        self.snapshot.error = Some(reason.into());
        self.snapshot.finished_at = Some(at);
        self.snapshot.provider_stream = None;
        self.touch(at);
        self.push_step(RunStepKind::Cancelled, "run cancelled", at);
        Ok(())
    }

    pub fn interrupt(
        &mut self,
        reason: impl Into<String>,
        at: i64,
    ) -> Result<(), RunTransitionError> {
        self.require_not_terminal("interrupt")?;
        self.snapshot.status = RunStatus::Interrupted;
        self.snapshot.error = Some(reason.into());
        self.snapshot.finished_at = Some(at);
        self.snapshot.provider_stream = None;
        self.touch(at);
        self.push_step(RunStepKind::Interrupted, "run interrupted", at);
        Ok(())
    }

    fn require_status(
        &self,
        action: &'static str,
        allowed: &[RunStatus],
    ) -> Result<(), RunTransitionError> {
        if allowed.contains(&self.snapshot.status) {
            return Ok(());
        }

        Err(RunTransitionError::InvalidTransition {
            action,
            status: self.snapshot.status,
        })
    }

    fn require_not_terminal(&self, action: &'static str) -> Result<(), RunTransitionError> {
        if !self.snapshot.status.is_terminal() {
            return Ok(());
        }

        Err(RunTransitionError::InvalidTransition {
            action,
            status: self.snapshot.status,
        })
    }

    fn touch(&mut self, at: i64) {
        self.snapshot.updated_at = at;
    }

    fn push_step(&mut self, kind: RunStepKind, detail: impl Into<String>, recorded_at: i64) {
        self.snapshot.recent_steps.push(RunStep {
            kind,
            detail: detail.into(),
            recorded_at,
        });

        if self.snapshot.recent_steps.len() > RECENT_STEP_LIMIT {
            self.snapshot.recent_steps.remove(0);
        }
    }
}

fn remove_by_id<T>(items: &mut Vec<T>, target_id: &str) -> bool
where
    T: RunScopedItem,
{
    let original_len = items.len();
    items.retain(|item| item.id() != target_id);
    original_len != items.len()
}

trait RunScopedItem {
    fn id(&self) -> &str;
}

impl RunScopedItem for ApprovalRequest {
    fn id(&self) -> &str {
        &self.id
    }
}

impl RunScopedItem for ActiveProcess {
    fn id(&self) -> &str {
        &self.id
    }
}

impl RunScopedItem for DelegateRun {
    fn id(&self) -> &str {
        &self.id
    }
}

impl fmt::Display for RunTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition { action, status } => {
                write!(
                    formatter,
                    "cannot {action} while run is in {}",
                    status.as_str()
                )
            }
            Self::MissingApproval { id } => write!(formatter, "missing approval {id}"),
            Self::MissingDelegate { id } => write!(formatter, "missing delegate {id}"),
            Self::MissingProcess { id } => write!(formatter, "missing process {id}"),
            Self::ProviderStreamInactive => {
                write!(formatter, "provider stream is not active")
            }
        }
    }
}

impl Error for RunTransitionError {}

impl fmt::Display for RunStatusParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown run status {}", self.value)
    }
}

impl Error for RunStatusParseError {}

#[cfg(test)]
mod tests {
    use super::{ActiveProcess, ApprovalRequest, DelegateRun, RunEngine, RunStatus, RunStepKind};
    use crate::verification::{CheckOutcome, EvidenceBundle};

    #[test]
    fn happy_path_transitions_from_queued_to_completed() {
        let mut engine = RunEngine::new("run-1", "session-1", Some("mission-1"), 1);

        engine.start(2).expect("start");
        engine
            .begin_provider_stream("resp-1", "gpt-5.4", 3)
            .expect("begin stream");
        engine
            .push_provider_text("hello world", 4)
            .expect("push provider text");
        engine.finish_provider_stream(5).expect("finish stream");
        engine.complete("done", 6).expect("complete");

        let snapshot = engine.snapshot();

        assert_eq!(snapshot.status, RunStatus::Completed);
        assert_eq!(snapshot.result.as_deref(), Some("done"));
        assert_eq!(snapshot.finished_at, Some(6));
        assert!(snapshot.provider_stream.is_none());
        assert_eq!(
            snapshot.recent_steps.last().expect("completion step").kind,
            RunStepKind::Completed
        );
    }

    #[test]
    fn approvals_move_the_engine_through_resuming() {
        let mut engine = RunEngine::new("run-1", "session-1", None, 1);

        engine.start(2).expect("start");
        engine
            .wait_for_approval(
                ApprovalRequest::new("approval-1", "tool-call-1", "write access", 3),
                3,
            )
            .expect("pause for approval");
        assert_eq!(engine.snapshot().status, RunStatus::WaitingApproval);
        assert_eq!(engine.snapshot().pending_approvals.len(), 1);

        engine
            .resolve_approval("approval-1", 4)
            .expect("resolve approval");
        assert_eq!(engine.snapshot().status, RunStatus::Resuming);
        assert!(engine.snapshot().pending_approvals.is_empty());

        engine.resume(5).expect("resume");
        assert_eq!(engine.snapshot().status, RunStatus::Running);
    }

    #[test]
    fn process_and_delegate_waits_are_recovery_friendly() {
        let mut engine = RunEngine::new("run-1", "session-1", None, 1);

        engine.start(2).expect("start");
        engine
            .wait_for_process(ActiveProcess::new("proc-1", "exec", "pid:42", 3), 3)
            .expect("wait for process");
        assert_eq!(engine.snapshot().status, RunStatus::WaitingProcess);
        assert_eq!(engine.snapshot().active_processes.len(), 1);

        engine
            .complete_process("proc-1", Some(0), 4)
            .expect("complete process");
        assert_eq!(engine.snapshot().status, RunStatus::Resuming);
        assert!(engine.snapshot().active_processes.is_empty());

        engine.resume(5).expect("resume after process");
        engine
            .wait_for_delegate(DelegateRun::new("delegate-1", "worker-a", 6), 6)
            .expect("wait for delegate");
        assert_eq!(engine.snapshot().status, RunStatus::WaitingDelegate);
        assert_eq!(engine.snapshot().delegate_runs.len(), 1);

        engine
            .complete_delegate("delegate-1", 7)
            .expect("complete delegate");
        assert_eq!(engine.snapshot().status, RunStatus::Resuming);
        assert!(engine.snapshot().delegate_runs.is_empty());
    }

    #[test]
    fn terminal_states_reject_further_transitions() {
        let mut engine = RunEngine::new("run-1", "session-1", None, 1);

        engine.start(2).expect("start");
        engine.cancel("operator stop", 3).expect("cancel");

        assert_eq!(engine.snapshot().status, RunStatus::Cancelled);
        assert!(engine.resume(4).is_err());
        assert!(engine.complete("done", 5).is_err());
    }

    #[test]
    fn evidence_bundles_attach_refs_without_duplicates() {
        let mut engine = RunEngine::new("run-1", "session-1", Some("mission-1"), 1);
        let mut bundle = EvidenceBundle::new("bundle-1", "run-1", 2);

        bundle
            .record_check("fmt", CheckOutcome::Passed, Some("rustfmt clean"), 3)
            .expect("record fmt");
        bundle.add_artifact_ref("artifact:verification-report");

        engine.start(2).expect("start");
        engine.record_evidence(&bundle, 4).expect("record evidence");
        engine
            .record_evidence(&bundle, 5)
            .expect("record evidence again");

        let snapshot = engine.snapshot();
        assert_eq!(
            snapshot.evidence_refs,
            vec![
                "bundle:bundle-1".to_string(),
                "check:fmt".to_string(),
                "artifact:verification-report".to_string(),
            ]
        );
        assert_eq!(
            snapshot.recent_steps.last().expect("evidence step").kind,
            RunStepKind::EvidenceRecorded
        );
    }
}
