use agent_persistence::{TraceLinkRecord, TraceRepository};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnTraceSource {
    pub surface: String,
    pub entrypoint: String,
}

impl TurnTraceSource {
    pub fn new(surface: impl Into<String>, entrypoint: impl Into<String>) -> Self {
        Self {
            surface: surface.into(),
            entrypoint: entrypoint.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTraceContext {
    pub trace_id: String,
    pub run_span_id: String,
    pub surface: String,
    pub entrypoint: String,
}

impl RuntimeTraceContext {
    pub fn for_run(run_id: &str, source: Option<&TurnTraceSource>) -> Self {
        let source = source
            .cloned()
            .unwrap_or_else(|| TurnTraceSource::new("runtime", "chat.turn"));
        Self {
            trace_id: otel_trace_id(run_id),
            run_span_id: otel_span_id("run", run_id),
            surface: source.surface,
            entrypoint: source.entrypoint,
        }
    }

    pub fn from_run_link_or_default(
        store: &impl TraceRepository,
        run_id: &str,
    ) -> Result<Self, agent_persistence::StoreError> {
        if let Some(link) = store.get_trace_link("run", run_id)? {
            return Ok(Self {
                trace_id: link.trace_id,
                run_span_id: link.span_id,
                surface: link.surface.unwrap_or_else(|| "runtime".to_string()),
                entrypoint: link.entrypoint.unwrap_or_else(|| "chat.turn".to_string()),
            });
        }

        Ok(Self::for_run(run_id, None))
    }

    pub fn run_link(&self, run_id: &str, session_id: &str, now: i64) -> TraceLinkRecord {
        self.link(
            "run",
            run_id,
            self.run_span_id.clone(),
            None,
            serde_json::json!({
                "run_id": run_id,
                "session_id": session_id,
            }),
            now,
        )
    }

    pub fn child_link(
        &self,
        entity_kind: &str,
        entity_id: &str,
        attributes: serde_json::Value,
        now: i64,
    ) -> TraceLinkRecord {
        self.link(
            entity_kind,
            entity_id,
            otel_span_id(entity_kind, entity_id),
            Some(self.run_span_id.clone()),
            attributes,
            now,
        )
    }

    fn link(
        &self,
        entity_kind: &str,
        entity_id: &str,
        span_id: String,
        parent_span_id: Option<String>,
        attributes: serde_json::Value,
        now: i64,
    ) -> TraceLinkRecord {
        TraceLinkRecord {
            entity_kind: entity_kind.to_string(),
            entity_id: entity_id.to_string(),
            trace_id: self.trace_id.clone(),
            span_id,
            parent_span_id,
            surface: Some(self.surface.clone()),
            entrypoint: Some(self.entrypoint.clone()),
            attributes_json: attributes.to_string(),
            created_at: now,
        }
    }
}

pub fn otel_trace_id(seed: &str) -> String {
    hash_hex_prefix(&format!("trace:{seed}"), 32)
}

pub fn otel_span_id(kind: &str, seed: &str) -> String {
    hash_hex_prefix(&format!("span:{kind}:{seed}"), 16)
}

fn hash_hex_prefix(seed: &str, hex_len: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(seed.as_bytes());
    let mut output = String::with_capacity(hex_len);
    for byte in digest {
        if output.len() >= hex_len {
            break;
        }
        output.push(HEX[(byte >> 4) as usize] as char);
        if output.len() >= hex_len {
            break;
        }
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_match_otel_lengths() {
        assert_eq!(otel_trace_id("run-1").len(), 32);
        assert_eq!(otel_span_id("run", "run-1").len(), 16);
        assert!(
            otel_trace_id("run-1")
                .chars()
                .all(|c| c.is_ascii_hexdigit())
        );
        assert!(
            otel_span_id("run", "run-1")
                .chars()
                .all(|c| c.is_ascii_hexdigit())
        );
    }
}
