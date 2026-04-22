mod agent_repos;
mod context_repos;
mod execution_repos;
mod inbox_repos;
mod payloads;
mod schema;
mod session_mission;

use crate::PersistenceScaffold;
use crate::config::AppConfig;
use crate::records::{
    AgentChainContinuationRecord, AgentProfileRecord, AgentScheduleRecord, ArtifactRecord,
    ContextOffloadRecord, ContextSummaryRecord, JobRecord, MissionRecord, PlanRecord, RunRecord,
    SessionInboxEventRecord, SessionRecord, TranscriptRecord,
};
use crate::repository::{
    AgentRepository, ArtifactRepository, ContextOffloadRepository, ContextSummaryRepository,
    JobRepository, MissionRepository, PlanRepository, RunRepository, SessionInboxRepository,
    SessionRepository, TranscriptRepository,
};
use agent_runtime::context::{ContextOffloadPayload, ContextOffloadSnapshot};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreLayout {
    pub artifacts_dir: PathBuf,
    pub metadata_db: PathBuf,
    pub runs_dir: PathBuf,
    pub transcripts_dir: PathBuf,
}

impl StoreLayout {
    pub fn from_config(config: &AppConfig) -> Self {
        let root = &config.data_dir;

        Self {
            artifacts_dir: root.join("artifacts"),
            metadata_db: root.join("state.sqlite"),
            runs_dir: root.join("runs"),
            transcripts_dir: root.join("transcripts"),
        }
    }
}

#[derive(Debug)]
pub enum StoreError {
    ImmutableSessionAgentProfile {
        session_id: String,
        existing_agent_profile_id: String,
        attempted_agent_profile_id: String,
    },
    InvalidIdentifier {
        id: String,
        reason: &'static str,
    },
    InvalidContextOffload {
        session_id: String,
        reason: String,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    MissingPayload {
        path: PathBuf,
    },
    IntegrityMismatch {
        path: PathBuf,
    },
    SchemaMismatch {
        table: &'static str,
        reason: String,
    },
    Sqlite(rusqlite::Error),
}

#[derive(Debug)]
pub struct PersistenceStore {
    layout: StoreLayout,
    connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionStateSnapshot {
    pub sessions: Vec<SessionRecord>,
    pub missions: Vec<MissionRecord>,
    pub jobs: Vec<JobRecord>,
    pub runs: Vec<RunRecord>,
    pub inbox_events: Vec<SessionInboxEventRecord>,
}

type TranscriptRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    i64,
    String,
    i64,
);

const DEFAULT_MISSION_EXECUTION_INTENT: &str = "autonomous";
const DEFAULT_MISSION_SCHEDULE_JSON: &str = r#"{"not_before":null,"interval_seconds":null}"#;
const DEFAULT_MISSION_ACCEPTANCE_JSON: &str = "[]";
const LEGACY_MISSION_PREFIX: &str = "legacy-mission-";

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ImmutableSessionAgentProfile {
                session_id,
                existing_agent_profile_id,
                attempted_agent_profile_id,
            } => {
                write!(
                    formatter,
                    "session {session_id} cannot change agent profile from {existing_agent_profile_id} to {attempted_agent_profile_id}"
                )
            }
            Self::InvalidIdentifier { id, reason } => {
                write!(formatter, "invalid storage identifier {id}: {reason}")
            }
            Self::InvalidContextOffload { session_id, reason } => {
                write!(
                    formatter,
                    "invalid context offload for {session_id}: {reason}"
                )
            }
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "filesystem error at {}: {source}",
                    path.display()
                )
            }
            Self::MissingPayload { path } => {
                write!(formatter, "missing payload at {}", path.display())
            }
            Self::IntegrityMismatch { path } => {
                write!(
                    formatter,
                    "payload integrity mismatch at {}",
                    path.display()
                )
            }
            Self::SchemaMismatch { table, reason } => {
                write!(formatter, "schema mismatch in {table}: {reason}")
            }
            Self::Sqlite(source) => write!(formatter, "sqlite error: {source}"),
        }
    }
}

impl Error for StoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Sqlite(source) => Some(source),
            Self::ImmutableSessionAgentProfile { .. }
            | Self::InvalidIdentifier { .. }
            | Self::InvalidContextOffload { .. }
            | Self::MissingPayload { .. }
            | Self::IntegrityMismatch { .. }
            | Self::SchemaMismatch { .. } => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(source: rusqlite::Error) -> Self {
        Self::Sqlite(source)
    }
}

impl PersistenceStore {
    pub fn open(scaffold: &PersistenceScaffold) -> Result<Self, StoreError> {
        prepare_layout(&scaffold.stores)?;

        let connection = Connection::open(&scaffold.stores.metadata_db)?;
        bootstrap_schema(&connection)?;
        validate_schema(&connection)?;

        let store = Self {
            layout: scaffold.stores.clone(),
            connection,
        };
        store.reconcile_orphan_payloads()?;

        Ok(store)
    }

