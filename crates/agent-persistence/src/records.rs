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
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRecord {
    pub id: String,
    pub run_id: String,
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
    InvalidMessageRole { value: String },
    InvalidPromptOverride(SessionError),
    InvalidRunStatus(RunStatusParseError),
    InvalidSessionSettings(serde_json::Error),
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

impl fmt::Display for RecordConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMessageRole { value } => {
                write!(formatter, "invalid transcript role {value}")
            }
            Self::InvalidPromptOverride(source) => {
                write!(formatter, "invalid prompt override: {source}")
            }
            Self::InvalidRunStatus(source) => {
                write!(formatter, "invalid run status: {source}")
            }
            Self::InvalidSessionSettings(source) => {
                write!(formatter, "invalid session settings: {source}")
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
            Self::InvalidPromptOverride(source) => Some(source),
            Self::InvalidRunStatus(source) => Some(source),
            Self::InvalidSessionSettings(source) => Some(source),
            Self::SerializeSessionSettings(source) => Some(source),
            Self::InvalidMessageRole { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RunRecord, SessionRecord, TranscriptRecord};
    use agent_runtime::run::{RunEngine, RunSnapshot};
    use agent_runtime::session::{MessageRole, PromptOverride, Session, TranscriptEntry};

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
        engine.start(2).expect("start");
        engine.complete("done", 3).expect("complete");

        let stored = RunRecord::try_from(engine.snapshot()).expect("snapshot to record");
        let restored = RunSnapshot::try_from(stored).expect("record to snapshot");

        assert_eq!(restored.id, "run-1");
        assert_eq!(restored.session_id, "session-1");
        assert_eq!(restored.mission_id.as_deref(), Some("mission-1"));
        assert_eq!(restored.status.as_str(), "completed");
        assert_eq!(restored.result.as_deref(), Some("done"));
        assert_eq!(restored.finished_at, Some(3));
    }
}
