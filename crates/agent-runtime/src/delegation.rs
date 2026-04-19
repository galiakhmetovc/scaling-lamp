use crate::run::DelegateRun;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::{Component, Path};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DelegateRuntime {
    delegates: BTreeMap<String, DelegateHandle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegateHandle {
    pub id: String,
    pub parent_run_id: String,
    pub child_run_id: String,
    pub label: String,
    pub goal: String,
    pub bounded_context: Vec<String>,
    pub write_scope: DelegateWriteScope,
    pub expected_output: String,
    pub owner: String,
    pub status: DelegateStatus,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub result: Option<DelegateResultPackage>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegateRequest {
    pub id: String,
    pub parent_run_id: String,
    pub child_run_id: String,
    pub label: String,
    pub goal: String,
    pub bounded_context: Vec<String>,
    pub write_scope: DelegateWriteScope,
    pub expected_output: String,
    pub owner: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegateWriteScope {
    pub allowed_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegateResultPackage {
    pub summary: String,
    pub changed_paths: Vec<String>,
    pub artifact_refs: Vec<String>,
    pub residual_risks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegateSupervisorStatus {
    pub id: String,
    pub child_run_id: String,
    pub owner: String,
    pub status: DelegateStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelegateStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegateError {
    DuplicateDelegateId { id: String },
    EmptyBoundedContext,
    EmptyExpectedOutput,
    EmptyGoal,
    EmptyOwner,
    EmptySummary,
    EmptyWriteScope,
    InvalidPath { path: String },
    PathOutsideWriteScope { delegate_id: String, path: String },
    UnknownDelegate { id: String },
}

impl DelegateRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        parent_run_id: impl Into<String>,
        child_run_id: impl Into<String>,
        label: impl Into<String>,
        goal: impl Into<String>,
        bounded_context: Vec<String>,
        write_scope: DelegateWriteScope,
        expected_output: impl Into<String>,
        owner: impl Into<String>,
    ) -> Result<Self, DelegateError> {
        let goal = goal.into().trim().to_string();
        let expected_output = expected_output.into().trim().to_string();
        let owner = owner.into().trim().to_string();
        let label = label.into().trim().to_string();

        if goal.is_empty() {
            return Err(DelegateError::EmptyGoal);
        }
        if expected_output.is_empty() {
            return Err(DelegateError::EmptyExpectedOutput);
        }
        if owner.is_empty() {
            return Err(DelegateError::EmptyOwner);
        }
        if bounded_context.is_empty() {
            return Err(DelegateError::EmptyBoundedContext);
        }

        Ok(Self {
            id: id.into(),
            parent_run_id: parent_run_id.into(),
            child_run_id: child_run_id.into(),
            label,
            goal,
            bounded_context,
            write_scope,
            expected_output,
            owner,
        })
    }
}

impl DelegateWriteScope {
    pub fn new(allowed_paths: Vec<String>) -> Result<Self, DelegateError> {
        if allowed_paths.is_empty() {
            return Err(DelegateError::EmptyWriteScope);
        }

        for path in &allowed_paths {
            validate_relative_path(path)?;
        }

        Ok(Self { allowed_paths })
    }

    pub fn allows(&self, candidate: &str) -> Result<bool, DelegateError> {
        validate_relative_path(candidate)?;
        let candidate = Path::new(candidate);

        Ok(self
            .allowed_paths
            .iter()
            .map(Path::new)
            .any(|allowed| candidate == allowed || candidate.starts_with(allowed)))
    }
}

impl DelegateResultPackage {
    pub fn new(
        summary: impl Into<String>,
        changed_paths: Vec<String>,
        artifact_refs: Vec<String>,
        residual_risks: Vec<String>,
    ) -> Result<Self, DelegateError> {
        let summary = summary.into().trim().to_string();
        if summary.is_empty() {
            return Err(DelegateError::EmptySummary);
        }

        for path in &changed_paths {
            validate_relative_path(path)?;
        }

        Ok(Self {
            summary,
            changed_paths,
            artifact_refs,
            residual_risks,
        })
    }
}

impl DelegateHandle {
    fn new(request: DelegateRequest, started_at: i64) -> Self {
        Self {
            id: request.id,
            parent_run_id: request.parent_run_id,
            child_run_id: request.child_run_id,
            label: request.label,
            goal: request.goal,
            bounded_context: request.bounded_context,
            write_scope: request.write_scope,
            expected_output: request.expected_output,
            owner: request.owner,
            status: DelegateStatus::Running,
            started_at,
            finished_at: None,
            result: None,
            error: None,
        }
    }

    pub fn as_run_ref(&self) -> DelegateRun {
        DelegateRun::new(self.id.clone(), self.label.clone(), self.started_at)
    }
}

impl DelegateRuntime {
    pub fn start(
        &mut self,
        request: DelegateRequest,
        started_at: i64,
    ) -> Result<DelegateHandle, DelegateError> {
        if self.delegates.contains_key(&request.id) {
            return Err(DelegateError::DuplicateDelegateId { id: request.id });
        }

        let handle = DelegateHandle::new(request, started_at);
        self.delegates.insert(handle.id.clone(), handle.clone());
        Ok(handle)
    }

    pub fn complete(
        &mut self,
        id: &str,
        result: DelegateResultPackage,
        finished_at: i64,
    ) -> Result<(), DelegateError> {
        let handle = self
            .delegates
            .get_mut(id)
            .ok_or_else(|| DelegateError::UnknownDelegate { id: id.to_string() })?;

        for path in &result.changed_paths {
            if !handle.write_scope.allows(path)? {
                return Err(DelegateError::PathOutsideWriteScope {
                    delegate_id: handle.id.clone(),
                    path: path.clone(),
                });
            }
        }

        handle.status = DelegateStatus::Completed;
        handle.finished_at = Some(finished_at);
        handle.result = Some(result);
        handle.error = None;
        Ok(())
    }

    pub fn fail(
        &mut self,
        id: &str,
        error: impl Into<String>,
        finished_at: i64,
    ) -> Result<(), DelegateError> {
        let handle = self
            .delegates
            .get_mut(id)
            .ok_or_else(|| DelegateError::UnknownDelegate { id: id.to_string() })?;
        handle.status = DelegateStatus::Failed;
        handle.finished_at = Some(finished_at);
        handle.error = Some(error.into());
        Ok(())
    }

    pub fn cancel(
        &mut self,
        id: &str,
        reason: impl Into<String>,
        finished_at: i64,
    ) -> Result<(), DelegateError> {
        let handle = self
            .delegates
            .get_mut(id)
            .ok_or_else(|| DelegateError::UnknownDelegate { id: id.to_string() })?;
        handle.status = DelegateStatus::Cancelled;
        handle.finished_at = Some(finished_at);
        handle.error = Some(reason.into());
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&DelegateHandle> {
        self.delegates.get(id)
    }

    pub fn active_handles(&self) -> Vec<&DelegateHandle> {
        self.delegates
            .values()
            .filter(|handle| handle.status == DelegateStatus::Running)
            .collect()
    }

    pub fn supervisor_view(&self) -> Vec<DelegateSupervisorStatus> {
        self.delegates
            .values()
            .map(|handle| DelegateSupervisorStatus {
                id: handle.id.clone(),
                child_run_id: handle.child_run_id.clone(),
                owner: handle.owner.clone(),
                status: handle.status,
            })
            .collect()
    }
}

fn validate_relative_path(path: &str) -> Result<(), DelegateError> {
    let path = path.trim();
    if path.is_empty() {
        return Err(DelegateError::InvalidPath {
            path: path.to_string(),
        });
    }

    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Err(DelegateError::InvalidPath {
            path: path.to_string(),
        });
    }

    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::CurDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(DelegateError::InvalidPath {
            path: path.to_string(),
        });
    }

    Ok(())
}

impl fmt::Display for DelegateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDelegateId { id } => write!(formatter, "delegate {id} already exists"),
            Self::EmptyBoundedContext => {
                write!(formatter, "delegate bounded context cannot be empty")
            }
            Self::EmptyExpectedOutput => {
                write!(formatter, "delegate expected output cannot be empty")
            }
            Self::EmptyGoal => write!(formatter, "delegate goal cannot be empty"),
            Self::EmptyOwner => write!(formatter, "delegate owner cannot be empty"),
            Self::EmptySummary => write!(formatter, "delegate result summary cannot be empty"),
            Self::EmptyWriteScope => write!(formatter, "delegate write scope cannot be empty"),
            Self::InvalidPath { path } => write!(formatter, "delegate path {path} is invalid"),
            Self::PathOutsideWriteScope { delegate_id, path } => {
                write!(
                    formatter,
                    "delegate {delegate_id} changed path outside write scope: {path}"
                )
            }
            Self::UnknownDelegate { id } => write!(formatter, "unknown delegate {id}"),
        }
    }
}

