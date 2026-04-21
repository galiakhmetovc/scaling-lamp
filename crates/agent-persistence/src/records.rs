use agent_runtime::context::{ContextOffloadSnapshot, ContextSummary};
use agent_runtime::inbox::{
    SessionInboxEvent, SessionInboxEventParseError, SessionInboxEventPayload,
    SessionInboxEventStatus,
};
use agent_runtime::mission::{
    JobKind, JobKindParseError, JobResult, JobSpec, JobSpecValidationError, JobStatus,
    JobStatusParseError, MissionExecutionIntent, MissionExecutionIntentParseError, MissionSchedule,
    MissionSpec, MissionStatus, MissionStatusParseError,
};
use agent_runtime::plan::PlanSnapshot;
use agent_runtime::run::{RunSnapshot, RunStatus, RunStatusParseError};
use agent_runtime::session::{
    MessageRole, PromptOverride, Session, SessionError, SessionSettings, TranscriptEntry,
};
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PlanRecordPayload {
    goal: Option<String>,
    items: Vec<agent_runtime::plan::PlanItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub title: String,
    pub prompt_override: Option<String>,
    pub settings_json: String,
    pub active_mission_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_job_id: Option<String>,
    pub delegation_label: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionRecord {
    pub id: String,
    pub session_id: String,
    pub objective: String,
    pub status: String,
    pub execution_intent: String,
    pub schedule_json: String,
    pub acceptance_json: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub id: String,
    pub session_id: String,
    pub mission_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub result: Option<String>,
    pub provider_usage_json: String,
    pub recent_steps_json: String,
    pub evidence_refs_json: String,
    pub pending_approvals_json: String,
    pub provider_loop_json: String,
    pub delegate_runs_json: String,
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRecord {
    pub id: String,
    pub session_id: String,
    pub mission_id: Option<String>,
    pub run_id: Option<String>,
    pub parent_job_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub input_json: Option<String>,
    pub result_json: Option<String>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub attempt_count: i64,
    pub max_attempts: i64,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<i64>,
    pub heartbeat_at: Option<i64>,
    pub cancel_requested_at: Option<i64>,
    pub last_progress_message: Option<String>,
    pub callback_json: Option<String>,
    pub callback_sent_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptRecord {
    pub id: String,
    pub session_id: String,
    pub run_id: Option<String>,
    pub kind: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInboxEventRecord {
    pub id: String,
    pub session_id: String,
    pub job_id: Option<String>,
    pub kind: String,
    pub payload_json: String,
    pub status: String,
    pub created_at: i64,
    pub available_at: i64,
    pub claimed_at: Option<i64>,
    pub processed_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSummaryRecord {
    pub session_id: String,
    pub summary_text: String,
    pub covered_message_count: i64,
    pub summary_token_estimate: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextOffloadRecord {
    pub session_id: String,
    pub refs_json: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRecord {
    pub session_id: String,
    pub items_json: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub id: String,
    pub session_id: String,
    pub kind: String,
    pub metadata_json: String,
    pub path: std::path::PathBuf,
    pub bytes: Vec<u8>,
    pub created_at: i64,
}

#[derive(Debug)]
pub enum RecordConversionError {
    InvalidJobInput(serde_json::Error),
    InvalidJobKind(JobKindParseError),
    InvalidJobResult(serde_json::Error),
    InvalidJobSpec(JobSpecValidationError),
    InvalidJobStatus(JobStatusParseError),
    InvalidContextOffloadRefs(serde_json::Error),
    InvalidInboxEventPayload(serde_json::Error),
    InvalidInboxEventStatus(SessionInboxEventParseError),
    InvalidContextSummaryCoveredMessageCount { value: i64 },
    InvalidContextSummaryTokenEstimate { value: i64 },
    InvalidMessageRole { value: String },
    InvalidMissionAcceptance(serde_json::Error),
    InvalidMissionExecutionIntent(MissionExecutionIntentParseError),
    InvalidMissionSchedule(serde_json::Error),
    InvalidMissionStatus(MissionStatusParseError),
    InvalidPlanItems(serde_json::Error),
    MissingJobInput,
    InvalidPromptOverride(SessionError),
    InvalidRunDelegateRuns(serde_json::Error),
    InvalidRunRecentSteps(serde_json::Error),
    InvalidRunPendingApprovals(serde_json::Error),
    InvalidRunProviderLoop(serde_json::Error),
    InvalidRunProviderUsage(serde_json::Error),
    InvalidRunEvidenceRefs(serde_json::Error),
    InvalidRunStatus(RunStatusParseError),
    InvalidSessionSettings(serde_json::Error),
    SerializeContextOffloadRefs(serde_json::Error),
    SerializeInboxEventPayload(serde_json::Error),
    SerializeJobInput(serde_json::Error),
    SerializeJobResult(serde_json::Error),
    SerializeMissionAcceptance(serde_json::Error),
    SerializeMissionSchedule(serde_json::Error),
    SerializePlanItems(serde_json::Error),
    SerializeRunDelegateRuns(serde_json::Error),
    SerializeRunEvidenceRefs(serde_json::Error),
    SerializeRunRecentSteps(serde_json::Error),
    SerializeRunPendingApprovals(serde_json::Error),
    SerializeRunProviderLoop(serde_json::Error),
    SerializeRunProviderUsage(serde_json::Error),
    SerializeSessionSettings(serde_json::Error),
}

impl TryFrom<&Session> for SessionRecord {
    type Error = RecordConversionError;

    fn try_from(session: &Session) -> Result<Self, Self::Error> {
        let settings_json = serde_json::to_string(&session.settings)
            .map_err(RecordConversionError::SerializeSessionSettings)?;

        Ok(Self {
            id: session.id.clone(),
            title: session.title.clone(),
            prompt_override: session
                .prompt_override
                .as_ref()
                .map(|prompt_override| prompt_override.as_str().to_string()),
            settings_json,
            active_mission_id: session.active_mission_id.clone(),
            parent_session_id: session.parent_session_id.clone(),
            parent_job_id: session.parent_job_id.clone(),
            delegation_label: session.delegation_label.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
        })
    }
}

impl TryFrom<SessionRecord> for Session {
    type Error = RecordConversionError;

    fn try_from(record: SessionRecord) -> Result<Self, Self::Error> {
        let settings = serde_json::from_str::<SessionSettings>(&record.settings_json)
            .map_err(RecordConversionError::InvalidSessionSettings)?;
        let prompt_override = record
            .prompt_override
            .map(PromptOverride::new)
            .transpose()
            .map_err(RecordConversionError::InvalidPromptOverride)?;

        Ok(Self {
            id: record.id,
            title: record.title,
            prompt_override,
            settings,
            active_mission_id: record.active_mission_id,
            parent_session_id: record.parent_session_id,
            parent_job_id: record.parent_job_id,
            delegation_label: record.delegation_label,
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
    }
}

impl TryFrom<&MissionSpec> for MissionRecord {
    type Error = RecordConversionError;

    fn try_from(mission: &MissionSpec) -> Result<Self, Self::Error> {
        Ok(Self {
            id: mission.id.clone(),
            session_id: mission.session_id.clone(),
            objective: mission.objective.clone(),
            status: mission.status.as_str().to_string(),
            execution_intent: mission.execution_intent.as_str().to_string(),
            schedule_json: serde_json::to_string(&mission.schedule)
                .map_err(RecordConversionError::SerializeMissionSchedule)?,
            acceptance_json: serde_json::to_string(&mission.acceptance_criteria)
                .map_err(RecordConversionError::SerializeMissionAcceptance)?,
            created_at: mission.created_at,
            updated_at: mission.updated_at,
            completed_at: mission.completed_at,
        })
    }
}

impl TryFrom<MissionRecord> for MissionSpec {
    type Error = RecordConversionError;

    fn try_from(record: MissionRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: record.id,
            session_id: record.session_id,
            objective: record.objective,
            status: MissionStatus::try_from(record.status.as_str())
                .map_err(RecordConversionError::InvalidMissionStatus)?,
            execution_intent: MissionExecutionIntent::try_from(record.execution_intent.as_str())
                .map_err(RecordConversionError::InvalidMissionExecutionIntent)?,
            schedule: serde_json::from_str::<MissionSchedule>(&record.schedule_json)
                .map_err(RecordConversionError::InvalidMissionSchedule)?,
            acceptance_criteria: serde_json::from_str(&record.acceptance_json)
                .map_err(RecordConversionError::InvalidMissionAcceptance)?,
            created_at: record.created_at,
            updated_at: record.updated_at,
            completed_at: record.completed_at,
        })
    }
}

impl From<&TranscriptEntry> for TranscriptRecord {
    fn from(entry: &TranscriptEntry) -> Self {
        Self {
            id: entry.id.clone(),
            session_id: entry.session_id.clone(),
            run_id: entry.run_id.clone(),
            kind: entry.role.as_str().to_string(),
            content: entry.content.clone(),
            created_at: entry.created_at,
        }
    }
}

impl From<&ContextSummary> for ContextSummaryRecord {
    fn from(summary: &ContextSummary) -> Self {
        Self {
            session_id: summary.session_id.clone(),
            summary_text: summary.summary_text.clone(),
            covered_message_count: i64::from(summary.covered_message_count),
            summary_token_estimate: i64::from(summary.summary_token_estimate),
            updated_at: summary.updated_at,
        }
    }
}

impl TryFrom<ContextSummaryRecord> for ContextSummary {
    type Error = RecordConversionError;

    fn try_from(record: ContextSummaryRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            session_id: record.session_id,
            summary_text: record.summary_text,
            covered_message_count: u32::try_from(record.covered_message_count).map_err(|_| {
                RecordConversionError::InvalidContextSummaryCoveredMessageCount {
                    value: record.covered_message_count,
                }
            })?,
            summary_token_estimate: u32::try_from(record.summary_token_estimate).map_err(|_| {
                RecordConversionError::InvalidContextSummaryTokenEstimate {
                    value: record.summary_token_estimate,
                }
            })?,
            updated_at: record.updated_at,
        })
    }
}

impl TryFrom<&ContextOffloadSnapshot> for ContextOffloadRecord {
    type Error = RecordConversionError;

    fn try_from(snapshot: &ContextOffloadSnapshot) -> Result<Self, Self::Error> {
        Ok(Self {
            session_id: snapshot.session_id.clone(),
            refs_json: serde_json::to_string(&snapshot.refs)
                .map_err(RecordConversionError::SerializeContextOffloadRefs)?,
            updated_at: snapshot.updated_at,
        })
    }
}

impl TryFrom<ContextOffloadRecord> for ContextOffloadSnapshot {
    type Error = RecordConversionError;

    fn try_from(record: ContextOffloadRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            session_id: record.session_id,
            refs: serde_json::from_str(&record.refs_json)
                .map_err(RecordConversionError::InvalidContextOffloadRefs)?,
            updated_at: record.updated_at,
        })
    }
}

impl TryFrom<&PlanSnapshot> for PlanRecord {
    type Error = RecordConversionError;

    fn try_from(snapshot: &PlanSnapshot) -> Result<Self, Self::Error> {
        Ok(Self {
            session_id: snapshot.session_id.clone(),
            items_json: serde_json::to_string(&PlanRecordPayload {
                goal: snapshot.goal.clone(),
                items: snapshot.items.clone(),
            })
            .map_err(RecordConversionError::SerializePlanItems)?,
            updated_at: snapshot.updated_at,
        })
    }
}

impl TryFrom<PlanRecord> for PlanSnapshot {
    type Error = RecordConversionError;

    fn try_from(record: PlanRecord) -> Result<Self, Self::Error> {
        let PlanRecord {
            session_id,
            items_json,
            updated_at,
        } = record;
        let payload = serde_json::from_str::<PlanRecordPayload>(&items_json)
            .map_or_else(
                |_| {
                    serde_json::from_str::<Vec<agent_runtime::plan::PlanItem>>(&items_json)
                        .map(|items| PlanRecordPayload { goal: None, items })
                },
                Ok,
            )
            .map_err(RecordConversionError::InvalidPlanItems)?;

        Ok(Self {
            session_id,
            goal: payload.goal,
            items: payload.items,
            updated_at,
        })
    }
}

impl TryFrom<&RunSnapshot> for RunRecord {
    type Error = RecordConversionError;

    fn try_from(snapshot: &RunSnapshot) -> Result<Self, Self::Error> {
        Ok(Self {
            id: snapshot.id.clone(),
            session_id: snapshot.session_id.clone(),
            mission_id: snapshot.mission_id.clone(),
            status: snapshot.status.as_str().to_string(),
            error: snapshot.error.clone(),
            result: snapshot.result.clone(),
            provider_usage_json: serde_json::to_string(&snapshot.latest_provider_usage)
                .map_err(RecordConversionError::SerializeRunProviderUsage)?,
            recent_steps_json: serde_json::to_string(&snapshot.recent_steps)
                .map_err(RecordConversionError::SerializeRunRecentSteps)?,
            evidence_refs_json: serde_json::to_string(&snapshot.evidence_refs)
                .map_err(RecordConversionError::SerializeRunEvidenceRefs)?,
            pending_approvals_json: serde_json::to_string(&snapshot.pending_approvals)
                .map_err(RecordConversionError::SerializeRunPendingApprovals)?,
            provider_loop_json: serde_json::to_string(&snapshot.provider_loop)
                .map_err(RecordConversionError::SerializeRunProviderLoop)?,
            delegate_runs_json: serde_json::to_string(&snapshot.delegate_runs)
                .map_err(RecordConversionError::SerializeRunDelegateRuns)?,
            started_at: snapshot.started_at,
            updated_at: snapshot.updated_at,
            finished_at: snapshot.finished_at,
        })
    }
}

impl TryFrom<RunRecord> for RunSnapshot {
    type Error = RecordConversionError;

    fn try_from(record: RunRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: record.id,
            session_id: record.session_id,
            mission_id: record.mission_id,
            status: RunStatus::try_from(record.status.as_str())
                .map_err(RecordConversionError::InvalidRunStatus)?,
            started_at: record.started_at,
            updated_at: record.updated_at,
            finished_at: record.finished_at,
            error: record.error,
            result: record.result,
            latest_provider_usage: serde_json::from_str(&record.provider_usage_json)
                .map_err(RecordConversionError::InvalidRunProviderUsage)?,
            recent_steps: serde_json::from_str(&record.recent_steps_json)
                .map_err(RecordConversionError::InvalidRunRecentSteps)?,
            evidence_refs: serde_json::from_str(&record.evidence_refs_json)
                .map_err(RecordConversionError::InvalidRunEvidenceRefs)?,
            pending_approvals: serde_json::from_str(&record.pending_approvals_json)
                .map_err(RecordConversionError::InvalidRunPendingApprovals)?,
            provider_loop: serde_json::from_str(&record.provider_loop_json)
                .map_err(RecordConversionError::InvalidRunProviderLoop)?,
            delegate_runs: serde_json::from_str(&record.delegate_runs_json)
                .map_err(RecordConversionError::InvalidRunDelegateRuns)?,
            ..RunSnapshot::default()
        })
    }
}

impl TryFrom<TranscriptRecord> for TranscriptEntry {
    type Error = RecordConversionError;

    fn try_from(record: TranscriptRecord) -> Result<Self, Self::Error> {
        let role = MessageRole::try_from(record.kind.as_str()).map_err(|_| {
            RecordConversionError::InvalidMessageRole {
                value: record.kind.clone(),
            }
        })?;

        Ok(TranscriptEntry::new(
            record.id,
            record.session_id,
            record.run_id.as_deref(),
            role,
            record.content,
            record.created_at,
        ))
    }
}

impl TryFrom<&SessionInboxEvent> for SessionInboxEventRecord {
    type Error = RecordConversionError;

    fn try_from(event: &SessionInboxEvent) -> Result<Self, Self::Error> {
        Ok(Self {
            id: event.id.clone(),
            session_id: event.session_id.clone(),
            job_id: event.job_id.clone(),
            kind: event.kind.as_str().to_string(),
            payload_json: serde_json::to_string(&event.payload)
                .map_err(RecordConversionError::SerializeInboxEventPayload)?,
            status: event.status.as_str().to_string(),
            created_at: event.created_at,
            available_at: event.available_at,
            claimed_at: event.claimed_at,
            processed_at: event.processed_at,
            error: event.error.clone(),
        })
    }
}

impl TryFrom<SessionInboxEventRecord> for SessionInboxEvent {
    type Error = RecordConversionError;

    fn try_from(record: SessionInboxEventRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: record.id,
            session_id: record.session_id,
            job_id: record.job_id,
            kind: agent_runtime::inbox::SessionInboxEventKind::try_from(record.kind.as_str())
                .map_err(RecordConversionError::InvalidInboxEventStatus)?,
            payload: serde_json::from_str::<SessionInboxEventPayload>(&record.payload_json)
                .map_err(RecordConversionError::InvalidInboxEventPayload)?,
            status: SessionInboxEventStatus::try_from(record.status.as_str())
                .map_err(RecordConversionError::InvalidInboxEventStatus)?,
            created_at: record.created_at,
            available_at: record.available_at,
            claimed_at: record.claimed_at,
            processed_at: record.processed_at,
            error: record.error,
        })
    }
}

