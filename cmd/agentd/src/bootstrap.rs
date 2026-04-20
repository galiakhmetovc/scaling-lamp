use crate::{cli, execution, prompting};
use agent_persistence::{
    AppConfig, ConfigError, ContextSummaryRepository, PersistenceScaffold, PersistenceStore,
    PlanRepository, RecordConversionError, RunRecord, RunRepository, SessionRepository, StoreError,
    TranscriptRepository, recovery,
};
use agent_runtime::RuntimeScaffold;
use agent_runtime::context::{CompactionPolicy, ContextSummary, approximate_token_count};
use agent_runtime::plan::PlanSnapshot;
use agent_runtime::prompt::SessionHead;
use agent_runtime::provider::{ProviderBuildError, ProviderDriver, ProviderError, build_driver};
use agent_runtime::run::{RunEngine, RunSnapshot, RunTransitionError};
use agent_runtime::scheduler::MissionVerificationSummary;
use agent_runtime::session::{MessageRole, Session, SessionSettings, TranscriptEntry};
use agent_runtime::tool::{SharedProcessRegistry, ToolCall};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub model: Option<String>,
    pub reasoning_visible: bool,
    pub think_level: Option<String>,
    pub compactifications: u32,
    pub context_tokens: u32,
    pub has_pending_approval: bool,
    pub last_message_preview: Option<String>,
    pub message_count: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPendingApproval {
    pub run_id: String,
    pub approval_id: String,
    pub reason: String,
    pub requested_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionPreferencesPatch {
    pub title: Option<String>,
    pub model: Option<Option<String>>,
    pub reasoning_visible: Option<bool>,
    pub think_level: Option<Option<String>>,
    pub compactifications: Option<u32>,
}

impl App {
    fn execution_service(&self) -> execution::ExecutionService {
        execution::ExecutionService::new(
            self.config.permissions.clone(),
            self.runtime.workspace.clone(),
            self.config.provider.max_output_tokens,
            self.processes.clone(),
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
    pub fn create_session_auto(
        &self,
        title: Option<&str>,
    ) -> Result<SessionSummary, BootstrapError> {
        let store = self.store()?;
        let now = unix_timestamp()?;
        let session = Session {
            id: format!("session-{}", unique_timestamp_token()?),
            title: title.unwrap_or("New Session").trim().to_string(),
            prompt_override: None,
            settings: SessionSettings::default(),
            active_mission_id: None,
            created_at: now,
            updated_at: now,
        };
        let record = agent_persistence::SessionRecord::try_from(&session)
            .map_err(BootstrapError::RecordConversion)?;
        store.put_session(&record)?;
        self.session_summary(&session.id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn list_session_summaries(&self) -> Result<Vec<SessionSummary>, BootstrapError> {
        let store = self.store()?;
        build_session_summaries(&store, &self.config, &self.runtime.workspace)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn session_summary(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        self.list_session_summaries()?
            .into_iter()
            .find(|summary| summary.id == session_id)
            .ok_or_else(|| BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn session_head(&self, session_id: &str) -> Result<SessionHead, BootstrapError> {
        let store = self.store()?;
        let session = Session::try_from(store.get_session(session_id)?.ok_or_else(|| {
            BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            }
        })?)
        .map_err(BootstrapError::RecordConversion)?;
        let transcripts = store.list_transcripts_for_session(session_id)?;
        let context_summary = store
            .get_context_summary(session_id)?
            .map(ContextSummary::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)?;
        let runs = store
            .load_execution_state()?
            .runs
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BootstrapError::RecordConversion)?;

        Ok(prompting::build_session_head(
            &session,
            &transcripts,
            context_summary.as_ref(),
            &runs,
            &self.runtime.workspace,
        ))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn pending_approvals(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionPendingApproval>, BootstrapError> {
        let snapshot = self.store()?.load_execution_state()?;
        let mut pending = Vec::new();

        for record in snapshot.runs {
            let run = RunSnapshot::try_from(record).map_err(BootstrapError::RecordConversion)?;
            if run.session_id != session_id
                || run.status != agent_runtime::run::RunStatus::WaitingApproval
            {
                continue;
            }
            for approval in run.pending_approvals {
                pending.push(SessionPendingApproval {
                    run_id: run.id.clone(),
                    approval_id: approval.id,
                    reason: approval.reason,
                    requested_at: approval.requested_at,
                });
            }
        }

        pending.sort_by_key(|approval| (approval.requested_at, approval.approval_id.clone()));
        Ok(pending)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn latest_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<SessionPendingApproval>, BootstrapError> {
        let pending = self.pending_approvals(session_id)?;
        if let Some(requested) = requested_approval_id {
            return Ok(pending
                .into_iter()
                .find(|approval| approval.approval_id == requested));
        }

        Ok(pending.into_iter().max_by(|left, right| {
            left.requested_at
                .cmp(&right.requested_at)
                .then_with(|| left.approval_id.cmp(&right.approval_id))
        }))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<SessionSummary, BootstrapError> {
        let store = self.store()?;
        let record =
            store
                .get_session(session_id)?
                .ok_or_else(|| BootstrapError::MissingRecord {
                    kind: "session",
                    id: session_id.to_string(),
                })?;
        let mut session = Session::try_from(record).map_err(BootstrapError::RecordConversion)?;

        if let Some(title) = patch.title {
            session.title = title.trim().to_string();
        }
        if let Some(model) = patch.model {
            session.settings.model = model.map(|value| value.trim().to_string());
        }
        if let Some(reasoning_visible) = patch.reasoning_visible {
            session.settings.reasoning_visible = reasoning_visible;
        }
        if let Some(think_level) = patch.think_level {
            session.settings.think_level = think_level.map(|value| value.trim().to_string());
        }
        if let Some(compactifications) = patch.compactifications {
            session.settings.compactifications = compactifications;
        }
        session.updated_at = unix_timestamp()?;

        let record = agent_persistence::SessionRecord::try_from(&session)
            .map_err(BootstrapError::RecordConversion)?;
        store.put_session(&record)?;
        self.session_summary(session_id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn delete_session(&self, session_id: &str) -> Result<(), BootstrapError> {
        let store = self.store()?;
        let deleted = store.delete_session(session_id)?;
        if !deleted {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn clear_session(
        &self,
        session_id: &str,
        title: Option<&str>,
    ) -> Result<SessionSummary, BootstrapError> {
        self.delete_session(session_id)?;
        self.create_session_auto(title)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn context_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<ContextSummary>, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }

        store
            .get_context_summary(session_id)?
            .map(ContextSummary::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn plan_snapshot(&self, session_id: &str) -> Result<PlanSnapshot, BootstrapError> {
        let store = self.store()?;
        if store.get_session(session_id)?.is_none() {
            return Err(BootstrapError::MissingRecord {
                kind: "session",
                id: session_id.to_string(),
            });
        }

        Ok(store
            .get_plan(session_id)?
            .map(PlanSnapshot::try_from)
            .transpose()
            .map_err(BootstrapError::RecordConversion)?
            .unwrap_or_else(|| PlanSnapshot {
                session_id: session_id.to_string(),
                goal: None,
                items: Vec::new(),
                updated_at: 0,
            }))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        let snapshot = self.plan_snapshot(session_id)?;
        if snapshot.is_empty() {
            return Ok("plan is empty".to_string());
        }

        let mut lines = vec!["Plan:".to_string()];
        if let Some(goal) = snapshot.goal {
            lines.push(format!("Goal: {goal}"));
        }
        for item in snapshot.items {
            lines.push(format!(
                "- [{}] {}: {}",
                item.status.as_str(),
                item.id,
                item.content
            ));
            if !item.depends_on.is_empty() {
                lines.push(format!("  depends_on: {}", item.depends_on.join(", ")));
            }
            if let Some(blocked_reason) = item.blocked_reason {
                lines.push(format!("  blocked_reason: {blocked_reason}"));
            }
            if let Some(parent_task_id) = item.parent_task_id {
                lines.push(format!("  parent_task_id: {parent_task_id}"));
            }
            for note in item.notes {
                lines.push(format!("  note: {note}"));
            }
        }
        Ok(lines.join("\n"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn compact_session(&self, session_id: &str) -> Result<SessionSummary, BootstrapError> {
        let store = self.store()?;
        let session_record =
            store
                .get_session(session_id)?
                .ok_or_else(|| BootstrapError::MissingRecord {
                    kind: "session",
                    id: session_id.to_string(),
                })?;
        let mut session =
            Session::try_from(session_record).map_err(BootstrapError::RecordConversion)?;
        let transcripts = store.list_transcripts_for_session(session_id)?;
        let policy = CompactionPolicy::default();

        if !policy.should_compact(transcripts.len()) {
            return self.session_summary(session_id);
        }

        let covered_message_count = policy.covered_message_count(transcripts.len());
        let summary_messages = transcripts
            .iter()
            .take(covered_message_count)
            .map(|record| {
                let role = MessageRole::try_from(record.kind.as_str()).map_err(|_| {
                    BootstrapError::RecordConversion(RecordConversionError::InvalidMessageRole {
                        value: record.kind.clone(),
                    })
                })?;
                Ok::<agent_runtime::provider::ProviderMessage, BootstrapError>(
                    agent_runtime::provider::ProviderMessage {
                        role,
                        content: record.content.clone(),
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let provider = self.provider_driver()?;
        let response = provider.complete(&agent_runtime::provider::ProviderRequest {
            model: session.settings.model.clone(),
            instructions: Some(compaction_instructions()),
            messages: summary_messages,
            previous_response_id: None,
            continuation_messages: Vec::new(),
            tools: Vec::new(),
            tool_outputs: Vec::new(),
            max_output_tokens: Some(policy.max_output_tokens),
            stream: agent_runtime::provider::ProviderStreamMode::Disabled,
        })?;
        let now = unix_timestamp()?;
        let summary_text = policy.trim_summary_text(&response.output_text);
        let context_summary = ContextSummary {
            session_id: session.id.clone(),
            summary_text: summary_text.clone(),
            covered_message_count: covered_message_count as u32,
            summary_token_estimate: approximate_token_count(&summary_text),
            updated_at: now,
        };
        store.put_context_summary(&agent_persistence::ContextSummaryRecord::from(
            &context_summary,
        ))?;

        session.settings.compactifications += 1;
        session.updated_at = now;
        let session_record = agent_persistence::SessionRecord::try_from(&session)
            .map_err(BootstrapError::RecordConversion)?;
        store.put_session(&session_record)?;
        self.session_summary(session_id)
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
        self.execute_chat_turn_with_control_and_observer(session_id, message, now, None, observer)
    }

    pub fn execute_chat_turn_with_control_and_observer(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut dyn FnMut(execution::ChatExecutionEvent),
    ) -> Result<execution::ChatTurnExecutionReport, BootstrapError> {
        let store = self.store()?;
        let provider = self.provider_driver()?;
        let mut observer = Some(observer as &mut dyn FnMut(execution::ChatExecutionEvent));
        self.execution_service()
            .execute_chat_turn_with_control(
                &store,
                provider.as_ref(),
                session_id,
                message,
                now,
                interrupt_after_tool_step,
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
        self.approve_run_with_control_and_observer(run_id, approval_id, now, None, observer)
    }

    pub fn approve_run_with_control_and_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
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
                .approve_model_run_with_control(
                    &store,
                    provider.as_ref(),
                    run_id,
                    approval_id,
                    now,
                    interrupt_after_tool_step,
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

fn build_session_summaries(
    store: &PersistenceStore,
    config: &AppConfig,
    workspace: &agent_runtime::workspace::WorkspaceRef,
) -> Result<Vec<SessionSummary>, BootstrapError> {
    let sessions = store
        .list_sessions()?
        .into_iter()
        .map(Session::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(BootstrapError::RecordConversion)?;
    let runs = store
        .load_execution_state()?
        .runs
        .into_iter()
        .map(RunSnapshot::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(BootstrapError::RecordConversion)?;

    sessions
        .into_iter()
        .map(|session| session_summary_from_session(store, config, &runs, &session, workspace))
        .collect()
}

fn session_summary_from_session(
    store: &PersistenceStore,
    config: &AppConfig,
    runs: &[RunSnapshot],
    session: &Session,
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
        context_tokens: session_head.context_tokens,
        has_pending_approval: session_head.pending_approval_count > 0,
        last_message_preview,
        message_count: session_head.message_count,
        created_at: session.created_at,
        updated_at,
    })
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
