use agent_persistence::{
    RunRepository, StoreError, ToolCallRepository, TraceLinkRecord, TraceRepository,
};
use reqwest::StatusCode;
use serde_json::{Map, Value, json};
use std::error::Error;
use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtlpExportReport {
    pub trace_id: String,
    pub span_count: usize,
    pub endpoint: String,
    pub status_code: u16,
}

pub trait OtlpTraceRepository: TraceRepository + RunRepository + ToolCallRepository {}

impl<T> OtlpTraceRepository for T where T: TraceRepository + RunRepository + ToolCallRepository {}

#[derive(Debug)]
pub enum OtlpExportError {
    MissingTrace { trace_id: String },
    MissingRunTrace { run_id: String },
    Store(StoreError),
    Serialize(serde_json::Error),
    Http(reqwest::Error),
    HttpStatus { status: StatusCode, body: String },
}

impl fmt::Display for OtlpExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTrace { trace_id } => write!(formatter, "trace {trace_id} not found"),
            Self::MissingRunTrace { run_id } => {
                write!(formatter, "run {run_id} has no trace link")
            }
            Self::Store(source) => write!(formatter, "trace store error: {source}"),
            Self::Serialize(source) => write!(formatter, "trace serialization error: {source}"),
            Self::Http(source) => write!(formatter, "OTLP HTTP export failed: {source}"),
            Self::HttpStatus { status, body } => {
                write!(formatter, "OTLP HTTP export returned {status}: {body}")
            }
        }
    }
}

impl Error for OtlpExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Store(source) => Some(source),
            Self::Serialize(source) => Some(source),
            Self::Http(source) => Some(source),
            Self::MissingTrace { .. } | Self::MissingRunTrace { .. } | Self::HttpStatus { .. } => {
                None
            }
        }
    }
}

impl From<StoreError> for OtlpExportError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<serde_json::Error> for OtlpExportError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialize(value)
    }
}

impl From<reqwest::Error> for OtlpExportError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value)
    }
}

pub fn trace_export_payload_json(
    store: &impl OtlpTraceRepository,
    trace_id: &str,
) -> Result<String, OtlpExportError> {
    let links = trace_links(store, trace_id)?;
    serde_json::to_string_pretty(&trace_export_payload(store, &links)?)
        .map_err(OtlpExportError::from)
}

pub fn export_trace_to_otlp_http(
    store: &impl OtlpTraceRepository,
    trace_id: &str,
    endpoint: &str,
    timeout: Duration,
) -> Result<OtlpExportReport, OtlpExportError> {
    let links = trace_links(store, trace_id)?;
    let payload = trace_export_payload(store, &links)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()?;
    let response = client
        .post(endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&payload)
        .send()?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_else(|_| String::new());
        return Err(OtlpExportError::HttpStatus { status, body });
    }

    Ok(OtlpExportReport {
        trace_id: trace_id.to_string(),
        span_count: links.len(),
        endpoint: endpoint.to_string(),
        status_code: status.as_u16(),
    })
}

pub fn export_run_trace_to_otlp_http(
    store: &impl OtlpTraceRepository,
    run_id: &str,
    endpoint: &str,
    timeout: Duration,
) -> Result<OtlpExportReport, OtlpExportError> {
    let trace =
        store
            .get_trace_link("run", run_id)?
            .ok_or_else(|| OtlpExportError::MissingRunTrace {
                run_id: run_id.to_string(),
            })?;
    export_trace_to_otlp_http(store, &trace.trace_id, endpoint, timeout)
}

fn trace_links(
    store: &impl TraceRepository,
    trace_id: &str,
) -> Result<Vec<TraceLinkRecord>, OtlpExportError> {
    let links = store.list_trace_links_for_trace(trace_id)?;
    if links.is_empty() {
        return Err(OtlpExportError::MissingTrace {
            trace_id: trace_id.to_string(),
        });
    }
    Ok(links)
}

fn trace_export_payload(
    store: &impl OtlpTraceRepository,
    links: &[TraceLinkRecord],
) -> Result<Value, OtlpExportError> {
    let spans = links
        .iter()
        .map(|link| trace_link_to_span(store, link))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(json!({
        "resourceSpans": [
            {
                "resource": {
                    "attributes": [
                        otlp_attribute("service.name", Value::String("teamd".to_string()))
                    ]
                },
                "scopeSpans": [
                    {
                        "scope": {
                            "name": "teamd.runtime"
                        },
                        "spans": spans
                    }
                ]
            }
        ]
    }))
}

