use super::*;
use crate::bootstrap::BootstrapError;
use crate::execution::ExecutionError;
use crate::http::types::{
    ApproveRunRequest, ChatTurnRequest, ErrorResponse, WorkerOutcomeResponse,
};

pub(super) fn handle_chat_turn(app: &App, mut request: Request) -> std::io::Result<()> {
    let body: ChatTurnRequest = match parse_json_body(&mut request) {
        Ok(body) => body,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid json body: {error}"),
                },
            );
        }
    };
    match app.execute_chat_turn(&body.session_id, &body.message, body.now) {
        Ok(report) => respond_json(
            request,
            StatusCode(200),
            &WorkerOutcomeResponse::ChatCompleted { report },
        ),
        Err(BootstrapError::Execution(ExecutionError::ApprovalRequired {
            approval_id,
            reason,
            ..
        })) => respond_json(
            request,
            StatusCode(200),
            &WorkerOutcomeResponse::ApprovalRequired {
                approval_id,
                reason,
            },
        ),
        Err(BootstrapError::Execution(ExecutionError::InterruptedByQueuedInput)) => respond_json(
            request,
            StatusCode(200),
            &WorkerOutcomeResponse::InterruptedByQueuedInput,
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_approve_run(app: &App, mut request: Request) -> std::io::Result<()> {
    let body: ApproveRunRequest = match parse_json_body(&mut request) {
        Ok(body) => body,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid json body: {error}"),
                },
            );
        }
    };
    match app.approve_run(&body.run_id, &body.approval_id, body.now) {
        Ok(report) => {
            if let Some(approval_id) = report.approval_id.clone() {
                return respond_json(
                    request,
                    StatusCode(200),
                    &WorkerOutcomeResponse::ApprovalRequired {
                        approval_id,
                        reason: "model requested another approval".to_string(),
                    },
                );
            }
            respond_json(
                request,
                StatusCode(200),
                &WorkerOutcomeResponse::ApprovalCompleted { report },
            )
        }
        Err(BootstrapError::Execution(ExecutionError::InterruptedByQueuedInput)) => respond_json(
            request,
            StatusCode(200),
            &WorkerOutcomeResponse::InterruptedByQueuedInput,
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}
