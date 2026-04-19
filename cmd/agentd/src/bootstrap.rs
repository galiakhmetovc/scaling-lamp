use crate::{cli, execution};
use agent_persistence::{
    AppConfig, ConfigError, PersistenceScaffold, PersistenceStore, RecordConversionError,
    RunRecord, RunRepository, SessionRepository, StoreError, TranscriptRepository, recovery,
};
use agent_runtime::RuntimeScaffold;
use agent_runtime::provider::{ProviderBuildError, ProviderDriver, ProviderError, build_driver};
use agent_runtime::run::{RunEngine, RunSnapshot, RunTransitionError};
use agent_runtime::scheduler::MissionVerificationSummary;
use agent_runtime::session::TranscriptEntry;
use agent_runtime::tool::ToolCall;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

#[derive(Debug)]
pub enum BootstrapError {
    Config(ConfigError),
    Clock(SystemTimeError),
    InvalidPath {
        path: PathBuf,
        reason: &'static str,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Stream(std::io::Error),
    MissingRecord {
        kind: &'static str,
        id: String,
    },
    ProviderBuild(ProviderBuildError),
    ProviderRequest(ProviderError),
    Execution(execution::ExecutionError),
    Recovery(recovery::RecoveryError),
    RecordConversion(RecordConversionError),
    RunTransition(RunTransitionError),
    Sqlite(rusqlite::Error),
    Store(StoreError),
    Usage {
        reason: String,
    },
}

#[derive(Debug)]
pub struct App {
    pub config: AppConfig,
    pub persistence: PersistenceScaffold,
    pub runtime: RuntimeScaffold,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTranscriptView {
    pub session_id: String,
    pub entries: Vec<SessionTranscriptLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTranscriptLine {
    pub role: String,
    pub content: String,
    pub run_id: Option<String>,
    pub created_at: i64,
}

impl App {
    fn execution_service(&self) -> execution::ExecutionService {
        execution::ExecutionService::new(
            self.config.permissions.clone(),
            self.runtime.workspace.clone(),
        )
    }

    pub fn run(&self) -> Result<(), BootstrapError> {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut input = stdin.lock();
        let mut output = stdout.lock();
        self.run_with_io(std::env::args().skip(1), &mut input, &mut output)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn run_with_args<I, S>(&self, args: I) -> Result<String, BootstrapError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        cli::execute(self, args)
    }

    pub fn run_with_io<I, S, R, W>(
        &self,
        args: I,
        input: &mut R,
        output: &mut W,
    ) -> Result<(), BootstrapError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
        R: BufRead,
        W: Write,
    {
        cli::execute_with_io(self, args, input, output)
    }

    pub fn store(&self) -> Result<PersistenceStore, BootstrapError> {
        PersistenceStore::open(&self.persistence).map_err(BootstrapError::Store)
    }

    pub fn provider_driver(&self) -> Result<Box<dyn ProviderDriver>, BootstrapError> {
        build_driver(&self.config.provider).map_err(BootstrapError::ProviderBuild)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn session_transcript(
        &self,
        session_id: &str,
    ) -> Result<SessionTranscriptView, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }

        let entries = store
            .list_transcripts_for_session(session_id)?
            .into_iter()
            .map(TranscriptEntry::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?
            .into_iter()
            .map(|entry| SessionTranscriptLine {
                role: entry.role.as_str().to_string(),
                content: entry.content,
                run_id: entry.run_id,
                created_at: entry.created_at,
            })
            .collect();

        Ok(SessionTranscriptView {
            session_id: session_id.to_string(),
            entries,
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn supervisor_tick(
        &self,
        now: i64,
        verifications: &[MissionVerificationSummary],
    ) -> Result<execution::SupervisorTickReport, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .supervisor_tick(&store, now, verifications)
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn execute_mission_turn_job(
        &self,
        job_id: &str,
        now: i64,
    ) -> Result<execution::MissionTurnExecutionReport, BootstrapError> {
        let store = self.store()?;
        let provider = self.provider_driver()?;
        self.execution_service()
            .execute_mission_turn_job(&store, provider.as_ref(), job_id, now)
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn execute_chat_turn(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
    ) -> Result<execution::ChatTurnExecutionReport, BootstrapError> {
        let store = self.store()?;
        let provider = self.provider_driver()?;
        self.execution_service()
            .execute_chat_turn(&store, provider.as_ref(), session_id, message, now)
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn execute_chat_turn_with_observer(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        observer: &mut dyn FnMut(execution::ChatExecutionEvent),
    ) -> Result<execution::ChatTurnExecutionReport, BootstrapError> {
        let store = self.store()?;
        let provider = self.provider_driver()?;
        let mut observer = Some(observer as &mut dyn FnMut(execution::ChatExecutionEvent));
        self.execution_service()
            .execute_chat_turn_with_observer(
                &store,
                provider.as_ref(),
                session_id,
                message,
                now,
                &mut observer,
            )
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn approve_run(
        &self,
        run_id: &str,
        approval_id: &str,
        now: i64,
    ) -> Result<execution::ApprovalContinuationReport, BootstrapError> {
        let store = self.store()?;
        let snapshot = RunSnapshot::try_from(store.get_run(run_id)?.ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "run",
                id: run_id.to_string(),
            }
        })?)
        .map_err(BootstrapError::RecordConversion)?;

        if snapshot.provider_loop.is_some() {
            let provider = self.provider_driver()?;
            return self
                .execution_service()
                .approve_model_run(&store, provider.as_ref(), run_id, approval_id, now)
                .map_err(BootstrapError::Execution);
        }

        let mut engine = RunEngine::from_snapshot(snapshot);
        engine
            .resolve_approval(approval_id, now)
            .map_err(BootstrapError::RunTransition)?;
        let record =
            RunRecord::try_from(engine.snapshot()).map_err(BootstrapError::RecordConversion)?;
        store.put_run(&record)?;
        Ok(execution::ApprovalContinuationReport {
            run_id: run_id.to_string(),
            run_status: engine.snapshot().status,
            response_id: None,
            output_text: None,
            approval_id: None,
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn approve_run_with_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        now: i64,
        observer: &mut dyn FnMut(execution::ChatExecutionEvent),
    ) -> Result<execution::ApprovalContinuationReport, BootstrapError> {
        let store = self.store()?;
        let snapshot = RunSnapshot::try_from(store.get_run(run_id)?.ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "run",
                id: run_id.to_string(),
            }
        })?)
        .map_err(BootstrapError::RecordConversion)?;

        if snapshot.provider_loop.is_some() {
            let provider = self.provider_driver()?;
            let mut observer = Some(observer as &mut dyn FnMut(execution::ChatExecutionEvent));
            return self
                .execution_service()
                .approve_model_run_with_observer(
                    &store,
                    provider.as_ref(),
                    run_id,
                    approval_id,
                    now,
                    &mut observer,
                )
                .map_err(BootstrapError::Execution);
        }

        self.approve_run(run_id, approval_id, now)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn request_tool_approval(
        &self,
        job_id: &str,
        run_id: &str,
        tool_call: &ToolCall,
        now: i64,
    ) -> Result<execution::ToolExecutionReport, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .request_tool_approval(&store, job_id, run_id, tool_call, now)
            .map_err(BootstrapError::Execution)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn resume_tool_call(
        &self,
        request: execution::ToolResumeRequest<'_>,
    ) -> Result<execution::ToolExecutionReport, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .resume_tool_call(&store, request)
            .map_err(BootstrapError::Execution)
    }
}

impl SessionTranscriptView {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render(&self) -> String {
        self.entries
            .iter()
            .map(|entry| format!("[{}] {}: {}", entry.created_at, entry.role, entry.content))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(source) => write!(formatter, "{source}"),
            Self::Clock(source) => write!(formatter, "system clock error: {source}"),
            Self::InvalidPath { path, reason } => {
                write!(
                    formatter,
                    "invalid bootstrap path {}: {reason}",
                    path.display()
                )
            }
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "bootstrap filesystem error at {}: {source}",
                    path.display()
                )
            }
            Self::Stream(source) => write!(formatter, "stream I/O error: {source}"),
            Self::MissingRecord { kind, id } => write!(formatter, "{kind} {id} was not found"),
            Self::ProviderBuild(source) => write!(formatter, "{source}"),
            Self::ProviderRequest(source) => write!(formatter, "{source}"),
            Self::Execution(source) => write!(formatter, "{source}"),
            Self::Recovery(source) => write!(formatter, "{source}"),
            Self::RecordConversion(source) => {
                write!(formatter, "record conversion error: {source}")
            }
            Self::RunTransition(source) => write!(formatter, "{source}"),
            Self::Sqlite(source) => write!(formatter, "sqlite error: {source}"),
            Self::Store(source) => write!(formatter, "{source}"),
            Self::Usage { reason } => write!(formatter, "{reason}"),
        }
    }
}

impl Error for BootstrapError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(source) => Some(source),
            Self::Clock(source) => Some(source),
            Self::Io { source, .. } => Some(source),
            Self::Stream(source) => Some(source),
            Self::ProviderBuild(source) => Some(source),
            Self::ProviderRequest(source) => Some(source),
            Self::Execution(source) => Some(source),
            Self::Recovery(source) => Some(source),
            Self::RecordConversion(source) => Some(source),
            Self::RunTransition(source) => Some(source),
            Self::Sqlite(source) => Some(source),
            Self::Store(source) => Some(source),
            Self::InvalidPath { .. } | Self::MissingRecord { .. } | Self::Usage { .. } => None,
        }
    }
}

