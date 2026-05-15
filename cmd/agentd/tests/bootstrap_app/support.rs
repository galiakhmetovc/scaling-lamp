pub(crate) use agent_persistence::{
    AgentRepository, AgentScheduleRecord, AppConfig, ArtifactRecord, ArtifactRepository,
    ConfigError, ContextOffloadRepository, ContextSummaryRepository, FileDeliveryRepository,
    JobRecord, JobRepository, MissionRecord, MissionRepository, PersistenceStore, PlanRecord,
    PlanRepository, RunRecord, RunRepository, SessionInboxRepository, SessionRecord,
    SessionRepository, StoreError, ToolCallRepository, TranscriptRepository, WorkspaceConfig,
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
pub(crate) use agent_runtime::session::{
    Session, SessionSettings, TranscriptEntry, scheduled_input_metadata,
};
pub(crate) use agent_runtime::tool::{FsWriteMode, FsWriteTextInput, ToolCall};
pub(crate) use agent_runtime::verification::VerificationStatus;
pub(crate) use agent_runtime::verification::{CheckOutcome, EvidenceBundle};
pub(crate) use agent_runtime::workspace::WorkspaceRef;
pub(crate) use agentd::bootstrap::{
    BootstrapError, SessionPreferencesPatch, build_from_config, build_from_config_without_recovery,
};
pub(crate) use agentd::execution;
pub(crate) use agentd::execution::ExecutionError;
pub(crate) use std::fs;
pub(crate) use std::io::{BufRead, BufReader, Cursor, Read, Write};
pub(crate) use std::net::TcpListener;
pub(crate) use std::path::PathBuf;
pub(crate) use std::sync::mpsc::{self, Receiver};
pub(crate) use std::thread;
pub(crate) use std::time::Duration;

const TEST_SERVER_READ_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) fn with_raw_postgres<T>(
    app: &agentd::bootstrap::App,
    operation: impl FnOnce(&mut postgres::Client) -> Result<T, StoreError>,
) -> T {
    let store = PersistenceStore::open(&app.persistence).expect("open raw postgres store");
    store
        .with_postgres_client(operation)
        .expect("execute raw postgres fixture")
}

pub(crate) fn insert_raw_session_with_settings(
    app: &agentd::bootstrap::App,
    id: &str,
    title: &str,
    settings_json: &str,
    created_at: i64,
    updated_at: i64,
) {
    with_raw_postgres(app, |client| {
        client.execute(
            "INSERT INTO sessions (
                id, title, prompt_override, settings_json, agent_profile_id,
                active_mission_id, parent_session_id, parent_job_id, delegation_label,
                created_at, updated_at
            ) VALUES ($1, $2, NULL, $3, $4, NULL, NULL, NULL, NULL, $5, $6)",
            &[
                &id,
                &title,
                &settings_json,
                &"default",
                &created_at,
                &updated_at,
            ],
        )?;
        Ok(())
    });
}

pub(crate) fn insert_raw_session(app: &agentd::bootstrap::App, id: &str, title: &str) {
    let settings_json =
        serde_json::to_string(&SessionSettings::default()).expect("serialize settings");
    insert_raw_session_with_settings(app, id, title, &settings_json, 11, 11);
}

pub(crate) struct RawRunFixture<'a> {
    pub(crate) id: &'a str,
    pub(crate) session_id: &'a str,
    pub(crate) status: &'a str,
    pub(crate) provider_usage_json: &'a str,
    pub(crate) pending_approvals_json: &'a str,
    pub(crate) provider_loop_json: &'a str,
    pub(crate) started_at: i64,
    pub(crate) updated_at: i64,
}

pub(crate) fn insert_raw_run_fixture(app: &agentd::bootstrap::App, fixture: RawRunFixture<'_>) {
    with_raw_postgres(app, |client| {
        client.execute(
            "INSERT INTO runs (
                id, session_id, mission_id, status, error, result, provider_usage_json,
                active_processes_json, recent_steps_json, evidence_refs_json,
                pending_approvals_json, provider_loop_json, delegate_runs_json,
                started_at, updated_at, finished_at
            ) VALUES ($1, $2, NULL, $3, NULL, NULL, $4, $5, $6, $7, $8, $9, $10, $11, $12, NULL)",
            &[
                &fixture.id,
                &fixture.session_id,
                &fixture.status,
                &fixture.provider_usage_json,
                &"[]",
                &"[]",
                &"[]",
                &fixture.pending_approvals_json,
                &fixture.provider_loop_json,
                &"[]",
                &fixture.started_at,
                &fixture.updated_at,
            ],
        )?;
        Ok(())
    });
}

