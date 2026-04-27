use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditLogConfig {
    pub path: PathBuf,
}

#[derive(Debug)]
pub enum AuditLogError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Serialize(serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub ts: i64,
    pub level: String,
    pub component: String,
    pub op: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub euid: Option<u32>,
    pub data_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, Value>,
}

impl AuditLogConfig {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            path: config.data_dir.join("audit/runtime.jsonl"),
        }
    }

    pub fn append_event(&self, event: &DiagnosticEvent) -> Result<(), AuditLogError> {
        append_event(&self.path, event)
    }

    pub fn read_tail_lines(&self, max_lines: usize) -> Result<Vec<String>, AuditLogError> {
        read_tail_lines(&self.path, max_lines)
    }

    pub fn append_event_best_effort(&self, event: &DiagnosticEvent) {
        let _ = self.append_event(event);
    }
}

impl DiagnosticEvent {
    pub fn new(
        level: impl Into<String>,
        component: impl Into<String>,
        op: impl Into<String>,
        message: impl Into<String>,
        data_dir: impl Into<String>,
    ) -> Self {
        Self {
            ts: unix_timestamp(),
            level: level.into(),
            component: component.into(),
            op: op.into(),
            message: message.into(),
            pid: None,
            uid: None,
            euid: None,
            data_dir: data_dir.into(),
            session_id: None,
            run_id: None,
            job_id: None,
            daemon_base_url: None,
            trace_id: None,
            span_id: None,
            parent_span_id: None,
            surface: None,
            entrypoint: None,
            elapsed_ms: None,
            outcome: None,
            error: None,
            fields: BTreeMap::new(),
        }
    }
}

pub fn append_event(path: &PathBuf, event: &DiagnosticEvent) -> Result<(), AuditLogError> {
    let parent = path.parent().ok_or_else(|| AuditLogError::Io {
        path: path.clone(),
        source: std::io::Error::other("audit path must have a parent directory"),
    })?;
    std::fs::create_dir_all(parent).map_err(|source| AuditLogError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| AuditLogError::Io {
            path: path.clone(),
            source,
        })?;
    serde_json::to_writer(&mut file, event).map_err(AuditLogError::Serialize)?;
    file.write_all(b"\n").map_err(|source| AuditLogError::Io {
        path: path.clone(),
        source,
    })?;
    Ok(())
}

pub fn read_tail_lines(path: &PathBuf, max_lines: usize) -> Result<Vec<String>, AuditLogError> {
    if max_lines == 0 {
        return Ok(Vec::new());
    }
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = std::fs::File::open(path).map_err(|source| AuditLogError::Io {
        path: path.clone(),
        source,
    })?;
    let reader = BufReader::new(file);
    let mut tail = VecDeque::with_capacity(max_lines);
    for line in reader.lines() {
        let line = line.map_err(|source| AuditLogError::Io {
            path: path.clone(),
            source,
        })?;
        if tail.len() == max_lines {
            tail.pop_front();
        }
        tail.push_back(line);
    }
    Ok(tail.into_iter().collect())
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn audit_log_appends_events_and_reads_bounded_tail() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("audit/runtime.jsonl");

        for index in 0..5 {
            let mut event = DiagnosticEvent::new(
                "info",
                "test",
                "append",
                format!("event-{index}"),
                temp.path().display().to_string(),
            );
            event.fields.insert("index".to_string(), json!(index));
            append_event(&path, &event).expect("append event");
        }

        let tail = read_tail_lines(&path, 2).expect("read tail");
        assert_eq!(tail.len(), 2);
        assert!(tail[0].contains("event-3"));
        assert!(tail[1].contains("event-4"));
    }
}
