use super::*;
use agent_runtime::tool::{
    MemoryAddInput, MemoryAddOutput, MemoryDeleteInput, MemoryDeleteOutput, MemoryItemOutput,
    MemoryListInput, MemoryListOutput, MemoryMessageInput, MemorySearchInput, MemorySearchOutput,
    MemoryUpdateInput, MemoryUpdateOutput,
};
use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{Map, Value, json};
use std::time::Duration;

use super::scopes::{AGENT_SHARED_SCOPE_ID, RuntimeScope, workspace_scope_id};

#[derive(Debug, Clone)]
struct Mem0ScopeIds {
    scope: RuntimeScope,
    user_id: Option<String>,
    agent_id: Option<String>,
    app_id: Option<String>,
    run_id: Option<String>,
}

impl ExecutionService {
    pub(super) fn add_semantic_memory(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &MemoryAddInput,
        now: i64,
    ) -> Result<MemoryAddOutput, ExecutionError> {
        self.ensure_mem0_enabled()?;
        let session = self.load_session(store, session_id)?;
        let ids = self.mem0_scope_ids(&session, input.scope.as_deref())?;
        let messages = self.mem0_messages(input)?;
        let mut metadata = memory_metadata(&input.metadata)?;
        insert_metadata(&mut metadata, "teamd_scope", ids.scope.as_str());
        insert_metadata(&mut metadata, "teamd_session_id", session.id.as_str());
        insert_metadata(
            &mut metadata,
            "teamd_agent_profile_id",
            session.agent_profile_id.as_str(),
        );
        insert_metadata(
            &mut metadata,
            "teamd_workspace_root",
            session.workspace_root.display().to_string(),
        );
        insert_metadata_if_missing(&mut metadata, "teamd_source", "memory_add");
        insert_metadata(&mut metadata, "teamd_created_at", now);

        let mut body = json!({
            "messages": messages,
            "metadata": Value::Object(metadata),
        });
        insert_optional_string(&mut body, "user_id", ids.user_id.as_deref());
        insert_optional_string(&mut body, "agent_id", ids.agent_id.as_deref());
        insert_optional_string(&mut body, "app_id", ids.app_id.as_deref());
        insert_optional_string(&mut body, "run_id", ids.run_id.as_deref());
        if let Some(infer) = input.infer {
            body["infer"] = Value::Bool(infer);
        }

        let response = self.mem0_request(Method::Post, "memories")?.json(&body);
        let value = send_mem0_json(response)?;
        Ok(MemoryAddOutput {
            status: response_status(&value, "created"),
            memories: parse_memory_items(&value),
        })
    }

    pub(super) fn search_semantic_memory(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &MemorySearchInput,
    ) -> Result<MemorySearchOutput, ExecutionError> {
        self.ensure_mem0_enabled()?;
        if input.query.trim().is_empty() {
            return Err(invalid_mem0_tool("memory_search query must not be empty"));
        }
        let session = self.load_session(store, session_id)?;
        let ids = self.mem0_scope_ids(&session, input.scope.as_deref())?;
        let limit = self.mem0_limit(input.limit);
        let body = mem0_search_body(input.query.as_str(), &ids, limit, &input.filters)?;
        let response = self.mem0_request(Method::Post, "search")?.json(&body);
        let value = send_mem0_json(response)?;
        let mut results = parse_memory_items(&value);
        let truncated = results.len() > limit;
        results.truncate(limit);
        Ok(MemorySearchOutput {
            query: input.query.clone(),
            results,
            truncated,
            limit,
        })
    }

    pub(super) fn list_semantic_memories(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &MemoryListInput,
    ) -> Result<MemoryListOutput, ExecutionError> {
        self.ensure_mem0_enabled()?;
        let session = self.load_session(store, session_id)?;
        let ids = self.mem0_scope_ids(&session, input.scope.as_deref())?;
        let limit = self.mem0_limit(input.limit);
        let offset = input.offset.unwrap_or(0);
        let response = self
            .mem0_request(Method::Get, "memories")?
            .query(&mem0_query_pairs(&ids));
        let value = send_mem0_json(response)?;
        let mut results = parse_memory_items(&value);
        results = filter_memory_items(results, &input.filters);
        let total_results = results.len();
        let end = offset.saturating_add(limit).min(total_results);
        let page = if offset < total_results {
            results[offset..end].to_vec()
        } else {
            Vec::new()
        };
        let next_offset = if end < total_results { Some(end) } else { None };
        Ok(MemoryListOutput {
            results: page,
            truncated: next_offset.is_some(),
            offset,
            limit,
            total_results,
            next_offset,
        })
    }

