use super::*;
use crate::http::types::{
    ErrorResponse, McpConnectorCreateRequest, McpConnectorDetailResponse,
    McpConnectorUpdateRequest, McpPromptGetRequest, McpResourceReadRequest,
};
use serde_json::json;
use std::collections::BTreeMap;
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

pub(super) fn handle_list_mcp_resources(app: &App, request: Request) -> std::io::Result<()> {
    let query = parse_query(request.url());
    let resources = app.list_mcp_resources(
        query.get("connector_id").map(String::as_str),
        query.get("query").map(String::as_str),
        query_usize(&query, "limit"),
        query_usize(&query, "offset"),
    );
    respond_json(request, StatusCode(200), &resources)
}

pub(super) fn handle_read_mcp_resource(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<McpResourceReadRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid MCP resource read request: {error}"),
                },
            );
        }
    };
    match app.read_mcp_resource(payload.connector_id.as_str(), payload.uri.as_str()) {
        Ok(output) => respond_json(
            request,
            StatusCode(200),
            &json!({
                "connector_id": output.connector_id,
                "uri": output.uri,
                "text": output.text,
                "contents": output.contents.into_iter().map(|content| json!({
                    "kind": content.kind,
                    "uri": content.uri,
                    "mime_type": content.mime_type,
                    "text": content.text,
                    "blob": content.blob,
                })).collect::<Vec<_>>(),
            }),
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_list_mcp_prompts(app: &App, request: Request) -> std::io::Result<()> {
    let query = parse_query(request.url());
    let prompts = app.list_mcp_prompts(
        query.get("connector_id").map(String::as_str),
        query.get("query").map(String::as_str),
        query_usize(&query, "limit"),
        query_usize(&query, "offset"),
    );
    respond_json(request, StatusCode(200), &prompts)
}

pub(super) fn handle_get_mcp_prompt(app: &App, mut request: Request) -> std::io::Result<()> {
    let payload = match parse_json_body::<McpPromptGetRequest>(&mut request) {
        Ok(payload) => payload,
        Err(error) => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: format!("invalid MCP prompt get request: {error}"),
                },
            );
        }
    };
    match app.get_mcp_prompt(
        payload.connector_id.as_str(),
        payload.name.as_str(),
        payload.arguments,
    ) {
        Ok(output) => respond_json(
            request,
            StatusCode(200),
            &json!({
                "connector_id": output.connector_id,
                "name": output.name,
                "description": output.description,
                "text": output.text,
                "messages": output.messages.into_iter().map(|message| json!({
                    "role": message.role,
                    "content_type": message.content_type,
                    "text": message.text,
                    "uri": message.uri,
                    "mime_type": message.mime_type,
                })).collect::<Vec<_>>(),
            }),
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

fn parse_query(url: &str) -> BTreeMap<String, String> {
    let Some((_, query)) = url.split_once('?') else {
        return BTreeMap::new();
    };
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            let key = percent_decode_query(key).ok()?;
            let value = percent_decode_query(value).ok()?;
            Some((key, value))
        })
        .collect()
}

fn percent_decode_query(input: &str) -> Result<String, ()> {
    let mut bytes = Vec::with_capacity(input.len());
    let input_bytes = input.as_bytes();
    let mut index = 0;
    while index < input_bytes.len() {
        match input_bytes[index] {
            b'+' => {
                bytes.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < input_bytes.len() => {
                let high = hex_value(input_bytes[index + 1]).ok_or(())?;
                let low = hex_value(input_bytes[index + 2]).ok_or(())?;
                bytes.push((high << 4) | low);
                index += 3;
            }
            value => {
                bytes.push(value);
                index += 1;
            }
        }
    }
    String::from_utf8(bytes).map_err(|_| ())
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn query_usize(query: &BTreeMap<String, String>, key: &str) -> Option<usize> {
    query.get(key).and_then(|value| value.parse::<usize>().ok())
}