impl TryFrom<&JobSpec> for JobRecord {
    type Error = RecordConversionError;

    fn try_from(job: &JobSpec) -> Result<Self, Self::Error> {
        job.validate()
            .map_err(RecordConversionError::InvalidJobSpec)?;
        Ok(Self {
            id: job.id.clone(),
            session_id: job.session_id.clone(),
            mission_id: job.mission_id.clone(),
            run_id: job.run_id.clone(),
            parent_job_id: job.parent_job_id.clone(),
            kind: job.kind.as_str().to_string(),
            status: job.status.as_str().to_string(),
            input_json: Some(
                serde_json::to_string(&job.input)
                    .map_err(RecordConversionError::SerializeJobInput)?,
            ),
            result_json: job
                .result
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(RecordConversionError::SerializeJobResult)?,
            error: job.error.clone(),
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            attempt_count: i64::from(job.attempt_count),
            max_attempts: i64::from(job.max_attempts),
            lease_owner: job.lease_owner.clone(),
            lease_expires_at: job.lease_expires_at,
            heartbeat_at: job.heartbeat_at,
            cancel_requested_at: job.cancel_requested_at,
            last_progress_message: job.last_progress_message.clone(),
            callback_json: job
                .callback
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(RecordConversionError::SerializeJobInput)?,
            callback_sent_at: job.callback_sent_at,
        })
    }
}

