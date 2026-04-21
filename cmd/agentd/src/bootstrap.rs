mod context_ops;
mod execution_ops;
mod session_ops;

use crate::{cli, execution, prompting};
use agent_persistence::{
    AppConfig, ConfigError, ContextSummaryRepository, JobRepository, PersistenceScaffold,
    PersistenceStore, PlanRepository, RecordConversionError, RunRecord, RunRepository,
    SessionRepository, StoreError, TranscriptRepository, recovery,
};
use agent_runtime::RuntimeScaffold;
use agent_runtime::context::{ContextSummary, approximate_token_count};
use agent_runtime::provider::{
    DEFAULT_PROVIDER_MAX_TOOL_ROUNDS, ProviderBuildError, ProviderDriver, ProviderError,
    build_driver,
};
use agent_runtime::run::{RunSnapshot, RunTransitionError};
use agent_runtime::session::SessionSettings;
use agent_runtime::skills::SessionSkillStatus as RuntimeSessionSkillStatus;
use agent_runtime::tool::SharedProcessRegistry;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone)]
pub struct App {
    pub config: AppConfig,
    pub persistence: PersistenceScaffold,
    pub runtime: RuntimeScaffold,
    pub processes: SharedProcessRegistry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTranscriptView {
    pub session_id: String,
    pub entries: Vec<SessionTranscriptLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTranscriptLine {
    pub role: String,
    pub content: String,
    pub run_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub model: Option<String>,
    pub reasoning_visible: bool,
    pub think_level: Option<String>,
    pub compactifications: u32,
    pub completion_nudges: Option<u32>,
    pub auto_approve: bool,
    pub context_tokens: u32,
    pub has_pending_approval: bool,
    pub last_message_preview: Option<String>,
    pub message_count: usize,
    pub background_job_count: usize,
    pub running_background_job_count: usize,
    pub queued_background_job_count: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionBackgroundJob {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub last_progress_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPendingApproval {
    pub run_id: String,
    pub approval_id: String,
    pub reason: String,
    pub requested_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSkillStatus {
    pub name: String,
    pub description: String,
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionPreferencesPatch {
    pub title: Option<String>,
    pub model: Option<Option<String>>,
    pub reasoning_visible: Option<bool>,
    pub think_level: Option<Option<String>>,
    pub compactifications: Option<u32>,
    pub completion_nudges: Option<Option<u32>>,
    pub auto_approve: Option<bool>,
}

impl App {
    fn execution_service(&self) -> execution::ExecutionService {
        execution::ExecutionService::new(
            self.config.permissions.clone(),
            self.runtime.workspace.clone(),
            self.processes.clone(),
            execution::ExecutionServiceConfig {
                provider_max_tool_rounds: self
                    .config
                    .provider
                    .max_tool_rounds
                    .unwrap_or(DEFAULT_PROVIDER_MAX_TOOL_ROUNDS)
                    as usize,
                provider_max_output_tokens: self.config.provider.max_output_tokens,
                skills_dir: self.config.daemon.skills_dir.clone(),
                a2a_public_base_url: self.config.daemon.public_base_url.clone(),
                a2a_callback_bearer_token: self.config.daemon.bearer_token.clone(),
                a2a_peers: self.config.daemon.a2a_peers.clone(),
            },
        )
    }

    pub fn run(&self) -> Result<(), BootstrapError> {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut input = stdin.lock();
        let mut output = stdout.lock();
        cli::execute_process_with_io(self, std::env::args().skip(1), &mut input, &mut output)
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

fn build_session_summaries(
    store: &PersistenceStore,
    config: &AppConfig,
    workspace: &agent_runtime::workspace::WorkspaceRef,
) -> Result<Vec<SessionSummary>, BootstrapError> {
    let sessions = store
        .list_sessions()?
        .into_iter()
        .map(agent_runtime::session::Session::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(BootstrapError::RecordConversion)?;
    let runs = store
        .load_execution_state()?
        .runs
        .into_iter()
        .map(RunSnapshot::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(BootstrapError::RecordConversion)?;
    let jobs = store
        .list_jobs()?
        .into_iter()
        .map(agent_runtime::mission::JobSpec::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(BootstrapError::RecordConversion)?;

    sessions
        .into_iter()
        .map(|session| {
            session_summary_from_session(store, config, &runs, &jobs, &session, workspace)
        })
        .collect()
}

fn session_summary_from_session(
    store: &PersistenceStore,
    config: &AppConfig,
    runs: &[RunSnapshot],
    jobs: &[agent_runtime::mission::JobSpec],
    session: &agent_runtime::session::Session,
    workspace: &agent_runtime::workspace::WorkspaceRef,
) -> Result<SessionSummary, BootstrapError> {
    let transcripts = store.list_transcripts_for_session(&session.id)?;
    let context_summary = store
        .get_context_summary(&session.id)?
        .map(ContextSummary::try_from)
        .transpose()
        .map_err(BootstrapError::RecordConversion)?;
    let session_head = prompting::build_session_head(
        session,
        &transcripts,
        context_summary.as_ref(),
        runs,
        workspace,
    );
    let last_message_preview = transcripts
        .last()
        .map(|record| prompting::preview_text(record.content.as_str(), 96));
    let transcript_updated_at = transcripts
        .last()
        .map(|record| record.created_at)
        .unwrap_or(session.updated_at);
    let context_updated_at = context_summary
        .as_ref()
        .map(|summary| summary.updated_at)
        .unwrap_or(session.updated_at);
    let run_updated_at = runs
        .iter()
        .filter(|run| run.session_id == session.id)
        .map(|run| run.updated_at)
        .max()
        .unwrap_or(session.updated_at);
    let session_jobs = jobs
        .iter()
        .filter(|job| job.session_id == session.id && job.status.is_active())
        .collect::<Vec<_>>();
    let background_job_count = session_jobs.len();
    let running_background_job_count = session_jobs
        .iter()
        .filter(|job| job.status == agent_runtime::mission::JobStatus::Running)
        .count();
    let queued_background_job_count = session_jobs
        .iter()
        .filter(|job| job.status == agent_runtime::mission::JobStatus::Queued)
        .count();
    let updated_at = session
        .updated_at
        .max(transcript_updated_at)
        .max(context_updated_at)
        .max(run_updated_at);
    Ok(SessionSummary {
        id: session.id.clone(),
        title: session.title.clone(),
        model: session
            .settings
            .model
            .clone()
            .or_else(|| config.provider.default_model.clone()),
        reasoning_visible: session.settings.reasoning_visible,
        think_level: session.settings.think_level.clone(),
        compactifications: session.settings.compactifications,
        completion_nudges: session.settings.completion_nudges,
        auto_approve: session.settings.auto_approve,
        context_tokens: session_head.context_tokens,
        has_pending_approval: session_head.pending_approval_count > 0,
        last_message_preview,
        message_count: session_head.message_count,
        background_job_count,
        running_background_job_count,
        queued_background_job_count,
        created_at: session.created_at,
        updated_at,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillCommand {
    Enable,
    Disable,
}

impl From<RuntimeSessionSkillStatus> for SessionSkillStatus {
    fn from(value: RuntimeSessionSkillStatus) -> Self {
        Self {
            name: value.name,
            description: value.description,
            mode: match value.mode {
                agent_runtime::skills::SkillActivationMode::Inactive => "inactive",
                agent_runtime::skills::SkillActivationMode::Automatic => "automatic",
                agent_runtime::skills::SkillActivationMode::Manual => "manual",
                agent_runtime::skills::SkillActivationMode::Disabled => "disabled",
            }
            .to_string(),
        }
    }
}

fn compaction_instructions() -> String {
    "Summarize the provided earlier conversation into a concise operational context summary. Preserve user goals, key decisions, important files and paths, blockers, approvals, and unresolved next steps. Keep the summary short and actionable.".to_string()
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
        processes: SharedProcessRegistry::default(),
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

fn unique_timestamp_token() -> Result<u128, BootstrapError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(BootstrapError::Clock)?
        .as_millis())
}