fn trace_link_to_span(
    store: &impl OtlpTraceRepository,
    link: &TraceLinkRecord,
) -> Result<Value, OtlpExportError> {
    let mut attributes = Vec::new();
    attributes.push(otlp_attribute(
        "teamd.entity_kind",
        Value::String(link.entity_kind.clone()),
    ));
    attributes.push(otlp_attribute(
        "teamd.entity_id",
        Value::String(link.entity_id.clone()),
    ));
    if let Some(surface) = link.surface.as_deref() {
        attributes.push(otlp_attribute(
            "teamd.surface",
            Value::String(surface.to_string()),
        ));
    }
    if let Some(entrypoint) = link.entrypoint.as_deref() {
        attributes.push(otlp_attribute(
            "teamd.entrypoint",
            Value::String(entrypoint.to_string()),
        ));
    }
    if let Ok(Value::Object(extra)) = serde_json::from_str::<Value>(&link.attributes_json) {
        for (key, value) in extra {
            if value.is_null() {
                continue;
            }
            attributes.push(otlp_attribute(&format!("teamd.{key}"), value));
        }
    }

    let timing = resolve_span_timing(store, link)?;

    let mut span = Map::new();
    span.insert("traceId".to_string(), Value::String(link.trace_id.clone()));
    span.insert("spanId".to_string(), Value::String(link.span_id.clone()));
    if let Some(parent_span_id) = link.parent_span_id.as_deref() {
        span.insert(
            "parentSpanId".to_string(),
            Value::String(parent_span_id.to_string()),
        );
    }
    span.insert(
        "name".to_string(),
        Value::String(format!("{} {}", link.entity_kind, link.entity_id)),
    );
    span.insert(
        "kind".to_string(),
        Value::String("SPAN_KIND_INTERNAL".to_string()),
    );
    span.insert(
        "startTimeUnixNano".to_string(),
        Value::String(timing.start_unix_nano.to_string()),
    );
    span.insert(
        "endTimeUnixNano".to_string(),
        Value::String(timing.end_unix_nano.to_string()),
    );
    span.insert("attributes".to_string(), Value::Array(attributes));
    span.insert("status".to_string(), json!({"code": "STATUS_CODE_UNSET"}));
    Ok(Value::Object(span))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpanTiming {
    start_unix_nano: i64,
    end_unix_nano: i64,
}

fn resolve_span_timing(
    store: &impl OtlpTraceRepository,
    link: &TraceLinkRecord,
) -> Result<SpanTiming, OtlpExportError> {
    let (start_seconds, end_seconds) = match link.entity_kind.as_str() {
        "run" => store
            .get_run(&link.entity_id)?
            .map(|run| (run.started_at, run.finished_at.unwrap_or(run.updated_at)))
            .unwrap_or((link.created_at, link.created_at)),
        "tool_call" => store
            .get_tool_call(&link.entity_id)?
            .map(|call| (call.requested_at, call.updated_at))
            .unwrap_or((link.created_at, link.created_at)),
        _ => (link.created_at, link.created_at),
    };

    let start_unix_nano = unix_seconds_to_nanos(start_seconds);
    let mut end_unix_nano = unix_seconds_to_nanos(end_seconds);
    if end_unix_nano <= start_unix_nano {
        end_unix_nano = start_unix_nano.saturating_add(1_000_000);
    }

    Ok(SpanTiming {
        start_unix_nano,
        end_unix_nano,
    })
}

fn unix_seconds_to_nanos(seconds: i64) -> i64 {
    seconds.saturating_mul(1_000_000_000)
}

fn otlp_attribute(key: &str, value: Value) -> Value {
    json!({
        "key": key,
        "value": otlp_any_value(value),
    })
}

fn otlp_any_value(value: Value) -> Value {
    match value {
        Value::Bool(value) => json!({ "boolValue": value }),
        Value::Number(value) => {
            if let Some(integer) = value.as_i64() {
                json!({ "intValue": integer.to_string() })
            } else if let Some(unsigned) = value.as_u64() {
                json!({ "intValue": unsigned.to_string() })
            } else if let Some(float) = value.as_f64() {
                json!({ "doubleValue": float })
            } else {
                json!({ "stringValue": value.to_string() })
            }
        }
        Value::String(value) => json!({ "stringValue": value }),
        Value::Array(values) => json!({
            "arrayValue": {
                "values": values.into_iter().map(otlp_any_value).collect::<Vec<_>>()
            }
        }),
        Value::Object(map) => json!({
            "kvlistValue": {
                "values": map
                    .into_iter()
                    .map(|(key, value)| json!({"key": key, "value": otlp_any_value(value)}))
                    .collect::<Vec<_>>()
            }
        }),
        Value::Null => json!({ "stringValue": "" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_persistence::{
        PersistenceScaffold, PersistenceStore, RunRecord, RunRepository, SessionRecord,
        SessionRepository, ToolCallRecord, ToolCallRepository,
    };

    #[test]
    fn trace_export_payload_uses_otlp_json_shape() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = test_store(temp.path());
        store
            .put_trace_link(&TraceLinkRecord {
                entity_kind: "run".to_string(),
                entity_id: "run-otel".to_string(),
                trace_id: "0123456789abcdef0123456789abcdef".to_string(),
                span_id: "0123456789abcdef".to_string(),
                parent_span_id: None,
                surface: Some("telegram".to_string()),
                entrypoint: Some("telegram.message".to_string()),
                attributes_json: json!({
                    "session_id": "session-otel",
                    "round": 2,
                    "ok": true,
                })
                .to_string(),
                created_at: 10,
            })
            .expect("put trace");

        let rendered = trace_export_payload_json(&store, "0123456789abcdef0123456789abcdef")
            .expect("render trace");

        assert!(rendered.contains("\"resourceSpans\""));
        assert!(rendered.contains("\"scopeSpans\""));
        assert!(rendered.contains("\"traceId\": \"0123456789abcdef0123456789abcdef\""));
        assert!(rendered.contains("\"key\": \"teamd.surface\""));
        assert!(rendered.contains("\"stringValue\": \"telegram\""));
        assert!(rendered.contains("\"key\": \"teamd.round\""));
        assert!(rendered.contains("\"intValue\": \"2\""));
    }

    #[test]
    fn trace_export_payload_uses_entity_timestamps_for_span_intervals() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = test_store(temp.path());
        store
            .put_session(&SessionRecord {
                id: "session-otel".to_string(),
                title: "OTel".to_string(),
                prompt_override: None,
                settings_json: "{}".to_string(),
                workspace_root: temp.path().display().to_string(),
                agent_profile_id: "default".to_string(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: None,
                created_at: 9,
                updated_at: 15,
            })
            .expect("put session");
        store
            .put_run(&RunRecord {
                id: "run-otel".to_string(),
                session_id: "session-otel".to_string(),
                mission_id: None,
                status: "completed".to_string(),
                error: None,
                result: None,
                provider_usage_json: "null".to_string(),
                active_processes_json: "[]".to_string(),
                recent_steps_json: "[]".to_string(),
                evidence_refs_json: "[]".to_string(),
                pending_approvals_json: "[]".to_string(),
                provider_loop_json: "null".to_string(),
                delegate_runs_json: "[]".to_string(),
                started_at: 10,
                updated_at: 15,
                finished_at: Some(15),
            })
            .expect("put run");
        store
            .put_tool_call(&ToolCallRecord {
                id: "toolcall-otel".to_string(),
                session_id: "session-otel".to_string(),
                run_id: "run-otel".to_string(),
                provider_tool_call_id: "call_otel".to_string(),
                tool_name: "web_fetch".to_string(),
                arguments_json: "{}".to_string(),
                summary: "web_fetch".to_string(),
                status: "completed".to_string(),
                error: None,
                result_summary: Some("ok".to_string()),
                result_preview: Some("ok".to_string()),
                result_artifact_id: None,
                result_truncated: false,
                result_byte_len: Some(2),
                requested_at: 11,
                updated_at: 14,
            })
            .expect("put tool call");
        store
            .put_trace_link(&TraceLinkRecord {
                entity_kind: "run".to_string(),
                entity_id: "run-otel".to_string(),
                trace_id: "0123456789abcdef0123456789abcdef".to_string(),
                span_id: "0123456789abcdef".to_string(),
                parent_span_id: None,
                surface: Some("telegram".to_string()),
                entrypoint: Some("telegram.message".to_string()),
                attributes_json: json!({"session_id": "session-otel"}).to_string(),
                created_at: 10,
            })
            .expect("put run trace");
        store
            .put_trace_link(&TraceLinkRecord {
                entity_kind: "tool_call".to_string(),
                entity_id: "toolcall-otel".to_string(),
                trace_id: "0123456789abcdef0123456789abcdef".to_string(),
                span_id: "fedcba9876543210".to_string(),
                parent_span_id: Some("0123456789abcdef".to_string()),
                surface: Some("telegram".to_string()),
                entrypoint: Some("telegram.message".to_string()),
                attributes_json: json!({"tool_name": "web_fetch"}).to_string(),
                created_at: 11,
            })
            .expect("put tool trace");
        store
            .put_trace_link(&TraceLinkRecord {
                entity_kind: "provider_round".to_string(),
                entity_id: "provider-round-run-otel-r1".to_string(),
                trace_id: "0123456789abcdef0123456789abcdef".to_string(),
                span_id: "0011223344556677".to_string(),
                parent_span_id: Some("0123456789abcdef".to_string()),
                surface: Some("telegram".to_string()),
                entrypoint: Some("telegram.message".to_string()),
                attributes_json: json!({"round": 1}).to_string(),
                created_at: 12,
            })
            .expect("put provider round trace");

        let rendered = trace_export_payload_json(&store, "0123456789abcdef0123456789abcdef")
            .expect("render trace");

        assert!(rendered.contains("\"startTimeUnixNano\": \"10000000000\""));
        assert!(rendered.contains("\"endTimeUnixNano\": \"15000000000\""));
        assert!(rendered.contains("\"startTimeUnixNano\": \"11000000000\""));
        assert!(rendered.contains("\"endTimeUnixNano\": \"14000000000\""));
        assert!(rendered.contains("\"startTimeUnixNano\": \"12000000000\""));
        assert!(rendered.contains("\"endTimeUnixNano\": \"12001000000\""));
    }

    fn test_store(root: &std::path::Path) -> PersistenceStore {
        let scaffold = PersistenceScaffold::from_config(agent_persistence::AppConfig {
            data_dir: root.join("state"),
            ..agent_persistence::AppConfig::default()
        });
        PersistenceStore::open(&scaffold).expect("open store")
    }
}
