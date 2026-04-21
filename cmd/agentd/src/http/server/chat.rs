use super::*;
use crate::bootstrap::BootstrapError;
use crate::execution::ExecutionError;
use crate::http::types::{
    ApproveRunRequest, ChatTurnRequest, ErrorResponse, WorkerOutcomeResponse,
    WorkerStreamEventResponse,
};
use std::io::{self, Cursor, Read};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use tiny_http::Header;

struct ChannelReader {
    receiver: Receiver<Vec<u8>>,
    current: Cursor<Vec<u8>>,
    closed: bool,
}

impl ChannelReader {
    fn new(receiver: Receiver<Vec<u8>>) -> Self {
        Self {
            receiver,
            current: Cursor::new(Vec::new()),
            closed: false,
        }
    }
}

impl Read for ChannelReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let bytes_read = self.current.read(buf)?;
            if bytes_read > 0 {
                return Ok(bytes_read);
            }

            if self.closed {
                return Ok(0);
            }

            match self.receiver.recv() {
                Ok(chunk) => self.current = Cursor::new(chunk),
                Err(_) => self.closed = true,
            }
        }
    }
}

fn send_stream_event(sender: &Sender<Vec<u8>>, event: &WorkerStreamEventResponse) {
    let Ok(mut payload) = serde_json::to_vec(event) else {
        return;
    };
    payload.push(b'\n');
    let _ = sender.send(payload);
}

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

fn respond_ndjson_stream<F>(request: Request, work: F) -> std::io::Result<()>
where
    F: FnOnce(Sender<Vec<u8>>) + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || work(sender));

    let mut response = Response::new(
        StatusCode(200),
        Vec::new(),
        ChannelReader::new(receiver),
        None,
        None,
    )
    .with_chunked_threshold(0);
    response.add_header(
        Header::from_bytes("Content-Type", "application/x-ndjson; charset=utf-8")
            .map_err(|_| std::io::Error::other("invalid content type header"))?,
    );
    request.respond(response)
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
    respond_ndjson_stream(request, move |sender| {
        let interrupt_after_tool_step = AtomicBool::new(body.interrupt_after_tool_step);
        let mut observer = |event| {
            send_stream_event(&sender, &WorkerStreamEventResponse::ChatEvent { event });
        };
        let outcome = map_chat_turn_outcome(app.execute_chat_turn_with_control_and_observer(
            &body.session_id,
            &body.message,
            body.now,
            Some(&interrupt_after_tool_step),
            &mut observer,
        ));
        send_stream_event(&sender, &WorkerStreamEventResponse::Finished { outcome });
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
    respond_ndjson_stream(request, move |sender| {
        let interrupt_after_tool_step = AtomicBool::new(body.interrupt_after_tool_step);
        let mut observer = |event| {
            send_stream_event(&sender, &WorkerStreamEventResponse::ChatEvent { event });
        };
        let outcome = map_approve_run_outcome(app.approve_run_with_control_and_observer(
            &body.run_id,
            &body.approval_id,
            body.now,
            Some(&interrupt_after_tool_step),
            &mut observer,
        ));
        send_stream_event(&sender, &WorkerStreamEventResponse::Finished { outcome });
    })
}
