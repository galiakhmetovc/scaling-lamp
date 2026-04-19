use crate::PersistenceScaffold;
use crate::config::AppConfig;
use crate::records::{
    ArtifactRecord, JobRecord, MissionRecord, RunRecord, SessionRecord, TranscriptRecord,
};
use crate::repository::{
    ArtifactRepository, JobRepository, MissionRepository, RunRepository, SessionRepository,
    TranscriptRepository,
};
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
                id, session_id, mission_id, status, error, result, evidence_refs_json,
                pending_approvals_json, delegate_runs_json, started_at, updated_at, finished_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
                session_id = excluded.session_id,
                mission_id = excluded.mission_id,
                status = excluded.status,
                error = excluded.error,
                result = excluded.result,
                evidence_refs_json = excluded.evidence_refs_json,
                pending_approvals_json = excluded.pending_approvals_json,
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
                record.evidence_refs_json,
                record.pending_approvals_json,
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
                "SELECT id, session_id, mission_id, status, error, result, evidence_refs_json,
                        pending_approvals_json, delegate_runs_json, started_at, updated_at, finished_at
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
                        evidence_refs_json: row.get(6)?,
                        pending_approvals_json: row.get(7)?,
                        delegate_runs_json: row.get(8)?,
                        started_at: row.get(9)?,
                        updated_at: row.get(10)?,
                        finished_at: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn list_runs(&self) -> Result<Vec<RunRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, session_id, mission_id, status, error, result, evidence_refs_json,
                    pending_approvals_json, delegate_runs_json, started_at, updated_at, finished_at
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
                evidence_refs_json: row.get(6)?,
                pending_approvals_json: row.get(7)?,
                delegate_runs_json: row.get(8)?,
                started_at: row.get(9)?,
                updated_at: row.get(10)?,
                finished_at: row.get(11)?,
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
    create_directory(
        layout
            .metadata_db
            .parent()
            .unwrap_or(layout.metadata_db.as_path()),
    )?;
    create_directory(&layout.runs_dir)?;
    create_directory(&layout.transcripts_dir)?;
    create_directory(&layout.artifacts_dir)?;
    Ok(())
}

fn create_directory(path: &Path) -> Result<(), StoreError> {
    fs::create_dir_all(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn bootstrap_schema(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;

         CREATE TABLE IF NOT EXISTS sessions (
             id TEXT PRIMARY KEY,
             title TEXT NOT NULL,
             prompt_override TEXT,
             settings_json TEXT NOT NULL,
             active_mission_id TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             FOREIGN KEY(active_mission_id) REFERENCES missions(id) ON DELETE SET NULL
         );

         CREATE TABLE IF NOT EXISTS missions (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             objective TEXT NOT NULL,
             status TEXT NOT NULL,
             execution_intent TEXT NOT NULL,
             schedule_json TEXT NOT NULL,
             acceptance_json TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             completed_at INTEGER,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
         );

         CREATE TABLE IF NOT EXISTS runs (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             mission_id TEXT,
             status TEXT NOT NULL,
             error TEXT,
             result TEXT,
             evidence_refs_json TEXT NOT NULL,
             pending_approvals_json TEXT NOT NULL,
             delegate_runs_json TEXT NOT NULL,
             started_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             finished_at INTEGER,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
             FOREIGN KEY(mission_id) REFERENCES missions(id) ON DELETE SET NULL
         );

         CREATE TABLE IF NOT EXISTS jobs (
             id TEXT PRIMARY KEY,
             mission_id TEXT NOT NULL,
             run_id TEXT,
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
             FOREIGN KEY(mission_id) REFERENCES missions(id) ON DELETE CASCADE,
             FOREIGN KEY(run_id) REFERENCES runs(id) ON DELETE SET NULL,
             FOREIGN KEY(parent_job_id) REFERENCES jobs(id) ON DELETE SET NULL
         );

         CREATE TABLE IF NOT EXISTS transcripts (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             run_id TEXT,
             kind TEXT NOT NULL,
             storage_key TEXT NOT NULL,
             byte_len INTEGER NOT NULL,
             sha256 TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
             FOREIGN KEY(run_id) REFERENCES runs(id) ON DELETE SET NULL
         );

         CREATE TABLE IF NOT EXISTS artifacts (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             kind TEXT NOT NULL,
             path TEXT NOT NULL,
             metadata_json TEXT NOT NULL,
             byte_len INTEGER NOT NULL,
             sha256 TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
         );",
    )?;

    migrate_schema(connection)?;

    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_missions_session_id ON missions(session_id);
         CREATE INDEX IF NOT EXISTS idx_runs_session_id ON runs(session_id);
         CREATE INDEX IF NOT EXISTS idx_runs_mission_id ON runs(mission_id);
         CREATE INDEX IF NOT EXISTS idx_jobs_mission_id ON jobs(mission_id);
         CREATE INDEX IF NOT EXISTS idx_jobs_run_id ON jobs(run_id);
         CREATE INDEX IF NOT EXISTS idx_jobs_parent_job_id ON jobs(parent_job_id);
         CREATE INDEX IF NOT EXISTS idx_transcripts_session_id ON transcripts(session_id);
         CREATE INDEX IF NOT EXISTS idx_transcripts_run_id ON transcripts(run_id);
         CREATE INDEX IF NOT EXISTS idx_artifacts_session_id ON artifacts(session_id);",
    )?;

    Ok(())
}

