use super::*;
use agent_runtime::mission::{JobResult, MissionStatus};
use agent_runtime::run::{ActiveProcess, ApprovalRequest};
use agent_runtime::tool::{ProcessKind, ToolCatalog, ToolOutput, ToolRuntime};

#[derive(Debug, Clone, Copy)]
struct ToolExecutionContext<'a> {
    approved_approval_id: Option<&'a str>,
    workspace_root: Option<&'a Path>,
    evidence: Option<&'a EvidenceBundle>,
    now: i64,
}

struct ToolExecutionFailureContext<'a> {
    store: &'a PersistenceStore,
    run: &'a mut RunEngine,
    job: &'a mut JobSpec,
    mission: &'a mut MissionSpec,
    now: i64,
}

impl ExecutionService {
    fn reject_tool_execution(
        &self,
        failure: ToolExecutionFailureContext<'_>,
        tool_name: &str,
        reason: String,
    ) -> Result<ToolExecutionReport, ExecutionError> {
        failure
            .run
            .fail(reason.clone(), failure.now)
            .map_err(ExecutionError::RunTransition)?;
        failure.job.status = JobStatus::Failed;
        failure.job.error = Some(reason.clone());
        failure.job.updated_at = failure.now;
        failure.job.finished_at = Some(failure.now);
        failure.mission.updated_at = failure.now;
        failure
            .store
            .put_run(
                &RunRecord::try_from(failure.run.snapshot())
                    .map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;
        failure
            .store
            .put_job(&JobRecord::try_from(&*failure.job).map_err(ExecutionError::RecordConversion)?)
            .map_err(ExecutionError::Store)?;
        failure
            .store
            .put_mission(
                &MissionRecord::try_from(&*failure.mission)
                    .map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;
        Err(ExecutionError::PermissionDenied {
            tool: tool_name.to_string(),
            reason,
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
        let mission_id =
            job.mission_id
                .clone()
                .ok_or_else(|| ExecutionError::UnsupportedJobInput {
                    id: job.id.clone(),
                    kind: job.kind.as_str().to_string(),
                })?;
        let mut mission = MissionSpec::try_from(
            store
                .get_mission(&mission_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingMission {
                    id: mission_id.clone(),
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
        let session_id = run_snapshot.session_id.clone();
        let mut run = RunEngine::from_snapshot(run_snapshot);
        if let Err(ExecutionError::PermissionDenied { tool, reason }) =
            self.ensure_agent_tool_allowed(store, &session_id, tool_call.name())
        {
            return self.reject_tool_execution(
                ToolExecutionFailureContext {
                    store,
                    run: &mut run,
                    job: &mut job,
                    mission: &mut mission,
                    now: context.now,
                },
                &tool,
                reason,
            );
        }
        let permission = self.permissions.resolve(definition, tool_call);

        if matches!(permission.action, PermissionAction::Deny) {
            let reason = format!(
                "tool {} denied by permission policy: {}",
                tool_call.name().as_str(),
                permission.reason
            );
            return self.reject_tool_execution(
                ToolExecutionFailureContext {
                    store,
                    run: &mut run,
                    job: &mut job,
                    mission: &mut mission,
                    now: context.now,
                },
                tool_call.name().as_str(),
                reason,
            );
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

        let output = match tool_call {
            ToolCall::PlanRead(_)
            | ToolCall::PlanWrite(_)
            | ToolCall::InitPlan(_)
            | ToolCall::AddTask(_)
            | ToolCall::SetTaskStatus(_)
            | ToolCall::AddTaskNote(_)
            | ToolCall::EditTask(_)
            | ToolCall::PlanSnapshot(_)
            | ToolCall::PlanLint(_)
            | ToolCall::ArtifactRead(_)
            | ToolCall::ArtifactSearch(_)
            | ToolCall::KnowledgeSearch(_)
            | ToolCall::KnowledgeRead(_)
            | ToolCall::SessionSearch(_)
            | ToolCall::SessionRead(_)
            | ToolCall::AgentList(_)
            | ToolCall::AgentRead(_)
            | ToolCall::AgentCreate(_)
            | ToolCall::ContinueLater(_)
            | ToolCall::ScheduleList(_)
            | ToolCall::ScheduleRead(_)
            | ToolCall::ScheduleCreate(_)
            | ToolCall::ScheduleUpdate(_)
            | ToolCall::ScheduleDelete(_)
            | ToolCall::MessageAgent(_)
            | ToolCall::GrantAgentChainContinuation(_) => {
                let mut tool_runtime = self.tool_runtime();
                self.execute_model_tool_call(
                    store,
                    &session_id,
                    &mut tool_runtime,
                    tool_call,
                    context.now,
                )?
            }
            _ => {
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
                let mut tool_runtime = ToolRuntime::with_shared_process_registry(
                    WorkspaceRef::new(workspace_root),
                    self.processes.clone(),
                );
                self.execute_model_tool_call(
                    store,
                    &session_id,
                    &mut tool_runtime,
                    tool_call,
                    context.now,
                )?
            }
        };
        let output_summary = output.summary();
        run.record_tool_completion(completed_tool_step_detail(tool_call, &output), context.now)
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
                    )
                    .with_command_details(start.command_display, start.cwd),
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

pub(super) fn process_kind_label(kind: ProcessKind) -> &'static str {
    match kind {
        ProcessKind::Exec => "exec",
    }
}

pub(super) fn completed_tool_step_detail(tool_call: &ToolCall, output: &ToolOutput) -> String {
    format!("{} -> {}", tool_call.summary(), output.summary())
}
