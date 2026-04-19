#![cfg_attr(not(test), allow(dead_code))]

use agent_persistence::{
    JobRecord, JobRepository, MissionRecord, MissionRepository, PersistenceStore,
    RecordConversionError, RunRecord, RunRepository, SessionRepository, StoreError,
    TranscriptRecord, TranscriptRepository,
};
use agent_runtime::mission::{
    JobExecutionInput, JobResult, JobSpec, JobStatus, MissionSpec, MissionStatus,
};
use agent_runtime::provider::{
    ProviderDriver, ProviderError, ProviderMessage, ProviderRequest, ProviderStreamMode,
};
use agent_runtime::run::{
    ActiveProcess, ApprovalRequest, RunEngine, RunSnapshot, RunStatus, RunTransitionError,
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionService {
    supervisor: SupervisorLoop,
}

#[derive(Debug)]
pub enum ExecutionError {
    MissingJob { id: String },
    MissingMission { id: String },
    MissingRun { id: String },
    MissingSession { id: String },
    UnsupportedJobInput { id: String, kind: String },
    Provider(ProviderError),
    RecordConversion(RecordConversionError),
    RunTransition(RunTransitionError),
    Store(StoreError),
    Tool(ToolError),
}

impl ExecutionService {
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

        let request = ProviderRequest {
            model: None,
            instructions: session.prompt_override.clone(),
            messages: store
                .list_transcripts_for_session(&session.id)
                .map_err(ExecutionError::Store)?
                .into_iter()
                .map(|record| {
                    let role = MessageRole::try_from(record.kind.as_str()).map_err(|_| {
                        ExecutionError::RecordConversion(
                            RecordConversionError::InvalidMessageRole {
                                value: record.kind.clone(),
                            },
                        )
                    })?;
                    Ok(ProviderMessage {
                        role,
                        content: record.content,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            max_output_tokens: Some(512),
            stream: ProviderStreamMode::Disabled,
        };

        let response = match provider.complete(&request) {
            Ok(response) => response,
            Err(source) => {
                run.fail(source.to_string(), now)
                    .map_err(ExecutionError::RunTransition)?;
                store
                    .put_run(
                        &RunRecord::try_from(run.snapshot())
                            .map_err(ExecutionError::RecordConversion)?,
                    )
                    .map_err(ExecutionError::Store)?;
                job.status = JobStatus::Failed;
                job.error = Some(source.to_string());
                job.finished_at = Some(now);
                job.updated_at = now;
                store
                    .put_job(&JobRecord::try_from(&job).map_err(ExecutionError::RecordConversion)?)
                    .map_err(ExecutionError::Store)?;
                return Err(ExecutionError::Provider(source));
            }
        };

        run.begin_provider_stream(&response.response_id, &response.model, now)
            .map_err(ExecutionError::RunTransition)?;
        run.push_provider_text(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        run.finish_provider_stream(now)
            .map_err(ExecutionError::RunTransition)?;
        run.complete(&response.output_text, now)
            .map_err(ExecutionError::RunTransition)?;
        store
            .put_run(
                &RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;

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

        if context.approved_approval_id.is_none() && definition.policy.requires_approval {
            let approval_id = format!("approval-{}-{}", job.id, tool_call.name().as_str());
            let reason = format!(
                "tool {} requires approval: {}",
                tool_call.name().as_str(),
                tool_call.summary()
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
        ProcessKind::ShellSnippet => "shell_snippet",
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
            Self::Provider(source) => write!(formatter, "execution provider error: {source}"),
            Self::RecordConversion(source) => {
                write!(formatter, "execution record conversion error: {source}")
            }
            Self::RunTransition(source) => {
                write!(formatter, "execution run transition error: {source}")
            }
            Self::Store(source) => write!(formatter, "execution store error: {source}"),
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
            | Self::UnsupportedJobInput { .. } => None,
        }
    }
}