fn validate_schema(connection: &Connection) -> Result<(), StoreError> {
    validate_column(connection, "missions", "execution_intent", true)?;
    validate_column(connection, "missions", "schedule_json", true)?;
    validate_column(connection, "missions", "acceptance_json", true)?;
    validate_column(connection, "jobs", "mission_id", true)?;
    validate_column(connection, "sessions", "settings_json", true)?;
    validate_column(connection, "runs", "evidence_refs_json", true)?;
    validate_column(connection, "runs", "pending_approvals_json", true)?;
    validate_column(connection, "runs", "delegate_runs_json", true)?;
    validate_column(connection, "runs", "result", false)?;
    validate_column(connection, "transcripts", "sha256", true)?;
    validate_column(connection, "artifacts", "session_id", true)?;
    validate_column(connection, "artifacts", "metadata_json", true)?;
    validate_column(connection, "artifacts", "sha256", true)?;
    validate_foreign_key(connection, "artifacts", "session_id", "sessions", "CASCADE")?;
    validate_foreign_key(connection, "jobs", "mission_id", "missions", "CASCADE")?;
    validate_foreign_key(
        connection,
        "sessions",
        "active_mission_id",
        "missions",
        "SET NULL",
    )?;
    Ok(())
}

fn migrate_schema(connection: &Connection) -> Result<(), StoreError> {
    add_column_if_missing(
        connection,
        "missions",
        "execution_intent",
        "TEXT NOT NULL DEFAULT 'autonomous'",
    )?;
    add_column_if_missing(
        connection,
        "missions",
        "schedule_json",
        "TEXT NOT NULL DEFAULT '{\"not_before\":null,\"interval_seconds\":null}'",
    )?;
    add_column_if_missing(
        connection,
        "runs",
        "evidence_refs_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    add_column_if_missing(
        connection,
        "runs",
        "pending_approvals_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    add_column_if_missing(
        connection,
        "runs",
        "delegate_runs_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    add_column_if_missing(
        connection,
        "missions",
        "acceptance_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    migrate_jobs_table(connection)?;
    Ok(())
}

fn validate_identifier(id: &str) -> Result<(), StoreError> {
    if id.is_empty() {
        return Err(StoreError::InvalidIdentifier {
            id: id.to_string(),
            reason: "must not be empty",
        });
    }

    if id == "." || id == ".." || id.contains('/') || id.contains('\\') {
        return Err(StoreError::InvalidIdentifier {
            id: id.to_string(),
            reason: "must not contain path traversal or separators",
        });
    }

    if !id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(StoreError::InvalidIdentifier {
            id: id.to_string(),
            reason: "must use only ascii letters, digits, hyphen, or underscore",
        });
    }

    Ok(())
}

