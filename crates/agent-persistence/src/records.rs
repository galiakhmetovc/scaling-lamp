use agent_runtime::mission::{
    JobKind, JobKindParseError, JobResult, JobSpec, JobSpecValidationError, JobStatus,
    JobStatusParseError, MissionExecutionIntent, MissionExecutionIntentParseError, MissionSchedule,
    MissionSpec, MissionStatus, MissionStatusParseError,
};
use agent_runtime::run::{RunSnapshot, RunStatus, RunStatusParseError};
use agent_runtime::session::{
    MessageRole, PromptOverride, Session, SessionError, SessionSettings, TranscriptEntry,
};
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: String,
    pub title: String,
    pub prompt_override: Option<String>,
    pub settings_json: String,
    pub active_mission_id: Option<String>,
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
    pub evidence_refs_json: String,
    pub pending_approvals_json: String,
    pub delegate_runs_json: String,
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRecord {
    pub id: String,
    pub mission_id: String,
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
    InvalidMessageRole { value: String },
    InvalidMissionAcceptance(serde_json::Error),
    InvalidMissionExecutionIntent(MissionExecutionIntentParseError),
    InvalidMissionSchedule(serde_json::Error),
    InvalidMissionStatus(MissionStatusParseError),
    MissingJobInput,
    InvalidPromptOverride(SessionError),
    InvalidRunDelegateRuns(serde_json::Error),
    InvalidRunPendingApprovals(serde_json::Error),
    InvalidRunEvidenceRefs(serde_json::Error),
    InvalidRunStatus(RunStatusParseError),
    InvalidSessionSettings(serde_json::Error),
    SerializeJobInput(serde_json::Error),
    SerializeJobResult(serde_json::Error),
    SerializeMissionAcceptance(serde_json::Error),
    SerializeMissionSchedule(serde_json::Error),
    SerializeRunDelegateRuns(serde_json::Error),
    SerializeRunEvidenceRefs(serde_json::Error),
    SerializeRunPendingApprovals(serde_json::Error),
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
            evidence_refs_json: serde_json::to_string(&snapshot.evidence_refs)
                .map_err(RecordConversionError::SerializeRunEvidenceRefs)?,
            pending_approvals_json: serde_json::to_string(&snapshot.pending_approvals)
                .map_err(RecordConversionError::SerializeRunPendingApprovals)?,
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
            evidence_refs: serde_json::from_str(&record.evidence_refs_json)
                .map_err(RecordConversionError::InvalidRunEvidenceRefs)?,
            pending_approvals: serde_json::from_str(&record.pending_approvals_json)
                .map_err(RecordConversionError::InvalidRunPendingApprovals)?,
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

impl TryFrom<&JobSpec> for JobRecord {
    type Error = RecordConversionError;

    fn try_from(job: &JobSpec) -> Result<Self, Self::Error> {
        job.validate()
            .map_err(RecordConversionError::InvalidJobSpec)?;
        Ok(Self {
            id: job.id.clone(),
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
        })
    }
}

impl TryFrom<JobRecord> for JobSpec {
    type Error = RecordConversionError;

    fn try_from(record: JobRecord) -> Result<Self, Self::Error> {
        let job = Self {
            id: record.id,
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
            Self::MissingJobInput => write!(formatter, "job input is missing"),
            Self::InvalidPromptOverride(source) => {
                write!(formatter, "invalid prompt override: {source}")
            }
            Self::InvalidRunDelegateRuns(source) => {
                write!(formatter, "invalid run delegate runs: {source}")
            }
            Self::InvalidRunPendingApprovals(source) => {
                write!(formatter, "invalid run pending approvals: {source}")
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
            Self::SerializeRunDelegateRuns(source) => {
                write!(formatter, "failed to serialize run delegate runs: {source}")
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
            Self::InvalidMissionAcceptance(source) => Some(source),
            Self::InvalidMissionExecutionIntent(source) => Some(source),
            Self::InvalidMissionSchedule(source) => Some(source),
            Self::InvalidMissionStatus(source) => Some(source),
            Self::InvalidPromptOverride(source) => Some(source),
            Self::InvalidRunDelegateRuns(source) => Some(source),
            Self::InvalidRunPendingApprovals(source) => Some(source),
            Self::InvalidRunEvidenceRefs(source) => Some(source),
            Self::InvalidRunStatus(source) => Some(source),
            Self::InvalidSessionSettings(source) => Some(source),
            Self::SerializeJobInput(source) => Some(source),
            Self::SerializeJobResult(source) => Some(source),
            Self::SerializeMissionAcceptance(source) => Some(source),
            Self::SerializeMissionSchedule(source) => Some(source),
            Self::SerializeRunDelegateRuns(source) => Some(source),
            Self::SerializeRunEvidenceRefs(source) => Some(source),
            Self::SerializeRunPendingApprovals(source) => Some(source),
            Self::SerializeSessionSettings(source) => Some(source),
            Self::InvalidMessageRole { .. } | Self::MissingJobInput => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{JobRecord, MissionRecord, RunRecord, SessionRecord, TranscriptRecord};
    use agent_runtime::mission::{
        AcceptanceCriterion, JobExecutionInput, JobKind, JobResult, JobSpec,
        JobSpecValidationError, JobStatus, MissionExecutionIntent, MissionSchedule, MissionSpec,
        MissionStatus,
    };
    use agent_runtime::run::{ApprovalRequest, DelegateRun, RunEngine, RunSnapshot};
    use agent_runtime::session::{MessageRole, PromptOverride, Session, TranscriptEntry};
    use agent_runtime::verification::{CheckOutcome, EvidenceBundle};

    #[test]
    fn session_records_round_trip_with_domain_sessions() {
        let session = Session {
            id: "session-1".to_string(),
            title: "Bootstrap".to_string(),
            prompt_override: Some(PromptOverride::new("Always verify").expect("prompt override")),
            settings: Default::default(),
            active_mission_id: Some("mission-1".to_string()),
            created_at: 10,
            updated_at: 11,
        };

        let stored = SessionRecord::try_from(&session).expect("session to record");
        let restored = Session::try_from(stored).expect("record to session");

        assert_eq!(restored, session);
    }

    #[test]
    fn transcript_records_round_trip_with_domain_entries() {
        let entry = TranscriptEntry::assistant(
            "message-1",
            "session-1",
            Some("run-1"),
            "starting verification",
            12,
        );

        let stored = TranscriptRecord::from(&entry);
        let restored = TranscriptEntry::try_from(stored).expect("record to entry");

        assert_eq!(restored, entry);
    }

    #[test]
    fn transcript_records_reject_unknown_roles() {
        let record = TranscriptRecord {
            id: "message-1".to_string(),
            session_id: "session-1".to_string(),
            run_id: None,
            kind: "unknown".to_string(),
            content: "content".to_string(),
            created_at: 12,
        };

        assert!(TranscriptEntry::try_from(record).is_err());
    }

    #[test]
    fn transcript_entry_serializes_role_names_stably() {
        let entry = TranscriptEntry::new(
            "message-1",
            "session-1",
            None,
            MessageRole::Tool,
            "patched files",
            13,
        );

        let stored = TranscriptRecord::from(&entry);

        assert_eq!(stored.kind, "tool");
    }

    #[test]
    fn run_records_round_trip_with_snapshot_core_fields() {
        let mut engine = RunEngine::new("run-1", "session-1", Some("mission-1"), 1);
        let mut evidence = EvidenceBundle::new("bundle-1", "run-1", 2);
        engine.start(2).expect("start");
        engine
            .wait_for_approval(
                ApprovalRequest::new("approval-1", "tool-call-1", "write access", 2),
                2,
            )
            .expect("wait for approval");
        engine
            .resolve_approval("approval-1", 2)
            .expect("resolve approval");
        engine.resume(2).expect("resume");
        engine
            .wait_for_delegate(DelegateRun::new("delegate-1", "worker-a", 2), 2)
            .expect("wait for delegate");
        evidence
            .record_check("fmt", CheckOutcome::Passed, Some("rustfmt clean"), 2)
            .expect("record fmt");
        evidence.add_artifact_ref("artifact:verification-report");
        engine
            .record_evidence(&evidence, 2)
            .expect("record evidence");
        engine
            .complete_delegate("delegate-1", 2)
            .expect("complete delegate");
        engine.resume(2).expect("resume");
        engine.complete("done", 3).expect("complete");

        let stored = RunRecord::try_from(engine.snapshot()).expect("snapshot to record");
        let restored = RunSnapshot::try_from(stored).expect("record to snapshot");

        assert_eq!(restored.id, "run-1");
        assert_eq!(restored.session_id, "session-1");
        assert_eq!(restored.mission_id.as_deref(), Some("mission-1"));
        assert_eq!(restored.status.as_str(), "completed");
        assert_eq!(restored.result.as_deref(), Some("done"));
        assert_eq!(restored.finished_at, Some(3));
        assert!(restored.pending_approvals.is_empty());
        assert!(restored.delegate_runs.is_empty());
        assert_eq!(
            restored.evidence_refs,
            vec![
                "bundle:bundle-1".to_string(),
                "check:fmt".to_string(),
                "artifact:verification-report".to_string(),
            ]
        );
    }

    #[test]
    fn mission_records_round_trip_with_schedule_and_acceptance_criteria() {
        let mission = MissionSpec {
            id: "mission-1".to_string(),
            session_id: "session-1".to_string(),
            objective: "Ship the autonomous runtime".to_string(),
            status: MissionStatus::Running,
            execution_intent: MissionExecutionIntent::Scheduled,
            schedule: MissionSchedule {
                not_before: Some(20),
                interval_seconds: Some(3600),
            },
            acceptance_criteria: vec![
                AcceptanceCriterion::new("criterion-1", "all workspace tests pass")
                    .expect("criterion"),
            ],
            created_at: 10,
            updated_at: 11,
            completed_at: None,
        };

        let stored = MissionRecord::try_from(&mission).expect("mission to record");
        let restored = MissionSpec::try_from(stored).expect("record to mission");

        assert_eq!(restored, mission);
    }

    #[test]
    fn job_records_round_trip_with_typed_input_and_result() {
        let mut job = JobSpec::mission_turn(
            "job-1",
            "mission-1",
            Some("run-1"),
            Some("job-root"),
            "Ship the autonomous runtime",
            30,
        );
        job.status = JobStatus::Completed;
        job.result = Some(JobResult::Summary {
            outcome: "done".to_string(),
        });
        job.updated_at = 31;
        job.started_at = Some(30);
        job.finished_at = Some(31);

        let stored = JobRecord::try_from(&job).expect("job to record");
        let restored = JobSpec::try_from(stored).expect("record to job");

        assert_eq!(restored.kind, JobKind::MissionTurn);
        assert_eq!(restored.status, JobStatus::Completed);
        assert_eq!(restored.mission_id, "mission-1");
        assert_eq!(restored.run_id.as_deref(), Some("run-1"));
        assert_eq!(restored.parent_job_id.as_deref(), Some("job-root"));
        assert_eq!(
            restored.input,
            JobExecutionInput::MissionTurn {
                mission_id: "mission-1".to_string(),
                goal: "Ship the autonomous runtime".to_string(),
            }
        );
        assert_eq!(
            restored.result,
            Some(JobResult::Summary {
                outcome: "done".to_string(),
            })
        );
    }

    #[test]
    fn job_records_reject_mismatched_mission_turn_identifiers() {
        let mut stored = JobRecord::try_from(&JobSpec::mission_turn(
            "job-1",
            "mission-1",
            Some("run-1"),
            None,
            "Ship the autonomous runtime",
            30,
        ))
        .expect("job to record");
        stored.mission_id = "mission-2".to_string();

        assert!(matches!(
            JobSpec::try_from(stored),
            Err(super::RecordConversionError::InvalidJobSpec(
                JobSpecValidationError::MissionIdMismatch { .. }
            ))
        ));
    }
}
