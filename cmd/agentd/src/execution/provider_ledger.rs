use super::{ExecutionError, ToolExecutionStatus};
use crate::store_retry::{retry_store_sync, sqlite_lock_retry_attempts, sqlite_lock_retry_delay};
use crate::trace::RuntimeTraceContext;
use agent_persistence::{
    ArtifactRecord, ArtifactRepository, PersistenceStore, ToolCallRecord, ToolCallRepository,
    TraceRepository,
};
use agent_runtime::provider::{ProviderRequest, ProviderStreamMode};
use std::path::PathBuf;

use super::provider_ids::sanitize_identifier;

pub(super) struct ToolCallLedgerUpdate<'a> {
    pub(super) store: &'a PersistenceStore,
    pub(super) session_id: &'a str,
    pub(super) run_id: &'a str,
    pub(super) provider_tool_call_id: &'a str,
    pub(super) tool_name: &'a str,
    pub(super) arguments_json: &'a str,
    pub(super) summary: &'a str,
    pub(super) status: ToolExecutionStatus,
    pub(super) error: Option<String>,
    pub(super) now: i64,
}

pub(super) struct ToolCallResultLedgerUpdate<'a> {
    pub(super) store: &'a PersistenceStore,
    pub(super) session_id: &'a str,
    pub(super) run_id: &'a str,
    pub(super) provider_tool_call_id: &'a str,
    pub(super) tool_name: &'a str,
    pub(super) result_summary: &'a str,
    pub(super) result_output: &'a str,
    pub(super) result_preview_char_limit: usize,
    pub(super) now: i64,
}

fn tool_call_ledger_id(run_id: &str, provider_tool_call_id: &str) -> String {
    format!(
        "toolcall-{}-{}",
        sanitize_identifier(run_id),
        sanitize_identifier(provider_tool_call_id)
    )
}

fn provider_round_trace_id(run_id: &str, round: usize) -> String {
    format!("provider-round-{}-r{round}", sanitize_identifier(run_id))
}

pub(super) fn record_provider_round_trace(
    store: &PersistenceStore,
    run_id: &str,
    session_id: &str,
    round: usize,
    max_rounds: usize,
    request: &ProviderRequest,
    now: i64,
) -> Result<(), ExecutionError> {
    let trace_context = RuntimeTraceContext::from_run_link_or_default(store, run_id)
        .map_err(ExecutionError::Store)?;
    let entity_id = provider_round_trace_id(run_id, round);
    store
        .put_trace_link(&trace_context.child_link(
            "provider_round",
            &entity_id,
            serde_json::json!({
                "session_id": session_id,
                "run_id": run_id,
                "round": round,
                "max_rounds": max_rounds,
                "model": request.model.as_deref(),
                "stream": matches!(request.stream, ProviderStreamMode::Enabled),
                "message_count": request.messages.len(),
                "tool_definition_count": request.tools.len(),
                "tool_output_count": request.tool_outputs.len(),
                "continuation_message_count": request.continuation_messages.len(),
            }),
            now,
        ))
        .map_err(ExecutionError::Store)
}

