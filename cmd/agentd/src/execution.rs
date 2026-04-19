#![cfg_attr(not(test), allow(dead_code))]

use agent_persistence::{
    JobRecord, JobRepository, MissionRecord, MissionRepository, PersistenceStore,
    RecordConversionError, RunRecord, RunRepository, SessionRepository, StoreError,
    TranscriptRecord, TranscriptRepository,
};
use agent_runtime::mission::{
    JobExecutionInput, JobResult, JobSpec, JobStatus, MissionSpec, MissionStatus,
};
use agent_runtime::permission::{PermissionAction, PermissionConfig};
use agent_runtime::provider::{
    ProviderContinuationMessage, ProviderDriver, ProviderError, ProviderMessage, ProviderRequest,
    ProviderResponse, ProviderStreamMode, ProviderToolDefinition, ProviderToolOutput,
};
use agent_runtime::run::{
    ActiveProcess, ApprovalRequest, PendingToolApproval, ProviderLoopState, RunEngine, RunSnapshot,
    RunStatus, RunTransitionError,
};
use agent_runtime::scheduler::{
    MissionVerificationSummary, SupervisorAction, SupervisorLoop, SupervisorTickInput,
};
use agent_runtime::session::{MessageRole, TranscriptEntry};
use agent_runtime::tool::{ProcessKind, ToolCall, ToolCatalog, ToolError, ToolOutput, ToolRuntime};
use agent_runtime::verification::EvidenceBundle;
use agent_runtime::workspace::WorkspaceRef;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::Path;

