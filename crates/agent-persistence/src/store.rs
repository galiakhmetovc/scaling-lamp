mod agent_repos;
mod context_repos;
mod execution_repos;
mod inbox_repos;
mod mcp_repos;
mod memory_repos;
mod payloads;
mod schema;
mod session_mission;

use crate::PersistenceScaffold;
use crate::audit::{AuditLogConfig, DiagnosticEvent};
use crate::config::AppConfig;
use crate::records::{
    AgentChainContinuationRecord, AgentProfileRecord, AgentScheduleRecord, ArtifactRecord,
    ContextOffloadRecord, ContextSummaryRecord, JobRecord, McpConnectorRecord, MissionRecord,
    PlanRecord, RunRecord, SessionInboxEventRecord, SessionRecord, SessionRetentionRecord,
    TranscriptRecord,
};
use crate::repository::{
    AgentRepository, ArtifactRepository, ContextOffloadRepository, ContextSummaryRepository,
    JobRepository, McpRepository, MissionRepository, PlanRepository, RunRepository,
    SessionInboxRepository, SessionRepository, SessionRetentionRepository, TranscriptRepository,
};
use agent_runtime::archive::{
    ArchivedArtifactEntry, ArchivedSummary, ArchivedTranscriptEntry, SessionArchiveManifest,
};
use agent_runtime::context::{ContextOffloadPayload, ContextOffloadSnapshot};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreLayout {
    pub artifacts_dir: PathBuf,
    pub archives_dir: PathBuf,
    pub metadata_db: PathBuf,
    pub runs_dir: PathBuf,
    pub transcripts_dir: PathBuf,
}

