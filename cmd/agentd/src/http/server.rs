use crate::bootstrap::{App, BootstrapError};
use crate::execution::ExecutionError;
use crate::http::types::{
    ApproveRunRequest, ChatTurnRequest, ClearSessionRequest, CreateSessionRequest, ErrorResponse,
    SessionDetailResponse, SessionPendingApprovalsResponse, SessionPreferencesRequest,
    SessionSkillsResponse, SessionSummaryResponse, SessionTranscriptResponse, SkillCommandRequest,
    StatusResponse, WorkerOutcomeResponse,
};
use agent_persistence::{JobRepository, MissionRepository, SessionRepository};
use serde::{Serialize, de::DeserializeOwned};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

pub fn serve(app: App, shutdown: Arc<AtomicBool>) -> std::io::Result<()> {
    let bind = format!(
        "{}:{}",
        app.config.daemon.bind_host, app.config.daemon.bind_port
    );
    let server = Server::http(&bind).map_err(std::io::Error::other)?;

    while !shutdown.load(Ordering::Relaxed) {
        match server.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(request)) => handle_request(&app, request)?,
            Ok(None) => continue,
            Err(error) => return Err(error),
        }
    }

    Ok(())
}

fn handle_request(app: &App, request: Request) -> std::io::Result<()> {
    if !is_authorized(app, &request) {
        return respond_json(
            request,
            StatusCode(401),
            &ErrorResponse {
                error: "authorization required".to_string(),
            },
        );
    }

    match (request.method(), request.url()) {
        (&Method::Get, "/v1/status") => handle_status(app, request),
        (&Method::Get, "/v1/sessions") => handle_list_sessions(app, request),
        (&Method::Post, "/v1/sessions") => handle_create_session(app, request),
        (&Method::Post, "/v1/chat/turn") => handle_chat_turn(app, request),
        (&Method::Post, "/v1/runs/approve") => handle_approve_run(app, request),
        _ => handle_nested_routes(app, request),
    }
}

fn handle_status(app: &App, request: Request) -> std::io::Result<()> {
    let store = match app.store() {
        Ok(store) => store,
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };
    let session_count = match store.list_sessions() {
        Ok(sessions) => sessions.len(),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Store(error));
            return respond_json(request, status, &payload);
        }
    };
    let mission_count = match store.list_missions() {
        Ok(missions) => missions.len(),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Store(error));
            return respond_json(request, status, &payload);
        }
    };
    let run_count = match store.load_execution_state() {
        Ok(state) => state.runs.len(),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Store(error));
            return respond_json(request, status, &payload);
        }
    };
    let job_count = match store.list_jobs() {
        Ok(jobs) => jobs.len(),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Store(error));
            return respond_json(request, status, &payload);
        }
    };
    let response = StatusResponse {
        ok: true,
        bind_host: app.config.daemon.bind_host.clone(),
        bind_port: app.config.daemon.bind_port,
        permission_mode: app.config.permissions.mode.as_str().to_string(),
        session_count,
        mission_count,
        run_count,
        job_count,
        components: app.runtime.component_count(),
        data_dir: app.config.data_dir.display().to_string(),
        state_db: app.persistence.stores.metadata_db.display().to_string(),
    };
    respond_json(request, StatusCode(200), &response)
}

fn handle_create_session(app: &App, mut request: Request) -> std::io::Result<()> {
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    let payload = if body.trim().is_empty() {
        CreateSessionRequest {
            id: None,
            title: None,
        }
    } else {
        match serde_json::from_str::<CreateSessionRequest>(&body) {
            Ok(payload) => payload,
            Err(error) => {
                return respond_json(
                    request,
                    StatusCode(400),
                    &ErrorResponse {
                        error: format!("invalid session request: {error}"),
                    },
                );
            }
        }
    };

    let session_result = match payload.id.as_deref() {
        Some(id) => app.create_session(id, payload.title.as_deref().unwrap_or("New Session")),
        None => app.create_session_auto(payload.title.as_deref()),
    };
    let session = match session_result {
        Ok(session) => session,
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };
    respond_json(
        request,
        StatusCode(201),
        &SessionSummaryResponse::from(session),
    )
}

fn handle_list_sessions(app: &App, request: Request) -> std::io::Result<()> {
    let sessions = match app.list_session_summaries() {
        Ok(sessions) => sessions
            .into_iter()
            .map(SessionSummaryResponse::from)
            .collect::<Vec<_>>(),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };
    respond_json(request, StatusCode(200), &sessions)
}

fn handle_nested_routes(app: &App, request: Request) -> std::io::Result<()> {
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    let method = request.method().clone();
    let Some(session_tail) = path.strip_prefix("/v1/sessions/") else {
        return respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        );
    };
    let segments = session_tail
        .split('/')
        .map(str::to_string)
        .collect::<Vec<_>>();
    match (method, segments.as_slice()) {
        (Method::Get, [session_id]) => handle_session_summary(app, request, session_id.as_str()),
        (Method::Get, [session_id, detail]) if detail == "detail" => {
            handle_session_detail(app, request, session_id.as_str())
        }
        (Method::Delete, [session_id]) => handle_delete_session(app, request, session_id.as_str()),
        (Method::Get, [session_id, transcript]) if transcript == "transcript" => {
            handle_session_transcript(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, approvals]) if approvals == "approvals" => {
            handle_pending_approvals(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, skills]) if skills == "skills" => {
            handle_session_skills(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, skills, action])
            if skills == "skills" && action == "enable" =>
        {
            handle_enable_session_skill(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, skills, action])
            if skills == "skills" && action == "disable" =>
        {
            handle_disable_session_skill(app, request, session_id.as_str())
        }
        (Method::Patch, [session_id, preferences]) if preferences == "preferences" => {
            handle_update_preferences(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, clear]) if clear == "clear" => {
            handle_clear_session(app, request, session_id.as_str())
        }
        (Method::Post, [session_id, compact]) if compact == "compact" => {
            handle_compact_session(app, request, session_id.as_str())
        }
        (Method::Get, [session_id, plan]) if plan == "plan" => {
            handle_render_plan(app, request, session_id.as_str())
        }
        _ => respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        ),
    }
}

