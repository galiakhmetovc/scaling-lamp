use agent_persistence::{StoreError, TraceLinkRecord, TraceRepository};
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
    store: &impl TraceRepository,
    trace_id: &str,
) -> Result<String, OtlpExportError> {
    let links = trace_links(store, trace_id)?;
    serde_json::to_string_pretty(&trace_export_payload(&links)?).map_err(OtlpExportError::from)
}

pub fn export_trace_to_otlp_http(
    store: &impl TraceRepository,
    trace_id: &str,
    endpoint: &str,
    timeout: Duration,
) -> Result<OtlpExportReport, OtlpExportError> {
    let links = trace_links(store, trace_id)?;
    let payload = trace_export_payload(&links)?;
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
    store: &impl TraceRepository,
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

fn trace_export_payload(links: &[TraceLinkRecord]) -> Result<Value, OtlpExportError> {
    let spans = links
        .iter()
        .map(trace_link_to_span)
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

fn trace_link_to_span(link: &TraceLinkRecord) -> Result<Value, OtlpExportError> {
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
        Value::String(unix_seconds_to_nanos(link.created_at)),
    );
    span.insert(
        "endTimeUnixNano".to_string(),
        Value::String(unix_seconds_to_nanos(link.created_at)),
    );
    span.insert("attributes".to_string(), Value::Array(attributes));
    span.insert("status".to_string(), json!({"code": "STATUS_CODE_UNSET"}));
    Ok(Value::Object(span))
}

fn unix_seconds_to_nanos(seconds: i64) -> String {
    seconds.saturating_mul(1_000_000_000).to_string()
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
    use agent_persistence::{PersistenceScaffold, PersistenceStore};

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

    fn test_store(root: &std::path::Path) -> PersistenceStore {
        let scaffold = PersistenceScaffold::from_config(agent_persistence::AppConfig {
            data_dir: root.join("state"),
            ..agent_persistence::AppConfig::default()
        });
        PersistenceStore::open(&scaffold).expect("open store")
    }
}