    pub(super) fn update_semantic_memory(
        &self,
        input: &MemoryUpdateInput,
    ) -> Result<MemoryUpdateOutput, ExecutionError> {
        self.ensure_mem0_enabled()?;
        let memory_id = input.memory_id.trim();
        if memory_id.is_empty() {
            return Err(invalid_mem0_tool(
                "memory_update memory_id must not be empty",
            ));
        }
        if input.text.trim().is_empty() {
            return Err(invalid_mem0_tool("memory_update text must not be empty"));
        }
        let mut body = json!({
            "text": input.text,
        });
        if !input.metadata.is_null() {
            body["metadata"] = input.metadata.clone();
        }
        let response = self
            .mem0_request(Method::Put, &format!("memories/{memory_id}"))?
            .json(&body);
        let value = send_mem0_json(response)?;
        Ok(MemoryUpdateOutput {
            memory_id: memory_id.to_string(),
            updated: true,
            memory: parse_memory_items(&value).into_iter().next(),
        })
    }

    pub(super) fn delete_semantic_memory(
        &self,
        input: &MemoryDeleteInput,
    ) -> Result<MemoryDeleteOutput, ExecutionError> {
        self.ensure_mem0_enabled()?;
        let memory_id = input.memory_id.trim();
        if memory_id.is_empty() {
            return Err(invalid_mem0_tool(
                "memory_delete memory_id must not be empty",
            ));
        }
        let response = self.mem0_request(Method::Delete, &format!("memories/{memory_id}"))?;
        let value = send_mem0_json(response)?;
        Ok(MemoryDeleteOutput {
            memory_id: memory_id.to_string(),
            deleted: value
                .get("deleted")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| response_status(&value, "deleted") == "deleted"),
        })
    }

    fn ensure_mem0_enabled(&self) -> Result<(), ExecutionError> {
        if !self.config.mem0.enabled {
            return Err(invalid_mem0_tool(
                "semantic memory is disabled; set mem0.enabled=true or TEAMD_MEM0_ENABLED=true",
            ));
        }
        Ok(())
    }

    fn mem0_client(&self) -> Result<Client, ExecutionError> {
        Client::builder()
            .timeout(Duration::from_millis(self.config.mem0.request_timeout_ms))
            .build()
            .map_err(|error| {
                invalid_mem0_tool(format!("failed to build Mem0 HTTP client: {error}"))
            })
    }

    fn mem0_request(&self, method: Method, path: &str) -> Result<RequestBuilder, ExecutionError> {
        let client = self.mem0_client()?;
        let base = self.config.mem0.api_base.trim().trim_end_matches('/');
        let path = path.trim_start_matches('/');
        let mut request = match method {
            Method::Get => client.get(format!("{base}/{path}")),
            Method::Post => client.post(format!("{base}/{path}")),
            Method::Put => client.put(format!("{base}/{path}")),
            Method::Delete => client.delete(format!("{base}/{path}")),
        };
        if let Some(api_key) = self
            .config
            .mem0
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            request = request.header("X-API-Key", api_key);
        }
        Ok(request)
    }

    fn mem0_scope_ids(
        &self,
        session: &Session,
        raw_scope: Option<&str>,
    ) -> Result<Mem0ScopeIds, ExecutionError> {
        let scope = RuntimeScope::parse(raw_scope, "memory")?;
        let user_id = match scope {
            RuntimeScope::Operator | RuntimeScope::Workspace => {
                Some(self.config.mem0.default_user_id.trim().to_string())
            }
            RuntimeScope::Agent | RuntimeScope::AgentShared | RuntimeScope::Session => None,
        };
        let agent_id = match scope {
            RuntimeScope::Agent => Some(session.agent_profile_id.clone()),
            RuntimeScope::AgentShared => Some(AGENT_SHARED_SCOPE_ID.to_string()),
            RuntimeScope::Operator | RuntimeScope::Workspace | RuntimeScope::Session => None,
        };
        let app_id = match scope {
            RuntimeScope::Workspace => Some(workspace_scope_id(session)),
            RuntimeScope::Operator
            | RuntimeScope::Agent
            | RuntimeScope::AgentShared
            | RuntimeScope::Session => None,
        };
        let run_id = match scope {
            RuntimeScope::Session => Some(session.id.clone()),
            RuntimeScope::Operator
            | RuntimeScope::Agent
            | RuntimeScope::AgentShared
            | RuntimeScope::Workspace => None,
        };
        Ok(Mem0ScopeIds {
            scope,
            user_id,
            agent_id,
            app_id,
            run_id,
        })
    }

    fn mem0_messages(&self, input: &MemoryAddInput) -> Result<Vec<Value>, ExecutionError> {
        if !input.messages.is_empty() {
            let messages = input
                .messages
                .iter()
                .map(memory_message_json)
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(messages);
        }
        let text = input.text.trim();
        if text.is_empty() {
            return Err(invalid_mem0_tool(
                "memory_add requires either text or messages",
            ));
        }
        Ok(vec![json!({"role": "user", "content": text})])
    }

    fn mem0_limit(&self, requested: Option<usize>) -> usize {
        requested
            .unwrap_or(self.config.mem0.default_limit)
            .clamp(1, self.config.mem0.max_limit)
    }
}

