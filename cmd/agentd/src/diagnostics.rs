use crate::bootstrap::BootstrapError;
use agent_persistence::AppConfig;
use agent_persistence::audit::{AuditLogConfig, DiagnosticEvent};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct DiagnosticEventBuilder {
    event: DiagnosticEvent,
}

impl DiagnosticEventBuilder {
    pub fn new(
        config: &AppConfig,
        level: impl Into<String>,
        component: impl Into<String>,
        op: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::from_data_dir(
            config.data_dir.display().to_string(),
            level,
            component,
            op,
            message,
        )
    }

    pub fn from_data_dir(
        data_dir: impl Into<String>,
        level: impl Into<String>,
        component: impl Into<String>,
        op: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let mut event = DiagnosticEvent::new(level, component, op, message, data_dir);
        event.pid = Some(std::process::id());
        event.uid = Some(current_uid());
        event.euid = Some(current_euid());
        Self { event }
    }

    pub fn session_id(mut self, session_id: impl Into<String>) -> Self {
        self.event.session_id = Some(session_id.into());
        self
    }

    pub fn run_id(mut self, run_id: impl Into<String>) -> Self {
        self.event.run_id = Some(run_id.into());
        self
    }

    pub fn job_id(mut self, job_id: impl Into<String>) -> Self {
        self.event.job_id = Some(job_id.into());
        self
    }

    pub fn daemon_base_url(mut self, daemon_base_url: impl Into<String>) -> Self {
        self.event.daemon_base_url = Some(daemon_base_url.into());
        self
    }

    pub fn trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.event.trace_id = Some(trace_id.into());
        self
    }

    pub fn span_id(mut self, span_id: impl Into<String>) -> Self {
        self.event.span_id = Some(span_id.into());
        self
    }

    pub fn parent_span_id(mut self, parent_span_id: impl Into<String>) -> Self {
        self.event.parent_span_id = Some(parent_span_id.into());
        self
    }

    pub fn surface(mut self, surface: impl Into<String>) -> Self {
        self.event.surface = Some(surface.into());
        self
    }

    pub fn entrypoint(mut self, entrypoint: impl Into<String>) -> Self {
        self.event.entrypoint = Some(entrypoint.into());
        self
    }

    pub fn outcome(mut self, outcome: impl Into<String>) -> Self {
        self.event.outcome = Some(outcome.into());
        self
    }

    pub fn error(mut self, error: impl Into<String>) -> Self {
        self.event.error = Some(error.into());
        self
    }

    pub fn elapsed_ms(mut self, elapsed_ms: u64) -> Self {
        self.event.elapsed_ms = Some(elapsed_ms);
        self
    }

    pub fn field<T>(mut self, key: &str, value: T) -> Self
    where
        T: Serialize,
    {
        if let Ok(value) = serde_json::to_value(value) {
            self.event.fields.insert(key.to_string(), value);
        }
        self
    }

    pub fn field_value(mut self, key: &str, value: Value) -> Self {
        self.event.fields.insert(key.to_string(), value);
        self
    }

    pub fn event(&self) -> &DiagnosticEvent {
        &self.event
    }

    pub fn emit(&self, audit: &AuditLogConfig) {
        audit.append_event_best_effort(&self.event);
    }
}

pub fn render_diagnostic_tail(
    audit: &AuditLogConfig,
    max_lines: usize,
) -> Result<String, BootstrapError> {
    let lines = audit.read_tail_lines(max_lines).map_err(map_audit_error)?;
    if lines.is_empty() {
        return Ok("диагностический лог пуст".to_string());
    }
    Ok(lines.join("\n"))
}

fn map_audit_error(error: agent_persistence::audit::AuditLogError) -> BootstrapError {
    match error {
        agent_persistence::audit::AuditLogError::Io { path, source } => {
            BootstrapError::Io { path, source }
        }
        agent_persistence::audit::AuditLogError::Serialize(source) => BootstrapError::Usage {
            reason: format!("diagnostic log serialization failed: {source}"),
        },
    }
}

fn current_uid() -> u32 {
    unsafe { libc::getuid() }
}

fn current_euid() -> u32 {
    unsafe { libc::geteuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_diagnostic_tail_reads_recent_lines() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = AppConfig {
            data_dir: temp.path().join("state-root"),
            ..AppConfig::default()
        };
        let audit = AuditLogConfig::from_config(&config);
        DiagnosticEventBuilder::new(&config, "info", "test", "one", "first").emit(&audit);
        DiagnosticEventBuilder::new(&config, "info", "test", "two", "second").emit(&audit);

        let tail = render_diagnostic_tail(&audit, 1).expect("render tail");
        assert!(tail.contains("\"message\":\"second\""));
        assert!(!tail.contains("\"message\":\"first\""));
    }
}
