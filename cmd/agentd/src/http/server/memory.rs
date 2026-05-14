use super::*;
use crate::http::types::{
    ErrorResponse, KvDeleteRequest, KvDeleteResponse, KvEntryResponse, KvListResponse,
    KvPutRequest, KvPutResponse, MemoryRecallItemResponse, MemoryRecallPreviewRequest,
    MemoryRecallPreviewResponse, SemanticMemoryDeleteResponse, SemanticMemoryItemResponse,
    SemanticMemoryListResponse, SemanticMemorySearchRequest, SemanticMemorySearchResponse,
    SemanticMemoryUpdateRequest, SemanticMemoryUpdateResponse,
};
use agent_runtime::tool::{
    KvDeleteInput, KvListInput, KvPutInput, MemoryDeleteInput, MemoryListInput, MemorySearchInput,
    MemoryUpdateInput,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tiny_http::Method;

const MEMORY_LIST_DEFAULT_LIMIT: usize = 20;
const MEMORY_LIST_MAX_LIMIT: usize = 100;
const KV_LIST_DEFAULT_LIMIT: usize = 50;
const KV_LIST_MAX_LIMIT: usize = 200;

pub(super) fn handle_semantic_memory_list(app: &App, request: Request) -> std::io::Result<()> {
    let query = parse_query(request.url());
    let session_id = optional_query_value(&query, "session_id");
    let input = MemoryListInput {
        scope: query
            .get("scope")
            .cloned()
            .or_else(|| session_id.is_none().then(|| "operator".to_string())),
        limit: Some(
            query_usize(&query, "limit")
                .unwrap_or(MEMORY_LIST_DEFAULT_LIMIT)
                .clamp(1, MEMORY_LIST_MAX_LIMIT),
        ),
        offset: Some(query_usize(&query, "offset").unwrap_or(0)),
        filters: Value::Null,
    };
    match app.semantic_memory_list_context(session_id, input) {
        Ok(output) => respond_json(
            request,
            StatusCode(200),
            &SemanticMemoryListResponse {
                results: output
                    .results
                    .into_iter()
                    .map(SemanticMemoryItemResponse::from)
                    .collect(),
                truncated: output.truncated,
                offset: output.offset,
                limit: output.limit,
                total_results: output.total_results,
                next_offset: output.next_offset,
            },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_semantic_memory_search(
    app: &App,
    mut request: Request,
) -> std::io::Result<()> {
    match parse_json_body::<SemanticMemorySearchRequest>(&mut request) {
        Ok(payload) => {
            let input = MemorySearchInput {
                query: payload.query,
                scope: payload
                    .scope
                    .or_else(|| payload.session_id.is_none().then(|| "operator".to_string())),
                limit: payload.limit,
                filters: payload.filters,
            };
            match app.semantic_memory_search_context(payload.session_id.as_deref(), input) {
                Ok(output) => respond_json(
                    request,
                    StatusCode(200),
                    &SemanticMemorySearchResponse {
                        query: output.query,
                        results: output
                            .results
                            .into_iter()
                            .map(SemanticMemoryItemResponse::from)
                            .collect(),
                        truncated: output.truncated,
                        limit: output.limit,
                    },
                ),
                Err(error) => {
                    let (status, payload) = map_bootstrap_error(error);
                    respond_json(request, status, &payload)
                }
            }
        }
        Err(error) => respond_json(
            request,
            StatusCode(400),
            &ErrorResponse {
                error: format!("invalid semantic memory search request: {error}"),
            },
        ),
    }
}

pub(super) fn handle_semantic_memory_nested_routes(
    app: &App,
    request: Request,
) -> std::io::Result<()> {
    let path = request
        .url()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string();
    let Some(memory_id) = path.strip_prefix("/v1/memory/semantic/") else {
        return respond_json(
            request,
            StatusCode(404),
            &ErrorResponse {
                error: "route not found".to_string(),
            },
        );
    };
    let memory_id = match percent_decode_query(memory_id) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            return respond_json(
                request,
                StatusCode(400),
                &ErrorResponse {
                    error: "memory id is required".to_string(),
                },
            );
        }
    };

    let method = request.method().clone();
    match method {
        Method::Patch => handle_semantic_memory_update(app, request, memory_id),
        Method::Delete => {
            let input = MemoryDeleteInput { memory_id };
            match app.semantic_memory_delete(input) {
                Ok(output) => respond_json(
                    request,
                    StatusCode(200),
                    &SemanticMemoryDeleteResponse {
                        memory_id: output.memory_id,
                        deleted: output.deleted,
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

fn handle_semantic_memory_update(
    app: &App,
    mut request: Request,
    memory_id: String,
) -> std::io::Result<()> {
    match parse_json_body::<SemanticMemoryUpdateRequest>(&mut request) {
        Ok(payload) => {
            let input = MemoryUpdateInput {
                memory_id,
                text: payload.text,
                metadata: payload.metadata,
            };
            match app.semantic_memory_update(input) {
                Ok(output) => respond_json(
                    request,
                    StatusCode(200),
                    &SemanticMemoryUpdateResponse {
                        memory_id: output.memory_id,
                        updated: output.updated,
                        memory: output.memory.map(SemanticMemoryItemResponse::from),
                    },
                ),
                Err(error) => {
                    let (status, payload) = map_bootstrap_error(error);
                    respond_json(request, status, &payload)
                }
            }
        }
        Err(error) => respond_json(
            request,
            StatusCode(400),
            &ErrorResponse {
                error: format!("invalid semantic memory update request: {error}"),
            },
        ),
    }
}

pub(super) fn handle_kv_list(app: &App, request: Request) -> std::io::Result<()> {
    let query = parse_query(request.url());
    let session_id = optional_query_value(&query, "session_id");
    let input = KvListInput {
        scope: query
            .get("scope")
            .cloned()
            .or_else(|| session_id.is_none().then(|| "operator".to_string())),
        prefix: query
            .get("prefix")
            .cloned()
            .filter(|value| !value.is_empty()),
        limit: Some(
            query_usize(&query, "limit")
                .unwrap_or(KV_LIST_DEFAULT_LIMIT)
                .clamp(1, KV_LIST_MAX_LIMIT),
        ),
        offset: Some(query_usize(&query, "offset").unwrap_or(0)),
    };
    match app.kv_list_context(session_id, input, unix_timestamp()) {
        Ok(output) => respond_json(
            request,
            StatusCode(200),
            &KvListResponse {
                results: output
                    .results
                    .into_iter()
                    .map(KvEntryResponse::from)
                    .collect(),
                truncated: output.truncated,
                offset: output.offset,
                limit: output.limit,
                next_offset: output.next_offset,
            },
        ),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

pub(super) fn handle_kv_put(app: &App, mut request: Request) -> std::io::Result<()> {
    match parse_json_body::<KvPutRequest>(&mut request) {
        Ok(payload) => {
            let session_id = payload.session_id.as_deref();
            let mut input = KvPutInput {
                key: payload.key,
                value: payload.value,
                scope: payload.scope,
                metadata: payload.metadata,
                expected_revision: payload.expected_revision,
                ttl_seconds: payload.ttl_seconds,
            };
            if input.scope.is_none() && session_id.is_none() {
                input.scope = Some("operator".to_string());
            }
            match app.kv_put_context(session_id, input, unix_timestamp()) {
                Ok(output) => respond_json(
                    request,
                    StatusCode(200),
                    &KvPutResponse {
                        entry: KvEntryResponse::from(output.entry),
                    },
                ),
                Err(error) => {
                    let (status, payload) = map_bootstrap_error(error);
                    respond_json(request, status, &payload)
                }
            }
        }
        Err(error) => respond_json(
            request,
            StatusCode(400),
            &ErrorResponse {
                error: format!("invalid kv put request: {error}"),
            },
        ),
    }
}

pub(super) fn handle_kv_delete(app: &App, mut request: Request) -> std::io::Result<()> {
    match parse_json_body::<KvDeleteRequest>(&mut request) {
        Ok(payload) => {
            let session_id = payload.session_id.as_deref();
            let mut input = KvDeleteInput {
                key: payload.key,
                scope: payload.scope,
                expected_revision: payload.expected_revision,
            };
            if input.scope.is_none() && session_id.is_none() {
                input.scope = Some("operator".to_string());
            }
            match app.kv_delete_context(session_id, input) {
                Ok(output) => respond_json(
                    request,
                    StatusCode(200),
                    &KvDeleteResponse {
                        key: output.key,
                        deleted: output.deleted,
                    },
                ),
                Err(error) => {
                    let (status, payload) = map_bootstrap_error(error);
                    respond_json(request, status, &payload)
                }
            }
        }
        Err(error) => respond_json(
            request,
            StatusCode(400),
            &ErrorResponse {
                error: format!("invalid kv delete request: {error}"),
            },
        ),
    }
}

pub(super) fn handle_memory_recall_preview(app: &App, mut request: Request) -> std::io::Result<()> {
    match parse_json_body::<MemoryRecallPreviewRequest>(&mut request) {
        Ok(payload) => {
            match app.memory_recall_preview(payload.session_id.as_str(), payload.query.as_deref()) {
                Ok(Some(recall)) => respond_json(
                    request,
                    StatusCode(200),
                    &MemoryRecallPreviewResponse {
                        enabled: true,
                        query: Some(recall.query),
                        items: recall
                            .items
                            .into_iter()
                            .map(MemoryRecallItemResponse::from)
                            .collect(),
                        truncated: recall.truncated,
                    },
                ),
                Ok(None) => respond_json(
                    request,
                    StatusCode(200),
                    &MemoryRecallPreviewResponse {
                        enabled: app.config.memory_recall.enabled && app.config.mem0.enabled,
                        query: None,
                        items: Vec::new(),
                        truncated: false,
                    },
                ),
                Err(error) => {
                    let (status, payload) = map_bootstrap_error(error);
                    respond_json(request, status, &payload)
                }
            }
        }
        Err(error) => respond_json(
            request,
            StatusCode(400),
            &ErrorResponse {
                error: format!("invalid memory recall preview request: {error}"),
            },
        ),
    }
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
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

fn optional_query_value<'a>(query: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    query
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
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