impl TryFrom<JobRecord> for JobSpec {
    type Error = RecordConversionError;

    fn try_from(record: JobRecord) -> Result<Self, Self::Error> {
        let job = Self {
            id: record.id,
            session_id: record.session_id,
            mission_id: record.mission_id,
            run_id: record.run_id,
            parent_job_id: record.parent_job_id,
            kind: JobKind::try_from(record.kind.as_str())
                .map_err(RecordConversionError::InvalidJobKind)?,
            status: JobStatus::try_from(record.status.as_str())
                .map_err(RecordConversionError::InvalidJobStatus)?,
            input: serde_json::from_str(
                record
                    .input_json
                    .as_deref()
                    .ok_or(RecordConversionError::MissingJobInput)?,
            )
            .map_err(RecordConversionError::InvalidJobInput)?,
            result: record
                .result_json
                .as_deref()
                .map(serde_json::from_str::<JobResult>)
                .transpose()
                .map_err(RecordConversionError::InvalidJobResult)?,
            error: record.error,
            created_at: record.created_at,
            updated_at: record.updated_at,
            started_at: record.started_at,
            finished_at: record.finished_at,
            attempt_count: record.attempt_count.max(0) as u32,
            max_attempts: record.max_attempts.max(0) as u32,
            lease_owner: record.lease_owner,
            lease_expires_at: record.lease_expires_at,
            heartbeat_at: record.heartbeat_at,
            cancel_requested_at: record.cancel_requested_at,
            last_progress_message: record.last_progress_message,
            callback: record
                .callback_json
                .as_deref()
                .map(serde_json::from_str::<agent_runtime::mission::JobCallbackTarget>)
                .transpose()
                .map_err(RecordConversionError::InvalidJobInput)?,
            callback_sent_at: record.callback_sent_at,
        };
        job.validate()
            .map_err(RecordConversionError::InvalidJobSpec)?;
        Ok(job)
    }
}