fn add_column_if_missing(
    connection: &Connection,
    table: &'static str,
    column: &'static str,
    definition: &'static str,
) -> Result<(), StoreError> {
    if table_has_column(connection, table, column)? {
        return Ok(());
    }

    connection.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {definition};"
    ))?;
    Ok(())
}

fn migrate_jobs_table(connection: &Connection) -> Result<(), StoreError> {
    if !table_exists(connection, "jobs")? {
        return Ok(());
    }

    if table_has_column(connection, "jobs", "mission_id")?
        && foreign_key_exists(connection, "jobs", "mission_id", "missions", "CASCADE")?
        && foreign_key_exists(connection, "jobs", "run_id", "runs", "SET NULL")?
    {
        return Ok(());
    }

    connection.execute_batch(&format!(
        "PRAGMA foreign_keys = OFF;
         BEGIN IMMEDIATE;
         ALTER TABLE jobs RENAME TO jobs_legacy;
         INSERT OR IGNORE INTO missions (
             id, session_id, objective, status, execution_intent, schedule_json, acceptance_json,
             created_at, updated_at, completed_at
         )
         SELECT DISTINCT
             '{LEGACY_MISSION_PREFIX}' || runs.id,
             runs.session_id,
             'Recovered legacy mission for run ' || runs.id,
             CASE
                 WHEN runs.finished_at IS NULL THEN 'ready'
                 ELSE 'completed'
             END,
             '{DEFAULT_MISSION_EXECUTION_INTENT}',
             '{DEFAULT_MISSION_SCHEDULE_JSON}',
             '{DEFAULT_MISSION_ACCEPTANCE_JSON}',
             runs.started_at,
             runs.updated_at,
             runs.finished_at
         FROM jobs_legacy
         INNER JOIN runs ON runs.id = jobs_legacy.run_id
         WHERE runs.mission_id IS NULL;
         UPDATE runs
         SET mission_id = '{LEGACY_MISSION_PREFIX}' || id
         WHERE mission_id IS NULL
           AND EXISTS (
               SELECT 1
               FROM jobs_legacy
               WHERE jobs_legacy.run_id = runs.id
           );
         CREATE TABLE jobs (
             id TEXT PRIMARY KEY,
             mission_id TEXT NOT NULL,
             run_id TEXT,
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
             FOREIGN KEY(mission_id) REFERENCES missions(id) ON DELETE CASCADE,
             FOREIGN KEY(run_id) REFERENCES runs(id) ON DELETE SET NULL,
             FOREIGN KEY(parent_job_id) REFERENCES jobs(id) ON DELETE SET NULL
         );
         INSERT INTO jobs (
             id, mission_id, run_id, parent_job_id, kind, status, input_json, result_json, error,
             created_at, updated_at, started_at, finished_at
         )
         SELECT
             jobs_legacy.id,
             COALESCE(runs.mission_id, '{LEGACY_MISSION_PREFIX}' || runs.id),
             jobs_legacy.run_id,
             jobs_legacy.parent_job_id,
             jobs_legacy.kind,
             jobs_legacy.status,
             jobs_legacy.input_json,
             jobs_legacy.result_json,
             jobs_legacy.error,
             jobs_legacy.created_at,
             jobs_legacy.updated_at,
             jobs_legacy.started_at,
             jobs_legacy.finished_at
         FROM jobs_legacy
         INNER JOIN runs ON runs.id = jobs_legacy.run_id;
         DROP TABLE jobs_legacy;
         COMMIT;
         PRAGMA foreign_keys = ON;"
    ))?;

    Ok(())
}