impl Error for DelegateError {}

#[cfg(test)]
mod tests {
    use super::{
        DelegateRequest, DelegateResultPackage, DelegateRuntime, DelegateStatus,
        DelegateSupervisorStatus, DelegateWriteScope,
    };

    #[test]
    fn delegate_request_requires_goal_owner_and_write_scope() {
        assert!(
            DelegateRequest::new(
                "delegate-1",
                "run-1",
                "child-run-1",
                "worker-a",
                "   ",
                vec!["scheduler context".to_string()],
                DelegateWriteScope::new(vec!["crates/agent-runtime/src".to_string()])
                    .expect("write scope"),
                "return a result package",
                "worker-a",
            )
            .is_err()
        );
    }

    #[test]
    fn start_delegate_creates_running_handle_with_bounded_context() {
        let mut runtime = DelegateRuntime::default();
        let request = DelegateRequest::new(
            "delegate-1",
            "run-1",
            "child-run-1",
            "worker-a",
            "inspect the scheduler",
            vec![
                "scheduler context".to_string(),
                "verification state".to_string(),
            ],
            DelegateWriteScope::new(vec!["crates/agent-runtime/src/scheduler.rs".to_string()])
                .expect("write scope"),
            "return changed paths and summary",
            "worker-a",
        )
        .expect("request");

        let handle = runtime.start(request, 10).expect("start delegate");

        assert_eq!(handle.status, DelegateStatus::Running);
        assert_eq!(handle.owner, "worker-a");
        assert_eq!(handle.goal, "inspect the scheduler");
        assert_eq!(handle.started_at, 10);
        assert_eq!(handle.as_run_ref().id, "delegate-1");
        assert_eq!(runtime.active_handles().len(), 1);
        assert_eq!(
            runtime.supervisor_view(),
            vec![DelegateSupervisorStatus {
                id: "delegate-1".to_string(),
                child_run_id: "child-run-1".to_string(),
                owner: "worker-a".to_string(),
                status: DelegateStatus::Running,
            }]
        );
    }