#[derive(Debug, Clone, Copy)]
enum Method {
    Get,
    Post,
    Put,
    Delete,
}

fn invalid_mem0_tool(reason: impl Into<String>) -> ExecutionError {
    ExecutionError::Tool(ToolError::InvalidMemoryTool {
        reason: reason.into(),
    })
}

fn memory_message_json(input: &MemoryMessageInput) -> Result<Value, ExecutionError> {
    let role = input.role.trim();
    let content = input.content.trim();
    match role {
        "system" | "user" | "assistant" | "tool" => {}
        _ => {
            return Err(invalid_mem0_tool(format!(
                "unsupported memory message role {role}; use user or assistant for normal memories"
            )));
        }
    }
    if content.is_empty() {
        return Err(invalid_mem0_tool(
            "memory_add messages[].content must not be empty",
        ));
    }
    Ok(json!({"role": role, "content": content}))
}

fn memory_metadata(value: &Value) -> Result<Map<String, Value>, ExecutionError> {
    match value {
        Value::Null => Ok(Map::new()),
        Value::Object(map) => Ok(map.clone()),
        other => Err(invalid_mem0_tool(format!(
            "metadata/filters must be a JSON object, got {other}"
        ))),
    }
}

fn insert_metadata(map: &mut Map<String, Value>, key: &str, value: impl Into<Value>) {
    map.insert(key.to_string(), value.into());
}

fn insert_metadata_if_missing(map: &mut Map<String, Value>, key: &str, value: impl Into<Value>) {
    map.entry(key.to_string()).or_insert_with(|| value.into());
}

fn insert_optional_string(body: &mut Value, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        body[key] = Value::String(value.to_string());
    }
}

fn mem0_query_pairs(ids: &Mem0ScopeIds) -> Vec<(&'static str, String)> {
    let mut pairs = Vec::new();
    if let Some(user_id) = &ids.user_id {
        pairs.push(("user_id", user_id.clone()));
    }
    if let Some(agent_id) = &ids.agent_id {
        pairs.push(("agent_id", agent_id.clone()));
    }
    if let Some(app_id) = &ids.app_id {
        pairs.push(("app_id", app_id.clone()));
    }
    if let Some(run_id) = &ids.run_id {
        pairs.push(("run_id", run_id.clone()));
    }
    pairs
}

fn mem0_search_body(
    query: &str,
    ids: &Mem0ScopeIds,
    limit: usize,
    input_filters: &Value,
) -> Result<Value, ExecutionError> {
    Ok(json!({
        "query": query,
        "filters": mem0_filters(input_filters, ids)?,
        "top_k": limit,
    }))
}

fn mem0_filters(input_filters: &Value, ids: &Mem0ScopeIds) -> Result<Value, ExecutionError> {
    let entity_filter = mem0_entity_filter(ids);
    let input = memory_metadata(input_filters)?;
    if input.is_empty() {
        return Ok(Value::Object(entity_filter));
    }
    if input
        .keys()
        .any(|key| matches!(key.as_str(), "AND" | "OR" | "NOT"))
    {
        return Ok(json!({
            "AND": [
                Value::Object(entity_filter),
                Value::Object(input)
            ]
        }));
    }
    let mut filters = input;
    for (key, value) in entity_filter {
        filters.insert(key, value);
    }
    Ok(Value::Object(filters))
}