impl StoreLayout {
    pub fn from_config(config: &AppConfig) -> Self {
        let root = &config.data_dir;

        Self {
            artifacts_dir: root.join("artifacts"),
            archives_dir: root.join("archives"),
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
    InvalidArchiveManifest {
        path: PathBuf,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenMode {
    BootstrapAndReconcile,
    RuntimeRequestPath,
}

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
            Self::InvalidArchiveManifest { path, reason } => {
                write!(
                    formatter,
                    "invalid archive manifest at {}: {reason}",
                    path.display()
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
            | Self::InvalidArchiveManifest { .. }
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
        Self::open_internal(scaffold, OpenMode::BootstrapAndReconcile)
    }

    pub fn open_runtime(scaffold: &PersistenceScaffold) -> Result<Self, StoreError> {
        Self::open_internal(scaffold, OpenMode::RuntimeRequestPath)
    }

    fn open_internal(scaffold: &PersistenceScaffold, mode: OpenMode) -> Result<Self, StoreError> {
        prepare_layout(&scaffold.stores)?;

        let connection = Connection::open(&scaffold.stores.metadata_db)?;
        configure_connection(&connection, &scaffold.config, mode)?;
        if mode == OpenMode::BootstrapAndReconcile {
            bootstrap_schema(&connection)?;
            validate_schema(&connection)?;
        }

        let store = Self {
            layout: scaffold.stores.clone(),
            connection,
        };
        if mode == OpenMode::BootstrapAndReconcile {
            store.reconcile_orphan_payloads()?;
        }

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

    pub fn session_exists(&self, id: &str) -> Result<bool, StoreError> {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)",
                [id],
                |row| row.get::<_, i64>(0),
            )
            .map(|exists| exists != 0)
            .map_err(StoreError::from)
    }

    pub fn count_sessions(&self) -> Result<usize, StoreError> {
        self.count_rows("sessions")
    }

    pub fn count_missions(&self) -> Result<usize, StoreError> {
        self.count_rows("missions")
    }

    pub fn count_runs(&self) -> Result<usize, StoreError> {
        self.count_rows("runs")
    }

    pub fn count_jobs(&self) -> Result<usize, StoreError> {
        self.count_rows("jobs")
    }

    fn count_rows(&self, table: &'static str) -> Result<usize, StoreError> {
        self.connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count.max(0) as usize)
            .map_err(StoreError::from)
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

    fn session_archive_dir(&self, session_id: &str) -> Result<PathBuf, StoreError> {
        validate_identifier(session_id)?;
        Ok(self.layout.archives_dir.join("sessions").join(session_id))
    }

    fn session_archive_manifest_path(&self, session_id: &str) -> Result<PathBuf, StoreError> {
        Ok(self.session_archive_dir(session_id)?.join("manifest.json"))
    }

    fn session_archive_summary_path(&self, session_id: &str) -> Result<PathBuf, StoreError> {
        Ok(self.session_archive_dir(session_id)?.join("summary.json"))
    }

    fn session_archive_transcript_path(&self, session_id: &str) -> Result<PathBuf, StoreError> {
        Ok(self
            .session_archive_dir(session_id)?
            .join("transcript.ndjson"))
    }

    fn session_archive_artifacts_dir(&self, session_id: &str) -> Result<PathBuf, StoreError> {
        Ok(self.session_archive_dir(session_id)?.join("artifacts"))
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

        self.append_diagnostic_event(
            "session_transcript_payload_paths",
            "enumerated transcript payload paths",
            Some(session_id),
            std::collections::BTreeMap::from([(
                "count".to_string(),
                serde_json::json!(paths.len()),
            )]),
        );
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

        self.append_diagnostic_event(
            "session_artifact_payload_paths",
            "enumerated artifact payload paths",
            Some(session_id),
            std::collections::BTreeMap::from([(
                "count".to_string(),
                serde_json::json!(paths.len()),
            )]),
        );
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

    fn list_artifact_ids_for_session(&self, session_id: &str) -> Result<Vec<String>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM artifacts WHERE session_id = ?1 ORDER BY id ASC")?;
        let mut rows = statement.query([session_id])?;
        let mut ids = Vec::new();

        while let Some(row) = rows.next()? {
            ids.push(row.get::<_, String>(0)?);
        }

        Ok(ids)
    }

    pub fn list_artifacts_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<ArtifactRecord>, StoreError> {
        let mut artifacts = Vec::new();
        for artifact_id in self.list_artifact_ids_for_session(session_id)? {
            if let Some(artifact) = self.get_artifact(&artifact_id)? {
                artifacts.push(artifact);
            }
        }
        Ok(artifacts)
    }

    pub fn archive_session_bundle(
        &self,
        session_id: &str,
        archived_at: i64,
    ) -> Result<SessionArchiveManifest, StoreError> {
        if self.get_session(session_id)?.is_none() {
            return Err(StoreError::InvalidIdentifier {
                id: session_id.to_string(),
                reason: "session does not exist",
            });
        }

        let archive_dir = self.session_archive_dir(session_id)?;
        let artifacts_dir = self.session_archive_artifacts_dir(session_id)?;
        payloads::create_directory(&archive_dir)?;
        payloads::create_directory(&artifacts_dir)?;

        let transcripts = self.list_transcripts_for_session(session_id)?;
        let transcript_entries = transcripts
            .iter()
            .map(|record| ArchivedTranscriptEntry {
                id: record.id.clone(),
                run_id: record.run_id.clone(),
                kind: record.kind.clone(),
                content: record.content.clone(),
                created_at: record.created_at,
            })
            .collect::<Vec<_>>();
        let transcript_path = self.session_archive_transcript_path(session_id)?;
        let transcript_bytes = transcript_entries
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| StoreError::InvalidArchiveManifest {
                path: transcript_path.clone(),
                reason: source.to_string(),
            })?
            .join("\n");
        fs::write(&transcript_path, transcript_bytes.as_bytes()).map_err(|source| {
            StoreError::Io {
                path: transcript_path.clone(),
                source,
            }
        })?;

        let summary_path = if let Some(summary) = self.get_context_summary(session_id)? {
            let archived_summary = ArchivedSummary {
                summary_text: summary.summary_text,
                covered_message_count: u32::try_from(summary.covered_message_count).unwrap_or(0),
                summary_token_estimate: u32::try_from(summary.summary_token_estimate).unwrap_or(0),
                updated_at: summary.updated_at,
            };
            let path = self.session_archive_summary_path(session_id)?;
            let summary_json = serde_json::to_vec_pretty(&archived_summary).map_err(|source| {
                StoreError::InvalidArchiveManifest {
                    path: path.clone(),
                    reason: source.to_string(),
                }
            })?;
            fs::write(&path, summary_json).map_err(|source| StoreError::Io {
                path: path.clone(),
                source,
            })?;
            Some("summary.json".to_string())
        } else {
            None
        };

        let mut artifacts = Vec::new();
        for artifact_id in self.list_artifact_ids_for_session(session_id)? {
            let artifact =
                self.get_artifact(&artifact_id)?
                    .ok_or_else(|| StoreError::InvalidIdentifier {
                        id: artifact_id.clone(),
                        reason: "artifact missing during archive",
                    })?;
            let file_name = artifact
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| StoreError::InvalidIdentifier {
                    id: artifact.id.clone(),
                    reason: "artifact path must resolve to a valid file name",
                })?
                .to_string();
            let relative_path = PathBuf::from("artifacts").join(&file_name);
            let archive_path = archive_dir.join(&relative_path);
            if let Some(parent) = archive_path.parent() {
                payloads::create_directory(parent)?;
            }
            fs::write(&archive_path, &artifact.bytes).map_err(|source| StoreError::Io {
                path: archive_path.clone(),
                source,
            })?;
            artifacts.push(ArchivedArtifactEntry {
                artifact_id: artifact.id.clone(),
                kind: artifact.kind.clone(),
                relative_path: relative_path.display().to_string(),
                byte_len: artifact.bytes.len() as u64,
                sha256: sha256_hex(&artifact.bytes),
                created_at: artifact.created_at,
            });
        }

        let manifest = SessionArchiveManifest {
            session_id: session_id.to_string(),
            archive_version: 1,
            archived_at,
            transcript_path: "transcript.ndjson".to_string(),
            transcript_count: u32::try_from(transcript_entries.len()).unwrap_or(u32::MAX),
            summary_path,
            artifacts,
        };
        let manifest_path = self.session_archive_manifest_path(session_id)?;
        let manifest_json = serde_json::to_vec_pretty(&manifest).map_err(|source| {
            StoreError::InvalidArchiveManifest {
                path: manifest_path.clone(),
                reason: source.to_string(),
            }
        })?;
        fs::write(&manifest_path, manifest_json).map_err(|source| StoreError::Io {
            path: manifest_path.clone(),
            source,
        })?;

        Ok(manifest)
    }

    pub fn read_session_archive_manifest(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionArchiveManifest>, StoreError> {
        let path = self.session_archive_manifest_path(session_id)?;
        let content = match read_string_payload(&path) {
            Ok(content) => content,
            Err(StoreError::MissingPayload { .. }) => return Ok(None),
            Err(error) => return Err(error),
        };
        serde_json::from_str(&content).map(Some).map_err(|source| {
            StoreError::InvalidArchiveManifest {
                path,
                reason: source.to_string(),
            }
        })
    }

    pub fn read_session_archive_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<ArchivedSummary>, StoreError> {
        let Some(manifest) = self.read_session_archive_manifest(session_id)? else {
            return Ok(None);
        };
        let Some(relative_path) = manifest.summary_path else {
            return Ok(None);
        };
        let path = self.session_archive_dir(session_id)?.join(relative_path);
        let content = match read_string_payload(&path) {
            Ok(content) => content,
            Err(StoreError::MissingPayload { .. }) => return Ok(None),
            Err(error) => return Err(error),
        };
        serde_json::from_str(&content).map(Some).map_err(|source| {
            StoreError::InvalidArchiveManifest {
                path,
                reason: source.to_string(),
            }
        })
    }

    pub fn read_session_archive_transcripts(
        &self,
        session_id: &str,
    ) -> Result<Option<Vec<ArchivedTranscriptEntry>>, StoreError> {
        let Some(manifest) = self.read_session_archive_manifest(session_id)? else {
            return Ok(None);
        };
        let path = self
            .session_archive_dir(session_id)?
            .join(manifest.transcript_path);
        let content = match read_string_payload(&path) {
            Ok(content) => content,
            Err(StoreError::MissingPayload { .. }) => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut entries = Vec::new();
        for (index, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let entry =
                serde_json::from_str::<ArchivedTranscriptEntry>(line).map_err(|source| {
                    StoreError::InvalidArchiveManifest {
                        path: path.clone(),
                        reason: format!("line {}: {}", index + 1, source),
                    }
                })?;
            entries.push(entry);
        }
        Ok(Some(entries))
    }

    fn audit_log_config(&self) -> AuditLogConfig {
        let root = self
            .layout
            .metadata_db
            .parent()
            .unwrap_or(self.layout.metadata_db.as_path())
            .to_path_buf();
        AuditLogConfig {
            path: root.join("audit/runtime.jsonl"),
        }
    }

    fn append_diagnostic_event(
        &self,
        op: &str,
        message: &str,
        session_id: Option<&str>,
        fields: std::collections::BTreeMap<String, serde_json::Value>,
    ) {
        self.audit_log_config()
            .append_event_best_effort(&DiagnosticEvent {
                ts: unix_timestamp(),
                level: "info".to_string(),
                component: "store".to_string(),
                op: op.to_string(),
                message: message.to_string(),
                pid: Some(process::id()),
                uid: None,
                euid: None,
                data_dir: self
                    .layout
                    .metadata_db
                    .parent()
                    .unwrap_or(self.layout.metadata_db.as_path())
                    .display()
                    .to_string(),
                session_id: session_id.map(str::to_string),
                run_id: None,
                job_id: None,
                daemon_base_url: None,
                elapsed_ms: None,
                outcome: Some("ok".to_string()),
                error: None,
                fields,
            });
    }
}

fn configure_connection(
    connection: &Connection,
    config: &AppConfig,
    mode: OpenMode,
) -> Result<(), StoreError> {
    connection.busy_timeout(config.runtime_timing.sqlite_busy_timeout())?;
    connection.execute_batch("PRAGMA foreign_keys = ON;")?;
    if mode == OpenMode::BootstrapAndReconcile {
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
    }
    Ok(())
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

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