impl From<ConfigError> for BootstrapError {
    fn from(source: ConfigError) -> Self {
        Self::Config(source)
    }
}

impl From<rusqlite::Error> for BootstrapError {
    fn from(source: rusqlite::Error) -> Self {
        Self::Sqlite(source)
    }
}

impl From<StoreError> for BootstrapError {
    fn from(source: StoreError) -> Self {
        Self::Store(source)
    }
}

impl From<ProviderBuildError> for BootstrapError {
    fn from(source: ProviderBuildError) -> Self {
        Self::ProviderBuild(source)
    }
}

impl From<ProviderError> for BootstrapError {
    fn from(source: ProviderError) -> Self {
        Self::ProviderRequest(source)
    }
}

impl From<execution::ExecutionError> for BootstrapError {
    fn from(source: execution::ExecutionError) -> Self {
        Self::Execution(source)
    }
}

impl From<recovery::RecoveryError> for BootstrapError {
    fn from(source: recovery::RecoveryError) -> Self {
        Self::Recovery(source)
    }
}

pub fn build() -> Result<App, BootstrapError> {
    let config = AppConfig::load()?;
    build_from_config(config)
}

pub fn build_from_config(config: AppConfig) -> Result<App, BootstrapError> {
    config.validate()?;

    let persistence = PersistenceScaffold::from_config(config.clone());
    ensure_runtime_layout(&persistence)?;
    reconcile_recovery_state(&persistence)?;

    Ok(App {
        config,
        persistence,
        runtime: RuntimeScaffold::default(),
    })
}

