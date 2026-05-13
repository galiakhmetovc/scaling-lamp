use super::*;
use agent_persistence::TranscriptRecord;
use agent_persistence::audit::{AuditLogConfig, DiagnosticEvent};
use agent_runtime::prompt::{MemoryRecall, MemoryRecallItem};
use agent_runtime::tool::{MemoryItemOutput, MemorySearchInput};
use serde_json::{Value, json};
use std::collections::HashSet;

impl ExecutionService {
    pub fn preview_memory_recall_for_session(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        query: Option<&str>,
    ) -> Result<Option<MemoryRecall>, ExecutionError> {
        if !self.memory_recall_should_run() {
            return Ok(None);
        }

        let session = self.load_session(store, session_id)?;
        let resolved_query = match query.map(str::trim).filter(|value| !value.is_empty()) {
            Some(query) => truncate_chars(query, self.config.memory_recall.max_query_chars),
            None => {
                let transcripts = store
                    .list_transcripts_for_session(session_id)
                    .map_err(ExecutionError::Store)?;
                let Some(query) = latest_user_memory_recall_query(
                    &transcripts,
                    self.config.memory_recall.max_query_chars,
                ) else {
                    return Ok(None);
                };
                query
            }
        };

        self.execute_memory_recall_for_prompt(store, &session, resolved_query.as_str())
            .map(Some)
            .map_err(|(error, _scope_errors)| error)
    }

    pub(super) fn memory_recall_for_prompt(
        &self,
        store: &PersistenceStore,
        session: &Session,
        transcripts: &[TranscriptRecord],
    ) -> Option<MemoryRecall> {
        if !self.memory_recall_should_run() {
            return None;
        }
        let query = latest_user_memory_recall_query(
            transcripts,
            self.config.memory_recall.max_query_chars,
        )?;

        let outcome = self.execute_memory_recall_for_prompt(store, session, query.as_str());
        match outcome {
            Ok(recall) => {
                self.record_memory_recall_audit(MemoryRecallAudit {
                    session_id: session.id.as_str(),
                    outcome: "ok",
                    error: None,
                    query: query.as_str(),
                    result_count: recall.items.len(),
                    truncated: recall.truncated,
                    scope_errors: Vec::new(),
                });
                Some(recall).filter(|recall| !recall.items.is_empty())
            }
            Err((error, scope_errors)) => {
                self.record_memory_recall_audit(MemoryRecallAudit {
                    session_id: session.id.as_str(),
                    outcome: "error",
                    error: Some(error.to_string()),
                    query: query.as_str(),
                    result_count: 0,
                    truncated: false,
                    scope_errors,
                });
                None
            }
        }
    }

    fn memory_recall_should_run(&self) -> bool {
        self.config.memory_recall.enabled && self.config.mem0.enabled
    }

    fn execute_memory_recall_for_prompt(
        &self,
        store: &PersistenceStore,
        session: &Session,
        query: &str,
    ) -> Result<MemoryRecall, (ExecutionError, Vec<String>)> {
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let mut truncated = false;
        let mut scope_errors = Vec::new();

        for scope in &self.config.memory_recall.scopes {
            if items.len() >= self.config.memory_recall.max_results {
                truncated = true;
                break;
            }
            let remaining = self.config.memory_recall.max_results - items.len();
            let input = MemorySearchInput {
                query: query.to_string(),
                scope: Some(scope.clone()),
                limit: Some(remaining),
                filters: Value::Null,
            };
            let output = match self.search_semantic_memory(store, session.id.as_str(), &input) {
                Ok(output) => output,
                Err(error) => {
                    scope_errors.push(format!("{scope}: {error}"));
                    continue;
                }
            };
            truncated |= output.truncated;
            for memory in output.results {
                if items.len() >= self.config.memory_recall.max_results {
                    truncated = true;
                    break;
                }
                let dedup_key = memory_dedup_key(scope.as_str(), &memory);
                if !seen.insert(dedup_key) {
                    continue;
                }
                items.push(memory_recall_item(
                    scope.as_str(),
                    memory,
                    self.config.memory_recall.max_memory_chars,
                ));
            }
        }

        if items.is_empty() && !scope_errors.is_empty() {
            return Err((
                ExecutionError::ProviderLoop {
                    reason: "memory recall failed for all configured scopes".to_string(),
                },
                scope_errors,
            ));
        }

        Ok(MemoryRecall {
            query: query.to_string(),
            items,
            truncated,
        })
    }

    fn record_memory_recall_audit(&self, audit_input: MemoryRecallAudit<'_>) {
        let audit = AuditLogConfig {
            path: self.config.data_dir.join("audit/runtime.jsonl"),
        };
        let mut event = DiagnosticEvent::new(
            if audit_input.error.is_some() {
                "warn"
            } else {
                "info"
            },
            "memory_recall",
            "pre_turn",
            "memory recall pre-turn pass completed",
            self.config.data_dir.display().to_string(),
        );
        event.session_id = Some(audit_input.session_id.to_string());
        event.outcome = Some(audit_input.outcome.to_string());
        event.error = audit_input.error;
        event.fields.insert(
            "query_chars".to_string(),
            json!(audit_input.query.chars().count()),
        );
        event.fields.insert(
            "scopes".to_string(),
            json!(self.config.memory_recall.scopes.clone()),
        );
        event
            .fields
            .insert("result_count".to_string(), json!(audit_input.result_count));
        event
            .fields
            .insert("truncated".to_string(), json!(audit_input.truncated));
        if !audit_input.scope_errors.is_empty() {
            event
                .fields
                .insert("scope_errors".to_string(), json!(audit_input.scope_errors));
        }
        audit.append_event_best_effort(&event);
    }
}

struct MemoryRecallAudit<'a> {
    session_id: &'a str,
    outcome: &'a str,
    error: Option<String>,
    query: &'a str,
    result_count: usize,
    truncated: bool,
    scope_errors: Vec<String>,
}

fn latest_user_memory_recall_query(
    transcripts: &[TranscriptRecord],
    max_query_chars: usize,
) -> Option<String> {
    transcripts.iter().rev().find_map(|record| {
        if record.kind != "user" {
            return None;
        }
        let query = truncate_chars(record.content.trim(), max_query_chars);
        (!query.trim().is_empty()).then_some(query)
    })
}

fn memory_recall_item(
    scope: &str,
    memory: MemoryItemOutput,
    max_memory_chars: usize,
) -> MemoryRecallItem {
    MemoryRecallItem {
        scope: scope.to_string(),
        memory_id: memory.id,
        memory: truncate_chars(memory.memory.trim(), max_memory_chars),
        score: memory.score,
        source: memory
            .metadata
            .get("teamd_source")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn memory_dedup_key(scope: &str, memory: &MemoryItemOutput) -> String {
    if !memory.id.trim().is_empty() {
        return format!("{scope}:id:{}", memory.id);
    }
    format!(
        "{scope}:text:{}",
        memory
            .memory
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}
