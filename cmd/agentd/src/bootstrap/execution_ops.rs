use super::*;
use agent_runtime::run::{RunEngine, RunSnapshot};
use agent_runtime::scheduler::MissionVerificationSummary;
use agent_runtime::tool::ToolCall;
use std::sync::atomic::AtomicBool;

impl App {
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
    pub fn background_worker_tick(
        &self,
        now: i64,
    ) -> Result<execution::BackgroundWorkerTickReport, BootstrapError> {
        let store = self.store()?;
        let provider = self.provider_driver()?;
        self.execution_service()
            .background_worker_tick(&store, provider.as_ref(), now)
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