pub(crate) struct RawJobFixture<'a> {
    pub(crate) id: &'a str,
    pub(crate) session_id: &'a str,
    pub(crate) kind: &'a str,
    pub(crate) status: &'a str,
    pub(crate) input_json: &'a str,
    pub(crate) callback_json: &'a str,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) started_at: Option<i64>,
}

pub(crate) fn insert_raw_job_fixture(app: &agentd::bootstrap::App, fixture: RawJobFixture<'_>) {
    with_raw_postgres(app, |client| {
        client.execute(
            "INSERT INTO jobs (
                id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
                result_json, error, created_at, updated_at, started_at, finished_at,
                attempt_count, max_attempts, lease_owner, lease_expires_at, heartbeat_at,
                cancel_requested_at, last_progress_message, callback_json, callback_sent_at
            ) VALUES ($1, $2, NULL, NULL, NULL, $3, $4, $5, NULL, NULL, $6, $7, $8, NULL, 0, 3, NULL, NULL, NULL, NULL, NULL, $9, NULL)",
            &[
                &fixture.id,
                &fixture.session_id,
                &fixture.kind,
                &fixture.status,
                &fixture.input_json,
                &fixture.created_at,
                &fixture.updated_at,
                &fixture.started_at,
                &fixture.callback_json,
            ],
        )?;
        Ok(())
    });
}

pub(crate) fn openai_stream_message_response(response_id: &str, text: &str) -> String {
    let text = serde_json::to_string(text).expect("serialize text");
    format!(
        "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"{response_id}\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":{text}}}]}}],\"usage\":{{\"input_tokens\":16,\"output_tokens\":3,\"total_tokens\":19}}}}}}\n\n"
    )
}

pub(crate) fn openai_message_response_json(response_id: &str, text: &str) -> String {
    let text = serde_json::to_string(text).expect("serialize text");
    format!(
        "{{\"id\":\"{response_id}\",\"model\":\"gpt-5.4\",\"output\":[{{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":{text}}}]}}],\"usage\":{{\"input_tokens\":16,\"output_tokens\":3,\"total_tokens\":19}}}}"
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
    let (ready_tx, ready_rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        ready_tx.send(()).expect("send server ready");
        for body in bodies {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(TEST_SERVER_READ_TIMEOUT))
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
    ready_rx.recv().expect("await server ready");

    (format!("http://{address}/v1"), receiver, handle)
}

pub(crate) fn spawn_json_server_sequence(
    bodies: Vec<String>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        ready_tx.send(()).expect("send server ready");
        for body in bodies {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(TEST_SERVER_READ_TIMEOUT))
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
    ready_rx.recv().expect("await server ready");

    (format!("http://{address}"), receiver, handle)
}

pub(crate) fn spawn_json_server_status_sequence(
    responses: Vec<(u16, String)>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        ready_tx.send(()).expect("send server ready");
        for (status_code, body) in responses {
            let (mut stream, _) = listener.accept().expect("accept connection");
            stream
                .set_read_timeout(Some(TEST_SERVER_READ_TIMEOUT))
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
    ready_rx.recv().expect("await server ready");

    (format!("http://{address}"), receiver, handle)
}

pub(crate) fn spawn_http_server(
    path: &'static str,
    content_type: &'static str,
    body: impl Into<String>,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let body = body.into();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("local addr");
    let (sender, receiver) = mpsc::channel();

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        stream
            .set_read_timeout(Some(TEST_SERVER_READ_TIMEOUT))
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
            "HTTP/1.1 200 OK\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            content_type,
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

pub(crate) fn spawn_text_server(
    path: &'static str,
    body: &'static str,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    spawn_http_server(path, "text/plain", body)
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
                .set_read_timeout(Some(TEST_SERVER_READ_TIMEOUT))
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