    #[test]
    fn complete_delegate_records_result_package_within_write_scope() {
        let mut runtime = DelegateRuntime::default();
        let request = DelegateRequest::new(
            "delegate-1",
            "run-1",
            "child-run-1",
            "worker-a",
            "inspect the scheduler",
            vec!["scheduler context".to_string()],
            DelegateWriteScope::new(vec!["crates/agent-runtime/src".to_string()])
                .expect("write scope"),
            "return changed paths and summary",
            "worker-a",
        )
        .expect("request");
        runtime.start(request, 10).expect("start delegate");

        let package = DelegateResultPackage::new(
            "delegation complete",
            vec!["crates/agent-runtime/src/scheduler.rs".to_string()],
            vec!["artifact:delegate-report".to_string()],
            vec!["manual review recommended".to_string()],
        )
        .expect("result package");

        runtime
            .complete("delegate-1", package.clone(), 20)
            .expect("complete delegate");

        let handle = runtime.get("delegate-1").expect("handle");
        assert_eq!(handle.status, DelegateStatus::Completed);
        assert_eq!(handle.finished_at, Some(20));
        assert_eq!(handle.result.as_ref(), Some(&package));
        assert!(runtime.active_handles().is_empty());
    }

    #[test]
    fn complete_delegate_rejects_changed_paths_outside_write_scope() {
        let mut runtime = DelegateRuntime::default();
        let request = DelegateRequest::new(
            "delegate-1",
            "run-1",
            "child-run-1",
            "worker-a",
            "inspect the scheduler",
            vec!["scheduler context".to_string()],
            DelegateWriteScope::new(vec!["crates/agent-runtime/src".to_string()])
                .expect("write scope"),
            "return changed paths and summary",
            "worker-a",
        )
        .expect("request");
        runtime.start(request, 10).expect("start delegate");

        let package = DelegateResultPackage::new(
            "delegation complete",
            vec!["README.md".to_string()],
            vec![],
            vec![],
        )
        .expect("result package");

        assert!(runtime.complete("delegate-1", package, 20).is_err());
    }
}
