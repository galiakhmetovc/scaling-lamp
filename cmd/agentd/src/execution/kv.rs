use super::scopes::{kv_namespace_id, kv_namespace_id_for_context};
use super::*;
use agent_persistence::{KvEntryRecord, KvRepository};
use agent_runtime::tool::{
    KvDeleteInput, KvDeleteOutput, KvEntryOutput, KvGetInput, KvGetOutput, KvListInput,
    KvListOutput, KvPutInput, KvPutOutput,
};
use serde_json::Value;

impl ExecutionService {
    pub(super) fn get_kv_entry(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &KvGetInput,
        now: i64,
    ) -> Result<KvGetOutput, ExecutionError> {
        let session = self.load_session(store, session_id)?;
        let key =
            normalized_kv_key_with_limit(&input.key, self.config.runtime_limits.kv_key_max_bytes)?;
        let (scope, namespace_id) = kv_namespace_id(
            &session,
            self.config.mem0.default_user_id.as_str(),
            input.scope.as_deref(),
        )?;
        let entry = store
            .get_kv_entry(scope.as_str(), namespace_id.as_str(), key.as_str(), now)
            .map_err(map_kv_store_error)?
            .map(kv_entry_output)
            .transpose()?;
        Ok(KvGetOutput {
            key,
            found: entry.is_some(),
            entry,
        })
    }

    pub(super) fn put_kv_entry(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &KvPutInput,
        now: i64,
    ) -> Result<KvPutOutput, ExecutionError> {
        self.put_kv_entry_context(store, Some(session_id), input, now)
    }

    pub(super) fn put_kv_entry_context(
        &self,
        store: &PersistenceStore,
        session_id: Option<&str>,
        input: &KvPutInput,
        now: i64,
    ) -> Result<KvPutOutput, ExecutionError> {
        let session = session_id
            .map(|session_id| self.load_session(store, session_id))
            .transpose()?;
        let key =
            normalized_kv_key_with_limit(&input.key, self.config.runtime_limits.kv_key_max_bytes)?;
        let (scope, namespace_id) = kv_namespace_id_for_context(
            session.as_ref(),
            self.config.mem0.default_user_id.as_str(),
            input.scope.as_deref(),
        )?;
        let value_json = bounded_json_string(
            "kv_put value",
            &input.value,
            self.config.runtime_limits.kv_value_max_bytes,
        )?;
        validate_kv_metadata(&input.metadata)?;
        let metadata_json = bounded_json_string(
            "kv_put metadata",
            &input.metadata,
            self.config.runtime_limits.kv_metadata_max_bytes,
        )?;
        let expires_at = match input.ttl_seconds {
            Some(ttl_seconds) if ttl_seconds <= 0 => {
                return Err(invalid_kv_tool(
                    "kv_put ttl_seconds must be greater than zero",
                ));
            }
            Some(ttl_seconds) => Some(now.saturating_add(ttl_seconds)),
            None => None,
        };
        let stored = store
            .put_kv_entry(
                &KvEntryRecord {
                    scope: scope.as_str().to_string(),
                    namespace_id,
                    key,
                    value_json,
                    metadata_json,
                    revision: 0,
                    created_at: now,
                    updated_at: now,
                    expires_at,
                },
                input.expected_revision,
            )
            .map_err(map_kv_store_error)?;
        Ok(KvPutOutput {
            entry: kv_entry_output(stored)?,
        })
    }

    pub(super) fn list_kv_entries(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &KvListInput,
        now: i64,
    ) -> Result<KvListOutput, ExecutionError> {
        self.list_kv_entries_context(store, Some(session_id), input, now)
    }

    pub(super) fn list_kv_entries_context(
        &self,
        store: &PersistenceStore,
        session_id: Option<&str>,
        input: &KvListInput,
        now: i64,
    ) -> Result<KvListOutput, ExecutionError> {
        let session = session_id
            .map(|session_id| self.load_session(store, session_id))
            .transpose()?;
        let (scope, namespace_id) = kv_namespace_id_for_context(
            session.as_ref(),
            self.config.mem0.default_user_id.as_str(),
            input.scope.as_deref(),
        )?;
        let prefix = input
            .prefix
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(prefix) = prefix {
            validate_kv_key_length(
                prefix,
                "kv_list prefix",
                self.config.runtime_limits.kv_key_max_bytes,
            )?;
        }
        let offset = input.offset.unwrap_or(0);
        let limit = input
            .limit
            .unwrap_or(self.config.runtime_limits.kv_list_default_limit)
            .clamp(1, self.config.runtime_limits.kv_list_max_limit);
        let mut records = store
            .list_kv_entries(
                scope.as_str(),
                namespace_id.as_str(),
                prefix,
                limit.saturating_add(1),
                offset,
                now,
            )
            .map_err(map_kv_store_error)?;
        let truncated = records.len() > limit;
        if truncated {
            records.truncate(limit);
        }
        let next_offset = if truncated {
            Some(offset.saturating_add(limit))
        } else {
            None
        };
        let results = records
            .into_iter()
            .map(kv_entry_output)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(KvListOutput {
            results,
            truncated,
            offset,
            limit,
            next_offset,
        })
    }

