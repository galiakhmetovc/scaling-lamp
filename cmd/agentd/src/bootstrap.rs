mod agent_ops;
mod context_ops;
mod execution_ops;
mod mcp_ops;
mod session_ops;

pub use agent_ops::{AgentScheduleCreateOptions, AgentScheduleUpdatePatch, AgentScheduleView};
pub use mcp_ops::{McpConnectorCreateOptions, McpConnectorUpdatePatch, McpConnectorView};
pub(crate) use mcp_ops::{render_mcp_connector_view, render_mcp_connectors_view};

use crate::diagnostics::DiagnosticEventBuilder;
use crate::store_retry::{
    SQLITE_LOCK_RETRY_ATTEMPTS, SQLITE_LOCK_RETRY_DELAY_MS, retry_store_sync,
};
use crate::{about::RuntimeReleaseUpdater, cli, execution, mcp::SharedMcpRegistry, prompting};
use agent_persistence::{
    AgentRepository, AppConfig, ConfigError, ContextSummaryRepository, JobRepository,
    PersistenceScaffold, PersistenceStore, PlanRepository, RecordConversionError, RunRecord,
    RunRepository, RunSummaryRollup, SessionActiveJobCounts, SessionRepository, StoreError,
    TranscriptRepository, audit::AuditLogConfig, recovery,
};
use agent_runtime::RuntimeScaffold;
use agent_runtime::agent::{AgentSchedule, AgentScheduleDeliveryMode, AgentScheduleMode};
use agent_runtime::context::ContextSummary;
use agent_runtime::provider::{
    DEFAULT_PROVIDER_MAX_TOOL_ROUNDS, ProviderBuildError, ProviderDriver, ProviderError,
    build_driver,
};
use agent_runtime::run::RunTransitionError;
use agent_runtime::session::SessionSettings;
use agent_runtime::skills::SessionSkillStatus as RuntimeSessionSkillStatus;
use agent_runtime::tool::SharedProcessRegistry;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::time::{Duration, SystemTime, SystemTimeError, UNIX_EPOCH};

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
    pub mcp: SharedMcpRegistry,
    pub(crate) updater: RuntimeReleaseUpdater,
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
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_status: Option<String>,
    #[serde(default)]
    pub approval_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDebugView {
    pub session_id: String,
    pub entries: Vec<SessionDebugEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDebugEntry {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub detail_title: String,
    pub detail: String,
    pub created_at: i64,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub artifact_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionScheduleSummary {
    pub id: String,
    pub mode: AgentScheduleMode,
    pub delivery_mode: AgentScheduleDeliveryMode,
    pub enabled: bool,
    pub next_fire_at: i64,
    pub target_session_id: Option<String>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
}

impl From<AgentSchedule> for SessionScheduleSummary {
    fn from(value: AgentSchedule) -> Self {
        Self {
            id: value.id,
            mode: value.mode,
            delivery_mode: value.delivery_mode,
            enabled: value.enabled,
            next_fire_at: value.next_fire_at,
            target_session_id: value.target_session_id,
            last_result: value.last_result,
            last_error: value.last_error,
        }
    }
}

pub(crate) fn session_head_schedule_summary(
    value: &SessionScheduleSummary,
) -> agent_runtime::prompt::SessionHeadScheduleSummary {
    agent_runtime::prompt::SessionHeadScheduleSummary {
        id: value.id.clone(),
        mode: value.mode,
        delivery_mode: value.delivery_mode,
        enabled: value.enabled,
        next_fire_at: value.next_fire_at,
        target_session_id: value.target_session_id.clone(),
        last_result: value.last_result.clone(),
        last_error: value.last_error.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub agent_profile_id: String,
    pub agent_name: String,
    pub scheduled_by: Option<String>,
    pub schedule: Option<SessionScheduleSummary>,
    pub model: Option<String>,
    pub reasoning_visible: bool,
    pub think_level: Option<String>,
    pub compactifications: u32,
    pub completion_nudges: Option<u32>,
    pub auto_approve: bool,
    pub context_tokens: u32,
    pub usage_input_tokens: Option<u32>,
    pub usage_output_tokens: Option<u32>,
    pub usage_total_tokens: Option<u32>,
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
pub struct RuntimeStatusSnapshot {
    pub permission_mode: String,
    pub session_count: usize,
    pub mission_count: usize,
    pub run_count: usize,
    pub job_count: usize,
    pub components: usize,
    pub data_dir: String,
    pub state_db: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInteragentSummary {
    pub chain_id: String,
    pub hop_count: Option<u32>,
    pub max_hops: Option<u32>,
    pub state: String,
    pub origin_session_id: Option<String>,
    pub origin_agent_id: Option<String>,
    pub target_agent_id: Option<String>,
    pub recipient_session_id: Option<String>,
    pub parent_interagent_session_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub delegation_label: Option<String>,
    pub continuation_grant_pending: bool,
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
            self.mcp.clone(),
            execution::ExecutionServiceConfig {
                data_dir: self.config.data_dir.clone(),
                provider_max_tool_rounds: self
                    .config
                    .provider
                    .max_tool_rounds
                    .unwrap_or(DEFAULT_PROVIDER_MAX_TOOL_ROUNDS)
                    as usize,
                provider_max_output_tokens: self.config.provider.max_output_tokens,
                session_defaults: SessionSettings {
                    working_memory_limit: self.config.session_defaults.working_memory_limit,
                    project_memory_enabled: self.config.session_defaults.project_memory_enabled,
                    ..SessionSettings::default()
                },
                context_compaction_min_messages: self.config.context.compaction_min_messages,
                context_compaction_keep_tail_messages: self
                    .config
                    .context
                    .compaction_keep_tail_messages,
                context_compaction_max_output_tokens: self
                    .config
                    .context
                    .compaction_max_output_tokens,
                context_compaction_max_summary_chars: self
                    .config
                    .context
                    .compaction_max_summary_chars,
                context_auto_compaction_trigger_ratio: self
                    .config
                    .context
                    .auto_compaction_trigger_ratio,
                context_window_tokens_override: self.config.context.context_window_tokens_override,
                skills_dir: self.config.daemon.skills_dir.clone(),
                a2a_public_base_url: self.config.daemon.public_base_url.clone(),
                a2a_callback_bearer_token: self.config.daemon.bearer_token.clone(),
                a2a_peers: self.config.daemon.a2a_peers.clone(),
                web_search_backend: self.config.web.search_backend,
                web_search_url: self.config.web.search_url.clone(),
                runtime_timing: self.config.runtime_timing.clone(),
                runtime_limits: self.config.runtime_limits.clone(),
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
        retry_store_sync(
            SQLITE_LOCK_RETRY_ATTEMPTS,
            Duration::from_millis(SQLITE_LOCK_RETRY_DELAY_MS),
            || PersistenceStore::open_runtime(&self.persistence),
        )
        .map_err(BootstrapError::Store)
    }

    pub fn runtime_status_snapshot(&self) -> Result<RuntimeStatusSnapshot, BootstrapError> {
        let store = self.store()?;
        Ok(RuntimeStatusSnapshot {
            permission_mode: self.config.permissions.mode.as_str().to_string(),
            session_count: store.count_sessions()?,
            mission_count: store.count_missions()?,
            run_count: store.count_runs()?,
            job_count: store.count_jobs()?,
            components: self.runtime.component_count(),
            data_dir: self.config.data_dir.display().to_string(),
            state_db: self.persistence.stores.metadata_db.display().to_string(),
        })
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
            .map(|entry| match entry.role.as_str() {
                "tool" => format!(
                    "[{}] tool:{}|{}: {}",
                    entry.created_at,
                    entry.tool_name.as_deref().unwrap_or("tool"),
                    entry.tool_status.as_deref().unwrap_or("completed"),
                    entry.content
                ),
                "approval" => format!(
                    "[{}] approval:{}: {}",
                    entry.created_at,
                    entry.approval_id.as_deref().unwrap_or("approval"),
                    entry.content
                ),
                _ => format!("[{}] {}: {}", entry.created_at, entry.role, entry.content),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn build_session_summaries(
    store: &PersistenceStore,
    config: &AppConfig,
    _workspace: &agent_runtime::workspace::WorkspaceRef,
) -> Result<Vec<SessionSummary>, BootstrapError> {
    let audit = AuditLogConfig::from_config(config);
    let emit_step =
        |step: &str,
         elapsed_ms: u64,
         fields: std::collections::BTreeMap<String, serde_json::Value>| {
            let mut event = DiagnosticEventBuilder::new(
                config,
                "info",
                "session_ops",
                step,
                "session summary list sub-step completed",
            )
            .elapsed_ms(elapsed_ms)
            .outcome("ok");
            for (key, value) in fields {
                event = event.field_value(key.as_str(), value);
            }
            event.emit(&audit);
        };

    let step_started = Instant::now();
    let sessions = store
        .list_sessions()?
        .into_iter()
        .filter_map(|record| agent_runtime::session::Session::try_from(record).ok())
        .collect::<Vec<_>>();
    emit_step(
        "list_session_summaries.loaded_sessions",
        step_started.elapsed().as_millis() as u64,
        std::collections::BTreeMap::from([(
            "session_count".to_string(),
            serde_json::json!(sessions.len()),
        )]),
    );

    let step_started = Instant::now();
    let schedules = store
        .list_agent_schedules()?
        .into_iter()
        .filter_map(|record| AgentSchedule::try_from(record).ok())
        .collect::<Vec<_>>();
    let schedules_by_id = schedules
        .into_iter()
        .map(|schedule| (schedule.id.clone(), schedule))
        .collect::<std::collections::HashMap<_, _>>();
    emit_step(
        "list_session_summaries.loaded_schedules",
        step_started.elapsed().as_millis() as u64,
        std::collections::BTreeMap::from([(
            "schedule_count".to_string(),
            serde_json::json!(schedules_by_id.len()),
        )]),
    );

    let step_started = Instant::now();
    let agent_names = store
        .list_agent_profiles()?
        .into_iter()
        .map(|record| (record.id, record.name))
        .collect::<std::collections::HashMap<_, _>>();
    emit_step(
        "list_session_summaries.loaded_agent_profiles",
        step_started.elapsed().as_millis() as u64,
        std::collections::BTreeMap::from([(
            "agent_profile_count".to_string(),
            serde_json::json!(agent_names.len()),
        )]),
    );

    let step_started = Instant::now();
    let transcript_stats = store
        .list_transcript_session_stats()?
        .into_iter()
        .map(|stats| (stats.session_id.clone(), stats))
        .collect::<std::collections::HashMap<_, _>>();
    emit_step(
        "list_session_summaries.loaded_transcript_stats",
        step_started.elapsed().as_millis() as u64,
        std::collections::BTreeMap::from([(
            "transcript_session_count".to_string(),
            serde_json::json!(transcript_stats.len()),
        )]),
    );

    let step_started = Instant::now();
    let context_summaries = store
        .list_context_summaries()?
        .into_iter()
        .filter_map(|record| ContextSummary::try_from(record).ok())
        .map(|summary| (summary.session_id.clone(), summary))
        .collect::<std::collections::HashMap<_, _>>();
    emit_step(
        "list_session_summaries.loaded_context_summaries",
        step_started.elapsed().as_millis() as u64,
        std::collections::BTreeMap::from([(
            "context_summary_count".to_string(),
            serde_json::json!(context_summaries.len()),
        )]),
    );

    let step_started = Instant::now();
    let mut latest_run_rollups = std::collections::HashMap::<String, RunSummaryRollup>::new();
    let mut has_pending_approvals = std::collections::HashMap::<String, bool>::new();
    let mut active_job_counts = std::collections::HashMap::<String, SessionActiveJobCounts>::new();
    for session in &sessions {
        if let Some(run_rollup) = store.get_latest_run_summary_rollup_for_session(&session.id)? {
            latest_run_rollups.insert(session.id.clone(), run_rollup);
        }
        if store.session_has_pending_approval(&session.id)? {
            has_pending_approvals.insert(session.id.clone(), true);
        }
        if let Some(counts) = store.get_active_job_counts_for_session(&session.id)? {
            active_job_counts.insert(session.id.clone(), counts);
        }
    }
    emit_step(
        "list_session_summaries.loaded_session_scoped_execution_rollups",
        step_started.elapsed().as_millis() as u64,
        std::collections::BTreeMap::from([
            (
                "session_count".to_string(),
                serde_json::json!(sessions.len()),
            ),
            (
                "sessions_with_latest_runs".to_string(),
                serde_json::json!(latest_run_rollups.len()),
            ),
            (
                "sessions_with_pending_approvals".to_string(),
                serde_json::json!(has_pending_approvals.len()),
            ),
            (
                "sessions_with_active_jobs".to_string(),
                serde_json::json!(active_job_counts.len()),
            ),
        ]),
    );

    let summary_caches = SessionSummaryCaches {
        agent_names: &agent_names,
        transcript_stats: &transcript_stats,
        context_summaries: &context_summaries,
    };

    let mut summaries = Vec::new();
    for session in sessions {
        let schedule = session
            .delegation_label
            .as_deref()
            .and_then(|label| label.strip_prefix("agent-schedule:"))
            .and_then(|schedule_id| schedules_by_id.get(schedule_id))
            .cloned()
            .map(SessionScheduleSummary::from);
        if let Ok(summary) = session_list_summary_from_session(
            config,
            latest_run_rollups.get(&session.id),
            has_pending_approvals
                .get(&session.id)
                .copied()
                .unwrap_or(false),
            active_job_counts.get(&session.id),
            &schedule,
            &session,
            &summary_caches,
        ) {
            summaries.push(summary);
        }
    }

    Ok(summaries)
}

pub(crate) fn build_single_session_summary(
    store: &PersistenceStore,
    config: &AppConfig,
    _workspace: &agent_runtime::workspace::WorkspaceRef,
    session_id: &str,
) -> Result<SessionSummary, BootstrapError> {
    let emit_step = |step: &str, elapsed_ms: u64| {
        DiagnosticEventBuilder::new(
            config,
            "info",
            "session_ops",
            step,
            "session summary sub-step completed",
        )
        .session_id(session_id.to_string())
        .elapsed_ms(elapsed_ms)
        .outcome("ok")
        .emit(&AuditLogConfig::from_config(config));
    };

    let step_started = Instant::now();
    let session =
        agent_runtime::session::Session::try_from(store.get_session(session_id)?.ok_or_else(
            || BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            },
        )?)
        .map_err(BootstrapError::RecordConversion)?;
    emit_step(
        "session_summary.loaded_session",
        step_started.elapsed().as_millis() as u64,
    );

    let step_started = Instant::now();
    let latest_run_rollup = store.get_latest_run_summary_rollup_for_session(&session.id)?;
    let has_pending_approval = store.session_has_pending_approval(&session.id)?;
    emit_step(
        "session_summary.loaded_run_rollups",
        step_started.elapsed().as_millis() as u64,
    );

    let step_started = Instant::now();
    let active_job_counts = store.get_active_job_counts_for_session(&session.id)?;
    emit_step(
        "session_summary.loaded_active_job_counts",
        step_started.elapsed().as_millis() as u64,
    );

    let scheduled_by = session
        .delegation_label
        .as_deref()
        .and_then(|label| label.strip_prefix("agent-schedule:"))
        .map(str::to_string);
    let step_started = Instant::now();
    let schedule = scheduled_by
        .as_deref()
        .map(|schedule_id| {
            store
                .get_agent_schedule(schedule_id)?
                .map(AgentSchedule::try_from)
                .transpose()
                .map_err(BootstrapError::RecordConversion)
                .map(|schedule| schedule.map(SessionScheduleSummary::from))
        })
        .transpose()?
        .flatten();
    emit_step(
        "session_summary.loaded_schedule",
        step_started.elapsed().as_millis() as u64,
    );

    let step_started = Instant::now();
    let agent_name = store
        .get_agent_profile(&session.agent_profile_id)?
        .map(|record| record.name)
        .unwrap_or_else(|| session.agent_profile_id.clone());
    emit_step(
        "session_summary.loaded_agent_profile",
        step_started.elapsed().as_millis() as u64,
    );

    let agent_names =
        std::collections::HashMap::from([(session.agent_profile_id.clone(), agent_name)]);
    let step_started = Instant::now();
    let transcript_stats = std::collections::HashMap::from([(
        session.id.clone(),
        agent_persistence::TranscriptSessionStats {
            session_id: session.id.clone(),
            transcript_count: store.count_transcripts_for_session(&session.id)?,
            latest_transcript_created_at: store
                .get_latest_transcript_created_at_for_session(&session.id)?,
        },
    )]);
    emit_step(
        "session_summary.loaded_transcript_stats",
        step_started.elapsed().as_millis() as u64,
    );

    let step_started = Instant::now();
    let context_summaries = store
        .get_context_summary(&session.id)?
        .and_then(|record| ContextSummary::try_from(record).ok())
        .map(|summary| std::collections::HashMap::from([(summary.session_id.clone(), summary)]))
        .unwrap_or_default();
    emit_step(
        "session_summary.loaded_context_summary",
        step_started.elapsed().as_millis() as u64,
    );

    let summary_caches = SessionSummaryCaches {
        agent_names: &agent_names,
        transcript_stats: &transcript_stats,
        context_summaries: &context_summaries,
    };

    let step_started = Instant::now();
    let summary = session_list_summary_from_session(
        config,
        latest_run_rollup.as_ref(),
        has_pending_approval,
        active_job_counts.as_ref(),
        &schedule,
        &session,
        &summary_caches,
    )?;
    emit_step(
        "session_summary.assembled",
        step_started.elapsed().as_millis() as u64,
    );

    Ok(summary)
}

struct SessionSummaryCaches<'a> {
    agent_names: &'a std::collections::HashMap<String, String>,
    transcript_stats:
        &'a std::collections::HashMap<String, agent_persistence::TranscriptSessionStats>,
    context_summaries: &'a std::collections::HashMap<String, ContextSummary>,
}

fn session_list_summary_from_session(
    config: &AppConfig,
    latest_run_rollup: Option<&RunSummaryRollup>,
    has_pending_approval: bool,
    active_job_counts: Option<&SessionActiveJobCounts>,
    schedule: &Option<SessionScheduleSummary>,
    session: &agent_runtime::session::Session,
    caches: &SessionSummaryCaches<'_>,
) -> Result<SessionSummary, BootstrapError> {
    let transcript_stats = caches.transcript_stats.get(&session.id);
    let transcript_count = transcript_stats
        .map(|stats| stats.transcript_count)
        .unwrap_or(0);
    let latest_transcript_created_at =
        transcript_stats.and_then(|stats| stats.latest_transcript_created_at);
    let context_summary = caches.context_summaries.get(&session.id);
    let agent_name = caches
        .agent_names
        .get(&session.agent_profile_id)
        .cloned()
        .unwrap_or_else(|| session.agent_profile_id.clone());
    let scheduled_by = session
        .delegation_label
        .as_deref()
        .and_then(|label| label.strip_prefix("agent-schedule:"))
        .map(str::to_string);
    let transcript_updated_at = latest_transcript_created_at.unwrap_or(session.updated_at);
    let context_updated_at = context_summary
        .as_ref()
        .map(|summary| summary.updated_at)
        .unwrap_or(session.updated_at);
    let run_updated_at = latest_run_rollup
        .map(|run| run.updated_at)
        .unwrap_or(session.updated_at);
    let background_job_count = active_job_counts
        .map(|counts| counts.active_count)
        .unwrap_or(0);
    let running_background_job_count = active_job_counts
        .map(|counts| counts.running_count)
        .unwrap_or(0);
    let queued_background_job_count = active_job_counts
        .map(|counts| counts.queued_count)
        .unwrap_or(0);
    let latest_usage = latest_run_rollup.and_then(|run| run.latest_provider_usage.clone());
    let approximated_context_tokens = context_summary
        .as_ref()
        .map(|summary| summary.summary_token_estimate)
        .unwrap_or(0);
    let updated_at = session
        .updated_at
        .max(transcript_updated_at)
        .max(context_updated_at)
        .max(run_updated_at);

    Ok(SessionSummary {
        id: session.id.clone(),
        title: session.title.clone(),
        agent_profile_id: session.agent_profile_id.clone(),
        agent_name,
        scheduled_by,
        schedule: schedule.clone(),
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
        context_tokens: latest_usage
            .as_ref()
            .map(|usage| usage.input_tokens)
            .unwrap_or(approximated_context_tokens),
        usage_input_tokens: latest_usage.as_ref().map(|usage| usage.input_tokens),
        usage_output_tokens: latest_usage.as_ref().map(|usage| usage.output_tokens),
        usage_total_tokens: latest_usage.as_ref().map(|usage| usage.total_tokens),
        has_pending_approval,
        last_message_preview: None,
        message_count: transcript_count,
        background_job_count,
        running_background_job_count,
        queued_background_job_count,
        created_at: session.created_at,
        updated_at,
    })
}

pub(crate) fn latest_provider_usage(
    runs: &[agent_runtime::run::RunSnapshot],
    session_id: &str,
) -> Option<agent_runtime::provider::ProviderUsage> {
    runs.iter()
        .filter(|run| run.session_id == session_id)
        .max_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.started_at.cmp(&right.started_at))
                .then_with(|| left.id.cmp(&right.id))
        })
        .and_then(|run| run.latest_provider_usage.as_ref())
        .cloned()
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

pub(crate) fn compaction_instructions() -> String {
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

pub fn build_without_recovery() -> Result<App, BootstrapError> {
    let config = AppConfig::load()?;
    build_from_config_without_recovery(config)
}

pub fn build_for_args<I, S>(args: I) -> Result<App, BootstrapError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if should_reconcile_recovery_for_args(args) {
        build()
    } else {
        build_without_recovery()
    }
}

pub fn build_from_config(config: AppConfig) -> Result<App, BootstrapError> {
    build_from_config_inner(config, true)
}

pub fn build_from_config_without_recovery(config: AppConfig) -> Result<App, BootstrapError> {
    build_from_config_inner(config, false)
}

fn build_from_config_inner(
    config: AppConfig,
    reconcile_recovery: bool,
) -> Result<App, BootstrapError> {
    let started = Instant::now();
    config.validate()?;

    let persistence = PersistenceScaffold::from_config(config.clone());
    DiagnosticEventBuilder::new(
        &config,
        "info",
        "bootstrap",
        "build_from_config.start",
        "building app from config",
    )
    .field("bind_host", &config.daemon.bind_host)
    .field("bind_port", config.daemon.bind_port)
    .field("reconcile_recovery", reconcile_recovery)
    .field("home", std::env::var("HOME").ok())
    .field("xdg_state_home", std::env::var("XDG_STATE_HOME").ok())
    .field("teamd_data_dir", std::env::var("TEAMD_DATA_DIR").ok())
    .emit(&persistence.audit);
    ensure_runtime_layout(&persistence)?;
    if reconcile_recovery {
        reconcile_recovery_state(&persistence)?;
    } else {
        initialize_metadata_schema(&persistence)?;
    }
    let mcp = SharedMcpRegistry::from_runtime_timing(&config.runtime_timing);

    let app = App {
        config,
        persistence,
        runtime: RuntimeScaffold::default(),
        processes: SharedProcessRegistry::default(),
        mcp,
        updater: RuntimeReleaseUpdater::github_default()?,
    };
    app.ensure_builtin_agents_bootstrapped()?;
    app.ensure_mcp_connectors_bootstrapped()?;
    DiagnosticEventBuilder::new(
        &app.config,
        "info",
        "bootstrap",
        "build_from_config.finish",
        "app build completed",
    )
    .elapsed_ms(started.elapsed().as_millis() as u64)
    .outcome("ok")
    .field(
        "audit_path",
        app.persistence.audit.path.display().to_string(),
    )
    .emit(&app.persistence.audit);
    Ok(app)
}

fn should_reconcile_recovery_for_args<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .next()
        .is_some_and(|arg| arg.as_ref() == "daemon")
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

fn initialize_metadata_schema(persistence: &PersistenceScaffold) -> Result<(), BootstrapError> {
    let _store = PersistenceStore::open_bootstrap_schema(persistence)?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::should_reconcile_recovery_for_args;

    #[test]
    fn recovery_reconcile_is_daemon_only() {
        assert!(should_reconcile_recovery_for_args(["daemon"]));
        assert!(!should_reconcile_recovery_for_args(["version"]));
        assert!(!should_reconcile_recovery_for_args(["session", "list"]));
        assert!(!should_reconcile_recovery_for_args(["telegram", "run"]));
        assert!(!should_reconcile_recovery_for_args(["tui"]));
        assert!(!should_reconcile_recovery_for_args(
            std::iter::empty::<&str>()
        ));
    }
}