fn handle_session_detail(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    let store = match app.store() {
        Ok(store) => store,
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            return respond_json(request, status, &payload);
        }
    };

    match store.get_session(session_id) {
        Ok(Some(record)) => respond_json(
            request,
            StatusCode(200),
            &SessionDetailResponse {
                id: record.id,
                title: record.title,
                prompt_override: record.prompt_override,
                settings_json: record.settings_json,
                active_mission_id: record.active_mission_id,
                created_at: record.created_at,
                updated_at: record.updated_at,
            },
        ),
        Ok(None) => respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: format!("session {session_id} not found"),
            },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(BootstrapError::Store(error));
            respond_json(request, status, &payload)
        }
    }
}

fn handle_session_summary(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.session_summary(session_id) {
        Ok(summary) => respond_json(
            request,
            StatusCode(200),
            &SessionSummaryResponse::from(summary),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_session_transcript(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.session_transcript(session_id) {
        Ok(transcript) => {
            respond_json::<SessionTranscriptResponse>(request, StatusCode(200), &transcript)
        }
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_pending_approvals(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.pending_approvals(session_id) {
        Ok(approvals) => {
            respond_json::<SessionPendingApprovalsResponse>(request, StatusCode(200), &approvals)
        }
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_session_skills(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.session_skills(session_id) {
        Ok(skills) => respond_json::<SessionSkillsResponse>(request, StatusCode(200), &skills),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_enable_session_skill(
    app: &App,
    mut request: Request,
    session_id: &str,
) -> std::io::Result<()> {
    let body: SkillCommandRequest = match parse_json_body(&mut request) {
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

    match app.enable_session_skill(session_id, &body.name) {
        Ok(skills) => respond_json::<SessionSkillsResponse>(request, StatusCode(200), &skills),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_disable_session_skill(
    app: &App,
    mut request: Request,
    session_id: &str,
) -> std::io::Result<()> {
    let body: SkillCommandRequest = match parse_json_body(&mut request) {
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

    match app.disable_session_skill(session_id, &body.name) {
        Ok(skills) => respond_json::<SessionSkillsResponse>(request, StatusCode(200), &skills),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_update_preferences(
    app: &App,
    mut request: Request,
    session_id: &str,
) -> std::io::Result<()> {
    let patch: SessionPreferencesRequest = match parse_json_body(&mut request) {
        Ok(patch) => patch,
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
    match app.update_session_preferences(session_id, patch) {
        Ok(summary) => respond_json(
            request,
            StatusCode(200),
            &SessionSummaryResponse::from(summary),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_delete_session(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.delete_session(session_id) {
        Ok(()) => respond_json(
            request,
            StatusCode(200),
            &serde_json::json!({ "deleted": true }),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_clear_session(app: &App, mut request: Request, session_id: &str) -> std::io::Result<()> {
    let body: ClearSessionRequest = match parse_json_body(&mut request) {
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
    match app.clear_session(session_id, body.title.as_deref()) {
        Ok(summary) => respond_json(
            request,
            StatusCode(200),
            &SessionSummaryResponse::from(summary),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_compact_session(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.compact_session(session_id) {
        Ok(summary) => respond_json(
            request,
            StatusCode(200),
            &SessionSummaryResponse::from(summary),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_render_plan(app: &App, request: Request, session_id: &str) -> std::io::Result<()> {
    match app.render_plan(session_id) {
        Ok(plan) => respond_json(
            request,
            StatusCode(200),
            &serde_json::json!({ "plan": plan }),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn handle_chat_turn(app: &App, mut request: Request) -> std::io::Result<()> {
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

fn handle_approve_run(app: &App, mut request: Request) -> std::io::Result<()> {
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

fn parse_json_body<T>(request: &mut Request) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .map_err(|error| error.to_string())?;
    if body.trim().is_empty() {
        return serde_json::from_str("null").map_err(|error| error.to_string());
    }
    serde_json::from_str(&body).map_err(|error| error.to_string())
}

fn is_authorized(app: &App, request: &Request) -> bool {
    let Some(expected_token) = app.config.daemon.bearer_token.as_deref() else {
        return true;
    };

    request.headers().iter().any(|header| {
        header.field.equiv("Authorization")
            && header.value.as_str().trim() == format!("Bearer {expected_token}")
    })
}

fn respond_json<T>(request: Request, status: StatusCode, payload: &T) -> std::io::Result<()>
where
    T: Serialize,
{
    let body =
        serde_json::to_vec(payload).map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut response = Response::from_data(body).with_status_code(status);
    response.add_header(
        Header::from_bytes("Content-Type", "application/json; charset=utf-8")
            .map_err(|_| std::io::Error::other("invalid content type header"))?,
    );
    request.respond(response)
}

pub fn map_bootstrap_error(error: BootstrapError) -> (StatusCode, ErrorResponse) {
    match error {
        BootstrapError::MissingRecord { kind, id } => (
            StatusCode(404),
            ErrorResponse {
                error: format!("{kind} {id} not found"),
            },
        ),
        BootstrapError::Usage { reason } => (StatusCode(400), ErrorResponse { error: reason }),
        other => (
            StatusCode(500),
            ErrorResponse {
                error: other.to_string(),
            },
        ),
    }
}