pub(super) fn record_tool_call_ledger(
    update: ToolCallLedgerUpdate<'_>,
) -> Result<(), ExecutionError> {
    retry_store_sync(
        sqlite_lock_retry_attempts(),
        sqlite_lock_retry_delay(),
        || {
            let id = tool_call_ledger_id(update.run_id, update.provider_tool_call_id);
            let existing = update.store.get_tool_call(&id)?;
            let requested_at = existing
                .as_ref()
                .map(|record| record.requested_at)
                .unwrap_or(update.now);
            let trace_context =
                RuntimeTraceContext::from_run_link_or_default(update.store, update.run_id)?;
            update.store.put_tool_call(&ToolCallRecord {
                id: id.clone(),
                session_id: update.session_id.to_string(),
                run_id: update.run_id.to_string(),
                provider_tool_call_id: update.provider_tool_call_id.to_string(),
                tool_name: update.tool_name.to_string(),
                arguments_json: update.arguments_json.to_string(),
                summary: update.summary.to_string(),
                status: update.status.as_str().to_string(),
                error: update.error.clone(),
                result_summary: existing
                    .as_ref()
                    .and_then(|record| record.result_summary.clone()),
                result_preview: existing
                    .as_ref()
                    .and_then(|record| record.result_preview.clone()),
                result_artifact_id: existing
                    .as_ref()
                    .and_then(|record| record.result_artifact_id.clone()),
                result_truncated: existing
                    .as_ref()
                    .is_some_and(|record| record.result_truncated),
                result_byte_len: existing.as_ref().and_then(|record| record.result_byte_len),
                requested_at,
                updated_at: update.now,
            })?;
            update.store.put_trace_link(&trace_context.child_link(
                "tool_call",
                &id,
                serde_json::json!({
                    "session_id": update.session_id,
                    "run_id": update.run_id,
                    "provider_tool_call_id": update.provider_tool_call_id,
                    "tool_name": update.tool_name,
                    "status": update.status.as_str(),
                    "summary": update.summary,
                }),
                requested_at,
            ))
        },
    )
    .map_err(ExecutionError::Store)
}

pub(super) fn record_tool_call_result(
    update: ToolCallResultLedgerUpdate<'_>,
) -> Result<(), ExecutionError> {
    retry_store_sync(
        sqlite_lock_retry_attempts(),
        sqlite_lock_retry_delay(),
        || {
            let id = tool_call_ledger_id(update.run_id, update.provider_tool_call_id);
            let Some(mut record) = update.store.get_tool_call(&id)? else {
                return Ok(());
            };
            let trace_context =
                RuntimeTraceContext::from_run_link_or_default(update.store, update.run_id)?;

            let (result_preview, result_truncated) =
                tool_result_preview(update.result_output, update.result_preview_char_limit);
            let result_artifact_id = if result_truncated {
                let artifact_id =
                    tool_result_artifact_id(update.run_id, update.provider_tool_call_id);
                update.store.put_artifact(&ArtifactRecord {
                    id: artifact_id.clone(),
                    session_id: update.session_id.to_string(),
                    kind: "tool_output".to_string(),
                    metadata_json: serde_json::json!({
                        "tool_call_id": id,
                        "run_id": update.run_id,
                        "provider_tool_call_id": update.provider_tool_call_id,
                        "tool_name": update.tool_name,
                        "summary": update.result_summary,
                        "created_at": update.now,
                    })
                    .to_string(),
                    path: PathBuf::from("artifacts").join(format!("{artifact_id}.bin")),
                    bytes: update.result_output.as_bytes().to_vec(),
                    created_at: update.now,
                })?;
                update.store.put_trace_link(&trace_context.child_link(
                    "artifact",
                    &artifact_id,
                    serde_json::json!({
                        "session_id": update.session_id,
                        "run_id": update.run_id,
                        "tool_call_id": id,
                        "kind": "tool_output",
                        "byte_len": update.result_output.len(),
                    }),
                    update.now,
                ))?;
                Some(artifact_id)
            } else {
                None
            };

            record.result_summary = Some(update.result_summary.to_string());
            record.result_preview = Some(result_preview);
            record.result_artifact_id = result_artifact_id;
            record.result_truncated = result_truncated;
            record.result_byte_len = Some(update.result_output.len() as i64);
            record.updated_at = update.now;
            update.store.put_tool_call(&record)
        },
    )
    .map_err(ExecutionError::Store)
}

fn tool_result_preview(result_output: &str, char_limit: usize) -> (String, bool) {
    let mut chars = result_output.chars();
    let preview = chars.by_ref().take(char_limit).collect::<String>();
    let truncated = chars.next().is_some();
    if truncated {
        (
            format!("{preview}\n... <truncated; use session tool-result with this tool_call_id>"),
            true,
        )
    } else {
        (preview, false)
    }
}

fn tool_result_artifact_id(run_id: &str, provider_tool_call_id: &str) -> String {
    format!(
        "artifact-tool-result-{}-{}",
        sanitize_identifier(run_id),
        sanitize_identifier(provider_tool_call_id)
    )
}