    pub(super) fn delete_kv_entry(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &KvDeleteInput,
    ) -> Result<KvDeleteOutput, ExecutionError> {
        self.delete_kv_entry_context(store, Some(session_id), input)
    }

    pub(super) fn delete_kv_entry_context(
        &self,
        store: &PersistenceStore,
        session_id: Option<&str>,
        input: &KvDeleteInput,
    ) -> Result<KvDeleteOutput, ExecutionError> {
        let session = session_id
            .map(|session_id| self.load_session(store, session_id))
            .transpose()?;
        let key =
            normalized_kv_key_with_limit(&input.key, self.config.runtime_limits.kv_key_max_bytes)?;
        let (scope, namespace_id) = kv_namespace_id_for_context(
            session.as_ref(),
            self.config.mem0.default_user_id.as_str(),
            input.scope.as_deref(),
        )?;
        let deleted = store
            .delete_kv_entry(
                scope.as_str(),
                namespace_id.as_str(),
                key.as_str(),
                input.expected_revision,
            )
            .map_err(map_kv_store_error)?;
        Ok(KvDeleteOutput { key, deleted })
    }
}

fn normalized_kv_key_with_limit(raw: &str, max_bytes: usize) -> Result<String, ExecutionError> {
    let key = raw.trim();
    if key.is_empty() {
        return Err(invalid_kv_tool("kv key must not be empty"));
    }
    validate_kv_key_length(key, "kv key", max_bytes)?;
    Ok(key.to_string())
}

fn validate_kv_key_length(
    value: &str,
    label: &str,
    max_bytes: usize,
) -> Result<(), ExecutionError> {
    if value.len() > max_bytes {
        return Err(invalid_kv_tool(format!(
            "{label} is too large: {} bytes > {}",
            value.len(),
            max_bytes
        )));
    }
    Ok(())
}

fn validate_kv_metadata(value: &Value) -> Result<(), ExecutionError> {
    match value {
        Value::Null | Value::Object(_) => Ok(()),
        other => Err(invalid_kv_tool(format!(
            "kv_put metadata must be a JSON object or null, got {other}"
        ))),
    }
}

fn bounded_json_string(
    label: &str,
    value: &Value,
    max_bytes: usize,
) -> Result<String, ExecutionError> {
    let serialized = serde_json::to_string(value).map_err(|error| {
        invalid_kv_tool(format!("failed to serialize {label} as JSON: {error}"))
    })?;
    if serialized.len() > max_bytes {
        return Err(invalid_kv_tool(format!(
            "{label} is too large: {} bytes > {max_bytes}",
            serialized.len()
        )));
    }
    Ok(serialized)
}

fn kv_entry_output(record: KvEntryRecord) -> Result<KvEntryOutput, ExecutionError> {
    let value = serde_json::from_str(&record.value_json).map_err(|error| {
        invalid_kv_tool(format!(
            "stored kv value for key {} is not valid JSON: {error}",
            record.key
        ))
    })?;
    let metadata = serde_json::from_str(&record.metadata_json).map_err(|error| {
        invalid_kv_tool(format!(
            "stored kv metadata for key {} is not valid JSON: {error}",
            record.key
        ))
    })?;
    Ok(KvEntryOutput {
        scope: record.scope,
        namespace_id: record.namespace_id,
        key: record.key,
        value,
        metadata,
        revision: record.revision,
        created_at: record.created_at,
        updated_at: record.updated_at,
        expires_at: record.expires_at,
    })
}

fn map_kv_store_error(error: StoreError) -> ExecutionError {
    match error {
        StoreError::KvRevisionConflict {
            scope,
            namespace_id,
            key,
            expected_revision,
            actual_revision,
        } => invalid_kv_tool(format!(
            "kv revision conflict for {scope}/{namespace_id}/{key}: expected revision {expected_revision}, actual revision {}",
            actual_revision
                .map(|revision| revision.to_string())
                .unwrap_or_else(|| "missing".to_string())
        )),
        other => ExecutionError::Store(other),
    }
}

fn invalid_kv_tool(reason: impl Into<String>) -> ExecutionError {
    ExecutionError::Tool(ToolError::InvalidMemoryTool {
        reason: reason.into(),
    })
}
