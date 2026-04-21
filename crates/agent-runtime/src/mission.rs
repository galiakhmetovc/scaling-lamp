use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionSpec {
    pub id: String,
    pub session_id: String,
    pub objective: String,
    pub status: MissionStatus,
    pub execution_intent: MissionExecutionIntent,
    pub schedule: MissionSchedule,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissionStatus {
    Draft,
    Ready,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissionExecutionIntent {
    Assisted,
    Autonomous,
    Scheduled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionSchedule {
    pub not_before: Option<i64>,
    pub interval_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceCriterion {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobKind {
    ChatTurn,
    ApprovalContinuation,
    MissionTurn,
    Verification,
    Delegate,
    Maintenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobExecutionInput {
    ChatTurn { message: String },
    ApprovalContinuation { run_id: String, approval_id: String },
    MissionTurn { mission_id: String, goal: String },
    Verification { checks: Vec<String> },
    Delegate { label: String, goal: String },
    Maintenance { summary: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobResult {
    Summary { outcome: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSpec {
    pub id: String,
    pub session_id: String,
    pub mission_id: Option<String>,
    pub run_id: Option<String>,
    pub parent_job_id: Option<String>,
    pub kind: JobKind,
    pub status: JobStatus,
    pub input: JobExecutionInput,
    pub result: Option<JobResult>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub attempt_count: u32,
    pub max_attempts: u32,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<i64>,
    pub heartbeat_at: Option<i64>,
    pub cancel_requested_at: Option<i64>,
    pub last_progress_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionError {
    EmptyObjective,
    EmptyAcceptanceCriterion,
    ZeroIntervalSeconds,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobSpecValidationError {
    KindInputMismatch {
        expected: JobKind,
        actual: JobKind,
    },
    MissingMissionId,
    MissionIdMismatch {
        job_mission_id: String,
        input_mission_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionStatusParseError {
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionExecutionIntentParseError {
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobKindParseError {
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobStatusParseError {
    value: String,
}

impl Default for MissionSpec {
    fn default() -> Self {
        Self {
            id: "mission-bootstrap".to_string(),
            session_id: "session-bootstrap".to_string(),
            objective: "bootstrap autonomous runtime".to_string(),
            status: MissionStatus::Ready,
            execution_intent: MissionExecutionIntent::Autonomous,
            schedule: MissionSchedule::once(),
            acceptance_criteria: Vec::new(),
            created_at: 0,
            updated_at: 0,
            completed_at: None,
        }
    }
}

impl MissionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl TryFrom<&str> for MissionStatus {
    type Error = MissionStatusParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "draft" => Ok(Self::Draft),
            "ready" => Ok(Self::Ready),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(MissionStatusParseError {
                value: value.to_string(),
            }),
        }
    }
}

impl MissionExecutionIntent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assisted => "assisted",
            Self::Autonomous => "autonomous",
            Self::Scheduled => "scheduled",
        }
    }
}

impl TryFrom<&str> for MissionExecutionIntent {
    type Error = MissionExecutionIntentParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "assisted" => Ok(Self::Assisted),
            "autonomous" => Ok(Self::Autonomous),
            "scheduled" => Ok(Self::Scheduled),
            _ => Err(MissionExecutionIntentParseError {
                value: value.to_string(),
            }),
        }
    }
}

impl MissionSchedule {
    pub fn once() -> Self {
        Self {
            not_before: None,
            interval_seconds: None,
        }
    }

    pub fn interval_seconds(seconds: u64) -> Result<Self, MissionError> {
        if seconds == 0 {
            return Err(MissionError::ZeroIntervalSeconds);
        }

        Ok(Self {
            not_before: None,
            interval_seconds: Some(seconds),
        })
    }
}

impl AcceptanceCriterion {
    pub fn new(
        id: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<Self, MissionError> {
        let description = description.into().trim().to_string();

        if description.is_empty() {
            return Err(MissionError::EmptyAcceptanceCriterion);
        }

        Ok(Self {
            id: id.into(),
            description,
        })
    }
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ChatTurn => "chat_turn",
            Self::ApprovalContinuation => "approval_continuation",
            Self::MissionTurn => "mission_turn",
            Self::Verification => "verification",
            Self::Delegate => "delegate",
            Self::Maintenance => "maintenance",
        }
    }
}

impl TryFrom<&str> for JobKind {
    type Error = JobKindParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "chat_turn" => Ok(Self::ChatTurn),
            "approval_continuation" => Ok(Self::ApprovalContinuation),
            "mission_turn" => Ok(Self::MissionTurn),
            "verification" => Ok(Self::Verification),
            "delegate" => Ok(Self::Delegate),
            "maintenance" => Ok(Self::Maintenance),
            _ => Err(JobKindParseError {
                value: value.to_string(),
            }),
        }
    }
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Blocked => "blocked",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Blocked)
    }
}

impl TryFrom<&str> for JobStatus {
    type Error = JobStatusParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "blocked" => Ok(Self::Blocked),
            _ => Err(JobStatusParseError {
                value: value.to_string(),
            }),
        }
    }
}

impl JobSpec {
    pub fn mission_turn(
        id: impl Into<String>,
        session_id: impl Into<String>,
        mission_id: impl Into<String>,
        run_id: Option<&str>,
        parent_job_id: Option<&str>,
        goal: impl Into<String>,
        created_at: i64,
    ) -> Self {
        let session_id = session_id.into();
        let mission_id = mission_id.into();
        let goal = goal.into();

        Self {
            id: id.into(),
            session_id,
            mission_id: Some(mission_id.clone()),
            run_id: run_id.map(str::to_owned),
            parent_job_id: parent_job_id.map(str::to_owned),
            kind: JobKind::MissionTurn,
            status: JobStatus::Queued,
            input: JobExecutionInput::MissionTurn { mission_id, goal },
            result: None,
            error: None,
            created_at,
            updated_at: created_at,
            started_at: None,
            finished_at: None,
            attempt_count: 0,
            max_attempts: 1,
            lease_owner: None,
            lease_expires_at: None,
            heartbeat_at: None,
            cancel_requested_at: None,
            last_progress_message: None,
        }
    }

    pub fn chat_turn(
        id: impl Into<String>,
        session_id: impl Into<String>,
        run_id: Option<&str>,
        parent_job_id: Option<&str>,
        message: impl Into<String>,
        created_at: i64,
    ) -> Self {
        Self {
            id: id.into(),
            session_id: session_id.into(),
            mission_id: None,
            run_id: run_id.map(str::to_owned),
            parent_job_id: parent_job_id.map(str::to_owned),
            kind: JobKind::ChatTurn,
            status: JobStatus::Queued,
            input: JobExecutionInput::ChatTurn {
                message: message.into(),
            },
            result: None,
            error: None,
            created_at,
            updated_at: created_at,
            started_at: None,
            finished_at: None,
            attempt_count: 0,
            max_attempts: 1,
            lease_owner: None,
            lease_expires_at: None,
            heartbeat_at: None,
            cancel_requested_at: None,
            last_progress_message: None,
        }
    }

    pub fn validate(&self) -> Result<(), JobSpecValidationError> {
        let input_kind = self.input.kind();
        if self.kind != input_kind {
            return Err(JobSpecValidationError::KindInputMismatch {
                expected: self.kind,
                actual: input_kind,
            });
        }

        if let JobExecutionInput::MissionTurn { mission_id, .. } = &self.input {
            let Some(job_mission_id) = self.mission_id.as_ref() else {
                return Err(JobSpecValidationError::MissingMissionId);
            };

            if mission_id != job_mission_id {
                return Err(JobSpecValidationError::MissionIdMismatch {
                    job_mission_id: job_mission_id.clone(),
                    input_mission_id: mission_id.clone(),
                });
            }
        }

        Ok(())
    }
}

impl JobExecutionInput {
    pub fn kind(&self) -> JobKind {
        match self {
            Self::ChatTurn { .. } => JobKind::ChatTurn,
            Self::ApprovalContinuation { .. } => JobKind::ApprovalContinuation,
            Self::MissionTurn { .. } => JobKind::MissionTurn,
            Self::Verification { .. } => JobKind::Verification,
            Self::Delegate { .. } => JobKind::Delegate,
            Self::Maintenance { .. } => JobKind::Maintenance,
        }
    }
}

impl fmt::Display for MissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyObjective => write!(formatter, "mission objective cannot be blank"),
            Self::EmptyAcceptanceCriterion => {
                write!(formatter, "acceptance criterion cannot be blank")
            }
            Self::ZeroIntervalSeconds => {
                write!(
                    formatter,
                    "mission interval seconds must be greater than zero"
                )
            }
        }
    }
}

