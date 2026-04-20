use super::*;
use crate::execution::ExecutionError;
use crate::http::types::{ApproveRunRequest, ChatTurnRequest, WorkerOutcomeResponse};
use std::sync::atomic::AtomicBool;

impl DaemonClient {
    pub fn execute_chat_turn_with_control_and_observer(
        &self,
        session_id: &str,
        message: &str,
        now: i64,
        _interrupt_after_tool_step: Option<&AtomicBool>,
        _observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatTurnExecutionReport, BootstrapError> {
        match self.post_json::<WorkerOutcomeResponse, _>(
            "/v1/chat/turn",
            &ChatTurnRequest {
                session_id: session_id.to_string(),
                message: message.to_string(),
                now,
            },
        )? {
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
        _interrupt_after_tool_step: Option<&AtomicBool>,
        _observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ApprovalContinuationReport, BootstrapError> {
        match self.post_json::<WorkerOutcomeResponse, _>(
            "/v1/runs/approve",
            &ApproveRunRequest {
                run_id: run_id.to_string(),
                approval_id: approval_id.to_string(),
                now,
            },
        )? {
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
