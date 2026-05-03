use super::*;
use agent_persistence::ToolCallRepository;
use agent_persistence::audit::{AuditLogConfig, DiagnosticEvent};
use agent_runtime::provider::{ProviderMessage, ProviderRequest, ProviderStreamMode};
use agent_runtime::session::MessageRole;
use agent_runtime::tool::{MemoryAddInput, MemorySearchInput};
use serde::Deserialize;
use serde_json::{Value, json};

const MEMORY_CURATOR_INSTRUCTIONS: &str = r#"You are TeamD's memory curator.

Read the compact turn packet and decide whether any durable memories should be stored.
Return only valid JSON with this shape:
{"candidates":[{"action":"add","scope":"operator|agent|workspace|session","text":"short durable fact","confidence":0.0-1.0,"reason":"why this is durable"}],"rejected":[]}

Rules:
- Store only stable facts that will matter in future turns: operator preferences, durable project facts, agent operating preferences, or explicit long-term instructions.
- Do not store raw transcript, one-off task progress, temporary status, secrets, passwords, tokens, API keys, pairing keys, private credentials, or sensitive document contents.
- Prefer scope=operator for the human's personal preferences.
- Prefer scope=workspace for durable project facts.
- Use action=add only.
- Keep each text self-contained and concise.
"#;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct MemoryCuratorDecision {
    candidates: Vec<MemoryCuratorCandidate>,
    rejected: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct MemoryCuratorCandidate {
    action: String,
    scope: String,
    text: String,
    confidence: f64,
    reason: String,
}

