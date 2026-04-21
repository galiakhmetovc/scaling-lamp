use super::*;
use crate::http::types::{
    ClearSessionRequest, CreateSessionRequest, ErrorResponse, SessionBackgroundJobResponse,
    SessionBackgroundJobsResponse, SessionDetailResponse, SessionPendingApprovalsResponse,
    SessionPreferencesRequest, SessionSkillsResponse, SessionSummaryResponse,
    SessionTranscriptResponse, SkillCommandRequest,
};
use agent_persistence::SessionRepository;
use tiny_http::Method;

pub(super) fn handle_create_session(app: &App, mut request: Request) -> std::io::Result<()> {
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

pub(super) fn handle_list_sessions(app: &App, request: Request) -> std::io::Result<()> {
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

pub(super) fn handle_nested_routes(app: &App, request: Request) -> std::io::Result<()> {
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
        (Method::Get, [session_id, jobs]) if jobs == "jobs" => {
            handle_session_background_jobs(app, request, session_id.as_str())
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

fn handle_session_background_jobs(
    app: &App,
    request: Request,
    session_id: &str,
) -> std::io::Result<()> {
    match app.session_background_jobs(session_id) {
        Ok(jobs) => {
            let response = jobs
                .into_iter()
                .map(SessionBackgroundJobResponse::from)
                .collect::<Vec<_>>();
            respond_json::<SessionBackgroundJobsResponse>(request, StatusCode(200), &response)
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