impl fmt::Display for RecordConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJobInput(source) => write!(formatter, "invalid job input: {source}"),
            Self::InvalidJobKind(source) => write!(formatter, "invalid job kind: {source}"),
            Self::InvalidJobResult(source) => write!(formatter, "invalid job result: {source}"),
            Self::InvalidJobSpec(source) => {
                write!(formatter, "invalid job specification: {source}")
            }
            Self::InvalidJobStatus(source) => write!(formatter, "invalid job status: {source}"),
            Self::InvalidContextOffloadRefs(source) => {
                write!(formatter, "invalid context offload refs: {source}")
            }
            Self::InvalidInboxEventPayload(source) => {
                write!(formatter, "invalid inbox event payload: {source}")
            }
            Self::InvalidInboxEventStatus(source) => {
                write!(formatter, "invalid inbox event status: {source}")
            }
            Self::InvalidContextSummaryCoveredMessageCount { value } => {
                write!(
                    formatter,
                    "invalid context summary covered_message_count: {value}"
                )
            }
            Self::InvalidContextSummaryTokenEstimate { value } => {
                write!(
                    formatter,
                    "invalid context summary summary_token_estimate: {value}"
                )
            }
            Self::InvalidMessageRole { value } => {
                write!(formatter, "invalid transcript role {value}")
            }
            Self::InvalidMissionAcceptance(source) => {
                write!(formatter, "invalid mission acceptance criteria: {source}")
            }
            Self::InvalidMissionExecutionIntent(source) => {
                write!(formatter, "invalid mission execution intent: {source}")
            }
            Self::InvalidMissionSchedule(source) => {
                write!(formatter, "invalid mission schedule: {source}")
            }
            Self::InvalidMissionStatus(source) => {
                write!(formatter, "invalid mission status: {source}")
            }
            Self::InvalidPlanItems(source) => {
                write!(formatter, "invalid plan items: {source}")
            }
            Self::MissingJobInput => write!(formatter, "job input is missing"),
            Self::InvalidPromptOverride(source) => {
                write!(formatter, "invalid prompt override: {source}")
            }
            Self::InvalidRunDelegateRuns(source) => {
                write!(formatter, "invalid run delegate runs: {source}")
            }
            Self::InvalidRunRecentSteps(source) => {
                write!(formatter, "invalid run recent steps: {source}")
            }
            Self::InvalidRunPendingApprovals(source) => {
                write!(formatter, "invalid run pending approvals: {source}")
            }
            Self::InvalidRunProviderLoop(source) => {
                write!(formatter, "invalid run provider loop state: {source}")
            }
            Self::InvalidRunProviderUsage(source) => {
                write!(formatter, "invalid run provider usage: {source}")
            }
            Self::InvalidRunEvidenceRefs(source) => {
                write!(formatter, "invalid run evidence refs: {source}")
            }
            Self::InvalidRunStatus(source) => {
                write!(formatter, "invalid run status: {source}")
            }
            Self::InvalidSessionSettings(source) => {
                write!(formatter, "invalid session settings: {source}")
            }
            Self::SerializeContextOffloadRefs(source) => {
                write!(
                    formatter,
                    "failed to serialize context offload refs: {source}"
                )
            }
            Self::SerializeInboxEventPayload(source) => {
                write!(
                    formatter,
                    "failed to serialize inbox event payload: {source}"
                )
            }
            Self::SerializeJobInput(source) => {
                write!(formatter, "failed to serialize job input: {source}")
            }
            Self::SerializeJobResult(source) => {
                write!(formatter, "failed to serialize job result: {source}")
            }
            Self::SerializeMissionAcceptance(source) => {
                write!(
                    formatter,
                    "failed to serialize mission acceptance criteria: {source}"
                )
            }
            Self::SerializeMissionSchedule(source) => {
                write!(formatter, "failed to serialize mission schedule: {source}")
            }
            Self::SerializePlanItems(source) => {
                write!(formatter, "failed to serialize plan items: {source}")
            }
            Self::SerializeRunDelegateRuns(source) => {
                write!(formatter, "failed to serialize run delegate runs: {source}")
            }
            Self::SerializeRunRecentSteps(source) => {
                write!(formatter, "failed to serialize run recent steps: {source}")
            }
            Self::SerializeRunEvidenceRefs(source) => {
                write!(formatter, "failed to serialize run evidence refs: {source}")
            }
            Self::SerializeRunPendingApprovals(source) => {
                write!(
                    formatter,
                    "failed to serialize run pending approvals: {source}"
                )
            }
            Self::SerializeRunProviderLoop(source) => {
                write!(
                    formatter,
                    "failed to serialize run provider loop state: {source}"
                )
            }
            Self::SerializeRunProviderUsage(source) => {
                write!(
                    formatter,
                    "failed to serialize run provider usage: {source}"
                )
            }
            Self::SerializeSessionSettings(source) => {
                write!(formatter, "failed to serialize session settings: {source}")
            }
        }
    }
}

