mod payloads;
mod schema;

use crate::PersistenceScaffold;
use crate::config::AppConfig;
use crate::records::{
    ArtifactRecord, ContextOffloadRecord, ContextSummaryRecord, JobRecord, MissionRecord,
    PlanRecord, RunRecord, SessionRecord, TranscriptRecord,
};
use crate::repository::{
    ArtifactRepository, ContextOffloadRepository, ContextSummaryRepository, JobRepository,
    MissionRepository, PlanRepository, RunRepository, SessionRepository, TranscriptRepository,
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
            Self::InvalidIdentifier { .. }
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

impl SessionRepository for PersistenceStore {
    fn put_session(&self, record: &SessionRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO sessions (
                id, title, prompt_override, settings_json, active_mission_id, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                prompt_override = excluded.prompt_override,
                settings_json = excluded.settings_json,
                active_mission_id = excluded.active_mission_id,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
            params![
                record.id,
                record.title,
                record.prompt_override,
                &record.settings_json,
                record.active_mission_id,
                record.created_at,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    fn get_session(&self, id: &str) -> Result<Option<SessionRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, title, prompt_override, settings_json, active_mission_id, created_at, updated_at
                 FROM sessions WHERE id = ?1",
                [id],
                |row| {
                    Ok(SessionRecord {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        prompt_override: row.get(2)?,
                        settings_json: row.get(3)?,
                        active_mission_id: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_sessions(&self) -> Result<Vec<SessionRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, title, prompt_override, settings_json, active_mission_id, created_at, updated_at
             FROM sessions
             ORDER BY created_at ASC, id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut sessions = Vec::new();

        while let Some(row) = rows.next()? {
            sessions.push(SessionRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                prompt_override: row.get(2)?,
                settings_json: row.get(3)?,
                active_mission_id: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            });
        }

        Ok(sessions)
    }

    fn delete_session(&self, id: &str) -> Result<bool, StoreError> {
        let transcript_paths = self.session_transcript_payload_paths(id)?;
        let artifact_paths = self.session_artifact_payload_paths(id)?;
        let deleted = self
            .connection
            .execute("DELETE FROM sessions WHERE id = ?1", [id])?;

        if deleted == 0 {
            return Ok(false);
        }

        for path in transcript_paths.into_iter().chain(artifact_paths) {
            remove_payload_if_exists(&path)?;
            remove_payload_if_exists(&backup_path(&path))?;
        }

        Ok(true)
    }
}

impl MissionRepository for PersistenceStore {
    fn put_mission(&self, record: &MissionRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO missions (
                id, session_id, objective, status, execution_intent, schedule_json, acceptance_json,
                created_at, updated_at, completed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                objective = excluded.objective,
                status = excluded.status,
                execution_intent = excluded.execution_intent,
                schedule_json = excluded.schedule_json,
                acceptance_json = excluded.acceptance_json,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                completed_at = excluded.completed_at",
            params![
                record.id,
                record.session_id,
                record.objective,
                record.status,
                record.execution_intent,
                record.schedule_json,
                record.acceptance_json,
                record.created_at,
                record.updated_at,
                record.completed_at
            ],
        )?;
        Ok(())
    }

    fn get_mission(&self, id: &str) -> Result<Option<MissionRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, session_id, objective, status, execution_intent, schedule_json,
                        acceptance_json, created_at, updated_at, completed_at
                 FROM missions WHERE id = ?1",
                [id],
                |row| {
                    Ok(MissionRecord {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        objective: row.get(2)?,
                        status: row.get(3)?,
                        execution_intent: row.get(4)?,
                        schedule_json: row.get(5)?,
                        acceptance_json: row.get(6)?,
                        created_at: row.get(7)?,
                        updated_at: row.get(8)?,
                        completed_at: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_missions(&self) -> Result<Vec<MissionRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, objective, status, execution_intent, schedule_json,
                    acceptance_json, created_at, updated_at, completed_at
             FROM missions
             ORDER BY created_at ASC, id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut missions = Vec::new();

        while let Some(row) = rows.next()? {
            missions.push(MissionRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                objective: row.get(2)?,
                status: row.get(3)?,
                execution_intent: row.get(4)?,
                schedule_json: row.get(5)?,
                acceptance_json: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                completed_at: row.get(9)?,
            });
        }

        Ok(missions)
    }
}

impl RunRepository for PersistenceStore {
    fn put_run(&self, record: &RunRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO runs (
                id, session_id, mission_id, status, error, result, recent_steps_json, evidence_refs_json,
                pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                mission_id = excluded.mission_id,
                status = excluded.status,
                error = excluded.error,
                result = excluded.result,
                recent_steps_json = excluded.recent_steps_json,
                evidence_refs_json = excluded.evidence_refs_json,
                pending_approvals_json = excluded.pending_approvals_json,
                provider_loop_json = excluded.provider_loop_json,
                delegate_runs_json = excluded.delegate_runs_json,
                started_at = excluded.started_at,
                updated_at = excluded.updated_at,
                finished_at = excluded.finished_at",
            params![
                record.id,
                record.session_id,
                record.mission_id,
                record.status,
                record.error,
                record.result,
                record.recent_steps_json,
                record.evidence_refs_json,
                record.pending_approvals_json,
                record.provider_loop_json,
                record.delegate_runs_json,
                record.started_at,
                record.updated_at,
                record.finished_at
            ],
        )?;
        Ok(())
    }

    fn get_run(&self, id: &str) -> Result<Option<RunRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, session_id, mission_id, status, error, result, recent_steps_json,
                        evidence_refs_json, pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
                 FROM runs WHERE id = ?1",
                [id],
                |row| {
                    Ok(RunRecord {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        mission_id: row.get(2)?,
                        status: row.get(3)?,
                        error: row.get(4)?,
                        result: row.get(5)?,
                        recent_steps_json: row.get(6)?,
                        evidence_refs_json: row.get(7)?,
                        pending_approvals_json: row.get(8)?,
                        provider_loop_json: row.get(9)?,
                        delegate_runs_json: row.get(10)?,
                        started_at: row.get(11)?,
                        updated_at: row.get(12)?,
                        finished_at: row.get(13)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_runs(&self) -> Result<Vec<RunRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, mission_id, status, error, result, recent_steps_json, evidence_refs_json,
                    pending_approvals_json, provider_loop_json, delegate_runs_json, started_at, updated_at, finished_at
             FROM runs
             ORDER BY started_at ASC, id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut runs = Vec::new();

        while let Some(row) = rows.next()? {
            runs.push(RunRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                mission_id: row.get(2)?,
                status: row.get(3)?,
                error: row.get(4)?,
                result: row.get(5)?,
                recent_steps_json: row.get(6)?,
                evidence_refs_json: row.get(7)?,
                pending_approvals_json: row.get(8)?,
                provider_loop_json: row.get(9)?,
                delegate_runs_json: row.get(10)?,
                started_at: row.get(11)?,
                updated_at: row.get(12)?,
                finished_at: row.get(13)?,
            });
        }

        Ok(runs)
    }
}

impl JobRepository for PersistenceStore {
    fn put_job(&self, record: &JobRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO jobs (
                id, mission_id, run_id, parent_job_id, kind, status, input_json, result_json, error,
                created_at, updated_at, started_at, finished_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                mission_id = excluded.mission_id,
                run_id = excluded.run_id,
                parent_job_id = excluded.parent_job_id,
                kind = excluded.kind,
                status = excluded.status,
                input_json = excluded.input_json,
                result_json = excluded.result_json,
                error = excluded.error,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                started_at = excluded.started_at,
                finished_at = excluded.finished_at",
            params![
                record.id,
                record.mission_id,
                record.run_id,
                record.parent_job_id,
                record.kind,
                record.status,
                record.input_json,
                record.result_json,
                record.error,
                record.created_at,
                record.updated_at,
                record.started_at,
                record.finished_at
            ],
        )?;
        Ok(())
    }

    fn get_job(&self, id: &str) -> Result<Option<JobRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT id, mission_id, run_id, parent_job_id, kind, status, input_json,
                        result_json, error, created_at, updated_at, started_at, finished_at
                 FROM jobs WHERE id = ?1",
                [id],
                |row| {
                    Ok(JobRecord {
                        id: row.get(0)?,
                        mission_id: row.get(1)?,
                        run_id: row.get(2)?,
                        parent_job_id: row.get(3)?,
                        kind: row.get(4)?,
                        status: row.get(5)?,
                        input_json: row.get(6)?,
                        result_json: row.get(7)?,
                        error: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                        started_at: row.get(11)?,
                        finished_at: row.get(12)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_jobs(&self) -> Result<Vec<JobRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, mission_id, run_id, parent_job_id, kind, status, input_json,
                    result_json, error, created_at, updated_at, started_at, finished_at
             FROM jobs
             ORDER BY created_at ASC, id ASC",
        )?;
        let mut rows = statement.query([])?;
        let mut jobs = Vec::new();

        while let Some(row) = rows.next()? {
            jobs.push(JobRecord {
                id: row.get(0)?,
                mission_id: row.get(1)?,
                run_id: row.get(2)?,
                parent_job_id: row.get(3)?,
                kind: row.get(4)?,
                status: row.get(5)?,
                input_json: row.get(6)?,
                result_json: row.get(7)?,
                error: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                started_at: row.get(11)?,
                finished_at: row.get(12)?,
            });
        }

        Ok(jobs)
    }
}

impl TranscriptRepository for PersistenceStore {
    fn put_transcript(&self, record: &TranscriptRecord) -> Result<(), StoreError> {
        let path = self.transcript_path(&record.id)?;
        let storage_key = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| StoreError::InvalidIdentifier {
                id: record.id.clone(),
                reason: "must produce a valid payload filename",
            })?
            .to_string();
        let sha256 = sha256_hex(record.content.as_bytes());

        persist_payload_with_commit(&path, record.content.as_bytes(), || {
            self.connection
                .execute(
                    "INSERT INTO transcripts (
                        id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(id) DO UPDATE SET
                        session_id = excluded.session_id,
                        run_id = excluded.run_id,
                        kind = excluded.kind,
                        storage_key = excluded.storage_key,
                        byte_len = excluded.byte_len,
                        sha256 = excluded.sha256,
                        created_at = excluded.created_at",
                    params![
                        record.id,
                        record.session_id,
                        record.run_id,
                        record.kind,
                        storage_key,
                        record.content.len() as i64,
                        sha256,
                        record.created_at
                    ],
                )
                .map(|_| ())
                .map_err(StoreError::from)
        })
    }

    fn get_transcript(&self, id: &str) -> Result<Option<TranscriptRecord>, StoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
                 FROM transcripts WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            )
            .optional()?;

        match row {
            Some(row) => Ok(Some(self.hydrate_transcript_record(row)?)),
            None => Ok(None),
        }
    }

    fn list_transcripts_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<TranscriptRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, run_id, kind, storage_key, byte_len, sha256, created_at
             FROM transcripts
             WHERE session_id = ?1
             ORDER BY created_at ASC, id ASC",
        )?;
        let mut rows = statement.query([session_id])?;
        let mut transcripts = Vec::new();

        while let Some(row) = rows.next()? {
            let row: TranscriptRow = (
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            );
            transcripts.push(self.hydrate_transcript_record(row)?);
        }

        Ok(transcripts)
    }
}

impl ContextSummaryRepository for PersistenceStore {
    fn put_context_summary(&self, record: &ContextSummaryRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO context_summaries (
                session_id, summary_text, covered_message_count, summary_token_estimate, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(session_id) DO UPDATE SET
                summary_text = excluded.summary_text,
                covered_message_count = excluded.covered_message_count,
                summary_token_estimate = excluded.summary_token_estimate,
                updated_at = excluded.updated_at",
            params![
                record.session_id,
                record.summary_text,
                record.covered_message_count,
                record.summary_token_estimate,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    fn get_context_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<ContextSummaryRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT session_id, summary_text, covered_message_count, summary_token_estimate, updated_at
                 FROM context_summaries WHERE session_id = ?1",
                [session_id],
                |row| {
                    Ok(ContextSummaryRecord {
                        session_id: row.get(0)?,
                        summary_text: row.get(1)?,
                        covered_message_count: row.get(2)?,
                        summary_token_estimate: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }
}

impl ContextOffloadRepository for PersistenceStore {
    fn put_context_offload(
        &self,
        record: &ContextOffloadRecord,
        payloads: &[ContextOffloadPayload],
    ) -> Result<(), StoreError> {
        let snapshot = ContextOffloadSnapshot::try_from(record.clone()).map_err(|source| {
            StoreError::InvalidContextOffload {
                session_id: record.session_id.clone(),
                reason: source.to_string(),
            }
        })?;
        let referenced_artifact_ids = snapshot
            .refs
            .iter()
            .map(|reference| reference.artifact_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let payload_artifact_ids = payloads
            .iter()
            .map(|payload| payload.artifact_id.clone())
            .collect::<std::collections::BTreeSet<_>>();

        if referenced_artifact_ids != payload_artifact_ids {
            return Err(StoreError::InvalidContextOffload {
                session_id: record.session_id.clone(),
                reason: "payload artifact ids must exactly match snapshot refs".to_string(),
            });
        }

        let obsolete_artifact_ids = self
            .get_context_offload(&record.session_id)?
            .map(ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(|source| StoreError::InvalidContextOffload {
                session_id: record.session_id.clone(),
                reason: source.to_string(),
            })?
            .map(|existing| {
                existing
                    .refs
                    .into_iter()
                    .filter(|reference| !referenced_artifact_ids.contains(&reference.artifact_id))
                    .map(|reference| reference.artifact_id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for payload in payloads {
            let reference = snapshot
                .refs
                .iter()
                .find(|reference| reference.artifact_id == payload.artifact_id)
                .ok_or_else(|| StoreError::InvalidContextOffload {
                    session_id: record.session_id.clone(),
                    reason: format!(
                        "missing ref metadata for payload artifact {}",
                        payload.artifact_id
                    ),
                })?;
            self.put_artifact(&ArtifactRecord {
                id: payload.artifact_id.clone(),
                session_id: record.session_id.clone(),
                kind: "context_offload".to_string(),
                metadata_json: serde_json::json!({
                    "offload_ref_id": reference.id,
                    "label": reference.label,
                    "summary": reference.summary,
                    "token_estimate": reference.token_estimate,
                    "message_count": reference.message_count,
                    "created_at": reference.created_at,
                })
                .to_string(),
                path: self.artifact_relative_path(&payload.artifact_id)?,
                bytes: payload.bytes.clone(),
                created_at: reference.created_at,
            })?;
        }

        self.connection.execute(
            "INSERT INTO context_offloads (session_id, refs_json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET
                refs_json = excluded.refs_json,
                updated_at = excluded.updated_at",
            params![record.session_id, record.refs_json, record.updated_at],
        )?;

        for artifact_id in obsolete_artifact_ids {
            self.delete_artifact_by_id(&artifact_id)?;
        }

        Ok(())
    }

    fn get_context_offload(
        &self,
        session_id: &str,
    ) -> Result<Option<ContextOffloadRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT session_id, refs_json, updated_at
                 FROM context_offloads WHERE session_id = ?1",
                [session_id],
                |row| {
                    Ok(ContextOffloadRecord {
                        session_id: row.get(0)?,
                        refs_json: row.get(1)?,
                        updated_at: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn get_context_offload_payload(
        &self,
        artifact_id: &str,
    ) -> Result<Option<ContextOffloadPayload>, StoreError> {
        match self.get_artifact(artifact_id)? {
            Some(record) if record.kind == "context_offload" => Ok(Some(ContextOffloadPayload {
                artifact_id: record.id,
                bytes: record.bytes,
            })),
            Some(_) => Ok(None),
            None => Ok(None),
        }
    }
}

impl PlanRepository for PersistenceStore {
    fn put_plan(&self, record: &PlanRecord) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO plans (session_id, items_json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET
                items_json = excluded.items_json,
                updated_at = excluded.updated_at",
            params![record.session_id, record.items_json, record.updated_at],
        )?;
        Ok(())
    }

    fn get_plan(&self, session_id: &str) -> Result<Option<PlanRecord>, StoreError> {
        self.connection
            .query_row(
                "SELECT session_id, items_json, updated_at FROM plans WHERE session_id = ?1",
                [session_id],
                |row| {
                    Ok(PlanRecord {
                        session_id: row.get(0)?,
                        items_json: row.get(1)?,
                        updated_at: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }
}

impl ArtifactRepository for PersistenceStore {
    fn put_artifact(&self, record: &ArtifactRecord) -> Result<(), StoreError> {
        let path = self.artifact_path(&record.id)?;
        let relative_path = self.artifact_relative_path(&record.id)?;

        if record.path != relative_path {
            return Err(StoreError::InvalidIdentifier {
                id: record.id.clone(),
                reason: "artifact path must match the canonical storage path",
            });
        }
        let sha256 = sha256_hex(&record.bytes);

        persist_payload_with_commit(&path, &record.bytes, || {
            self.connection
                .execute(
                    "INSERT INTO artifacts (
                        id, session_id, kind, path, metadata_json, byte_len, sha256, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(id) DO UPDATE SET
                        session_id = excluded.session_id,
                        kind = excluded.kind,
                        path = excluded.path,
                        metadata_json = excluded.metadata_json,
                        byte_len = excluded.byte_len,
                        sha256 = excluded.sha256,
                        created_at = excluded.created_at",
                    params![
                        record.id,
                        &record.session_id,
                        record.kind,
                        record.path.to_string_lossy().to_string(),
                        &record.metadata_json,
                        record.bytes.len() as i64,
                        sha256,
                        record.created_at
                    ],
                )
                .map(|_| ())
                .map_err(StoreError::from)
        })
    }

    fn get_artifact(&self, id: &str) -> Result<Option<ArtifactRecord>, StoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT id, session_id, kind, path, metadata_json, byte_len, sha256, created_at
                 FROM artifacts WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            )
            .optional()?;

        match row {
            Some((id, session_id, kind, path, metadata_json, byte_len, sha256, created_at)) => {
                let path = self
                    .layout
                    .metadata_db
                    .parent()
                    .unwrap_or(self.layout.metadata_db.as_path())
                    .join(&path);
                let bytes = read_binary_payload(&path)?;
                validate_integrity(&path, bytes.len() as u64, &bytes, byte_len as u64, &sha256)?;

                Ok(Some(ArtifactRecord {
                    id,
                    session_id,
                    kind,
                    metadata_json,
                    path: PathBuf::from(
                        path.strip_prefix(
                            self.layout
                                .metadata_db
                                .parent()
                                .unwrap_or(self.layout.metadata_db.as_path()),
                        )
                        .unwrap_or(path.as_path()),
                    ),
                    bytes,
                    created_at,
                }))
            }
            None => Ok(None),
        }
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
mod tests {
    use super::{
        DEFAULT_MISSION_ACCEPTANCE_JSON, DEFAULT_MISSION_EXECUTION_INTENT,
        DEFAULT_MISSION_SCHEDULE_JSON, LEGACY_MISSION_PREFIX,
    };
    use crate::{
        ArtifactRecord, ArtifactRepository, ContextOffloadRecord, ContextOffloadRepository,
        JobRecord, JobRepository, MissionRecord, MissionRepository, PersistenceScaffold,
        PlanRecord, PlanRepository, RunRecord, RunRepository, SessionRecord, SessionRepository,
        TranscriptRecord, TranscriptRepository,
    };
    use agent_runtime::context::{
        ContextOffloadPayload, ContextOffloadRef, ContextOffloadSnapshot,
    };
    use agent_runtime::mission::JobExecutionInput;
    use agent_runtime::plan::{PlanItem, PlanItemStatus, PlanSnapshot};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn open_bootstraps_schema_and_round_trips_structured_and_file_backed_data() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });

        let session = SessionRecord {
            id: "session-1".to_string(),
            title: "Boot mission".to_string(),
            prompt_override: None,
            settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
            active_mission_id: None,
            created_at: 1,
            updated_at: 2,
        };
        let mission = MissionRecord {
            id: "mission-1".to_string(),
            session_id: session.id.clone(),
            objective: "Build stores".to_string(),
            status: "running".to_string(),
            execution_intent: "autonomous".to_string(),
            schedule_json: "{\"not_before\":null,\"interval_seconds\":null}".to_string(),
            acceptance_json: "[]".to_string(),
            created_at: 2,
            updated_at: 3,
            completed_at: None,
        };
        let run = RunRecord {
            id: "run-1".to_string(),
            session_id: session.id.clone(),
            mission_id: Some(mission.id.clone()),
            status: "running".to_string(),
            error: None,
            result: None,
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[\"bundle:bootstrap\"]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 3,
            updated_at: 4,
            finished_at: None,
        };
        let job = JobRecord {
            id: "job-1".to_string(),
            mission_id: mission.id.clone(),
            run_id: Some(run.id.clone()),
            parent_job_id: None,
            kind: "maintenance".to_string(),
            status: "queued".to_string(),
            input_json: Some(
                serde_json::to_string(&JobExecutionInput::Maintenance {
                    summary: "bootstrap schema".to_string(),
                })
                .expect("serialize maintenance input"),
            ),
            result_json: None,
            error: None,
            created_at: 4,
            updated_at: 5,
            started_at: None,
            finished_at: None,
        };
        let transcript = TranscriptRecord {
            id: "transcript-1".to_string(),
            session_id: session.id.clone(),
            run_id: Some(run.id.clone()),
            kind: "user".to_string(),
            content: "build the persistence layer".to_string(),
            created_at: 6,
        };
        let artifact = ArtifactRecord {
            id: "artifact-1".to_string(),
            session_id: session.id.clone(),
            kind: "report".to_string(),
            metadata_json: "{\"source\":\"verification\"}".to_string(),
            path: PathBuf::from("artifacts/artifact-1.bin"),
            bytes: b"verification output".to_vec(),
            created_at: 7,
        };
        let plan = PlanRecord {
            session_id: session.id.clone(),
            items_json: serde_json::to_string(&vec![PlanItem {
                id: "inspect".to_string(),
                content: "Inspect planning seams".to_string(),
                status: PlanItemStatus::Pending,
            }])
            .expect("serialize plan"),
            updated_at: 8,
        };
        let offload = ContextOffloadRecord {
            session_id: session.id.clone(),
            refs_json: serde_json::to_string(&vec![ContextOffloadRef {
                id: "offload-1".to_string(),
                label: "Earlier transcript".to_string(),
                summary: "Design notes".to_string(),
                artifact_id: "artifact-offload-1".to_string(),
                token_estimate: 120,
                message_count: 4,
                created_at: 8,
            }])
            .expect("serialize offload"),
            updated_at: 9,
        };
        let offload_payload = ContextOffloadPayload {
            artifact_id: "artifact-offload-1".to_string(),
            bytes: b"earlier transcript chunk".to_vec(),
        };

        {
            let store = super::PersistenceStore::open(&scaffold).expect("open store");
            store.put_session(&session).expect("store session");
            store.put_mission(&mission).expect("store mission");
            store
                .put_session(&SessionRecord {
                    active_mission_id: Some(mission.id.clone()),
                    ..session.clone()
                })
                .expect("attach active mission");
            store.put_run(&run).expect("store run");
            store.put_job(&job).expect("store job");
            store.put_transcript(&transcript).expect("store transcript");
            store.put_plan(&plan).expect("store plan");
            store
                .put_context_offload(&offload, std::slice::from_ref(&offload_payload))
                .expect("store offload");
            store.put_artifact(&artifact).expect("store artifact");
        }

        let reopened = super::PersistenceStore::open(&scaffold).expect("reopen store");

        assert_eq!(
            reopened.get_session(&session.id).expect("get session"),
            Some(SessionRecord {
                active_mission_id: Some(mission.id.clone()),
                ..session
            })
        );
        assert_eq!(
            reopened.get_mission(&mission.id).expect("get mission"),
            Some(mission)
        );
        assert_eq!(reopened.get_run(&run.id).expect("get run"), Some(run));
        assert_eq!(reopened.get_job(&job.id).expect("get job"), Some(job));
        assert_eq!(
            reopened
                .get_transcript(&transcript.id)
                .expect("get transcript"),
            Some(transcript)
        );
        assert_eq!(
            reopened
                .list_transcripts_for_session("session-1")
                .expect("list transcript history"),
            vec![TranscriptRecord {
                id: "transcript-1".to_string(),
                session_id: "session-1".to_string(),
                run_id: Some("run-1".to_string()),
                kind: "user".to_string(),
                content: "build the persistence layer".to_string(),
                created_at: 6,
            }]
        );
        assert_eq!(
            reopened.get_plan("session-1").expect("get plan"),
            Some(plan)
        );
        assert_eq!(
            reopened
                .get_context_offload("session-1")
                .expect("get offload"),
            Some(offload)
        );
        assert_eq!(
            reopened
                .get_context_offload_payload("artifact-offload-1")
                .expect("get offload payload"),
            Some(offload_payload)
        );
        assert_eq!(
            reopened.get_artifact(&artifact.id).expect("get artifact"),
            Some(artifact)
        );
        assert!(scaffold.stores.metadata_db.exists());
    }

    #[test]
    fn plan_repository_round_trips_structured_plan_snapshots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });
        let store = super::PersistenceStore::open(&scaffold).expect("open store");
        store
            .put_session(&SessionRecord {
                id: "session-plan".to_string(),
                title: "Plan Session".to_string(),
                prompt_override: None,
                settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
                active_mission_id: None,
                created_at: 1,
                updated_at: 1,
            })
            .expect("put session");

        let snapshot = PlanSnapshot {
            session_id: "session-plan".to_string(),
            items: vec![
                PlanItem {
                    id: "inspect".to_string(),
                    content: "Inspect seams".to_string(),
                    status: PlanItemStatus::Pending,
                },
                PlanItem {
                    id: "persist".to_string(),
                    content: "Persist plan".to_string(),
                    status: PlanItemStatus::Completed,
                },
            ],
            updated_at: 9,
        };

        store
            .put_plan(&PlanRecord::try_from(&snapshot).expect("plan record"))
            .expect("put plan");
        let restored = PlanSnapshot::try_from(
            store
                .get_plan("session-plan")
                .expect("get plan")
                .expect("plan exists"),
        )
        .expect("restore plan");

        assert_eq!(restored, snapshot);
    }

    #[test]
    fn context_offload_repository_round_trips_snapshot_and_payloads() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });
        let store = super::PersistenceStore::open(&scaffold).expect("open store");
        store
            .put_session(&SessionRecord {
                id: "session-offload".to_string(),
                title: "Offload Session".to_string(),
                prompt_override: None,
                settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
                active_mission_id: None,
                created_at: 1,
                updated_at: 1,
            })
            .expect("put session");

        let snapshot = ContextOffloadSnapshot {
            session_id: "session-offload".to_string(),
            refs: vec![ContextOffloadRef {
                id: "offload-1".to_string(),
                label: "Earlier transcript".to_string(),
                summary: "Requirements and design".to_string(),
                artifact_id: "artifact-offload-1".to_string(),
                token_estimate: 180,
                message_count: 7,
                created_at: 5,
            }],
            updated_at: 6,
        };
        let payload = ContextOffloadPayload {
            artifact_id: "artifact-offload-1".to_string(),
            bytes: b"offloaded transcript bytes".to_vec(),
        };

        store
            .put_context_offload(
                &ContextOffloadRecord::try_from(&snapshot).expect("offload record"),
                std::slice::from_ref(&payload),
            )
            .expect("put offload");

        let restored = ContextOffloadSnapshot::try_from(
            store
                .get_context_offload("session-offload")
                .expect("get offload")
                .expect("offload exists"),
        )
        .expect("restore offload");

        assert_eq!(restored, snapshot);
        assert_eq!(
            store
                .get_context_offload_payload("artifact-offload-1")
                .expect("get offload payload"),
            Some(payload)
        );
    }

    #[test]
    fn replacing_context_offload_snapshot_prunes_obsolete_artifacts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });
        let store = super::PersistenceStore::open(&scaffold).expect("open store");
        store
            .put_session(&SessionRecord {
                id: "session-offload-prune".to_string(),
                title: "Offload Prune".to_string(),
                prompt_override: None,
                settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
                active_mission_id: None,
                created_at: 1,
                updated_at: 1,
            })
            .expect("put session");

        let first = ContextOffloadSnapshot {
            session_id: "session-offload-prune".to_string(),
            refs: vec![ContextOffloadRef {
                id: "offload-1".to_string(),
                label: "Earlier transcript".to_string(),
                summary: "Version one".to_string(),
                artifact_id: "artifact-offload-old".to_string(),
                token_estimate: 42,
                message_count: 2,
                created_at: 2,
            }],
            updated_at: 3,
        };
        store
            .put_context_offload(
                &ContextOffloadRecord::try_from(&first).expect("first offload"),
                &[ContextOffloadPayload {
                    artifact_id: "artifact-offload-old".to_string(),
                    bytes: b"old payload".to_vec(),
                }],
            )
            .expect("put first offload");

        let second = ContextOffloadSnapshot {
            session_id: "session-offload-prune".to_string(),
            refs: vec![ContextOffloadRef {
                id: "offload-2".to_string(),
                label: "Replacement".to_string(),
                summary: "Version two".to_string(),
                artifact_id: "artifact-offload-new".to_string(),
                token_estimate: 55,
                message_count: 3,
                created_at: 4,
            }],
            updated_at: 5,
        };
        store
            .put_context_offload(
                &ContextOffloadRecord::try_from(&second).expect("second offload"),
                &[ContextOffloadPayload {
                    artifact_id: "artifact-offload-new".to_string(),
                    bytes: b"new payload".to_vec(),
                }],
            )
            .expect("replace offload");

        assert!(
            store
                .get_context_offload_payload("artifact-offload-old")
                .expect("get old payload")
                .is_none()
        );
        assert_eq!(
            store
                .get_context_offload_payload("artifact-offload-new")
                .expect("get new payload")
                .expect("new payload exists")
                .bytes,
            b"new payload".to_vec()
        );
    }

    #[test]
    fn open_migrates_legacy_mission_and_job_schema() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });

        fs::create_dir_all(
            scaffold
                .stores
                .metadata_db
                .parent()
                .unwrap_or(scaffold.stores.metadata_db.as_path()),
        )
        .expect("create db dir");

        let connection =
            rusqlite::Connection::open(&scaffold.stores.metadata_db).expect("open sqlite");
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE sessions (
                     id TEXT PRIMARY KEY,
                     title TEXT NOT NULL,
                     prompt_override TEXT,
                     settings_json TEXT NOT NULL,
                     active_mission_id TEXT,
                     created_at INTEGER NOT NULL,
                     updated_at INTEGER NOT NULL,
                     FOREIGN KEY(active_mission_id) REFERENCES missions(id) ON DELETE SET NULL
                 );
                 CREATE TABLE missions (
                     id TEXT PRIMARY KEY,
                     session_id TEXT NOT NULL,
                     objective TEXT NOT NULL,
                     status TEXT NOT NULL,
                     created_at INTEGER NOT NULL,
                     updated_at INTEGER NOT NULL,
                     completed_at INTEGER,
                     FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
                 );
                 CREATE TABLE runs (
                     id TEXT PRIMARY KEY,
                     session_id TEXT NOT NULL,
                     mission_id TEXT,
                     status TEXT NOT NULL,
                     error TEXT,
                     result TEXT,
                     started_at INTEGER NOT NULL,
                     updated_at INTEGER NOT NULL,
                     finished_at INTEGER,
                     FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                     FOREIGN KEY(mission_id) REFERENCES missions(id) ON DELETE SET NULL
                 );
                 CREATE TABLE jobs (
                     id TEXT PRIMARY KEY,
                     run_id TEXT NOT NULL,
                     parent_job_id TEXT,
                     kind TEXT NOT NULL,
                     status TEXT NOT NULL,
                     input_json TEXT,
                     result_json TEXT,
                     error TEXT,
                     created_at INTEGER NOT NULL,
                     updated_at INTEGER NOT NULL,
                     started_at INTEGER,
                     finished_at INTEGER,
                     FOREIGN KEY(run_id) REFERENCES runs(id) ON DELETE CASCADE,
                     FOREIGN KEY(parent_job_id) REFERENCES jobs(id) ON DELETE SET NULL
                 );
                 INSERT INTO sessions (
                     id, title, prompt_override, settings_json, active_mission_id, created_at, updated_at
                 ) VALUES (
                     'session-1', 'Legacy mission', NULL, '{\"model\":\"gpt-5.4\"}', NULL, 1, 1
                 );
                 INSERT INTO missions (
                     id, session_id, objective, status, created_at, updated_at, completed_at
                 ) VALUES (
                     'mission-1', 'session-1', 'Carry forward existing missions', 'ready', 2, 2, NULL
                 );
                 INSERT INTO runs (
                     id, session_id, mission_id, status, error, result, started_at, updated_at, finished_at
                 ) VALUES (
                     'run-1', 'session-1', NULL, 'running', NULL, NULL, 3, 4, NULL
                 );
                 INSERT INTO jobs (
                     id, run_id, parent_job_id, kind, status, input_json, result_json, error,
                     created_at, updated_at, started_at, finished_at
                 ) VALUES (
                     'job-1',
                     'run-1',
                     NULL,
                     'maintenance',
                     'queued',
                     '{\"Maintenance\":{\"summary\":\"legacy bootstrap\"}}',
                     NULL,
                     NULL,
                     4,
                     5,
                     NULL,
                     NULL
                 );",
            )
            .expect("create legacy schema");
        drop(connection);

        let reopened = super::PersistenceStore::open(&scaffold).expect("migrate legacy schema");

        assert_eq!(
            reopened
                .get_mission("mission-1")
                .expect("get migrated mission"),
            Some(MissionRecord {
                id: "mission-1".to_string(),
                session_id: "session-1".to_string(),
                objective: "Carry forward existing missions".to_string(),
                status: "ready".to_string(),
                execution_intent: DEFAULT_MISSION_EXECUTION_INTENT.to_string(),
                schedule_json: DEFAULT_MISSION_SCHEDULE_JSON.to_string(),
                acceptance_json: DEFAULT_MISSION_ACCEPTANCE_JSON.to_string(),
                created_at: 2,
                updated_at: 2,
                completed_at: None,
            })
        );

        assert_eq!(
            reopened.get_run("run-1").expect("get migrated run"),
            Some(RunRecord {
                id: "run-1".to_string(),
                session_id: "session-1".to_string(),
                mission_id: Some(format!("{LEGACY_MISSION_PREFIX}run-1")),
                status: "running".to_string(),
                error: None,
                result: None,
                recent_steps_json: "[]".to_string(),
                evidence_refs_json: "[]".to_string(),
                pending_approvals_json: "[]".to_string(),
                provider_loop_json: "null".to_string(),
                delegate_runs_json: "[]".to_string(),
                started_at: 3,
                updated_at: 4,
                finished_at: None,
            })
        );
        assert_eq!(
            reopened.get_job("job-1").expect("get migrated job"),
            Some(JobRecord {
                id: "job-1".to_string(),
                mission_id: format!("{LEGACY_MISSION_PREFIX}run-1"),
                run_id: Some("run-1".to_string()),
                parent_job_id: None,
                kind: "maintenance".to_string(),
                status: "queued".to_string(),
                input_json: Some(
                    serde_json::to_string(&JobExecutionInput::Maintenance {
                        summary: "legacy bootstrap".to_string(),
                    })
                    .expect("serialize maintenance input"),
                ),
                result_json: None,
                error: None,
                created_at: 4,
                updated_at: 5,
                started_at: None,
                finished_at: None,
            })
        );
        assert_eq!(
            reopened
                .get_mission(&format!("{LEGACY_MISSION_PREFIX}run-1"))
                .expect("get synthesized mission"),
            Some(MissionRecord {
                id: format!("{LEGACY_MISSION_PREFIX}run-1"),
                session_id: "session-1".to_string(),
                objective: "Recovered legacy mission for run run-1".to_string(),
                status: "ready".to_string(),
                execution_intent: DEFAULT_MISSION_EXECUTION_INTENT.to_string(),
                schedule_json: DEFAULT_MISSION_SCHEDULE_JSON.to_string(),
                acceptance_json: DEFAULT_MISSION_ACCEPTANCE_JSON.to_string(),
                created_at: 3,
                updated_at: 4,
                completed_at: None,
            })
        );
    }

    #[test]
    fn file_backed_payloads_reject_unsafe_identifiers() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });
        let store = super::PersistenceStore::open(&scaffold).expect("open store");

        let transcript = TranscriptRecord {
            id: "../escape".to_string(),
            session_id: "session-1".to_string(),
            run_id: None,
            kind: "user".to_string(),
            content: "hello".to_string(),
            created_at: 1,
        };

        let artifact = ArtifactRecord {
            id: "../escape".to_string(),
            session_id: "session-1".to_string(),
            kind: "binary".to_string(),
            metadata_json: "{\"mime\":\"application/octet-stream\"}".to_string(),
            path: PathBuf::from("artifacts/escape.bin"),
            bytes: vec![1, 2, 3],
            created_at: 1,
        };

        assert!(matches!(
            store.put_transcript(&transcript),
            Err(super::StoreError::InvalidIdentifier { .. })
        ));
        assert!(matches!(
            store.put_artifact(&artifact),
            Err(super::StoreError::InvalidIdentifier { .. })
        ));
    }

    #[test]
    fn list_transcripts_for_session_orders_by_timestamp_and_id() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });
        let store = super::PersistenceStore::open(&scaffold).expect("open store");

        let session = SessionRecord {
            id: "session-1".to_string(),
            title: "Boot mission".to_string(),
            prompt_override: None,
            settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        };
        store.put_session(&session).expect("store session");

        store
            .put_transcript(&TranscriptRecord {
                id: "transcript-b".to_string(),
                session_id: session.id.clone(),
                run_id: None,
                kind: "assistant".to_string(),
                content: "second".to_string(),
                created_at: 2,
            })
            .expect("store transcript b");
        store
            .put_transcript(&TranscriptRecord {
                id: "transcript-a".to_string(),
                session_id: session.id.clone(),
                run_id: None,
                kind: "user".to_string(),
                content: "first".to_string(),
                created_at: 2,
            })
            .expect("store transcript a");
        store
            .put_transcript(&TranscriptRecord {
                id: "transcript-c".to_string(),
                session_id: session.id.clone(),
                run_id: None,
                kind: "tool".to_string(),
                content: "third".to_string(),
                created_at: 3,
            })
            .expect("store transcript c");

        let history = store
            .list_transcripts_for_session(&session.id)
            .expect("list transcripts");

        assert_eq!(
            history
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["transcript-a", "transcript-b", "transcript-c"]
        );
    }

    #[test]
    fn list_execution_records_orders_sessions_missions_jobs_and_runs_stably() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });
        let store = super::PersistenceStore::open(&scaffold).expect("open store");

        let session_b = SessionRecord {
            id: "session-b".to_string(),
            title: "Second session".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            active_mission_id: None,
            created_at: 2,
            updated_at: 2,
        };
        let session_a = SessionRecord {
            id: "session-a".to_string(),
            title: "First session".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            active_mission_id: None,
            created_at: 2,
            updated_at: 2,
        };
        store.put_session(&session_b).expect("put session b");
        store.put_session(&session_a).expect("put session a");

        let mission_b = MissionRecord {
            id: "mission-b".to_string(),
            session_id: session_b.id.clone(),
            objective: "Second mission".to_string(),
            status: "ready".to_string(),
            execution_intent: "autonomous".to_string(),
            schedule_json: DEFAULT_MISSION_SCHEDULE_JSON.to_string(),
            acceptance_json: DEFAULT_MISSION_ACCEPTANCE_JSON.to_string(),
            created_at: 3,
            updated_at: 3,
            completed_at: None,
        };
        let mission_a = MissionRecord {
            id: "mission-a".to_string(),
            session_id: session_a.id.clone(),
            objective: "First mission".to_string(),
            status: "ready".to_string(),
            execution_intent: "autonomous".to_string(),
            schedule_json: DEFAULT_MISSION_SCHEDULE_JSON.to_string(),
            acceptance_json: DEFAULT_MISSION_ACCEPTANCE_JSON.to_string(),
            created_at: 3,
            updated_at: 3,
            completed_at: None,
        };
        store.put_mission(&mission_b).expect("put mission b");
        store.put_mission(&mission_a).expect("put mission a");

        let run_b = RunRecord {
            id: "run-b".to_string(),
            session_id: session_b.id.clone(),
            mission_id: Some(mission_b.id.clone()),
            status: "queued".to_string(),
            error: None,
            result: None,
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 5,
            updated_at: 5,
            finished_at: None,
        };
        let run_a = RunRecord {
            id: "run-a".to_string(),
            session_id: session_a.id.clone(),
            mission_id: Some(mission_a.id.clone()),
            status: "queued".to_string(),
            error: None,
            result: None,
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 5,
            updated_at: 5,
            finished_at: None,
        };
        store.put_run(&run_b).expect("put run b");
        store.put_run(&run_a).expect("put run a");

        let job_b = JobRecord {
            id: "job-b".to_string(),
            mission_id: mission_b.id.clone(),
            run_id: Some(run_b.id.clone()),
            parent_job_id: None,
            kind: "mission_turn".to_string(),
            status: "queued".to_string(),
            input_json: Some(
                serde_json::to_string(&JobExecutionInput::MissionTurn {
                    mission_id: mission_b.id.clone(),
                    goal: "second".to_string(),
                })
                .expect("serialize input b"),
            ),
            result_json: None,
            error: None,
            created_at: 4,
            updated_at: 4,
            started_at: None,
            finished_at: None,
        };
        let job_a = JobRecord {
            id: "job-a".to_string(),
            mission_id: mission_a.id.clone(),
            run_id: Some(run_a.id.clone()),
            parent_job_id: None,
            kind: "mission_turn".to_string(),
            status: "queued".to_string(),
            input_json: Some(
                serde_json::to_string(&JobExecutionInput::MissionTurn {
                    mission_id: mission_a.id.clone(),
                    goal: "first".to_string(),
                })
                .expect("serialize input a"),
            ),
            result_json: None,
            error: None,
            created_at: 4,
            updated_at: 4,
            started_at: None,
            finished_at: None,
        };
        store.put_job(&job_b).expect("put job b");
        store.put_job(&job_a).expect("put job a");

        let sessions = store.list_sessions().expect("list sessions");
        let missions = store.list_missions().expect("list missions");
        let jobs = store.list_jobs().expect("list jobs");
        let runs = store.list_runs().expect("list runs");

        assert_eq!(
            sessions
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["session-a", "session-b"]
        );
        assert_eq!(
            missions
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["mission-a", "mission-b"]
        );
        assert_eq!(
            jobs.iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["job-a", "job-b"]
        );
        assert_eq!(
            runs.iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["run-a", "run-b"]
        );
    }

    #[test]
    fn load_execution_state_returns_one_typed_snapshot_for_scheduler_inputs() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });
        let store = super::PersistenceStore::open(&scaffold).expect("open store");

        let session = SessionRecord {
            id: "session-1".to_string(),
            title: "Execution session".to_string(),
            prompt_override: None,
            settings_json: "{}".to_string(),
            active_mission_id: None,
            created_at: 1,
            updated_at: 2,
        };
        let mission = MissionRecord {
            id: "mission-1".to_string(),
            session_id: session.id.clone(),
            objective: "Tick the mission loop".to_string(),
            status: "ready".to_string(),
            execution_intent: "autonomous".to_string(),
            schedule_json: DEFAULT_MISSION_SCHEDULE_JSON.to_string(),
            acceptance_json: DEFAULT_MISSION_ACCEPTANCE_JSON.to_string(),
            created_at: 2,
            updated_at: 3,
            completed_at: None,
        };
        let job = JobRecord {
            id: "job-1".to_string(),
            mission_id: mission.id.clone(),
            run_id: None,
            parent_job_id: None,
            kind: "mission_turn".to_string(),
            status: "queued".to_string(),
            input_json: Some(
                serde_json::to_string(&JobExecutionInput::MissionTurn {
                    mission_id: mission.id.clone(),
                    goal: "advance".to_string(),
                })
                .expect("serialize mission turn"),
            ),
            result_json: None,
            error: None,
            created_at: 4,
            updated_at: 4,
            started_at: None,
            finished_at: None,
        };
        let run = RunRecord {
            id: "run-1".to_string(),
            session_id: session.id.clone(),
            mission_id: Some(mission.id.clone()),
            status: "queued".to_string(),
            error: None,
            result: None,
            recent_steps_json: "[]".to_string(),
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
            provider_loop_json: "null".to_string(),
            delegate_runs_json: "[]".to_string(),
            started_at: 5,
            updated_at: 5,
            finished_at: None,
        };

        store.put_session(&session).expect("put session");
        store.put_mission(&mission).expect("put mission");
        store.put_job(&job).expect("put job");
        store.put_run(&run).expect("put run");

        let snapshot = store.load_execution_state().expect("load execution state");

        assert_eq!(snapshot.sessions, vec![session]);
        assert_eq!(snapshot.missions, vec![mission]);
        assert_eq!(snapshot.jobs, vec![job]);
        assert_eq!(snapshot.runs, vec![run]);
    }

    #[test]
    fn open_removes_orphan_payload_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });

        fs::create_dir_all(&scaffold.stores.transcripts_dir).expect("create transcript dir");
        fs::create_dir_all(&scaffold.stores.artifacts_dir).expect("create artifact dir");

        let orphan_transcript = scaffold.stores.transcripts_dir.join("orphan.txt");
        let orphan_artifact = scaffold.stores.artifacts_dir.join("orphan.bin");
        fs::write(&orphan_transcript, "orphan transcript").expect("write transcript");
        fs::write(&orphan_artifact, "orphan artifact").expect("write artifact");

        let _store = super::PersistenceStore::open(&scaffold).expect("open store");

        assert!(!orphan_transcript.exists());
        assert!(!orphan_artifact.exists());
    }

    #[test]
    fn open_removes_payloads_that_do_not_match_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });
        let store = super::PersistenceStore::open(&scaffold).expect("open store");

        let session = SessionRecord {
            id: "session-1".to_string(),
            title: "Store payloads".to_string(),
            prompt_override: None,
            settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        };
        store.put_session(&session).expect("store session");

        let transcript = TranscriptRecord {
            id: "transcript-1".to_string(),
            session_id: session.id.clone(),
            run_id: None,
            kind: "user".to_string(),
            content: "original transcript".to_string(),
            created_at: 1,
        };
        store.put_transcript(&transcript).expect("store transcript");

        let artifact = ArtifactRecord {
            id: "artifact-1".to_string(),
            session_id: session.id.clone(),
            kind: "report".to_string(),
            metadata_json: "{\"source\":\"test\"}".to_string(),
            path: PathBuf::from("artifacts/artifact-1.bin"),
            bytes: b"original artifact".to_vec(),
            created_at: 1,
        };
        store.put_artifact(&artifact).expect("store artifact");
        drop(store);

        fs::write(
            scaffold.stores.transcripts_dir.join("transcript-1.txt"),
            "tampered transcript",
        )
        .expect("tamper transcript");
        fs::write(
            scaffold.stores.artifacts_dir.join("artifact-1.bin"),
            b"tampered artifact",
        )
        .expect("tamper artifact");

        let _store = super::PersistenceStore::open(&scaffold).expect("reopen store");

        assert!(
            !scaffold
                .stores
                .transcripts_dir
                .join("transcript-1.txt")
                .exists()
        );
        assert!(
            !scaffold
                .stores
                .artifacts_dir
                .join("artifact-1.bin")
                .exists()
        );
    }

    #[test]
    fn open_rejects_incompatible_existing_schema() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });

        fs::create_dir_all(
            scaffold
                .stores
                .metadata_db
                .parent()
                .unwrap_or(scaffold.stores.metadata_db.as_path()),
        )
        .expect("create db dir");

        let connection =
            rusqlite::Connection::open(&scaffold.stores.metadata_db).expect("open sqlite");
        connection
            .execute_batch(
                "CREATE TABLE sessions (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    prompt_override TEXT,
                    active_mission_id TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );",
            )
            .expect("create legacy schema");
        drop(connection);

        assert!(matches!(
            super::PersistenceStore::open(&scaffold),
            Err(super::StoreError::SchemaMismatch { .. })
        ));
    }

    #[test]
    fn failed_metadata_updates_restore_previous_payloads() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });
        let store = super::PersistenceStore::open(&scaffold).expect("open store");

        let session = SessionRecord {
            id: "session-1".to_string(),
            title: "Store payloads".to_string(),
            prompt_override: None,
            settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        };
        store.put_session(&session).expect("store session");

        let transcript = TranscriptRecord {
            id: "transcript-1".to_string(),
            session_id: session.id.clone(),
            run_id: None,
            kind: "user".to_string(),
            content: "original".to_string(),
            created_at: 1,
        };
        store.put_transcript(&transcript).expect("store transcript");

        let artifact = ArtifactRecord {
            id: "artifact-1".to_string(),
            session_id: session.id.clone(),
            kind: "report".to_string(),
            metadata_json: "{\"source\":\"test\"}".to_string(),
            path: PathBuf::from("artifacts/artifact-1.bin"),
            bytes: b"original".to_vec(),
            created_at: 1,
        };
        store.put_artifact(&artifact).expect("store artifact");

        let broken_transcript = TranscriptRecord {
            session_id: "missing-session".to_string(),
            content: "replacement".to_string(),
            ..transcript.clone()
        };
        let broken_artifact = ArtifactRecord {
            session_id: "missing-session".to_string(),
            bytes: b"replacement".to_vec(),
            ..artifact.clone()
        };

        assert!(store.put_transcript(&broken_transcript).is_err());
        assert!(store.put_artifact(&broken_artifact).is_err());

        assert_eq!(
            store
                .get_transcript(&transcript.id)
                .expect("get transcript after failure"),
            Some(transcript)
        );
        assert_eq!(
            store
                .get_artifact(&artifact.id)
                .expect("get artifact after failure"),
            Some(artifact)
        );
    }

    #[test]
    fn reads_fail_when_payloads_no_longer_match_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });
        let store = super::PersistenceStore::open(&scaffold).expect("open store");

        let session = SessionRecord {
            id: "session-1".to_string(),
            title: "Store payloads".to_string(),
            prompt_override: None,
            settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        };
        store.put_session(&session).expect("store session");

        let transcript = TranscriptRecord {
            id: "transcript-1".to_string(),
            session_id: session.id.clone(),
            run_id: None,
            kind: "user".to_string(),
            content: "original transcript".to_string(),
            created_at: 1,
        };
        store.put_transcript(&transcript).expect("store transcript");

        let artifact = ArtifactRecord {
            id: "artifact-1".to_string(),
            session_id: session.id.clone(),
            kind: "report".to_string(),
            metadata_json: "{\"source\":\"test\"}".to_string(),
            path: PathBuf::from("artifacts/artifact-1.bin"),
            bytes: b"original artifact".to_vec(),
            created_at: 1,
        };
        store.put_artifact(&artifact).expect("store artifact");

        fs::write(
            scaffold.stores.transcripts_dir.join("transcript-1.txt"),
            "tampered transcript",
        )
        .expect("tamper transcript");
        fs::write(
            scaffold.stores.artifacts_dir.join("artifact-1.bin"),
            b"tampered artifact",
        )
        .expect("tamper artifact");

        assert!(matches!(
            store.get_transcript(&transcript.id),
            Err(super::StoreError::IntegrityMismatch { .. })
        ));
        assert!(matches!(
            store.get_artifact(&artifact.id),
            Err(super::StoreError::IntegrityMismatch { .. })
        ));
    }

    #[test]
    fn open_restores_matching_backups_before_pruning_corrupt_payloads() {
        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold = PersistenceScaffold::from_config(crate::AppConfig {
            data_dir: temp.path().join("state-root"),
            ..crate::AppConfig::default()
        });
        let store = super::PersistenceStore::open(&scaffold).expect("open store");

        let session = SessionRecord {
            id: "session-1".to_string(),
            title: "Store payloads".to_string(),
            prompt_override: None,
            settings_json: "{\"model\":\"gpt-5.4\"}".to_string(),
            active_mission_id: None,
            created_at: 1,
            updated_at: 1,
        };
        store.put_session(&session).expect("store session");

        let transcript = TranscriptRecord {
            id: "transcript-1".to_string(),
            session_id: session.id.clone(),
            run_id: None,
            kind: "user".to_string(),
            content: "original transcript".to_string(),
            created_at: 1,
        };
        store.put_transcript(&transcript).expect("store transcript");

        let artifact = ArtifactRecord {
            id: "artifact-1".to_string(),
            session_id: session.id.clone(),
            kind: "report".to_string(),
            metadata_json: "{\"source\":\"test\"}".to_string(),
            path: PathBuf::from("artifacts/artifact-1.bin"),
            bytes: b"original artifact".to_vec(),
            created_at: 1,
        };
        store.put_artifact(&artifact).expect("store artifact");
        drop(store);

        let transcript_path = scaffold.stores.transcripts_dir.join("transcript-1.txt");
        let transcript_backup = scaffold.stores.transcripts_dir.join("transcript-1.txt.bak");
        fs::rename(&transcript_path, &transcript_backup).expect("backup transcript");
        fs::write(&transcript_path, "tampered transcript").expect("write bad transcript");

        let artifact_path = scaffold.stores.artifacts_dir.join("artifact-1.bin");
        let artifact_backup = scaffold.stores.artifacts_dir.join("artifact-1.bin.bak");
        fs::rename(&artifact_path, &artifact_backup).expect("backup artifact");
        fs::write(&artifact_path, b"tampered artifact").expect("write bad artifact");

        let reopened = super::PersistenceStore::open(&scaffold).expect("reopen store");

        assert_eq!(
            reopened
                .get_transcript(&transcript.id)
                .expect("get restored transcript"),
            Some(transcript)
        );
        assert_eq!(
            reopened
                .get_artifact(&artifact.id)
                .expect("get restored artifact"),
            Some(artifact)
        );
        assert!(!transcript_backup.exists());
        assert!(!artifact_backup.exists());
    }
}