const MAX_PROVIDER_TOOL_ROUNDS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorTickReport {
    pub actions: Vec<SupervisorAction>,
    pub queued_jobs: usize,
    pub dispatched_jobs: usize,
    pub blocked_jobs: usize,
    pub deferred_missions: usize,
    pub completed_missions: usize,
    pub budget_remaining: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionTurnExecutionReport {
    pub job_id: String,
    pub run_id: String,
    pub response_id: String,
    pub output_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTurnExecutionReport {
    pub session_id: String,
    pub run_id: String,
    pub response_id: String,
    pub output_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalContinuationReport {
    pub run_id: String,
    pub run_status: RunStatus,
    pub response_id: Option<String>,
    pub output_text: Option<String>,
    pub approval_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolExecutionReport {
    pub job_id: String,
    pub run_id: String,
    pub run_status: RunStatus,
    pub approval_id: Option<String>,
    pub output_summary: Option<String>,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolResumeRequest<'a> {
    pub job_id: &'a str,
    pub run_id: &'a str,
    pub approval_id: &'a str,
    pub tool_call: &'a ToolCall,
    pub workspace_root: &'a Path,
    pub evidence: Option<&'a EvidenceBundle>,
    pub now: i64,
}

#[derive(Debug, Clone, Copy)]
struct ToolExecutionContext<'a> {
    approved_approval_id: Option<&'a str>,
    workspace_root: Option<&'a Path>,
    evidence: Option<&'a EvidenceBundle>,
    now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionService {
    permissions: PermissionConfig,
    supervisor: SupervisorLoop,
    workspace: WorkspaceRef,
}

#[derive(Debug)]
pub enum ExecutionError {
    MissingJob {
        id: String,
    },
    MissingMission {
        id: String,
    },
    MissingRun {
        id: String,
    },
    MissingSession {
        id: String,
    },
    UnsupportedJobInput {
        id: String,
        kind: String,
    },
    PermissionDenied {
        tool: String,
        reason: String,
    },
    ApprovalRequired {
        tool: String,
        approval_id: String,
        reason: String,
    },
    Provider(ProviderError),
    ProviderLoop {
        reason: String,
    },
    RecordConversion(RecordConversionError),
    RunTransition(RunTransitionError),
    Store(StoreError),
    ToolCallParse {
        name: String,
        reason: String,
    },
    Tool(ToolError),
}

impl Default for ExecutionService {
    fn default() -> Self {
        Self::new(PermissionConfig::default(), WorkspaceRef::default())
    }
}

impl ExecutionService {
    pub fn new(permissions: PermissionConfig, workspace: WorkspaceRef) -> Self {
        Self {
            permissions,
            supervisor: SupervisorLoop::default(),
            workspace,
        }
    }
}

impl ExecutionService {
    fn automatic_provider_tools(
        &self,
        provider: &dyn ProviderDriver,
    ) -> Vec<ProviderToolDefinition> {
        if !provider.descriptor().capabilities.supports_tool_calls {
            return Vec::new();
        }

        ToolCatalog::default()
            .automatic_model_definitions()
            .into_iter()
            .map(|definition| ProviderToolDefinition {
                name: definition.name.as_str().to_string(),
                description: definition.description.to_string(),
                parameters: definition.name.input_schema(),
            })
            .collect()
    }

    fn transcript_messages(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<Vec<ProviderMessage>, ExecutionError> {
        store
            .list_transcripts_for_session(session_id)
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(|record| {
                let role = MessageRole::try_from(record.kind.as_str()).map_err(|_| {
                    ExecutionError::RecordConversion(RecordConversionError::InvalidMessageRole {
                        value: record.kind.clone(),
                    })
                })?;
                Ok(ProviderMessage {
                    role,
                    content: record.content,
                })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    fn persist_run(&self, store: &PersistenceStore, run: &RunEngine) -> Result<(), ExecutionError> {
        store
            .put_run(
                &RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)
    }

    fn find_job_by_run_id(
        &self,
        store: &PersistenceStore,
        run_id: &str,
    ) -> Result<Option<JobSpec>, ExecutionError> {
        store
            .load_execution_state()
            .map_err(ExecutionError::Store)?
            .jobs
            .into_iter()
            .find(|record| record.run_id.as_deref() == Some(run_id))
            .map(JobSpec::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)
    }

    fn apply_provider_response(
        &self,
        run: &mut RunEngine,
        response: &ProviderResponse,
        now: i64,
    ) -> Result<(), ExecutionError> {
        run.begin_provider_stream(&response.response_id, &response.model, now)
            .map_err(ExecutionError::RunTransition)?;
        if !response.output_text.is_empty() {
            run.push_provider_text(&response.output_text, now)
                .map_err(ExecutionError::RunTransition)?;
        }
        run.finish_provider_stream(now)
            .map_err(ExecutionError::RunTransition)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_provider_turn_loop(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        session_id: &str,
        instructions: Option<String>,
        run: &mut RunEngine,
        initial_loop_state: Option<ProviderLoopState>,
        now: i64,
    ) -> Result<ProviderResponse, ExecutionError> {
        let base_messages = self.transcript_messages(store, session_id)?;
        let catalog = ToolCatalog::default();
        let tools = self.automatic_provider_tools(provider);
        let mut pending_tool_outputs = initial_loop_state
            .as_ref()
            .map(|state| state.pending_tool_outputs.clone())
            .unwrap_or_default();
        let mut continuation_messages = initial_loop_state
            .as_ref()
            .map(|state| state.continuation_messages.clone())
            .unwrap_or_default();
        let mut tool_runtime = ToolRuntime::new(self.workspace.clone());
        let mut previous_response_id = initial_loop_state
            .as_ref()
            .and_then(|state| state.previous_response_id.clone());
        let mut seen_tool_signatures = initial_loop_state
            .as_ref()
            .map(|state| {
                state
                    .seen_tool_signatures
                    .iter()
                    .enumerate()
                    .map(|(index, signature)| (signature.clone(), index))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        let starting_round = initial_loop_state
            .as_ref()
            .map(|state| state.next_round)
            .unwrap_or(0);
        let supports_previous_response_id = provider
            .descriptor()
            .capabilities
            .supports_previous_response_id;

        for round in starting_round..MAX_PROVIDER_TOOL_ROUNDS {
            let request = ProviderRequest {
                model: None,
                instructions: instructions.clone(),
                messages: if supports_previous_response_id && previous_response_id.is_some() {
                    Vec::new()
                } else {
                    base_messages.clone()
                },
                previous_response_id: if supports_previous_response_id {
                    previous_response_id.clone()
                } else {
                    None
                },
                continuation_messages: continuation_messages.clone(),
                tools: tools.clone(),
                tool_outputs: if supports_previous_response_id {
                    pending_tool_outputs.clone()
                } else {
                    Vec::new()
                },
                max_output_tokens: Some(512),
                stream: ProviderStreamMode::Disabled,
            };

            let response = provider
                .complete(&request)
                .map_err(ExecutionError::Provider)?;
            self.apply_provider_response(run, &response, now)?;
            self.persist_run(store, run)?;

            if response.tool_calls.is_empty() {
                return Ok(response);
            }

            let signature = response
                .tool_calls
                .iter()
                .map(|tool_call| format!("{}:{}", tool_call.name, tool_call.arguments))
                .collect::<Vec<_>>()
                .join("|");
            if let Some(first_seen_round) = seen_tool_signatures.insert(signature.clone(), round) {
                return Err(ExecutionError::ProviderLoop {
                    reason: format!(
                        "provider repeated tool-call signature from round {}: {}",
                        first_seen_round + 1,
                        signature
                    ),
                });
            }

            if !supports_previous_response_id {
                continuation_messages.push(ProviderContinuationMessage::AssistantToolCalls {
                    tool_calls: response.tool_calls.clone(),
                });
            }

            let mut next_tool_outputs = Vec::new();
            for tool_call in &response.tool_calls {
                let parsed = ToolCall::from_openai_function(&tool_call.name, &tool_call.arguments)
                    .map_err(|source| ExecutionError::ToolCallParse {
                        name: tool_call.name.clone(),
                        reason: source.to_string(),
                    })?;
                let definition = catalog.definition_for_call(&parsed).ok_or_else(|| {
                    ExecutionError::ToolCallParse {
                        name: tool_call.name.clone(),
                        reason: "tool is not in the catalog".to_string(),
                    }
                })?;
                let permission = self.permissions.resolve(definition, &parsed);

                match permission.action {
                    PermissionAction::Allow => {}
                    PermissionAction::Deny => {
                        let reason = format!(
                            "tool {} denied by permission policy: {}",
                            parsed.name().as_str(),
                            permission.reason
                        );
                        run.fail(reason.clone(), now)
                            .map_err(ExecutionError::RunTransition)?;
                        self.persist_run(store, run)?;
                        return Err(ExecutionError::PermissionDenied {
                            tool: parsed.name().as_str().to_string(),
                            reason,
                        });
                    }
                    PermissionAction::Ask => {
                        let approval_id =
                            format!("approval-{}-{}", run.snapshot().id, parsed.name().as_str());
                        let reason = format!(
                            "tool {} requires approval: {} ({})",
                            parsed.name().as_str(),
                            parsed.summary(),
                            permission.reason
                        );
                        run.wait_for_approval(
                            ApprovalRequest::new(
                                approval_id.clone(),
                                tool_call.call_id.as_str(),
                                &reason,
                                now,
                            ),
                            now,
                        )
                        .map_err(ExecutionError::RunTransition)?;
                        run.set_provider_loop_state(
                            ProviderLoopState {
                                next_round: round + 1,
                                previous_response_id: supports_previous_response_id
                                    .then(|| response.response_id.clone()),
                                continuation_messages: continuation_messages.clone(),
                                pending_tool_outputs: next_tool_outputs.clone(),
                                seen_tool_signatures: seen_tool_signatures
                                    .keys()
                                    .cloned()
                                    .collect(),
                                pending_approval: Some(PendingToolApproval::new(
                                    approval_id.clone(),
                                    tool_call.call_id.clone(),
                                    tool_call.name.clone(),
                                    tool_call.arguments.clone(),
                                )),
                            },
                            now,
                        )
                        .map_err(ExecutionError::RunTransition)?;
                        self.persist_run(store, run)?;
                        return Err(ExecutionError::ApprovalRequired {
                            tool: parsed.name().as_str().to_string(),
                            approval_id,
                            reason,
                        });
                    }
                }

                let output = tool_runtime
                    .invoke(parsed.clone())
                    .map_err(ExecutionError::Tool)?;
                let summary = output.summary();
                let model_output = output.model_output();
                run.record_tool_completion(summary, now)
                    .map_err(ExecutionError::RunTransition)?;
                if supports_previous_response_id {
                    next_tool_outputs.push(ProviderToolOutput {
                        call_id: tool_call.call_id.clone(),
                        output: model_output,
                    });
                } else {
                    continuation_messages.push(ProviderContinuationMessage::ToolResult {
                        tool_call_id: tool_call.call_id.clone(),
                        content: model_output,
                    });
                }
            }

            if supports_previous_response_id {
                previous_response_id = Some(response.response_id.clone());
                pending_tool_outputs = next_tool_outputs;
            } else {
                previous_response_id = None;
                pending_tool_outputs.clear();
            }
            self.persist_run(store, run)?;
        }

        Err(ExecutionError::ProviderLoop {
            reason: format!(
                "provider exceeded {} tool-calling rounds without producing a final answer",
                MAX_PROVIDER_TOOL_ROUNDS
            ),
        })
    }

    pub fn supervisor_tick(
        &self,
        store: &PersistenceStore,
        now: i64,
        verifications: &[MissionVerificationSummary],
    ) -> Result<SupervisorTickReport, ExecutionError> {
        let state = store
            .load_execution_state()
            .map_err(ExecutionError::Store)?;
        let missions = state
            .missions
            .into_iter()
            .map(MissionSpec::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        let jobs = state
            .jobs
            .into_iter()
            .map(JobSpec::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        let runs = state
            .runs
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;

        let tick = self.supervisor.tick(SupervisorTickInput {
            now,
            missions: &missions,
            jobs: &jobs,
            runs: &runs,
            verifications,
        });

        let mission_by_id = missions
            .into_iter()
            .map(|mission| (mission.id.clone(), mission))
            .collect::<BTreeMap<_, _>>();
        let job_by_id = jobs
            .into_iter()
            .map(|job| (job.id.clone(), job))
            .collect::<BTreeMap<_, _>>();
        let mut report = SupervisorTickReport {
            actions: tick.actions.clone(),
            queued_jobs: 0,
            dispatched_jobs: 0,
            blocked_jobs: 0,
            deferred_missions: 0,
            completed_missions: 0,
            budget_remaining: tick.budget_remaining,
        };

        for action in &tick.actions {
            match action {
                SupervisorAction::QueueJob(job) => {
                    store
                        .put_job(
                            &JobRecord::try_from(job.as_ref())
                                .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                    touch_mission(store, &mission_by_id, &job.mission_id, now)?;
                    report.queued_jobs += 1;
                }
                SupervisorAction::DispatchJob { job_id, .. } => {
                    let mut job = job_by_id
                        .get(job_id)
                        .cloned()
                        .ok_or_else(|| ExecutionError::MissingJob { id: job_id.clone() })?;
                    job.status = JobStatus::Running;
                    job.updated_at = now;
                    if job.started_at.is_none() {
                        job.started_at = Some(now);
                    }
                    store
                        .put_job(
                            &JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                    touch_mission(store, &mission_by_id, &job.mission_id, now)?;
                    report.dispatched_jobs += 1;
                }
                SupervisorAction::RequestApproval { job_id, reason } => {
                    let mut job = job_by_id
                        .get(job_id)
                        .cloned()
                        .ok_or_else(|| ExecutionError::MissingJob { id: job_id.clone() })?;
                    job.status = JobStatus::Blocked;
                    job.error = Some(reason.clone());
                    job.updated_at = now;
                    store
                        .put_job(
                            &JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                    touch_mission(store, &mission_by_id, &job.mission_id, now)?;
                    report.blocked_jobs += 1;
                }
                SupervisorAction::DeferMission { mission_id, .. } => {
                    touch_mission(store, &mission_by_id, mission_id, now)?;
                    report.deferred_missions += 1;
                }
                SupervisorAction::CompleteMission { mission_id } => {
                    let mut mission = mission_by_id.get(mission_id).cloned().ok_or_else(|| {
                        ExecutionError::MissingMission {
                            id: mission_id.clone(),
                        }
                    })?;
                    mission.status = MissionStatus::Completed;
                    mission.updated_at = now;
                    mission.completed_at = Some(now);
                    store
                        .put_mission(
                            &MissionRecord::try_from(&mission)
                                .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                    report.completed_missions += 1;
                }
            }
        }

        Ok(report)
    }

    pub fn execute_mission_turn_job(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        job_id: &str,
        now: i64,
    ) -> Result<MissionTurnExecutionReport, ExecutionError> {
        let mut job = JobSpec::try_from(
            store
                .get_job(job_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingJob {
                    id: job_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let mut mission = MissionSpec::try_from(
            store
                .get_mission(&job.mission_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingMission {
                    id: job.mission_id.clone(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let session = store
            .get_session(&mission.session_id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| ExecutionError::MissingSession {
                id: mission.session_id.clone(),
            })?;

        let goal = match &job.input {
            JobExecutionInput::MissionTurn { mission_id, goal } if mission_id == &mission.id => {
                goal.clone()
            }
            _ => {
                return Err(ExecutionError::UnsupportedJobInput {
                    id: job.id.clone(),
                    kind: job.kind.as_str().to_string(),
                });
            }
        };

        let run_id = job
            .run_id
            .clone()
            .unwrap_or_else(|| format!("run-{}", job.id));
        let mut run = RunEngine::new(
            run_id.clone(),
            session.id.clone(),
            Some(mission.id.as_str()),
            now,
        );
        run.start(now).map_err(ExecutionError::RunTransition)?;
        store
            .put_run(
                &RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

        job.status = JobStatus::Running;
        job.run_id = Some(run_id.clone());
        job.error = None;
        job.updated_at = now;
        if job.started_at.is_none() {
            job.started_at = Some(now);
        }
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;

        mission.status = MissionStatus::Running;
        mission.updated_at = now;
        store
            .put_mission(
                &MissionRecord::try_from(&mission).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

        let user_entry = TranscriptEntry::user(
            format!("transcript-{}-01-user", job.id),
            session.id.clone(),
            Some(run_id.as_str()),
            &goal,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&user_entry))
            .map_err(ExecutionError::Store)?;

        let response = match self.execute_provider_turn_loop(
            store,
            provider,
            &session.id,
            session.prompt_override.clone(),
            &mut run,
            None,
            now,
        ) {
            Ok(response) => response,
            Err(source @ ExecutionError::ApprovalRequired { .. }) => {
                job.status = JobStatus::Blocked;
                job.error = Some(source.to_string());
                job.updated_at = now;
                mission.updated_at = now;
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                store
                    .put_mission(
                        &MissionRecord::try_from(&mission)
                            .map_err(ExecutionError::RecordConversion)?,
                    )
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
            Err(source) => {
                if !matches!(
                    source,
                    ExecutionError::PermissionDenied { .. }
                        | ExecutionError::ApprovalRequired { .. }
                ) {
                    run.fail(source.to_string(), now)
                        .map_err(ExecutionError::RunTransition)?;
                    self.persist_run(store, &run)?;
                }
                job.status = JobStatus::Failed;
                job.error = Some(source.to_string());
                job.finished_at = Some(now);
                job.updated_at = now;
                mission.updated_at = now;
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                store
                    .put_mission(
                        &MissionRecord::try_from(&mission)
                            .map_err(ExecutionError::RecordConversion)?,
                    )
                    .map_err(ExecutionError::Store)?;
                return Err(source);
            }
        };

        run.complete(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;

        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-{}-02-assistant", job.id),
            session.id,
            Some(run_id.as_str()),
            &response.output_text,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&assistant_entry))
            .map_err(ExecutionError::Store)?;

        job.status = JobStatus::Completed;
        job.result = Some(JobResult::Summary {
            outcome: response.output_text.clone(),
        });
        job.finished_at = Some(now);
        job.updated_at = now;
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;

        Ok(MissionTurnExecutionReport {
            job_id: job.id,
            run_id,
            response_id: response.response_id,
            output_text: response.output_text,
        })
    }

    pub fn execute_chat_turn(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        session_id: &str,
        message: &str,
        now: i64,
    ) -> Result<ChatTurnExecutionReport, ExecutionError> {
        let mut session = store
            .get_session(session_id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| ExecutionError::MissingSession {
                id: session_id.to_string(),
            })?;
        let run_id = format!("run-chat-{session_id}-{now}");
        let mut run = RunEngine::new(run_id.clone(), session.id.clone(), None, now);
        run.start(now).map_err(ExecutionError::RunTransition)?;
        store
            .put_run(
                &RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

        let user_entry = TranscriptEntry::user(
            format!("transcript-chat-{session_id}-{now}-01-user"),
            session.id.clone(),
            Some(run_id.as_str()),
            message,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&user_entry))
            .map_err(ExecutionError::Store)?;

        session.updated_at = now;
        store.put_session(&session).map_err(ExecutionError::Store)?;

        let response = match self.execute_provider_turn_loop(
            store,
            provider,
            &session.id,
            session.prompt_override.clone(),
            &mut run,
            None,
            now,
        ) {
            Ok(response) => response,
            Err(source) => {
                if !matches!(
                    source,
                    ExecutionError::PermissionDenied { .. }
                        | ExecutionError::ApprovalRequired { .. }
                ) {
                    run.fail(source.to_string(), now)
                        .map_err(ExecutionError::RunTransition)?;
                    self.persist_run(store, &run)?;
                }
                return Err(source);
            }
        };

        run.complete(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;

        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-chat-{session_id}-{now}-02-assistant"),
            session.id.clone(),
            Some(run_id.as_str()),
            &response.output_text,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&assistant_entry))
            .map_err(ExecutionError::Store)?;

        Ok(ChatTurnExecutionReport {
            session_id: session.id,
            run_id,
            response_id: response.response_id,
            output_text: response.output_text,
        })
    }

    pub fn approve_model_run(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        run_id: &str,
        approval_id: &str,
        now: i64,
    ) -> Result<ApprovalContinuationReport, ExecutionError> {
        let run_snapshot = RunSnapshot::try_from(
            store
                .get_run(run_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingRun {
                    id: run_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let mut run = RunEngine::from_snapshot(run_snapshot);
        let loop_state =
            run.snapshot()
                .provider_loop
                .clone()
                .ok_or_else(|| ExecutionError::ProviderLoop {
                    reason: format!("run {run_id} has no persisted provider continuation state"),
                })?;
        let pending_approval =
            loop_state
                .pending_approval
                .clone()
                .ok_or_else(|| ExecutionError::ProviderLoop {
                    reason: format!("run {run_id} has no pending provider approval to resume"),
                })?;
        if pending_approval.approval_id != approval_id {
            return Err(ExecutionError::ProviderLoop {
                reason: format!(
                    "approval {approval_id} does not match pending provider approval {}",
                    pending_approval.approval_id
                ),
            });
        }

        let session = store
            .get_session(&run.snapshot().session_id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| ExecutionError::MissingSession {
                id: run.snapshot().session_id.clone(),
            })?;
        let mut job = self.find_job_by_run_id(store, run_id)?;
        let mut mission = if let Some(job) = job.as_ref() {
            Some(
                MissionSpec::try_from(
                    store
                        .get_mission(&job.mission_id)
                        .map_err(ExecutionError::Store)?
                        .ok_or_else(|| ExecutionError::MissingMission {
                            id: job.mission_id.clone(),
                        })?,
                )
                .map_err(ExecutionError::RecordConversion)?,
            )
        } else {
            None
        };

        let parsed = ToolCall::from_openai_function(
            &pending_approval.tool_name,
            &pending_approval.tool_arguments,
        )
        .map_err(|source| ExecutionError::ToolCallParse {
            name: pending_approval.tool_name.clone(),
            reason: source.to_string(),
        })?;
        let catalog = ToolCatalog::default();
        let definition =
            catalog
                .definition_for_call(&parsed)
                .ok_or_else(|| ExecutionError::ToolCallParse {
                    name: pending_approval.tool_name.clone(),
                    reason: "tool is not in the catalog".to_string(),
                })?;
        let permission = self.permissions.resolve(definition, &parsed);
        if matches!(permission.action, PermissionAction::Deny) {
            let reason = format!(
                "tool {} denied by permission policy: {}",
                parsed.name().as_str(),
                permission.reason
            );
            run.fail(reason.clone(), now)
                .map_err(ExecutionError::RunTransition)?;
            self.persist_run(store, &run)?;
            if let Some(job) = job.as_mut() {
                job.status = JobStatus::Failed;
                job.error = Some(reason.clone());
                job.finished_at = Some(now);
                job.updated_at = now;
                store
                    .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
            }
            if let Some(mission) = mission.as_mut() {
                mission.updated_at = now;
                store
                    .put_mission(
                        &MissionRecord::try_from(&*mission)
                            .map_err(ExecutionError::RecordConversion)?,
                    )
                    .map_err(ExecutionError::Store)?;
            }
            return Err(ExecutionError::PermissionDenied {
                tool: parsed.name().as_str().to_string(),
                reason,
            });
        }

        run.resolve_approval(approval_id, now)
            .map_err(ExecutionError::RunTransition)?;
        if run.snapshot().status == RunStatus::Resuming {
            run.resume(now).map_err(ExecutionError::RunTransition)?;
        }

        if let Some(job) = job.as_mut() {
            job.status = JobStatus::Running;
            job.error = None;
            job.updated_at = now;
            if job.started_at.is_none() {
                job.started_at = Some(now);
            }
            store
                .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
                .map_err(ExecutionError::Store)?;
        }
        if let Some(mission) = mission.as_mut() {
            mission.status = MissionStatus::Running;
            mission.updated_at = now;
            store
                .put_mission(
                    &MissionRecord::try_from(&*mission)
                        .map_err(ExecutionError::RecordConversion)?,
                )
                .map_err(ExecutionError::Store)?;
        }

        let output = ToolRuntime::new(self.workspace.clone())
            .invoke(parsed.clone())
            .map_err(ExecutionError::Tool)?;
        let summary = output.summary();
        let model_output = output.model_output();
        run.record_tool_completion(summary, now)
            .map_err(ExecutionError::RunTransition)?;

        let mut resumed_loop_state = loop_state;
        resumed_loop_state.pending_approval = None;
        if provider
            .descriptor()
            .capabilities
            .supports_previous_response_id
        {
            resumed_loop_state
                .pending_tool_outputs
                .push(ProviderToolOutput {
                    call_id: pending_approval.provider_tool_call_id.clone(),
                    output: model_output,
                });
        } else {
            resumed_loop_state.continuation_messages.push(
                ProviderContinuationMessage::ToolResult {
                    tool_call_id: pending_approval.provider_tool_call_id.clone(),
                    content: model_output,
                },
            );
        }
        run.set_provider_loop_state(resumed_loop_state.clone(), now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;

        let response = match self.execute_provider_turn_loop(
            store,
            provider,
            &session.id,
            session.prompt_override.clone(),
            &mut run,
            Some(resumed_loop_state),
            now,
        ) {
            Ok(response) => response,
            Err(
                ref source @ ExecutionError::ApprovalRequired {
                    approval_id: ref next_approval_id,
                    ..
                },
            ) => {
                if let Some(job) = job.as_mut() {
                    job.status = JobStatus::Blocked;
                    job.error = Some(source.to_string());
                    job.updated_at = now;
                    store
                        .put_job(
                            &JobRecord::try_from(&*job)
                                .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                }
                if let Some(mission) = mission.as_mut() {
                    mission.updated_at = now;
                    store
                        .put_mission(
                            &MissionRecord::try_from(&*mission)
                                .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                }
                return Ok(ApprovalContinuationReport {
                    run_id: run_id.to_string(),
                    run_status: RunStatus::WaitingApproval,
                    response_id: None,
                    output_text: None,
                    approval_id: Some(next_approval_id.clone()),
                });
            }
            Err(source) => {
                if !matches!(
                    source,
                    ExecutionError::PermissionDenied { .. }
                        | ExecutionError::ApprovalRequired { .. }
                ) {
                    run.fail(source.to_string(), now)
                        .map_err(ExecutionError::RunTransition)?;
                    self.persist_run(store, &run)?;
                }
                if let Some(job) = job.as_mut() {
                    job.status = JobStatus::Failed;
                    job.error = Some(source.to_string());
                    job.finished_at = Some(now);
                    job.updated_at = now;
                    store
                        .put_job(
                            &JobRecord::try_from(&*job)
                                .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                }
                if let Some(mission) = mission.as_mut() {
                    mission.updated_at = now;
                    store
                        .put_mission(
                            &MissionRecord::try_from(&*mission)
                                .map_err(ExecutionError::RecordConversion)?,
                        )
                        .map_err(ExecutionError::Store)?;
                }
                return Err(source);
            }
        };

        run.complete(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;

        let assistant_entry = TranscriptEntry::assistant(
            format!("transcript-run-{run_id}-{now}-assistant"),
            session.id.clone(),
            Some(run_id),
            &response.output_text,
            now,
        );
        store
            .put_transcript(&TranscriptRecord::from(&assistant_entry))
            .map_err(ExecutionError::Store)?;

        if let Some(job) = job.as_mut() {
            job.status = JobStatus::Completed;
            job.result = Some(JobResult::Summary {
                outcome: response.output_text.clone(),
            });
            job.finished_at = Some(now);
            job.updated_at = now;
            store
                .put_job(&JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?)
                .map_err(ExecutionError::Store)?;
        }

        Ok(ApprovalContinuationReport {
            run_id: run_id.to_string(),
            run_status: RunStatus::Completed,
            response_id: Some(response.response_id),
            output_text: Some(response.output_text),
            approval_id: None,
        })
    }

    pub fn request_tool_approval(
        &self,
        store: &PersistenceStore,
        job_id: &str,
        run_id: &str,
        tool_call: &ToolCall,
        now: i64,
    ) -> Result<ToolExecutionReport, ExecutionError> {
        self.execute_tool_call_internal(
            store,
            job_id,
            run_id,
            tool_call,
            ToolExecutionContext {
                approved_approval_id: None,
                workspace_root: None,
                evidence: None,
                now,
            },
        )
    }

    pub fn resume_tool_call(
        &self,
        store: &PersistenceStore,
        request: ToolResumeRequest<'_>,
    ) -> Result<ToolExecutionReport, ExecutionError> {
        self.execute_tool_call_internal(
            store,
            request.job_id,
            request.run_id,
            request.tool_call,
            ToolExecutionContext {
                approved_approval_id: Some(request.approval_id),
                workspace_root: Some(request.workspace_root),
                evidence: request.evidence,
                now: request.now,
            },
        )
    }

    fn execute_tool_call_internal(
        &self,
        store: &PersistenceStore,
        job_id: &str,
        run_id: &str,
        tool_call: &ToolCall,
        context: ToolExecutionContext<'_>,
    ) -> Result<ToolExecutionReport, ExecutionError> {
        let catalog = ToolCatalog::default();
        let definition = catalog.definition_for_call(tool_call).ok_or_else(|| {
            ExecutionError::UnsupportedJobInput {
                id: job_id.to_string(),
                kind: tool_call.name().as_str().to_string(),
            }
        })?;
        let mut job = JobSpec::try_from(
            store
                .get_job(job_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingJob {
                    id: job_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let mut mission = MissionSpec::try_from(
            store
                .get_mission(&job.mission_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingMission {
                    id: job.mission_id.clone(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let run_snapshot = RunSnapshot::try_from(
            store
                .get_run(run_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingRun {
                    id: run_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let mut run = RunEngine::from_snapshot(run_snapshot);
        let permission = self.permissions.resolve(definition, tool_call);

        if matches!(permission.action, PermissionAction::Deny) {
            let reason = format!(
                "tool {} denied by permission policy: {}",
                tool_call.name().as_str(),
                permission.reason
            );
            run.fail(reason.clone(), context.now)
                .map_err(ExecutionError::RunTransition)?;
            job.status = JobStatus::Failed;
            job.error = Some(reason.clone());
            job.updated_at = context.now;
            job.finished_at = Some(context.now);
            mission.updated_at = context.now;
            store
                .put_run(
                    &RunRecord::try_from(run.snapshot())
                        .map_err(ExecutionError::RecordConversion)?,
                )
                .map_err(ExecutionError::Store)?;
            store
                .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                .map_err(ExecutionError::Store)?;
            store
                .put_mission(
                    &MissionRecord::try_from(&mission).map_err(ExecutionError::RecordConversion)?,
                )
                .map_err(ExecutionError::Store)?;
            return Err(ExecutionError::PermissionDenied {
                tool: tool_call.name().as_str().to_string(),
                reason,
            });
        }

        if context.approved_approval_id.is_none()
            && matches!(permission.action, PermissionAction::Ask)
        {
            let approval_id = format!("approval-{}-{}", job.id, tool_call.name().as_str());
            let reason = format!(
                "tool {} requires approval: {} ({})",
                tool_call.name().as_str(),
                tool_call.summary(),
                permission.reason
            );
            run.wait_for_approval(
                ApprovalRequest::new(
                    approval_id.clone(),
                    tool_call.name().as_str(),
                    &reason,
                    context.now,
                ),
                context.now,
            )
            .map_err(ExecutionError::RunTransition)?;
            job.status = JobStatus::Blocked;
            job.error = Some(reason);
            job.updated_at = context.now;
            mission.updated_at = context.now;
            store
                .put_run(
                    &RunRecord::try_from(run.snapshot())
                        .map_err(ExecutionError::RecordConversion)?,
                )
                .map_err(ExecutionError::Store)?;
            store
                .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                .map_err(ExecutionError::Store)?;
            store
                .put_mission(
                    &MissionRecord::try_from(&mission).map_err(ExecutionError::RecordConversion)?,
                )
                .map_err(ExecutionError::Store)?;
            return Ok(ToolExecutionReport {
                job_id: job.id,
                run_id: run_id.to_string(),
                run_status: run.snapshot().status,
                approval_id: Some(approval_id),
                output_summary: None,
                evidence_refs: run.snapshot().evidence_refs.clone(),
            });
        }

        let Some(workspace_root) = context.workspace_root else {
            return Ok(ToolExecutionReport {
                job_id: job.id,
                run_id: run_id.to_string(),
                run_status: run.snapshot().status,
                approval_id: None,
                output_summary: None,
                evidence_refs: run.snapshot().evidence_refs.clone(),
            });
        };

        if let Some(approval_id) = context.approved_approval_id {
            run.resolve_approval(approval_id, context.now)
                .map_err(ExecutionError::RunTransition)?;
            if run.snapshot().status == RunStatus::Resuming {
                run.resume(context.now)
                    .map_err(ExecutionError::RunTransition)?;
            }
        }

        job.status = JobStatus::Running;
        job.error = None;
        job.updated_at = context.now;
        if job.started_at.is_none() {
            job.started_at = Some(context.now);
        }
        mission.status = MissionStatus::Running;
        mission.updated_at = context.now;

        let mut tool_runtime = ToolRuntime::new(WorkspaceRef::new(workspace_root));
        let output = tool_runtime
            .invoke(tool_call.clone())
            .map_err(ExecutionError::Tool)?;
        let output_summary = output.summary();
        run.record_tool_completion(output_summary.clone(), context.now)
            .map_err(ExecutionError::RunTransition)?;
        if let Some(bundle) = context.evidence {
            run.record_evidence(bundle, context.now)
                .map_err(ExecutionError::RunTransition)?;
        }

        match output {
            ToolOutput::ProcessStart(start) => {
                run.wait_for_process(
                    ActiveProcess::new(
                        start.process_id,
                        process_kind_label(start.kind),
                        start.pid_ref,
                        context.now,
                    ),
                    context.now,
                )
                .map_err(ExecutionError::RunTransition)?;
            }
            _ => {
                run.complete(output_summary.clone(), context.now)
                    .map_err(ExecutionError::RunTransition)?;
                job.status = JobStatus::Completed;
                job.result = Some(JobResult::Summary {
                    outcome: output_summary.clone(),
                });
                job.finished_at = Some(context.now);
            }
        }

        store
            .put_run(
                &RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;
        store
            .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;
        store
            .put_mission(
                &MissionRecord::try_from(&mission).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

        Ok(ToolExecutionReport {
            job_id: job.id,
            run_id: run_id.to_string(),
            run_status: run.snapshot().status,
            approval_id: None,
            output_summary: Some(output_summary),
            evidence_refs: run.snapshot().evidence_refs.clone(),
        })
    }
}

fn touch_mission(
    store: &PersistenceStore,
    mission_by_id: &BTreeMap<String, MissionSpec>,
    mission_id: &str,
    now: i64,
) -> Result<(), ExecutionError> {
    let mut mission =
        mission_by_id
            .get(mission_id)
            .cloned()
            .ok_or_else(|| ExecutionError::MissingMission {
                id: mission_id.to_string(),
            })?;
    mission.updated_at = now;
    store
        .put_mission(&MissionRecord::try_from(&mission).map_err(ExecutionError::RecordConversion)?)
        .map_err(ExecutionError::Store)?;
    Ok(())
}

fn process_kind_label(kind: ProcessKind) -> &'static str {
    match kind {
        ProcessKind::Exec => "exec",
    }
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingJob { id } => write!(formatter, "execution job {id} was not found"),
            Self::MissingMission { id } => {
                write!(formatter, "execution mission {id} was not found")
            }
            Self::MissingRun { id } => write!(formatter, "execution run {id} was not found"),
            Self::MissingSession { id } => {
                write!(formatter, "execution session {id} was not found")
            }
            Self::UnsupportedJobInput { id, kind } => {
                write!(
                    formatter,
                    "execution job {id} has unsupported input for kind {kind}"
                )
            }
            Self::PermissionDenied { tool, reason } => {
                write!(
                    formatter,
                    "execution permission denied for {tool}: {reason}"
                )
            }
            Self::ApprovalRequired {
                tool,
                approval_id,
                reason,
            } => write!(
                formatter,
                "execution approval required for {tool} ({approval_id}): {reason}"
            ),
            Self::Provider(source) => write!(formatter, "execution provider error: {source}"),
            Self::ProviderLoop { reason } => {
                write!(formatter, "execution provider loop error: {reason}")
            }
            Self::RecordConversion(source) => {
                write!(formatter, "execution record conversion error: {source}")
            }
            Self::RunTransition(source) => {
                write!(formatter, "execution run transition error: {source}")
            }
            Self::Store(source) => write!(formatter, "execution store error: {source}"),
            Self::ToolCallParse { name, reason } => {
                write!(
                    formatter,
                    "execution failed to parse tool call {name}: {reason}"
                )
            }
            Self::Tool(source) => write!(formatter, "execution tool error: {source}"),
        }
    }
}

impl Error for ExecutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Provider(source) => Some(source),
            Self::RecordConversion(source) => Some(source),
            Self::RunTransition(source) => Some(source),
            Self::Store(source) => Some(source),
            Self::Tool(source) => Some(source),
            Self::MissingJob { .. }
            | Self::MissingMission { .. }
            | Self::MissingRun { .. }
            | Self::MissingSession { .. }
            | Self::PermissionDenied { .. }
            | Self::ApprovalRequired { .. }
            | Self::ProviderLoop { .. }
            | Self::ToolCallParse { .. }
            | Self::UnsupportedJobInput { .. } => None,
        }
    }
}
