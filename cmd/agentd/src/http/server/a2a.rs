use super::*;
use crate::http::types::{
    A2ADelegationCompletionRequest, A2ADelegationCreateRequest, ErrorResponse,
};
use tiny_http::Method;

pub(super) fn handle_create_delegation(app: &App, mut request: Request) -> std::io::Result<()> {
    let body: A2ADelegationCreateRequest = match parse_json_body(&mut request) {
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
    match app.accept_remote_delegation(body) {
        Ok(response) => respond_json(request, StatusCode(201), &response),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_nested_routes(app: &App, mut request: Request) -> std::io::Result<()> {
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    let method = request.method().clone();
    let Some(tail) = path.strip_prefix("/v1/a2a/delegations/") else {
        return respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        );
    };
    let segments = tail.split('/').collect::<Vec<_>>();
    match (method, segments.as_slice()) {
        (Method::Post, [job_id, "complete"]) => {
            let body: A2ADelegationCompletionRequest = match parse_json_body(&mut request) {
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
            match app.complete_remote_delegation(job_id, body) {
                Ok(()) => respond_json(request, StatusCode(200), &serde_json::json!({"ok": true})),
                Err(error) => {
                    let (status, payload) = map_bootstrap_error(error);
                    respond_json(request, status, &payload)
                }
            }
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
