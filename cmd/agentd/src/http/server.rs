mod a2a;
mod agents;
mod chat;
mod mcp;
mod sessions;
mod status;

use crate::bootstrap::{App, BootstrapError};
use crate::http::types::{DaemonStopResponse, ErrorResponse};
use serde::{Serialize, de::DeserializeOwned};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use tiny_http::{Header, Request, Response, Server, StatusCode};

pub fn serve(app: App, shutdown: Arc<AtomicBool>) -> std::io::Result<()> {
    let bind = format!(
        "{}:{}",
        app.config.daemon.bind_host, app.config.daemon.bind_port
    );
    let server = Server::http(&bind).map_err(std::io::Error::other)?;

    while !shutdown.load(Ordering::Relaxed) {
        match server.recv_timeout(
            app.config
                .runtime_timing
                .http_server_request_poll_interval(),
        ) {
            Ok(Some(request)) => {
                let app = app.clone();
                let shutdown = shutdown.clone();
                thread::spawn(move || {
                    let _ = handle_request(&app, &shutdown, request);
                });
            }
            Ok(None) => continue,
            Err(error) => return Err(error),
        }
    }

    Ok(())
}

fn handle_request(app: &App, shutdown: &Arc<AtomicBool>, request: Request) -> std::io::Result<()> {
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
        (&tiny_http::Method::Get, "/v1/status") => status::handle_status(app, request),
        (&tiny_http::Method::Get, "/v1/about") => status::handle_about(app, request),
        (&tiny_http::Method::Post, "/v1/diagnostics/tail") => {
            status::handle_diagnostics_tail(app, request)
        }
        (&tiny_http::Method::Post, "/v1/update") => status::handle_update_runtime(app, request),
        (&tiny_http::Method::Post, "/v1/daemon/stop") => handle_shutdown(shutdown, request),
        (&tiny_http::Method::Get, "/v1/agents") => agents::handle_list_agents(app, request),
        (&tiny_http::Method::Get, "/v1/agents/current") => {
            agents::handle_current_agent(app, request)
        }
        (&tiny_http::Method::Post, "/v1/agents/show") => agents::handle_show_agent(app, request),
        (&tiny_http::Method::Post, "/v1/agents/select") => {
            agents::handle_select_agent(app, request)
        }
        (&tiny_http::Method::Post, "/v1/agents") => agents::handle_create_agent(app, request),
        (&tiny_http::Method::Post, "/v1/agents/open") => {
            agents::handle_open_agent_home(app, request)
        }
        (&tiny_http::Method::Get, "/v1/agent-schedules") => {
            agents::handle_list_agent_schedules(app, request)
        }
        (&tiny_http::Method::Post, "/v1/agent-schedules/show") => {
            agents::handle_show_agent_schedule(app, request)
        }
        (&tiny_http::Method::Post, "/v1/agent-schedules/resolve") => {
            agents::handle_resolve_agent_schedule(app, request)
        }
        (&tiny_http::Method::Post, "/v1/agent-schedules") => {
            agents::handle_create_agent_schedule(app, request)
        }
        (&tiny_http::Method::Get, "/v1/mcp/connectors") => {
            mcp::handle_list_mcp_connectors(app, request)
        }
        (&tiny_http::Method::Post, "/v1/mcp/connectors") => {
            mcp::handle_create_mcp_connector(app, request)
        }
        (&tiny_http::Method::Post, "/v1/memory/session-search") => {
            sessions::handle_memory_session_search(app, request)
        }
        (&tiny_http::Method::Post, "/v1/memory/session-read") => {
            sessions::handle_memory_session_read(app, request)
        }
        (&tiny_http::Method::Post, "/v1/memory/knowledge-search") => {
            sessions::handle_memory_knowledge_search(app, request)
        }
        (&tiny_http::Method::Post, "/v1/memory/knowledge-read") => {
            sessions::handle_memory_knowledge_read(app, request)
        }
        (&tiny_http::Method::Post, "/v1/a2a/delegations") => {
            a2a::handle_create_delegation(app, request)
        }
        (&tiny_http::Method::Get, "/v1/sessions") => sessions::handle_list_sessions(app, request),
        (&tiny_http::Method::Post, "/v1/sessions") => sessions::handle_create_session(app, request),
        (&tiny_http::Method::Post, "/v1/chat/turn") => chat::handle_chat_turn(app, request),
        (&tiny_http::Method::Post, "/v1/chat/turn/stream") => {
            chat::handle_chat_turn_stream(app, request)
        }
        (&tiny_http::Method::Post, "/v1/runs/approve") => chat::handle_approve_run(app, request),
        (&tiny_http::Method::Post, "/v1/runs/approve/stream") => {
            chat::handle_approve_run_stream(app, request)
        }
        _ if request.url().starts_with("/v1/a2a/delegations/") => {
            a2a::handle_nested_routes(app, request)
        }
        _ if request.url().starts_with("/v1/agent-schedules/") => {
            agents::handle_agent_schedule_nested_routes(app, request)
        }
        _ if request.url().starts_with("/v1/mcp/connectors/") => {
            mcp::handle_mcp_connector_nested_routes(app, request)
        }
        _ => sessions::handle_nested_routes(app, request),
    }
}

fn handle_shutdown(shutdown: &Arc<AtomicBool>, request: Request) -> std::io::Result<()> {
    shutdown.store(true, Ordering::Relaxed);
    respond_json(
        request,
        StatusCode(200),
        &DaemonStopResponse { stopping: true },
    )
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
