use super::*;
use crate::execution::ExecutionError;
use crate::http::types::{
    ApproveRunRequest, ChatTurnRequest, WorkerOutcomeResponse, WorkerStreamEventResponse,
};
use std::sync::atomic::AtomicBool;

impl DaemonClient {
    pub fn execute_chat_turn_with_control_and_observer(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatTurnExecutionReport, BootstrapError> {
        self.execute_chat_turn_with_trace_control_and_observer(
            session_id,
            message,
            now,
            interrupt_after_tool_step,
            observer,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn execute_chat_turn_with_trace_control_and_observer(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut dyn FnMut(ChatExecutionEvent),
        surface: Option<&str>,
        entrypoint: Option<&str>,
    ) -> Result<ChatTurnExecutionReport, BootstrapError> {
        let mut final_outcome = None;
        self.post_json_long_stream(
            "/v1/chat/turn/stream",
            &ChatTurnRequest {
                session_id: session_id.to_string(),
                message: message.to_string(),
                now,
                interrupt_after_tool_step: interrupt_after_tool_step
                    .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::SeqCst)),
                surface: surface.map(str::to_string),
                entrypoint: entrypoint.map(str::to_string),
            },
            |line| {
                let event: WorkerStreamEventResponse =
                    serde_json::from_str(line).map_err(|error| {
                        BootstrapError::Stream(std::io::Error::other(format!(
                            "invalid daemon stream event: {error}"
                        )))
                    })?;
                match event {
                    WorkerStreamEventResponse::ChatEvent { event } => observer(event),
                    WorkerStreamEventResponse::Finished { outcome } => {
                        final_outcome = Some(outcome);
                    }
                }
                Ok(())
            },
        )?;
        match final_outcome.ok_or_else(|| {
            BootstrapError::Stream(std::io::Error::other(
                "daemon chat stream finished without a final outcome",
            ))
        })? {
            WorkerOutcomeResponse::ChatCompleted { report } => Ok(report),
            WorkerOutcomeResponse::ApprovalRequired {
                approval_id,
                reason,
            } => Err(BootstrapError::Execution(
                ExecutionError::ApprovalRequired {
                    tool: "remote_tool".to_string(),
                    approval_id,
                    reason,
                },
            )),
            WorkerOutcomeResponse::InterruptedByQueuedInput => Err(BootstrapError::Execution(
                ExecutionError::InterruptedByQueuedInput,
            )),
            WorkerOutcomeResponse::Failed { reason } => Err(BootstrapError::Usage { reason }),
            WorkerOutcomeResponse::ApprovalCompleted { .. } => Err(BootstrapError::Usage {
                reason: "unexpected approval response for chat turn".to_string(),
            }),
        }
    }

    pub fn approve_run_with_control_and_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ApprovalContinuationReport, BootstrapError> {
        let mut final_outcome = None;
        self.post_json_long_stream(
            "/v1/runs/approve/stream",
            &ApproveRunRequest {
                run_id: run_id.to_string(),
                approval_id: approval_id.to_string(),
                now,
                interrupt_after_tool_step: interrupt_after_tool_step
                    .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::SeqCst)),
            },
            |line| {
                let event: WorkerStreamEventResponse =
                    serde_json::from_str(line).map_err(|error| {
                        BootstrapError::Stream(std::io::Error::other(format!(
                            "invalid daemon stream event: {error}"
                        )))
                    })?;
                match event {
                    WorkerStreamEventResponse::ChatEvent { event } => observer(event),
                    WorkerStreamEventResponse::Finished { outcome } => {
                        final_outcome = Some(outcome);
                    }
                }
                Ok(())
            },
        )?;
        match final_outcome.ok_or_else(|| {
            BootstrapError::Stream(std::io::Error::other(
                "daemon approval stream finished without a final outcome",
            ))
        })? {
            WorkerOutcomeResponse::ApprovalCompleted { report } => Ok(report),
            WorkerOutcomeResponse::ApprovalRequired {
                approval_id,
                reason: _,
            } => Ok(ApprovalContinuationReport {
                run_id: run_id.to_string(),
                run_status: agent_runtime::run::RunStatus::WaitingApproval,
                response_id: None,
                output_text: None,
                approval_id: Some(approval_id),
            }),
            WorkerOutcomeResponse::InterruptedByQueuedInput => Err(BootstrapError::Execution(
                ExecutionError::InterruptedByQueuedInput,
            )),
            WorkerOutcomeResponse::Failed { reason } => Err(BootstrapError::Usage { reason }),
            WorkerOutcomeResponse::ChatCompleted { .. } => Err(BootstrapError::Usage {
                reason: "unexpected chat response for approval continuation".to_string(),
            }),
        }
    }
}
