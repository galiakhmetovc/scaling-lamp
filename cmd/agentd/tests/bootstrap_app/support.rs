pub(crate) use agent_persistence::{
    AgentRepository, AgentScheduleRecord, AppConfig, ConfigError, ContextOffloadRepository,
    ContextSummaryRepository, JobRecord, JobRepository, MissionRecord, MissionRepository,
    PersistenceStore, PlanRecord, PlanRepository, RunRecord, RunRepository, SessionInboxRepository,
    SessionRecord, SessionRepository, TranscriptRepository,
};
pub(crate) use agent_runtime::context::{
    ContextOffloadPayload, ContextOffloadRef, ContextOffloadSnapshot,
};
pub(crate) use agent_runtime::delegation::DelegateWriteScope;
pub(crate) use agent_runtime::mission::{
    JobResult, JobSpec, MissionExecutionIntent, MissionSchedule, MissionStatus,
};
pub(crate) use agent_runtime::permission::{
    PermissionAction, PermissionConfig, PermissionMode, PermissionRule,
};
pub(crate) use agent_runtime::plan::{PlanItem, PlanItemStatus, PlanSnapshot};
pub(crate) use agent_runtime::provider::{ConfiguredProvider, ProviderKind};
pub(crate) use agent_runtime::run::{
    ApprovalRequest, DelegateRun, RunEngine, RunSnapshot, RunStatus,
};
pub(crate) use agent_runtime::scheduler::{MissionVerificationSummary, SupervisorAction};
pub(crate) use agent_runtime::session::{Session, SessionSettings};
pub(crate) use agent_runtime::tool::{FsWriteInput, ToolCall};
pub(crate) use agent_runtime::verification::VerificationStatus;
pub(crate) use agent_runtime::verification::{CheckOutcome, EvidenceBundle};
pub(crate) use agent_runtime::workspace::WorkspaceRef;
pub(crate) use agentd::bootstrap::{BootstrapError, SessionPreferencesPatch, build_from_config};
pub(crate) use agentd::execution;
pub(crate) use agentd::execution::ExecutionError;
pub(crate) use std::fs;
pub(crate) use std::io::{BufRead, BufReader, Cursor, Read, Write};
pub(crate) use std::net::TcpListener;
pub(crate) use std::sync::mpsc::{self, Receiver};
pub(crate) use std::thread;
pub(crate) use std::time::Duration;

pub(crate) fn openai_stream_message_response(response_id: &str, text: &str) -> String {
    let text = serde_json::to_string(text).expect("serialize text");
    format!(
        "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"{response_id}\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":{text}}}]}}],\"usage\":{{\"input_tokens\":16,\"output_tokens\":3,\"total_tokens\":19}}}}}}\n\n"
    )
}

pub(crate) fn openai_stream_tool_call_response(
    response_id: &str,
    call_id: &str,
    tool_name: &str,
    arguments: &str,
) -> String {
    let arguments = serde_json::to_string(arguments).expect("serialize arguments");
    format!(
        "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"{response_id}\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"fc_1\",\"type\":\"function_call\",\"status\":\"completed\",\"call_id\":\"{call_id}\",\"name\":\"{tool_name}\",\"arguments\":{arguments}}}],\"usage\":{{\"input_tokens\":19,\"output_tokens\":7,\"total_tokens\":26}}}}}}\n\n"
    )
}

pub(crate) fn spawn_json_server(
    body: &'static str,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    spawn_json_server_sequence(vec![body.to_string()])
}

pub(crate) fn spawn_sse_server_sequence(
    bodies: Vec<String>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");

            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut raw_request = String::new();
            let mut content_length = 0usize;

            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request line");
                raw_request.push_str(&line);

                if line == "\r\n" {
                    break;
                }

                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = value.trim().parse().expect("parse content length");
                }
            }

            let mut body_bytes = vec![0; content_length];
            reader
                .read_exact(&mut body_bytes)
                .expect("read request body");
            raw_request.push_str(&String::from_utf8_lossy(&body_bytes));

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
            sender.send(raw_request).expect("send request");
        }
    });

    (format!("http://{address}/v1"), receiver, handle)
}

pub(crate) fn spawn_json_server_sequence(
    bodies: Vec<String>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");

            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut raw_request = String::new();
            let mut content_length = 0usize;

            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request line");
                raw_request.push_str(&line);

                if line == "\r\n" {
                    break;
                }

                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = value.trim().parse().expect("parse content length");
                }
            }

            let mut body_buf = vec![0u8; content_length];
            reader.read_exact(&mut body_buf).expect("read request body");
            raw_request.push_str(std::str::from_utf8(&body_buf).expect("utf8 body"));
            sender.send(raw_request).expect("send request");

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        }
    });

    (format!("http://{address}"), receiver, handle)
}

pub(crate) fn spawn_json_server_status_sequence(
    responses: Vec<(u16, String)>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for (status_code, body) in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");

            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut raw_request = String::new();
            let mut content_length = 0usize;

            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request line");
                raw_request.push_str(&line);

                if line == "\r\n" {
                    break;
                }

                let lower = line.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix("content-length:") {
                    content_length = value.trim().parse().expect("parse content length");
                }
            }

            let mut body_buf = vec![0u8; content_length];
            reader.read_exact(&mut body_buf).expect("read request body");
            raw_request.push_str(std::str::from_utf8(&body_buf).expect("utf8 body"));
            sender.send(raw_request).expect("send request");

            let status_line = match status_code {
                200 => "200 OK".to_string(),
                400 => "400 Bad Request".to_string(),
                401 => "401 Unauthorized".to_string(),
                403 => "403 Forbidden".to_string(),
                408 => "408 Request Timeout".to_string(),
                429 => "429 Too Many Requests".to_string(),
                500 => "500 Internal Server Error".to_string(),
                502 => "502 Bad Gateway".to_string(),
                503 => "503 Service Unavailable".to_string(),
                504 => "504 Gateway Timeout".to_string(),
                other => format!("{other} Test Status"),
            };
            let response = format!(
                "HTTP/1.1 {status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        }
    });

    (format!("http://{address}"), receiver, handle)
}

pub(crate) fn spawn_text_server(
    path: &'static str,
    body: &'static str,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set read timeout");

        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut raw_request = String::new();

        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("read request line");
            raw_request.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }

        sender.send(raw_request).expect("send request");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
        stream.flush().expect("flush response");
    });

    (format!("http://{address}{path}"), receiver, handle)
}

pub(crate) fn spawn_text_server_sequence(
    bodies: Vec<&'static str>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");

            let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
            let mut raw_request = String::new();

            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request line");
                raw_request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }

            sender.send(raw_request).expect("send request");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        }
    });

    (format!("http://{address}"), receiver, handle)
}