impl Error for MissionError {}

impl fmt::Display for JobSpecValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KindInputMismatch { expected, actual } => write!(
                formatter,
                "job kind {} does not match input kind {}",
                expected.as_str(),
                actual.as_str()
            ),
            Self::MissingMissionId => {
                write!(formatter, "mission turn jobs require a mission_id")
            }
            Self::MissionIdMismatch {
                job_mission_id,
                input_mission_id,
            } => write!(
                formatter,
                "job mission id {job_mission_id} does not match mission turn input {input_mission_id}"
            ),
        }
    }
}

impl Error for JobSpecValidationError {}

macro_rules! impl_parse_error_display {
    ($name:ident, $label:literal) => {
        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "unknown {} {}", $label, self.value)
            }
        }

        impl Error for $name {}
    };
}

impl_parse_error_display!(MissionStatusParseError, "mission status");
impl_parse_error_display!(MissionExecutionIntentParseError, "mission execution intent");
impl_parse_error_display!(JobKindParseError, "job kind");
impl_parse_error_display!(JobStatusParseError, "job status");

#[cfg(test)]
mod tests {
    use super::{
        AcceptanceCriterion, JobExecutionInput, JobKind, JobSpec, JobSpecValidationError,
        JobStatus, MissionExecutionIntent, MissionSchedule, MissionSpec, MissionStatus,
    };