fn mem0_entity_filter(ids: &Mem0ScopeIds) -> Map<String, Value> {
    let mut filter = Map::new();
    if let Some(user_id) = &ids.user_id {
        filter.insert("user_id".to_string(), Value::String(user_id.clone()));
    }
    if let Some(agent_id) = &ids.agent_id {
        filter.insert("agent_id".to_string(), Value::String(agent_id.clone()));
    }
    if let Some(app_id) = &ids.app_id {
        filter.insert("app_id".to_string(), Value::String(app_id.clone()));
    }
    if let Some(run_id) = &ids.run_id {
        filter.insert("run_id".to_string(), Value::String(run_id.clone()));
    }
    filter
}

fn send_mem0_json(request: RequestBuilder) -> Result<Value, ExecutionError> {
    let response = request
        .send()
        .map_err(|error| invalid_mem0_tool(format!("Mem0 request failed: {error}")))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| invalid_mem0_tool(format!("failed to read Mem0 response: {error}")))?;
    if !status.is_success() {
        return Err(invalid_mem0_tool(format!(
            "Mem0 returned HTTP {}: {}",
            status.as_u16(),
            truncate_for_error(&text)
        )));
    }
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text)
        .map_err(|error| invalid_mem0_tool(format!("failed to parse Mem0 JSON response: {error}")))
}

fn response_status(value: &Value, fallback: &str) -> String {
    value
        .get("status")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

fn parse_memory_items(value: &Value) -> Vec<MemoryItemOutput> {
    candidate_memory_values(value)
        .into_iter()
        .filter_map(memory_item_from_value)
        .collect()
}

fn candidate_memory_values(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.clone(),
        Value::Object(map) => ["results", "memories", "data"]
            .iter()
            .find_map(|key| match map.get(*key) {
                Some(Value::Array(items)) => Some(items.clone()),
                Some(Value::Object(_)) => map.get(*key).cloned().map(|item| vec![item]),
                _ => None,
            })
            .or_else(|| map.get("memory").cloned().map(|item| vec![item]))
            .unwrap_or_else(|| vec![value.clone()]),
        _ => Vec::new(),
    }
}

fn memory_item_from_value(value: Value) -> Option<MemoryItemOutput> {
    let map = value.as_object()?;
    let nested = map.get("data").and_then(Value::as_object);
    let id = string_field(map, nested, &["id", "memory_id", "uuid"])?;
    let memory = string_field(map, nested, &["memory", "text", "content", "data"])
        .unwrap_or_else(|| value.to_string());
    let score = map
        .get("score")
        .or_else(|| map.get("similarity"))
        .or_else(|| nested.and_then(|data| data.get("score")))
        .map(value_to_score_string);
    let metadata = map
        .get("metadata")
        .or_else(|| nested.and_then(|data| data.get("metadata")))
        .cloned()
        .unwrap_or(Value::Null);
    Some(MemoryItemOutput {
        id,
        memory,
        score,
        metadata,
        user_id: string_field(map, nested, &["user_id"]),
        agent_id: string_field(map, nested, &["agent_id"]),
        app_id: string_field(map, nested, &["app_id"]),
        run_id: string_field(map, nested, &["run_id"]),
    })
}

