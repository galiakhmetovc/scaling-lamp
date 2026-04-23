use super::*;
use crate::http::types::{
    ErrorResponse, McpConnectorCreateRequest, McpConnectorDetailResponse, McpConnectorUpdateRequest,
};
use tiny_http::{Method, StatusCode};

pub(super) fn handle_list_mcp_connectors(app: &App, request: Request) -> std::io::Result<()> {
    match app.list_mcp_connectors() {
        Ok(connectors) => respond_json(request, StatusCode(200), &connectors),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_create_mcp_connector(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<McpConnectorCreateRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid MCP connector create request: {error}"),
                },
            );
        }
    };

    match app.create_mcp_connector(&payload.id, payload.options) {
        Ok(connector) => respond_json(
            request,
            StatusCode(201),
            &McpConnectorDetailResponse { connector },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_mcp_connector_nested_routes(
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
    let Some(tail) = path.strip_prefix("/v1/mcp/connectors/") else {
        return respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        );
    };

    if let Some(id) = tail.strip_suffix("/restart") {
        return if method == Method::Post && !id.is_empty() {
            match app.restart_mcp_connector(id) {
                Ok(connector) => respond_json(
                    request,
                    StatusCode(200),
                    &McpConnectorDetailResponse { connector },
                ),
                Err(error) => {
                    let (status, payload) = map_bootstrap_error(error);
                    respond_json(request, status, &payload)
                }
            }
        } else {
            respond_json(
                request,
                StatusCode(404),
                &ErrorResponse {
                    error: "route not found".to_string(),
                },
            )
        };
    }

    match (method, tail) {
        (Method::Get, connector_id) if !connector_id.is_empty() => {
            match app.mcp_connector(connector_id) {
                Ok(connector) => respond_json(
                    request,
                    StatusCode(200),
                    &McpConnectorDetailResponse { connector },
                ),
                Err(error) => {
                    let (status, payload) = map_bootstrap_error(error);
                    respond_json(request, status, &payload)
                }
            }
        }
        (Method::Patch, connector_id) if !connector_id.is_empty() => {
            let mut request = request;
            let payload = match parse_json_body::<McpConnectorUpdateRequest>(&mut request) {
                Ok(payload) => payload,
                Err(error) => {
                    return respond_json(
                        request,
                        StatusCode(400),
                        &ErrorResponse {
                            error: format!("invalid MCP connector update request: {error}"),
                        },
                    );
                }
            };

            match app.update_mcp_connector(connector_id, payload.patch) {
                Ok(connector) => respond_json(
                    request,
                    StatusCode(200),
                    &McpConnectorDetailResponse { connector },
                ),
                Err(error) => {
                    let (status, payload) = map_bootstrap_error(error);
                    respond_json(request, status, &payload)
                }
            }
        }
        (Method::Delete, connector_id) if !connector_id.is_empty() => {
            match app.delete_mcp_connector(connector_id) {
                Ok(true) => respond_json(
                    request,
                    StatusCode(200),
                    &serde_json::json!({ "deleted": true }),
                ),
                Ok(false) => respond_json(
                    request,
                    StatusCode(404),
                    &ErrorResponse {
                        error: format!("mcp connector {connector_id} not found"),
                    },
                ),
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
