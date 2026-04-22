use super::*;
use crate::http::types::{
    AgentCreateRequest, AgentRenderResponse, AgentResolveRequest, AgentScheduleCreateRequest,
    AgentScheduleResolveRequest, AgentSelectRequest, ErrorResponse,
};
use tiny_http::{Method, StatusCode};

pub(super) fn handle_list_agents(app: &App, request: Request) -> std::io::Result<()> {
    match app.render_agents() {
        Ok(message) => respond_json(request, StatusCode(200), &AgentRenderResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_current_agent(app: &App, request: Request) -> std::io::Result<()> {
    match app.render_agent_profile(None) {
        Ok(message) => respond_json(request, StatusCode(200), &AgentRenderResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_show_agent(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<Option<AgentResolveRequest>>(&mut request) {
        Ok(payload) => payload.unwrap_or(AgentResolveRequest { identifier: None }),
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent show request: {error}"),
                },
            );
        }
    };

    match app.render_agent_profile(payload.identifier.as_deref()) {
        Ok(message) => respond_json(request, StatusCode(200), &AgentRenderResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_select_agent(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<AgentSelectRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent select request: {error}"),
                },
            );
        }
    };

    match app.select_agent_profile(&payload.identifier) {
        Ok(profile) => respond_json(
            request,
            StatusCode(200),
            &AgentRenderResponse {
                message: format!("текущий агент: {} ({})", profile.name, profile.id),
            },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_create_agent(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<AgentCreateRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent create request: {error}"),
                },
            );
        }
    };

    match app.create_agent_from_template(&payload.name, payload.template_identifier.as_deref()) {
        Ok(profile) => respond_json(
            request,
            StatusCode(201),
            &AgentRenderResponse {
                message: format!(
                    "создан агент {} ({}) из шаблона {}",
                    profile.name,
                    profile.id,
                    profile.template_kind.as_str()
                ),
            },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_open_agent_home(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<Option<AgentResolveRequest>>(&mut request) {
        Ok(payload) => payload.unwrap_or(AgentResolveRequest { identifier: None }),
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent open request: {error}"),
                },
            );
        }
    };

    match app.render_agent_home(payload.identifier.as_deref()) {
        Ok(message) => respond_json(request, StatusCode(200), &AgentRenderResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_list_agent_schedules(app: &App, request: Request) -> std::io::Result<()> {
    match app.render_agent_schedules() {
        Ok(message) => respond_json(request, StatusCode(200), &AgentRenderResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_show_agent_schedule(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<AgentScheduleResolveRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent schedule show request: {error}"),
                },
            );
        }
    };

    match app.render_agent_schedule(&payload.id) {
        Ok(message) => respond_json(request, StatusCode(200), &AgentRenderResponse { message }),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_create_agent_schedule(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<AgentScheduleCreateRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid agent schedule create request: {error}"),
                },
            );
        }
    };

    match app.create_agent_schedule(
        &payload.id,
        payload.interval_seconds,
        &payload.prompt,
        payload.agent_identifier.as_deref(),
    ) {
        Ok(schedule) => respond_json(
            request,
            StatusCode(201),
            &AgentRenderResponse {
                message: format!(
                    "создано расписание {} agent={} interval={}s",
                    schedule.id, schedule.agent_profile_id, schedule.interval_seconds
                ),
            },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_agent_schedule_nested_routes(
    app: &App,
    request: Request,
) -> std::io::Result<()> {
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    let method = request.method().clone();
    let Some(tail) = path.strip_prefix("/v1/agent-schedules/") else {
        return respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        );
    };

    match (method, tail) {
        (Method::Delete, schedule_id) => match app.delete_agent_schedule(schedule_id) {
            Ok(true) => respond_json(
                request,
                StatusCode(200),
                &AgentRenderResponse {
                    message: format!("расписание {schedule_id} удалено"),
                },
            ),
            Ok(false) => respond_json(
                request,
                StatusCode(404),
                &ErrorResponse {
                    error: format!("agent schedule {schedule_id} not found"),
                },
            ),
            Err(error) => {
                let (status, payload) = map_bootstrap_error(error);
                respond_json(request, status, &payload)
            }
        },
        _ => respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        ),
    }
}