fn string_field(
    map: &Map<String, Value>,
    nested: Option<&Map<String, Value>>,
    names: &[&str],
) -> Option<String> {
    for name in names {
        if let Some(value) = map
            .get(*name)
            .or_else(|| nested.and_then(|data| data.get(*name)))
        {
            if let Some(text) = value.as_str().filter(|text| !text.trim().is_empty()) {
                return Some(text.to_string());
            }
            if value.is_number() || value.is_boolean() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn value_to_score_string(value: &Value) -> String {
    value
        .as_f64()
        .map(|score| format!("{score:.4}"))
        .unwrap_or_else(|| value.to_string())
}

fn filter_memory_items(items: Vec<MemoryItemOutput>, filters: &Value) -> Vec<MemoryItemOutput> {
    let Ok(filters) = memory_metadata(filters) else {
        return items;
    };
    if filters.is_empty() {
        return items;
    }
    items
        .into_iter()
        .filter(|item| {
            let Value::Object(metadata) = &item.metadata else {
                return false;
            };
            filters
                .iter()
                .all(|(key, value)| metadata.get(key) == Some(value))
        })
        .collect()
}

fn truncate_for_error(text: &str) -> String {
    const LIMIT: usize = 512;
    if text.len() <= LIMIT {
        return text.to_string();
    }
    let mut end = LIMIT;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mem0_result_shapes() {
        let value = json!({
            "results": [
                {
                    "id": "mem_1",
                    "memory": "User prefers Telegram.",
                    "score": 0.87,
                    "metadata": {"topic": "surface"},
                    "user_id": "operator",
                    "app_id": "teamd-workspace-abc"
                }
            ]
        });

        let results = parse_memory_items(&value);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "mem_1");
        assert_eq!(results[0].memory, "User prefers Telegram.");
        assert_eq!(results[0].score.as_deref(), Some("0.8700"));
        assert_eq!(results[0].user_id.as_deref(), Some("operator"));
        assert_eq!(results[0].app_id.as_deref(), Some("teamd-workspace-abc"));
    }

    #[test]
    fn scope_controls_mem0_entity_ids() {
        let service = ExecutionService {
            config: ExecutionServiceConfig {
                mem0: agent_persistence::Mem0Config {
                    enabled: true,
                    default_user_id: "anton".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..ExecutionService::default()
        };
        let session = Session {
            id: "session-1".to_string(),
            agent_profile_id: "default".to_string(),
            workspace_root: "/srv/teamd/workspaces/default".into(),
            ..Default::default()
        };

        let operator = service
            .mem0_scope_ids(&session, Some("operator"))
            .expect("operator ids");
        let agent = service
            .mem0_scope_ids(&session, Some("agent"))
            .expect("agent ids");
        let shared = service
            .mem0_scope_ids(&session, Some("agent_shared"))
            .expect("shared ids");
        let workspace = service
            .mem0_scope_ids(&session, Some("workspace"))
            .expect("workspace ids");
        let session_scope = service
            .mem0_scope_ids(&session, Some("session"))
            .expect("session ids");

        assert_eq!(operator.user_id.as_deref(), Some("anton"));
        assert_eq!(operator.agent_id, None);
        assert_eq!(operator.app_id, None);
        assert_eq!(operator.run_id, None);
        assert_eq!(agent.user_id, None);
        assert_eq!(agent.agent_id.as_deref(), Some("default"));
        assert_eq!(agent.app_id, None);
        assert_eq!(agent.run_id, None);
        assert_eq!(shared.user_id, None);
        assert_eq!(shared.agent_id.as_deref(), Some("teamd-agent-shared"));
        assert_eq!(shared.app_id, None);
        assert_eq!(workspace.user_id.as_deref(), Some("anton"));
        assert_eq!(workspace.agent_id, None);
        assert_eq!(
            workspace.app_id.as_deref(),
            Some("teamd-workspace-06851bfb8133809d")
        );
        assert_eq!(workspace.run_id, None);
        assert_eq!(session_scope.user_id, None);
        assert_eq!(session_scope.agent_id, None);
        assert_eq!(session_scope.app_id, None);
        assert_eq!(session_scope.run_id.as_deref(), Some("session-1"));
    }

    #[test]
    fn search_body_uses_mem0_filters_and_top_k() {
        let ids = Mem0ScopeIds {
            scope: RuntimeScope::Workspace,
            user_id: Some("anton".to_string()),
            agent_id: None,
            app_id: Some("teamd-workspace-abc".to_string()),
            run_id: None,
        };

        let body = mem0_search_body(
            "preferred editor",
            &ids,
            4,
            &json!({"metadata": {"teamd_source": "memory_curator"}}),
        )
        .expect("search body");

        assert_eq!(body["query"], json!("preferred editor"));
        assert_eq!(body["top_k"], json!(4));
        assert!(body.get("limit").is_none());
        assert!(body.get("user_id").is_none());
        assert_eq!(
            body["filters"],
            json!({
                "metadata": {"teamd_source": "memory_curator"},
                "user_id": "anton",
                "app_id": "teamd-workspace-abc"
            })
        );
    }
}
