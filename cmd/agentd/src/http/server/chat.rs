use super::*;
use crate::bootstrap::BootstrapError;
use crate::execution::ExecutionError;
use crate::http::types::{
    ApproveRunRequest, ChatTurnRequest, ErrorResponse, WorkerOutcomeResponse,
    WorkerStreamEventResponse,
};
use std::io::{self, Write};
use std::sync::atomic::AtomicBool;

fn map_chat_turn_outcome(
    result: Result<crate::execution::ChatTurnExecutionReport, BootstrapError>,
) -> WorkerOutcomeResponse {
    match result {
        Ok(report) => WorkerOutcomeResponse::ChatCompleted { report },
        Err(BootstrapError::Execution(ExecutionError::ApprovalRequired {
            approval_id,
            reason,
            ..
        })) => WorkerOutcomeResponse::ApprovalRequired {
            approval_id,
            reason,
        },
        Err(BootstrapError::Execution(ExecutionError::InterruptedByQueuedInput)) => {
            WorkerOutcomeResponse::InterruptedByQueuedInput
        }
        Err(error) => WorkerOutcomeResponse::Failed {
            reason: error.to_string(),
        },
    }
}

fn map_approve_run_outcome(
    result: Result<crate::execution::ApprovalContinuationReport, BootstrapError>,
) -> WorkerOutcomeResponse {
    match result {
        Ok(report) => {
            if let Some(approval_id) = report.approval_id.clone() {
                WorkerOutcomeResponse::ApprovalRequired {
                    approval_id,
                    reason: "model requested another approval".to_string(),
                }
            } else {
                WorkerOutcomeResponse::ApprovalCompleted { report }
            }
        }
        Err(BootstrapError::Execution(ExecutionError::InterruptedByQueuedInput)) => {
            WorkerOutcomeResponse::InterruptedByQueuedInput
        }
        Err(BootstrapError::Execution(ExecutionError::ApprovalRequired {
            approval_id,
            reason,
            ..
        })) => WorkerOutcomeResponse::ApprovalRequired {
            approval_id,
            reason,
        },
        Err(error) => WorkerOutcomeResponse::Failed {
            reason: error.to_string(),
        },
    }
}

fn write_chunk<W: Write + ?Sized>(writer: &mut W, bytes: &[u8]) -> io::Result<()> {
    write!(writer, "{:X}\r\n", bytes.len())?;
    writer.write_all(bytes)?;
    writer.write_all(b"\r\n")?;
    writer.flush()
}

fn finish_chunked_response<W: Write + ?Sized>(writer: &mut W) -> io::Result<()> {
    writer.write_all(b"0\r\n\r\n")?;
    writer.flush()
}

fn respond_ndjson_stream<F>(request: Request, mut work: F) -> std::io::Result<()>
where
    F: FnMut(&mut dyn FnMut(WorkerStreamEventResponse) -> io::Result<()>) -> std::io::Result<()>,
{
    let mut writer = request.into_writer();
    writer.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson; charset=utf-8\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
    )?;
    writer.flush()?;

    let mut emit = |event: WorkerStreamEventResponse| {
        let mut payload =
            serde_json::to_vec(&event).map_err(|error| io::Error::other(error.to_string()))?;
        payload.push(b'\n');
        write_chunk(&mut *writer, &payload)
    };

    let work_result = work(&mut emit);
    let finish_result = finish_chunked_response(&mut *writer);
    work_result?;
    finish_result
}

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

pub(super) fn handle_chat_turn_stream(app: &App, mut request: Request) -> std::io::Result<()> {
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
    let app = app.clone();
    respond_ndjson_stream(request, move |emit| {
        let interrupt_after_tool_step = AtomicBool::new(body.interrupt_after_tool_step);
        let mut observer = |event| {
            let _ = emit(WorkerStreamEventResponse::ChatEvent { event });
        };
        let outcome = map_chat_turn_outcome(app.execute_chat_turn_with_control_and_observer(
            &body.session_id,
            &body.message,
            body.now,
            Some(&interrupt_after_tool_step),
            &mut observer,
        ));
        emit(WorkerStreamEventResponse::Finished { outcome })
    })
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

pub(super) fn handle_approve_run_stream(app: &App, mut request: Request) -> std::io::Result<()> {
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
    let app = app.clone();
    respond_ndjson_stream(request, move |emit| {
        let interrupt_after_tool_step = AtomicBool::new(body.interrupt_after_tool_step);
        let mut observer = |event| {
            let _ = emit(WorkerStreamEventResponse::ChatEvent { event });
        };
        let outcome = map_approve_run_outcome(app.approve_run_with_control_and_observer(
            &body.run_id,
            &body.approval_id,
            body.now,
            Some(&interrupt_after_tool_step),
            &mut observer,
        ));
        emit(WorkerStreamEventResponse::Finished { outcome })
    })
}