impl Error for RecordConversionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidJobInput(source) => Some(source),
            Self::InvalidJobKind(source) => Some(source),
            Self::InvalidJobResult(source) => Some(source),
            Self::InvalidJobSpec(source) => Some(source),
            Self::InvalidJobStatus(source) => Some(source),
            Self::InvalidContextOffloadRefs(source) => Some(source),
            Self::InvalidInboxEventPayload(source) => Some(source),
            Self::InvalidInboxEventStatus(source) => Some(source),
            Self::InvalidContextSummaryCoveredMessageCount { .. } => None,
            Self::InvalidContextSummaryTokenEstimate { .. } => None,
            Self::InvalidMissionAcceptance(source) => Some(source),
            Self::InvalidMissionExecutionIntent(source) => Some(source),
            Self::InvalidMissionSchedule(source) => Some(source),
            Self::InvalidMissionStatus(source) => Some(source),
            Self::InvalidPlanItems(source) => Some(source),
            Self::InvalidPromptOverride(source) => Some(source),
            Self::InvalidRunDelegateRuns(source) => Some(source),
            Self::InvalidRunRecentSteps(source) => Some(source),
            Self::InvalidRunPendingApprovals(source) => Some(source),
            Self::InvalidRunProviderLoop(source) => Some(source),
            Self::InvalidRunProviderUsage(source) => Some(source),
            Self::InvalidRunEvidenceRefs(source) => Some(source),
            Self::InvalidRunStatus(source) => Some(source),
            Self::InvalidSessionSettings(source) => Some(source),
            Self::SerializeContextOffloadRefs(source) => Some(source),
            Self::SerializeInboxEventPayload(source) => Some(source),
            Self::SerializeJobInput(source) => Some(source),
            Self::SerializeJobResult(source) => Some(source),
            Self::SerializeMissionAcceptance(source) => Some(source),
            Self::SerializeMissionSchedule(source) => Some(source),
            Self::SerializePlanItems(source) => Some(source),
            Self::SerializeRunDelegateRuns(source) => Some(source),
            Self::SerializeRunRecentSteps(source) => Some(source),
            Self::SerializeRunEvidenceRefs(source) => Some(source),
            Self::SerializeRunPendingApprovals(source) => Some(source),
            Self::SerializeRunProviderLoop(source) => Some(source),
            Self::SerializeRunProviderUsage(source) => Some(source),
            Self::SerializeSessionSettings(source) => Some(source),
            Self::InvalidMessageRole { .. } | Self::MissingJobInput => None,
        }
    }
}

#[cfg(test)]
mod tests;