fn write_temp_payload(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    fs::write(path, bytes).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn persist_payload_with_commit<F>(path: &Path, bytes: &[u8], commit: F) -> Result<(), StoreError>
where
    F: FnOnce() -> Result<(), StoreError>,
{
    let temp_path = path.with_extension("tmp");
    let backup_path = backup_path(path);
    let had_existing = path.exists();

    write_temp_payload(&temp_path, bytes)?;

    if had_existing {
        fs::rename(path, &backup_path).map_err(|source| StoreError::Io {
            path: backup_path.clone(),
            source,
        })?;
    }

    fs::rename(&temp_path, path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    match commit() {
        Ok(()) => {
            if had_existing && backup_path.exists() {
                fs::remove_file(&backup_path).map_err(|source| StoreError::Io {
                    path: backup_path,
                    source,
                })?;
            }
            Ok(())
        }
        Err(error) => {
            if had_existing {
                let _ = fs::remove_file(path);
                if backup_path.exists() {
                    let _ = fs::rename(&backup_path, path);
                }
            } else {
                let _ = fs::remove_file(path);
            }
            Err(error)
        }
    }
}

fn validate_column(
    connection: &Connection,
    table: &'static str,
    column: &'static str,
    required_not_null: bool,
) -> Result<(), StoreError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = statement.query([])?;

    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        let not_null: i64 = row.get(3)?;

        if name == column {
            if required_not_null && not_null != 1 {
                return Err(StoreError::SchemaMismatch {
                    table,
                    reason: format!("{column} must be NOT NULL"),
                });
            }
            return Ok(());
        }
    }

    Err(StoreError::SchemaMismatch {
        table,
        reason: format!("missing required column {column}"),
    })
}

fn table_exists(connection: &Connection, table: &'static str) -> Result<bool, StoreError> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |_row| Ok(()),
        )
        .optional()
        .map(|row| row.is_some())
        .map_err(StoreError::Sqlite)
}

fn table_has_column(
    connection: &Connection,
    table: &'static str,
    column: &'static str,
) -> Result<bool, StoreError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = statement.query([])?;

    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }

    Ok(false)
}

fn foreign_key_exists(
    connection: &Connection,
    table: &'static str,
    from_column: &'static str,
    target_table: &'static str,
    on_delete: &'static str,
) -> Result<bool, StoreError> {
    let mut statement = connection.prepare(&format!("PRAGMA foreign_key_list({table})"))?;
    let mut rows = statement.query([])?;

    while let Some(row) = rows.next()? {
        let fk_table: String = row.get(2)?;
        let fk_from: String = row.get(3)?;
        let fk_on_delete: String = row.get(6)?;

        if fk_table == target_table && fk_from == from_column && fk_on_delete == on_delete {
            return Ok(true);
        }
    }

    Ok(false)
}

fn validate_foreign_key(
    connection: &Connection,
    table: &'static str,
    from_column: &'static str,
    target_table: &'static str,
    on_delete: &'static str,
) -> Result<(), StoreError> {
    if foreign_key_exists(connection, table, from_column, target_table, on_delete)? {
        return Ok(());
    }

    Err(StoreError::SchemaMismatch {
        table,
        reason: format!(
            "missing foreign key for {from_column} -> {target_table} with ON DELETE {on_delete}"
        ),
    })
}

fn reconcile_directory(
    connection: &Connection,
    query: &str,
    directory: &Path,
) -> Result<(), StoreError> {
    let mut statement = connection.prepare(query)?;
    let mut rows = statement.query([])?;
    let mut expected = std::collections::BTreeMap::new();

    while let Some(row) = rows.next()? {
        let stored_path: String = row.get(0)?;
        let byte_len: i64 = row.get(1)?;
        let sha256: String = row.get(2)?;
        let file_name = PathBuf::from(stored_path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned);

        if let Some(file_name) = file_name {
            expected.insert(file_name, (byte_len as u64, sha256));
        }
    }

    if !directory.exists() {
        return Ok(());
    }

    restore_backups(directory, &expected)?;

    for entry in fs::read_dir(directory).map_err(|source| StoreError::Io {
        path: directory.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| StoreError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".bak"))
        {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        let should_remove = match expected.get(file_name) {
            Some((expected_len, expected_sha256)) => {
                let (actual_len, actual_sha256) = payload_fingerprint(&path)?;
                actual_len != *expected_len || actual_sha256 != *expected_sha256
            }
            None => true,
        };

        if should_remove {
            fs::remove_file(&path).map_err(|source| StoreError::Io {
                path: path.clone(),
                source,
            })?;
        }
    }

    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.bak", path.to_string_lossy()))
}