    #[test]
    fn mission_defaults_to_an_autonomous_one_shot_spec() {
        let mission = MissionSpec::default();

        assert_eq!(mission.id, "mission-bootstrap");
        assert_eq!(mission.session_id, "session-bootstrap");
        assert_eq!(mission.status, MissionStatus::Ready);
        assert_eq!(mission.execution_intent, MissionExecutionIntent::Autonomous);
        assert!(mission.schedule.interval_seconds.is_none());
        assert!(mission.acceptance_criteria.is_empty());
    }

    #[test]
    fn acceptance_criterion_rejects_blank_descriptions() {
        assert!(AcceptanceCriterion::new("criterion-1", "   ").is_err());
    }

    #[test]
    fn mission_schedule_rejects_zero_interval_seconds() {
        assert!(MissionSchedule::interval_seconds(0).is_err());
    }

    #[test]
    fn job_spec_tracks_mission_turn_inputs_and_parent_relationships() {
        let job = JobSpec::mission_turn(
            "job-1",
            "session-1",
            "mission-1",
            Some("run-1"),
            Some("job-root"),
            "finish the runtime",
            10,
        );

        assert_eq!(job.kind, JobKind::MissionTurn);
        assert_eq!(job.status, JobStatus::Queued);
        assert_eq!(job.session_id, "session-1");
        assert_eq!(job.mission_id.as_deref(), Some("mission-1"));
        assert_eq!(job.run_id.as_deref(), Some("run-1"));
        assert_eq!(job.parent_job_id.as_deref(), Some("job-root"));
        assert_eq!(
            job.input,
            JobExecutionInput::MissionTurn {
                mission_id: "mission-1".to_string(),
                goal: "finish the runtime".to_string(),
            }
        );
    }

    #[test]
    fn job_spec_rejects_mismatched_kind_and_input_payloads() {
        let mut job = JobSpec::mission_turn(
            "job-1",
            "session-1",
            "mission-1",
            None,
            None,
            "finish runtime",
            10,
        );
        job.kind = JobKind::Delegate;

        assert_eq!(
            job.validate(),
            Err(JobSpecValidationError::KindInputMismatch {
                expected: JobKind::Delegate,
                actual: JobKind::MissionTurn,
            })
        );
    }

    #[test]
    fn job_spec_rejects_mismatched_mission_turn_identifiers() {
        let mut job = JobSpec::mission_turn(
            "job-1",
            "session-1",
            "mission-1",
            None,
            None,
            "finish runtime",
            10,
        );
        job.input = JobExecutionInput::MissionTurn {
            mission_id: "mission-2".to_string(),
            goal: "finish runtime".to_string(),
        };

        assert_eq!(
            job.validate(),
            Err(JobSpecValidationError::MissionIdMismatch {
                job_mission_id: "mission-1".to_string(),
                input_mission_id: "mission-2".to_string(),
            })
        );
    }

    #[test]
    fn session_scoped_background_jobs_validate_without_a_mission() {
        let job = JobSpec::chat_turn("job-bg", "session-1", None, None, "hello", 20);

        assert_eq!(job.kind, JobKind::ChatTurn);
        assert!(job.mission_id.is_none());
        assert!(job.validate().is_ok());
    }
}