impl Default for MemoryCuratorCandidate {
    fn default() -> Self {
        Self {
            action: "add".to_string(),
            scope: "operator".to_string(),
            text: String::new(),
            confidence: 1.0,
            reason: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryCuratorAppliedAction {
    status: String,
    scope: String,
    text: String,
    reason: String,
}

impl ExecutionService {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_memory_curator_after_chat_turn(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        session: &Session,
        run_id: &str,
        user_text: &str,
        assistant_text: &str,
        at: i64,
    ) {
        if !self.memory_curator_should_run() {
            return;
        }

        let outcome = self.execute_memory_curator_after_chat_turn(
            store,
            provider,
            session,
            run_id,
            user_text,
            assistant_text,
            at,
        );
        match outcome {
            Ok(actions) => {
                self.record_memory_curator_audit(session.id.as_str(), run_id, "ok", None, &actions)
            }
            Err(error) => self.record_memory_curator_audit(
                session.id.as_str(),
                run_id,
                "error",
                Some(error.to_string()),
                &[],
            ),
        }
    }

    fn memory_curator_should_run(&self) -> bool {
        self.config.memory_curator.enabled
            && self.config.mem0.enabled
            && self.config.memory_curator.mode.trim() != "off"
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_memory_curator_after_chat_turn(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        session: &Session,
        run_id: &str,
        user_text: &str,
        assistant_text: &str,
        at: i64,
    ) -> Result<Vec<MemoryCuratorAppliedAction>, ExecutionError> {
        let tool_calls = store
            .list_tool_calls_for_run(run_id)
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(|call| {
                json!({
                    "tool": call.tool_name,
                    "status": call.status,
                    "summary": call.summary,
                    "error": call.error,
                    "result_summary": call.result_summary,
                })
            })
            .collect::<Vec<_>>();

        let packet = json!({
            "session_id": session.id,
            "run_id": run_id,
            "agent_profile_id": session.agent_profile_id,
            "workspace_root": session.workspace_root,
            "user_message": user_text,
            "assistant_message": assistant_text,
            "tool_calls": tool_calls,
        });

        let response = provider
            .complete(&ProviderRequest {
                model: session.settings.model.clone(),
                instructions: Some(MEMORY_CURATOR_INSTRUCTIONS.to_string()),
                messages: vec![ProviderMessage::new(MessageRole::User, packet.to_string())],
                think_level: Some("off".to_string()),
                previous_response_id: None,
                continuation_messages: Vec::new(),
                tools: Vec::new(),
                tool_outputs: Vec::new(),
                max_output_tokens: Some(self.config.memory_curator.max_output_tokens),
                stream: ProviderStreamMode::Disabled,
            })
            .map_err(ExecutionError::Provider)?;

        let decision = parse_memory_curator_decision(&response.output_text)?;
        let limited_candidates = decision
            .candidates
            .into_iter()
            .take(self.config.memory_curator.max_candidates);
        let mut actions = Vec::new();

        for candidate in limited_candidates {
            let action = match self.apply_memory_curator_candidate(
                store,
                session,
                run_id,
                candidate.clone(),
                at,
            ) {
                Ok(action) => action,
                Err(error) => MemoryCuratorAppliedAction {
                    status: "failed".to_string(),
                    scope: candidate.scope,
                    text: candidate.text,
                    reason: error.to_string(),
                },
            };
            actions.push(action);
        }

        if !decision.rejected.is_empty() {
            actions.push(MemoryCuratorAppliedAction {
                status: "rejected_by_curator".to_string(),
                scope: String::new(),
                text: String::new(),
                reason: format!("{} rejected candidates", decision.rejected.len()),
            });
        }

        Ok(actions)
    }

    fn apply_memory_curator_candidate(
        &self,
        store: &PersistenceStore,
        session: &Session,
        run_id: &str,
        candidate: MemoryCuratorCandidate,
        at: i64,
    ) -> Result<MemoryCuratorAppliedAction, ExecutionError> {
        let text = candidate.text.trim().to_string();
        let scope = match candidate.scope.trim() {
            "" => "operator".to_string(),
            value => value.to_string(),
        };
        if candidate.action.trim() != "add" {
            return Ok(MemoryCuratorAppliedAction {
                status: "skipped_unsupported_action".to_string(),
                scope,
                text,
                reason: candidate.action,
            });
        }
        if text.is_empty() {
            return Ok(MemoryCuratorAppliedAction {
                status: "skipped_empty".to_string(),
                scope,
                text,
                reason: "candidate text is empty".to_string(),
            });
        }
        if candidate.confidence < self.config.memory_curator.min_confidence {
            return Ok(MemoryCuratorAppliedAction {
                status: "skipped_low_confidence".to_string(),
                scope,
                text,
                reason: candidate.confidence.to_string(),
            });
        }
        if let Some(reason) = sensitive_memory_rejection_reason(text.as_str()) {
            return Ok(MemoryCuratorAppliedAction {
                status: "skipped_sensitive".to_string(),
                scope,
                text,
                reason: reason.to_string(),
            });
        }
        if self.config.memory_curator.mode.trim() == "review" {
            return Ok(MemoryCuratorAppliedAction {
                status: "review_required".to_string(),
                scope,
                text,
                reason: candidate.reason,
            });
        }

        let search = self.search_semantic_memory(
            store,
            session.id.as_str(),
            &MemorySearchInput {
                query: text.clone(),
                scope: Some(scope.clone()),
                limit: Some(3),
                filters: Value::Null,
            },
        )?;
        let normalized = normalize_memory_text(text.as_str());
        if search
            .results
            .iter()
            .any(|memory| normalize_memory_text(memory.memory.as_str()) == normalized)
        {
            return Ok(MemoryCuratorAppliedAction {
                status: "skipped_duplicate".to_string(),
                scope,
                text,
                reason: "same memory already exists".to_string(),
            });
        }

        self.add_semantic_memory(
            store,
            session.id.as_str(),
            &MemoryAddInput {
                text: text.clone(),
                messages: Vec::new(),
                scope: Some(scope.clone()),
                infer: None,
                metadata: json!({
                    "teamd_source": "memory_curator",
                    "teamd_curator_run_id": run_id,
                    "teamd_curator_confidence": candidate.confidence,
                    "teamd_curator_reason": candidate.reason,
                }),
            },
            at,
        )?;

        Ok(MemoryCuratorAppliedAction {
            status: "saved".to_string(),
            scope,
            text,
            reason: "memory_add completed".to_string(),
        })
    }

    fn record_memory_curator_audit(
        &self,
        session_id: &str,
        run_id: &str,
        outcome: &str,
        error: Option<String>,
        actions: &[MemoryCuratorAppliedAction],
    ) {
        let audit = AuditLogConfig {
            path: self.config.data_dir.join("audit/runtime.jsonl"),
        };
        let mut event = DiagnosticEvent::new(
            if error.is_some() { "warn" } else { "info" },
            "memory_curator",
            "post_turn",
            "memory curator post-turn pass completed",
            self.config.data_dir.display().to_string(),
        );
        event.session_id = Some(session_id.to_string());
        event.run_id = Some(run_id.to_string());
        event.outcome = Some(outcome.to_string());
        event.error = error;
        event
            .fields
            .insert("action_count".to_string(), json!(actions.len()));
        event.fields.insert(
            "saved_count".to_string(),
            json!(
                actions
                    .iter()
                    .filter(|action| action.status == "saved")
                    .count()
            ),
        );
        event.fields.insert(
            "statuses".to_string(),
            json!(
                actions
                    .iter()
                    .map(|action| action.status.as_str())
                    .collect::<Vec<_>>()
            ),
        );
        audit.append_event_best_effort(&event);
    }
}

fn parse_memory_curator_decision(raw: &str) -> Result<MemoryCuratorDecision, ExecutionError> {
    let json_text = extract_json_object(raw).ok_or_else(|| ExecutionError::ProviderLoop {
        reason: "memory curator response did not contain a JSON object".to_string(),
    })?;
    serde_json::from_str(json_text).map_err(|error| ExecutionError::ProviderLoop {
        reason: format!("failed to parse memory curator JSON: {error}"),
    })
}

fn extract_json_object(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    (start < end).then_some(&trimmed[start..=end])
}

fn sensitive_memory_rejection_reason(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    let sensitive_markers = [
        "password",
        "пароль",
        "token",
        "токен",
        "api key",
        "api_key",
        "apikey",
        "secret",
        "bearer",
        "ssh",
        "pairing key",
        "ключ",
    ];
    sensitive_markers
        .iter()
        .any(|marker| lower.contains(marker))
        .then_some("candidate looks like a secret or credential")
}

fn normalize_memory_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|ch: char| ch.is_ascii_punctuation())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_curator_json_inside_markdown_fence() {
        let decision = parse_memory_curator_decision(
            r#"```json
{"candidates":[{"scope":"operator","text":"Пользователю нравится зелёный цвет.","confidence":0.9}],"rejected":[]}
```"#,
        )
        .expect("parse decision");

        assert_eq!(decision.candidates.len(), 1);
        assert_eq!(decision.candidates[0].action, "add");
        assert_eq!(decision.candidates[0].scope, "operator");
        assert_eq!(decision.candidates[0].confidence, 0.9);
    }

    #[test]
    fn rejects_sensitive_memory_candidates() {
        assert_eq!(
            sensitive_memory_rejection_reason("API key is abc123"),
            Some("candidate looks like a secret or credential")
        );
        assert_eq!(
            sensitive_memory_rejection_reason("Пользователю нравится зелёный цвет."),
            None
        );
    }
}