fn payload_fingerprint(path: &Path) -> Result<(u64, String), StoreError> {
    let bytes = read_binary_payload(path)?;
    Ok((bytes.len() as u64, sha256_hex(&bytes)))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);

    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }

    encoded
}

fn validate_integrity(
    path: &Path,
    actual_len: u64,
    bytes: &[u8],
    expected_len: u64,
    expected_sha256: &str,
) -> Result<(), StoreError> {
    let actual_sha256 = sha256_hex(bytes);

    if actual_len != expected_len || actual_sha256 != expected_sha256 {
        return Err(StoreError::IntegrityMismatch {
            path: path.to_path_buf(),
        });
    }

    Ok(())
}

fn restore_backups(
    directory: &Path,
    expected: &std::collections::BTreeMap<String, (u64, String)>,
) -> Result<(), StoreError> {
    for entry in fs::read_dir(directory).map_err(|source| StoreError::Io {
        path: directory.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| StoreError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(original_name) = file_name.strip_suffix(".bak") else {
            continue;
        };
        let Some((expected_len, expected_sha256)) = expected.get(original_name) else {
            fs::remove_file(&path).map_err(|source| StoreError::Io {
                path: path.clone(),
                source,
            })?;
            continue;
        };

        let original_path = directory.join(original_name);
        let backup_matches = payload_fingerprint(&path)
            .map(|(len, sha256)| len == *expected_len && sha256 == *expected_sha256)
            .unwrap_or(false);

        if original_path.exists() {
            let original_matches = payload_fingerprint(&original_path)
                .map(|(len, sha256)| len == *expected_len && sha256 == *expected_sha256)
                .unwrap_or(false);

            if original_matches {
                fs::remove_file(&path).map_err(|source| StoreError::Io {
                    path: path.clone(),
                    source,
                })?;
                continue;
            }

            fs::remove_file(&original_path).map_err(|source| StoreError::Io {
                path: original_path.clone(),
                source,
            })?;
        }

        if backup_matches {
            fs::rename(&path, &original_path).map_err(|source| StoreError::Io {
                path: original_path,
                source,
            })?;
        } else {
            fs::remove_file(&path).map_err(|source| StoreError::Io {
                path: path.clone(),
                source,
            })?;
        }
    }

    Ok(())
}

fn read_string_payload(path: &Path) -> Result<String, StoreError> {
    fs::read_to_string(path).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => StoreError::MissingPayload {
            path: path.to_path_buf(),
        },
        _ => StoreError::Io {
            path: path.to_path_buf(),
            source,
        },
    })
}

fn read_binary_payload(path: &Path) -> Result<Vec<u8>, StoreError> {
    fs::read(path).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => StoreError::MissingPayload {
            path: path.to_path_buf(),
        },
        _ => StoreError::Io {
            path: path.to_path_buf(),
            source,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_MISSION_ACCEPTANCE_JSON, DEFAULT_MISSION_EXECUTION_INTENT,
        DEFAULT_MISSION_SCHEDULE_JSON, LEGACY_MISSION_PREFIX,
    };
    use crate::{
        ArtifactRecord, ArtifactRepository, JobRecord, JobRepository, MissionRecord,
        MissionRepository, PersistenceScaffold, RunRecord, RunRepository, SessionRecord,
        SessionRepository, TranscriptRecord, TranscriptRepository,
    };
    use agent_runtime::mission::JobExecutionInput;
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
            evidence_refs_json: "[\"bundle:bootstrap\"]".to_string(),
            pending_approvals_json: "[]".to_string(),
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
            reopened.get_artifact(&artifact.id).expect("get artifact"),
            Some(artifact)
        );
        assert!(scaffold.stores.metadata_db.exists());
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
                evidence_refs_json: "[]".to_string(),
                pending_approvals_json: "[]".to_string(),
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
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
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
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
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
            evidence_refs_json: "[]".to_string(),
            pending_approvals_json: "[]".to_string(),
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