fn ensure_runtime_layout(persistence: &PersistenceScaffold) -> Result<(), BootstrapError> {
    let audit_dir = persistence
        .audit
        .path
        .parent()
        .ok_or_else(|| BootstrapError::InvalidPath {
            path: persistence.audit.path.clone(),
            reason: "must have a parent directory",
        })?;

    ensure_directory_target(&persistence.config.data_dir)?;
    ensure_directory_target(&persistence.stores.artifacts_dir)?;
    ensure_directory_target(&persistence.stores.runs_dir)?;
    ensure_directory_target(&persistence.stores.transcripts_dir)?;
    ensure_directory_target(audit_dir)?;

    ensure_file_target(&persistence.stores.metadata_db)?;
    ensure_file_target(&persistence.audit.path)?;

    create_directory(&persistence.config.data_dir)?;
    create_directory(&persistence.stores.artifacts_dir)?;
    create_directory(&persistence.stores.runs_dir)?;
    create_directory(&persistence.stores.transcripts_dir)?;
    create_directory(audit_dir)?;

    Ok(())
}

fn ensure_directory_target(path: &Path) -> Result<(), BootstrapError> {
    if path.exists() && !path.is_dir() {
        return Err(BootstrapError::InvalidPath {
            path: path.to_path_buf(),
            reason: "must point to a directory",
        });
    }

    Ok(())
}

fn ensure_file_target(path: &Path) -> Result<(), BootstrapError> {
    if path.exists() && path.is_dir() {
        return Err(BootstrapError::InvalidPath {
            path: path.to_path_buf(),
            reason: "must point to a file path",
        });
    }

    Ok(())
}

fn create_directory(path: &Path) -> Result<(), BootstrapError> {
    fs::create_dir_all(path).map_err(|source| BootstrapError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn reconcile_recovery_state(persistence: &PersistenceScaffold) -> Result<(), BootstrapError> {
    let store = PersistenceStore::open(persistence)?;
    recovery::reconcile_runs(&store, persistence.recovery, unix_timestamp()?)?;
    Ok(())
}

fn unix_timestamp() -> Result<i64, BootstrapError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(BootstrapError::Clock)?
        .as_secs() as i64)
}