    pub fn load_execution_state(&self) -> Result<ExecutionStateSnapshot, StoreError> {
        Ok(ExecutionStateSnapshot {
            sessions: self.list_sessions()?,
            missions: self.list_missions()?,
            jobs: self.list_jobs()?,
            runs: self.list_runs()?,
            inbox_events: self.list_queued_session_inbox_events()?,
        })
    }

    fn transcript_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        validate_identifier(id)?;
        Ok(self.layout.transcripts_dir.join(format!("{id}.txt")))
    }

    fn artifact_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        validate_identifier(id)?;
        Ok(self.layout.artifacts_dir.join(format!("{id}.bin")))
    }

    fn artifact_relative_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        validate_identifier(id)?;
        Ok(PathBuf::from("artifacts").join(format!("{id}.bin")))
    }

    fn reconcile_orphan_payloads(&self) -> Result<(), StoreError> {
        reconcile_directory(
            &self.connection,
            "SELECT storage_key, byte_len, sha256 FROM transcripts",
            &self.layout.transcripts_dir,
        )?;
        reconcile_directory(
            &self.connection,
            "SELECT path, byte_len, sha256 FROM artifacts",
            &self.layout.artifacts_dir,
        )?;
        Ok(())
    }

    fn hydrate_transcript_record(
        &self,
        row: TranscriptRow,
    ) -> Result<TranscriptRecord, StoreError> {
        let (id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at) = row;
        let path = self.layout.transcripts_dir.join(storage_key);
        let content = read_string_payload(&path)?;
        validate_integrity(
            &path,
            content.len() as u64,
            content.as_bytes(),
            byte_len as u64,
            &sha256,
        )?;

        Ok(TranscriptRecord {
            id,
            session_id,
            run_id,
            kind,
            content,
            created_at,
        })
    }

    fn session_transcript_payload_paths(
        &self,
        session_id: &str,
    ) -> Result<Vec<PathBuf>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT storage_key FROM transcripts WHERE session_id = ?1 ORDER BY id ASC")?;
        let mut rows = statement.query([session_id])?;
        let mut paths = Vec::new();

        while let Some(row) = rows.next()? {
            let storage_key = row.get::<_, String>(0)?;
            paths.push(self.layout.transcripts_dir.join(storage_key));
        }

        Ok(paths)
    }

    fn session_artifact_payload_paths(&self, session_id: &str) -> Result<Vec<PathBuf>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT path FROM artifacts WHERE session_id = ?1 ORDER BY id ASC")?;
        let mut rows = statement.query([session_id])?;
        let mut paths = Vec::new();
        let root = self
            .layout
            .metadata_db
            .parent()
            .unwrap_or(self.layout.metadata_db.as_path());

        while let Some(row) = rows.next()? {
            let relative_path = row.get::<_, String>(0)?;
            paths.push(root.join(relative_path));
        }

        Ok(paths)
    }

    fn delete_artifact_by_id(&self, id: &str) -> Result<bool, StoreError> {
        let path = self.artifact_path(id)?;
        let deleted = self
            .connection
            .execute("DELETE FROM artifacts WHERE id = ?1", [id])?;

        if deleted == 0 {
            return Ok(false);
        }

        remove_payload_if_exists(&path)?;
        remove_payload_if_exists(&backup_path(&path))?;
        Ok(true)
    }
}

fn prepare_layout(layout: &StoreLayout) -> Result<(), StoreError> {
    payloads::prepare_layout(layout)
}

fn bootstrap_schema(connection: &Connection) -> Result<(), StoreError> {
    schema::bootstrap_schema(connection)
}

fn validate_schema(connection: &Connection) -> Result<(), StoreError> {
    schema::validate_schema(connection)
}

fn validate_identifier(id: &str) -> Result<(), StoreError> {
    schema::validate_identifier(id)
}

fn persist_payload_with_commit<F>(path: &Path, bytes: &[u8], commit: F) -> Result<(), StoreError>
where
    F: FnOnce() -> Result<(), StoreError>,
{
    payloads::persist_payload_with_commit(path, bytes, commit)
}

fn reconcile_directory(
    connection: &Connection,
    query: &str,
    directory: &Path,
) -> Result<(), StoreError> {
    payloads::reconcile_directory(connection, query, directory)
}

fn backup_path(path: &Path) -> PathBuf {
    payloads::backup_path(path)
}

fn remove_payload_if_exists(path: &Path) -> Result<(), StoreError> {
    payloads::remove_payload_if_exists(path)
}

fn sha256_hex(bytes: &[u8]) -> String {
    payloads::sha256_hex(bytes)
}

fn validate_integrity(
    path: &Path,
    actual_len: u64,
    bytes: &[u8],
    expected_len: u64,
    expected_sha256: &str,
) -> Result<(), StoreError> {
    payloads::validate_integrity(path, actual_len, bytes, expected_len, expected_sha256)
}

fn read_string_payload(path: &Path) -> Result<String, StoreError> {
    payloads::read_string_payload(path)
}

fn read_binary_payload(path: &Path) -> Result<Vec<u8>, StoreError> {
    payloads::read_binary_payload(path)
}

#[cfg(test)]
mod tests;
