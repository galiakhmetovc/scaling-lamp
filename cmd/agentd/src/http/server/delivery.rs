use super::*;
use crate::http::types::{
    DeliveryTargetCreateRequest, DeliveryTargetUpdateRequest, ErrorResponse,
    SessionOutputRouteCreateRequest, SessionOutputRouteUpdateRequest,
};
use tiny_http::{Method, StatusCode};

pub(super) fn handle_list_delivery_targets(app: &App, request: Request) -> std::io::Result<()> {
    match app.list_delivery_targets() {
        Ok(targets) => respond_json(request, StatusCode(200), &targets),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_create_delivery_target(
    app: &App,
    mut request: Request,
) -> std::io::Result<()> {
    let payload = match parse_json_body::<DeliveryTargetCreateRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid delivery target create request: {error}"),
                },
            );
        }
    };

    match app.create_delivery_target(&payload.target_id, payload.options) {
        Ok(target) => respond_json(
            request,
            StatusCode(201),
            &serde_json::json!({ "target": target }),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_list_session_output_routes(
    app: &App,
    request: Request,
) -> std::io::Result<()> {
    match app.list_session_output_routes() {
        Ok(routes) => respond_json(request, StatusCode(200), &routes),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_create_session_output_route(
    app: &App,
    mut request: Request,
) -> std::io::Result<()> {
    let payload = match parse_json_body::<SessionOutputRouteCreateRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid session output route create request: {error}"),
                },
            );
        }
    };

    match app.attach_session_output_route(&payload.session_id, &payload.target_id, payload.options)
    {
        Ok(route) => respond_json(
            request,
            StatusCode(201),
            &serde_json::json!({ "route": route }),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_delivery_target_nested_routes(
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
    let Some(target_id) = path.strip_prefix("/v1/delivery-targets/") else {
        return not_found(request);
    };

    match (method, target_id) {
        (Method::Get, id) if !id.is_empty() => match app.delivery_target(id) {
            Ok(target) => respond_json(
                request,
                StatusCode(200),
                &serde_json::json!({ "target": target }),
            ),
            Err(error) => {
                let (status, payload) = map_bootstrap_error(error);
                respond_json(request, status, &payload)
            }
        },
        (Method::Patch, id) if !id.is_empty() => {
            let mut request = request;
            let payload = match parse_json_body::<DeliveryTargetUpdateRequest>(&mut request) {
                Ok(payload) => payload,
                Err(error) => {
                    return respond_json(
                        request,
                        StatusCode(400),
                        &ErrorResponse {
                            error: format!("invalid delivery target update request: {error}"),
                        },
                    );
                }
            };
            match app.update_delivery_target(id, payload.patch) {
                Ok(target) => respond_json(
                    request,
                    StatusCode(200),
                    &serde_json::json!({ "target": target }),
                ),
                Err(error) => {
                    let (status, payload) = map_bootstrap_error(error);
                    respond_json(request, status, &payload)
                }
            }
        }
        _ => not_found(request),
    }
}

pub(super) fn handle_session_output_route_nested_routes(
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
    let Some(route_id) = path.strip_prefix("/v1/session-output-routes/") else {
        return not_found(request);
    };

    match (method, route_id) {
        (Method::Get, id) if !id.is_empty() => match app.session_output_route(id) {
            Ok(route) => respond_json(
                request,
                StatusCode(200),
                &serde_json::json!({ "route": route }),
            ),
            Err(error) => {
                let (status, payload) = map_bootstrap_error(error);
                respond_json(request, status, &payload)
            }
        },
        (Method::Patch, id) if !id.is_empty() => {
            let mut request = request;
            let payload = match parse_json_body::<SessionOutputRouteUpdateRequest>(&mut request) {
                Ok(payload) => payload,
                Err(error) => {
                    return respond_json(
                        request,
                        StatusCode(400),
                        &ErrorResponse {
                            error: format!("invalid session output route update request: {error}"),
                        },
                    );
                }
            };
            match app.update_session_output_route(id, payload.patch) {
                Ok(route) => respond_json(
                    request,
                    StatusCode(200),
                    &serde_json::json!({ "route": route }),
                ),
                Err(error) => {
                    let (status, payload) = map_bootstrap_error(error);
                    respond_json(request, status, &payload)
                }
            }
        }
        _ => not_found(request),
    }
}

fn not_found(request: Request) -> std::io::Result<()> {
    respond_json(
        request,
        StatusCode(404),
        &ErrorResponse {
            error: "route not found".to_string(),
        },
    )
}
