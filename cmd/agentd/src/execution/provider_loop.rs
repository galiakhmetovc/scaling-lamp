use super::*;
use crate::agents;
use crate::prompting;
use crate::store_retry::{
    SQLITE_LOCK_RETRY_ATTEMPTS, SQLITE_LOCK_RETRY_DELAY_MS, retry_store_sync,
};
use agent_persistence::{
    ArtifactRecord, ArtifactRepository, ContextOffloadRepository, ToolCallRecord,
    ToolCallRepository,
};
use agent_runtime::context::{
    CompactionPolicy, ContextOffloadPayload, ContextOffloadRef, ContextOffloadSnapshot,
    ContextSummary, approximate_token_count,
};
use agent_runtime::permission::PermissionAction;
use agent_runtime::plan::{PlanItem, PlanItemStatus, PlanSnapshot};
use agent_runtime::prompt::{
    AutonomyState, PromptAssembly, PromptAssemblyInput, RecentToolActivity,
    RecentToolActivityEntry, SessionHeadRuntime,
};
use agent_runtime::provider::{
    ProviderContinuationMessage, ProviderError, ProviderMessage, ProviderRequest, ProviderResponse,
    ProviderStreamEvent, ProviderStreamMode, ProviderToolCall, ProviderToolDefinition,
    ProviderToolOutput,
};
use agent_runtime::run::{
    ApprovalRequest, PendingLoopResetApproval, PendingProviderApproval, PendingToolApproval,
    ProviderLoopState, RunStepKind,
};
use agent_runtime::session::{MessageRole, TranscriptEntry};
use agent_runtime::skills::{resolve_session_skill_status, scan_skill_catalog_with_overrides};
use agent_runtime::tool::{
    AddTaskNoteOutput, AddTaskOutput, ArtifactReadOutput, ArtifactSearchOutput,
    ArtifactSearchResult, EditTaskOutput, InitPlanOutput, PlanLintOutput, PlanReadOutput,
    PlanSnapshotOutput, PlanWriteOutput, PromptBudgetLayerOutput, PromptBudgetReadOutput,
    PromptBudgetUpdateOutput, SetTaskStatusOutput, ToolCatalog, ToolDefinition, ToolFamily,
    ToolName, ToolOutput, ToolPolicy, ToolRuntime,
};
use agent_runtime::workspace::WorkspaceRef;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

const MAX_CONTEXT_OFFLOAD_REFS: usize = 16;
const INLINE_TOOL_OUTPUT_TOKEN_LIMIT: u32 = 512;
const INLINE_FIND_IN_FILES_PREVIEW_LIMIT: usize = 6;
const TOOL_RESULT_PREVIEW_CHAR_LIMIT: usize = 16 * 1024;
const MAX_CONSECUTIVE_IDENTICAL_TOOL_SIGNATURES: usize = 3;
const MAX_TRANSIENT_PROVIDER_RETRIES: usize = 3;
const MAX_EMPTY_RESPONSE_RECOVERIES: usize = 1;

type OffloadableToolOutput = (String, String, Vec<u8>, String);

#[derive(Debug, Clone)]
pub(super) struct CompletionGateDecision {
    pub(super) max_completion_nudges: usize,
    pub(super) nudge_message: String,
}

#[derive(Clone, Copy)]
pub(super) struct ProviderToolExecutionContext<'a> {
    pub(super) store: &'a PersistenceStore,
    pub(super) provider: &'a dyn ProviderDriver,
    pub(super) session_id: &'a str,
    pub(super) run_id: &'a str,
    pub(super) now: i64,
}

pub(super) struct ProviderToolCallInvocation<'a> {
    pub(super) tool_call_id: &'a str,
    pub(super) arguments_json: &'a str,
    pub(super) parsed: &'a ToolCall,
}

struct ToolCallLedgerUpdate<'a> {
    store: &'a PersistenceStore,
    session_id: &'a str,
    run_id: &'a str,
    provider_tool_call_id: &'a str,
    tool_name: &'a str,
    arguments_json: &'a str,
    summary: &'a str,
    status: ToolExecutionStatus,
    error: Option<String>,
    now: i64,
}

struct ToolCallResultLedgerUpdate<'a> {
    store: &'a PersistenceStore,
    session_id: &'a str,
    run_id: &'a str,
    provider_tool_call_id: &'a str,
    tool_name: &'a str,
    result_summary: &'a str,
    result_output: &'a str,
    now: i64,
}

#[derive(Debug, Clone)]
struct PromptMessages {
    messages: Vec<ProviderMessage>,
    context_offload: Option<ContextOffloadSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AutoCompactionDecision {
    estimated_prompt_tokens: u32,
    trigger_threshold_tokens: u32,
    context_window_tokens: u32,
}

fn recent_tool_activity_entry(record: ToolCallRecord) -> RecentToolActivityEntry {
    let summary = record
        .result_summary
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(record.summary);
    RecentToolActivityEntry {
        status: record.status,
        tool_name: record.tool_name,
        summary,
        artifact_id: record.result_artifact_id,
        error: record.error,
    }
}

#[derive(Debug, Clone)]
struct ObservedProviderResponse {
    response: ProviderResponse,
    reasoning_deltas: Vec<String>,
    text_deltas: Vec<String>,
}

#[derive(Debug, Clone)]
struct ProviderLoopCursor {
    max_rounds: usize,
    round: usize,
    pending_tool_outputs: Vec<ProviderToolOutput>,
    continuation_messages: Vec<ProviderContinuationMessage>,
    continuation_input_messages: Vec<ProviderMessage>,
    previous_response_id: Option<String>,
    seen_tool_signatures: Vec<String>,
    completion_nudges_used: usize,
    empty_response_recoveries_used: usize,
    supports_previous_response_id: bool,
    supports_streaming: bool,
}

impl ProviderLoopCursor {
    fn permits_repeated_tool_signature(response: &ProviderResponse) -> bool {
        !response.tool_calls.is_empty()
            && response.tool_calls.iter().all(|tool_call| {
                matches!(
                    tool_call.name.as_str(),
                    name if name == ToolName::ExecReadOutput.as_str()
                        || name == ToolName::SessionWait.as_str()
                )
            })
    }

    fn new(
        provider: &dyn ProviderDriver,
        initial_loop_state: Option<ProviderLoopState>,
        max_rounds: usize,
    ) -> Self {
        let supports_previous_response_id = provider
            .descriptor()
            .capabilities
            .supports_previous_response_id;
        let supports_streaming = provider.descriptor().capabilities.supports_streaming;
        let round = initial_loop_state
            .as_ref()
            .map(|state| state.next_round)
            .unwrap_or(0);
        let pending_tool_outputs = initial_loop_state
            .as_ref()
            .map(|state| state.pending_tool_outputs.clone())
            .unwrap_or_default();
        let continuation_messages = initial_loop_state
            .as_ref()
            .map(|state| state.continuation_messages.clone())
            .unwrap_or_default();
        let continuation_input_messages = initial_loop_state
            .as_ref()
            .map(|state| state.continuation_input_messages.clone())
            .unwrap_or_default();
        let previous_response_id = initial_loop_state
            .as_ref()
            .and_then(|state| state.previous_response_id.clone());
        let seen_tool_signatures = initial_loop_state
            .as_ref()
            .map(|state| state.seen_tool_signatures.clone())
            .unwrap_or_default();
        let completion_nudges_used = initial_loop_state
            .as_ref()
            .map(|state| state.completion_nudges_used)
            .unwrap_or_default();
        let empty_response_recoveries_used = initial_loop_state
            .as_ref()
            .map(|state| state.empty_response_recoveries_used)
            .unwrap_or_default();

        Self {
            max_rounds,
            round,
            pending_tool_outputs,
            continuation_messages,
            continuation_input_messages,
            previous_response_id,
            seen_tool_signatures,
            completion_nudges_used,
            empty_response_recoveries_used,
            supports_previous_response_id,
            supports_streaming,
        }
    }

    fn has_round_budget(&self) -> bool {
        self.round < self.max_rounds
    }

    fn reset_round_budget(&mut self) {
        self.round = 0;
    }

    fn stream_mode(&self, has_observer: bool) -> ProviderStreamMode {
        if has_observer && self.supports_streaming {
            ProviderStreamMode::Enabled
        } else {
            ProviderStreamMode::Disabled
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_request(
        &self,
        base_messages: &[ProviderMessage],
        model: Option<&str>,
        instructions: Option<&str>,
        think_level: Option<&str>,
        tools: &[ProviderToolDefinition],
        stream: ProviderStreamMode,
        max_output_tokens: Option<u32>,
    ) -> ProviderRequest {
        let messages = if self.supports_previous_response_id && self.previous_response_id.is_some()
        {
            self.continuation_input_messages.clone()
        } else {
            let mut messages = base_messages.to_vec();
            messages.extend(self.continuation_input_messages.clone());
            messages
        };
        ProviderRequest {
            model: model.map(str::to_string),
            instructions: instructions.map(str::to_string),
            messages,
            think_level: think_level.map(str::to_string),
            previous_response_id: if self.supports_previous_response_id {
                self.previous_response_id.clone()
            } else {
                None
            },
            continuation_messages: self.continuation_messages.clone(),
            tools: tools.to_vec(),
            tool_outputs: if self.supports_previous_response_id {
                self.pending_tool_outputs.clone()
            } else {
                Vec::new()
            },
            max_output_tokens,
            stream,
        }
    }

    fn persistent_state(
        &self,
        pending_approval: Option<PendingProviderApproval>,
    ) -> ProviderLoopState {
        ProviderLoopState {
            next_round: self.round,
            previous_response_id: self.previous_response_id.clone(),
            continuation_messages: self.continuation_messages.clone(),
            pending_tool_outputs: self.pending_tool_outputs.clone(),
            continuation_input_messages: self.continuation_input_messages.clone(),
            seen_tool_signatures: self.seen_tool_signatures.clone(),
            completion_nudges_used: self.completion_nudges_used,
            empty_response_recoveries_used: self.empty_response_recoveries_used,
            pending_approval,
        }
    }

    fn remember_tool_signature(
        &mut self,
        response: &ProviderResponse,
    ) -> Result<(), ExecutionError> {
        let signature = response
            .tool_calls
            .iter()
            .map(|tool_call| format!("{}:{}", tool_call.name, tool_call.arguments))
            .collect::<Vec<_>>()
            .join("|");
        let mut consecutive_repeats = 1usize;
        for previous in self.seen_tool_signatures.iter().rev() {
            if previous == &signature {
                consecutive_repeats += 1;
            } else {
                break;
            }
        }
        if consecutive_repeats >= MAX_CONSECUTIVE_IDENTICAL_TOOL_SIGNATURES
            && !Self::permits_repeated_tool_signature(response)
        {
            return Err(ExecutionError::ProviderLoop {
                reason: format!(
                    "provider repeated tool-call signature {} times in a row: {}",
                    consecutive_repeats, signature
                ),
            });
        }
        self.seen_tool_signatures.push(signature);
        Ok(())
    }

    fn note_assistant_tool_calls(&mut self, response: &ProviderResponse) {
        if !self.supports_previous_response_id {
            self.continuation_messages
                .push(ProviderContinuationMessage::AssistantToolCalls {
                    tool_calls: response.tool_calls.clone(),
                });
        }
    }

    fn begin_tool_round(&mut self) {
        if self.supports_previous_response_id {
            self.pending_tool_outputs.clear();
        }
    }

    fn record_tool_output(&mut self, tool_call_id: &str, model_output: String) {
        if self.supports_previous_response_id {
            self.pending_tool_outputs.push(ProviderToolOutput {
                call_id: tool_call_id.to_string(),
                output: model_output,
            });
        } else {
            self.continuation_messages
                .push(ProviderContinuationMessage::ToolResult {
                    tool_call_id: tool_call_id.to_string(),
                    content: model_output,
                });
        }
    }

    fn pending_approval_state(
        &self,
        response: &ProviderResponse,
        tool_call: &ProviderToolCall,
        parsed: &ToolCall,
        approval_id: &str,
    ) -> ProviderLoopState {
        ProviderLoopState {
            next_round: self.round + 1,
            previous_response_id: self
                .supports_previous_response_id
                .then(|| response.response_id.clone()),
            continuation_messages: self.continuation_messages.clone(),
            pending_tool_outputs: self.pending_tool_outputs.clone(),
            continuation_input_messages: Vec::new(),
            seen_tool_signatures: self.seen_tool_signatures.clone(),
            completion_nudges_used: self.completion_nudges_used,
            empty_response_recoveries_used: self.empty_response_recoveries_used,
            pending_approval: Some(PendingProviderApproval::Tool(PendingToolApproval::new(
                approval_id.to_string(),
                tool_call.call_id.clone(),
                parsed.name().as_str().to_string(),
                tool_call.arguments.clone(),
            ))),
        }
    }

    fn loop_reset_approval_state(&self, approval_id: &str) -> ProviderLoopState {
        let mut state = self.persistent_state(Some(PendingProviderApproval::LoopReset(
            PendingLoopResetApproval::new(approval_id.to_string(), self.round, self.max_rounds),
        )));
        state.continuation_input_messages.clear();
        state
    }

    fn completion_approval_state(
        &self,
        approval_id: &str,
        max_completion_nudges: usize,
    ) -> ProviderLoopState {
        self.persistent_state(Some(PendingProviderApproval::CompletionNudge(
            agent_runtime::run::PendingCompletionApproval::new(
                approval_id.to_string(),
                self.completion_nudges_used,
                max_completion_nudges,
            ),
        )))
    }

    fn queue_continuation_input_messages(&mut self, messages: Vec<ProviderMessage>) {
        self.continuation_input_messages.clear();
        self.continuation_input_messages.extend(messages);
    }

    fn queue_post_tool_continuation_messages(&mut self, messages: Vec<ProviderMessage>) {
        self.continuation_input_messages.clear();
        if self.supports_previous_response_id {
            self.continuation_input_messages.extend(messages);
            return;
        }

        self.continuation_messages
            .extend(
                messages
                    .into_iter()
                    .map(|message| ProviderContinuationMessage::Message {
                        role: message.role,
                        content: message.content,
                    }),
            );
    }

    fn clear_continuation_input_messages(&mut self) {
        self.continuation_input_messages.clear();
    }

    fn record_completion_nudge(&mut self) {
        self.completion_nudges_used += 1;
    }

    fn can_recover_from_empty_response(&self) -> bool {
        self.empty_response_recoveries_used < MAX_EMPTY_RESPONSE_RECOVERIES
            && (!self.pending_tool_outputs.is_empty() || !self.continuation_messages.is_empty())
    }

    fn record_empty_response_recovery(&mut self) {
        self.empty_response_recoveries_used += 1;
    }

    fn adopt_response_anchor(&mut self, response: &ProviderResponse) {
        if self.supports_previous_response_id {
            self.previous_response_id = Some(response.response_id.clone());
            self.pending_tool_outputs.clear();
        }
    }

    fn advance_after_response(&mut self, response: &ProviderResponse) {
        if self.supports_previous_response_id {
            self.previous_response_id = Some(response.response_id.clone());
        } else {
            self.previous_response_id = None;
            self.pending_tool_outputs.clear();
        }
        self.round += 1;
    }
}

fn normalized_mcp_pagination(
    total: usize,
    offset: Option<usize>,
    limit: Option<usize>,
    default_limit: usize,
    max_limit: usize,
) -> (usize, usize, Option<usize>) {
    let offset = offset.unwrap_or(0).min(total);
    let limit = limit.unwrap_or(default_limit).clamp(1, max_limit);
    let next_offset = if offset.saturating_add(limit) < total {
        Some(offset + limit)
    } else {
        None
    };
    (offset, limit, next_offset)
}

impl ExecutionService {
    fn is_stale_context_offload_payload_error(error: &agent_persistence::StoreError) -> bool {
        match error {
            agent_persistence::StoreError::MissingPayload { .. }
            | agent_persistence::StoreError::IntegrityMismatch { .. } => true,
            agent_persistence::StoreError::Io { source, .. } => {
                source.kind() == std::io::ErrorKind::NotFound
            }
            _ => false,
        }
    }

    fn load_context_offload_payload_for_refresh(
        &self,
        store: &PersistenceStore,
        artifact_id: &str,
    ) -> Result<Option<ContextOffloadPayload>, ExecutionError> {
        match store.get_context_offload_payload(artifact_id) {
            Ok(payload) => Ok(payload),
            Err(source) if Self::is_stale_context_offload_payload_error(&source) => Ok(None),
            Err(source) => Err(ExecutionError::Store(source)),
        }
    }

    fn load_context_offload_payload_for_tool(
        &self,
        store: &PersistenceStore,
        artifact_id: &str,
    ) -> Result<ContextOffloadPayload, ExecutionError> {
        match store.get_context_offload_payload(artifact_id) {
            Ok(Some(payload)) => Ok(payload),
            Ok(None)
            | Err(agent_persistence::StoreError::MissingPayload { .. })
            | Err(agent_persistence::StoreError::IntegrityMismatch { .. }) => {
                Err(ExecutionError::Tool(ToolError::InvalidArtifactTool {
                    reason: format!(
                        "artifact {} is missing from context offload storage",
                        artifact_id
                    ),
                }))
            }
            Err(source) => Err(ExecutionError::Store(source)),
        }
    }

    fn provider_tool_output(
        tool_name: &str,
        reason: &str,
        retryable: bool,
        details: serde_json::Value,
    ) -> String {
        serde_json::json!({
            "tool": tool_name,
            "error": reason,
            "retryable": retryable,
            "details": details,
        })
        .to_string()
    }

    fn invalid_provider_tool_output(tool_name: &str, reason: &str) -> String {
        serde_json::json!({
            "tool": tool_name,
            "error": format!("invalid tool call: {reason}"),
            "retryable": true,
        })
        .to_string()
    }

    fn tool_call_ledger_id(run_id: &str, provider_tool_call_id: &str) -> String {
        format!(
            "toolcall-{}-{}",
            sanitize_identifier(run_id),
            sanitize_identifier(provider_tool_call_id)
        )
    }

    fn record_tool_call_ledger(update: ToolCallLedgerUpdate<'_>) -> Result<(), ExecutionError> {
        retry_store_sync(
            SQLITE_LOCK_RETRY_ATTEMPTS,
            Duration::from_millis(SQLITE_LOCK_RETRY_DELAY_MS),
            || {
                let id = Self::tool_call_ledger_id(update.run_id, update.provider_tool_call_id);
                let existing = update.store.get_tool_call(&id)?;
                let requested_at = existing
                    .as_ref()
                    .map(|record| record.requested_at)
                    .unwrap_or(update.now);
                update.store.put_tool_call(&ToolCallRecord {
                    id,
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
                })
            },
        )
        .map_err(ExecutionError::Store)
    }

    fn record_tool_call_result(
        update: ToolCallResultLedgerUpdate<'_>,
    ) -> Result<(), ExecutionError> {
        retry_store_sync(
            SQLITE_LOCK_RETRY_ATTEMPTS,
            Duration::from_millis(SQLITE_LOCK_RETRY_DELAY_MS),
            || {
                let id = Self::tool_call_ledger_id(update.run_id, update.provider_tool_call_id);
                let Some(mut record) = update.store.get_tool_call(&id)? else {
                    return Ok(());
                };

                let (result_preview, result_truncated) =
                    Self::tool_result_preview(update.result_output);
                let result_artifact_id = if result_truncated {
                    let artifact_id =
                        Self::tool_result_artifact_id(update.run_id, update.provider_tool_call_id);
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

    fn tool_result_preview(result_output: &str) -> (String, bool) {
        let mut chars = result_output.chars();
        let preview = chars
            .by_ref()
            .take(TOOL_RESULT_PREVIEW_CHAR_LIMIT)
            .collect::<String>();
        let truncated = chars.next().is_some();
        if truncated {
            (
                format!(
                    "{preview}\n... <truncated; use session tool-result with this tool_call_id>"
                ),
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

    fn retryable_provider_tool_output(
        tool_name: &str,
        reason: &str,
        details: serde_json::Value,
    ) -> String {
        Self::provider_tool_output(tool_name, reason, true, details)
    }

    fn non_retryable_provider_tool_output(
        tool_name: &str,
        reason: &str,
        details: serde_json::Value,
    ) -> String {
        Self::provider_tool_output(tool_name, reason, false, details)
    }

    fn recoverable_tool_error_output(
        &self,
        parsed: &ToolCall,
        error: &ToolError,
    ) -> Option<String> {
        match error {
            ToolError::UnknownProcess { process_id } => Some(Self::retryable_provider_tool_output(
                parsed.name().as_str(),
                &format!("unknown process {process_id}"),
                serde_json::json!({
                    "requested_process_id": process_id,
                    "active_process_ids": self.processes.active_process_ids(Some(agent_runtime::tool::ProcessKind::Exec)),
                }),
            )),
            ToolError::ProcessFamilyMismatch {
                process_id,
                expected,
                actual,
            } => Some(Self::retryable_provider_tool_output(
                parsed.name().as_str(),
                &format!(
                    "process {process_id} has family mismatch: expected {} but found {}",
                    expected.as_prefix(),
                    actual.as_prefix()
                ),
                serde_json::json!({
                    "requested_process_id": process_id,
                    "expected_kind": expected.as_prefix(),
                    "actual_kind": actual.as_prefix(),
                    "active_process_ids": self.processes.active_process_ids(None),
                }),
            )),
            ToolError::ProcessIo { process_id, source } => {
                Some(Self::retryable_provider_tool_output(
                    parsed.name().as_str(),
                    &format!("process io error for {process_id}: {source}"),
                    serde_json::json!({
                        "process_or_executable": process_id,
                        "active_process_ids": self.processes.active_process_ids(None),
                    }),
                ))
            }
            ToolError::Workspace(agent_runtime::workspace::WorkspaceError::InvalidPath {
                path,
                reason,
            }) => Some(Self::retryable_provider_tool_output(
                parsed.name().as_str(),
                &format!("invalid workspace path {path}: {reason}"),
                serde_json::json!({
                    "requested_path": path,
                    "constraint": "workspace_relative_only",
                    "workspace_root": self.workspace.root.display().to_string(),
                }),
            )),
            ToolError::Workspace(agent_runtime::workspace::WorkspaceError::Io { path, source })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Some(Self::retryable_provider_tool_output(
                    parsed.name().as_str(),
                    &format!("workspace path not found: {}", path.display()),
                    serde_json::json!({
                        "requested_path": path.display().to_string(),
                        "hint": "check the exact relative path and list nearby files before retrying",
                    }),
                ))
            }
            ToolError::Workspace(agent_runtime::workspace::WorkspaceError::Io { path, source })
                if matches!(
                    source.kind(),
                    std::io::ErrorKind::IsADirectory | std::io::ErrorKind::NotADirectory
                ) =>
            {
                Some(Self::retryable_provider_tool_output(
                    parsed.name().as_str(),
                    &format!("workspace path is not a regular file: {}", path.display()),
                    serde_json::json!({
                        "requested_path": path.display().to_string(),
                        "io_error": source.to_string(),
                        "hint": "re-check whether the path should target a file or use a list/read-directory style tool instead",
                    }),
                ))
            }
            ToolError::InvalidPatch { path, reason } => Some(Self::retryable_provider_tool_output(
                parsed.name().as_str(),
                &format!("invalid patch for {path}: {reason}"),
                serde_json::json!({
                    "requested_path": path,
                    "patch_error": reason,
                    "hint": "re-read the file and construct the patch from the current content",
                }),
            )),
            ToolError::InvalidPlanWrite { reason }
                if Self::is_retryable_plan_write_reason(reason) =>
            {
                Some(Self::retryable_provider_tool_output(
                    parsed.name().as_str(),
                    &format!("invalid plan reference: {reason}"),
                    serde_json::json!({
                            "plan_error": reason,
                        "hint": "use canonical task_id values returned by add_task or plan_snapshot",
                    }),
                ))
            }
            _ => Some(Self::non_retryable_provider_tool_output(
                parsed.name().as_str(),
                &error.to_string(),
                serde_json::json!({
                    "requested_tool": parsed.name().as_str(),
                    "request_summary": parsed.summary(),
                    "error_kind": format!("{error:?}"),
                    "hint": "inspect the error details and adjust the tool arguments or choose a different tool before retrying",
                }),
            )),
        }
    }

    fn recoverable_execution_error_output(
        &self,
        parsed: &ToolCall,
        error: &ExecutionError,
    ) -> Option<String> {
        match error {
            ExecutionError::Tool(tool_error) => {
                self.recoverable_tool_error_output(parsed, tool_error)
            }
            ExecutionError::PermissionDenied { tool, reason } => Some(
                serde_json::json!({
                    "tool": tool,
                    "error": reason,
                    "retryable": false,
                    "details": {
                        "requested_tool": tool,
                        "constraint": "agent_allowed_tools",
                    },
                })
                .to_string(),
            ),
            _ => None,
        }
    }

    fn is_retryable_plan_write_reason(reason: &str) -> bool {
        reason.starts_with("unknown dependency ")
            || reason.starts_with("unknown task ")
            || reason.starts_with("unknown parent task ")
    }

    fn automatic_provider_tools(
        &self,
        provider: &dyn ProviderDriver,
        context_offload: Option<&ContextOffloadSnapshot>,
        agent_profile: &AgentProfile,
    ) -> Vec<ProviderToolDefinition> {
        if !provider.descriptor().capabilities.supports_tool_calls {
            return Vec::new();
        }

        let has_context_offload = context_offload.is_some_and(|snapshot| !snapshot.is_empty());
        let mut tools = ToolCatalog::default()
            .automatic_model_definitions()
            .into_iter()
            .filter(|definition| {
                agent_profile.allows_tool_id(definition.name.as_str())
                    && (has_context_offload
                        || !matches!(
                            definition.name,
                            ToolName::ArtifactRead | ToolName::ArtifactSearch
                        ))
            })
            .map(|definition| ProviderToolDefinition {
                name: definition.name.as_str().to_string(),
                description: definition.description.to_string(),
                parameters: definition.name.input_schema(),
            })
            .collect::<Vec<_>>();

        tools.extend(
            self.mcp
                .list_discovered_tools()
                .into_iter()
                .filter(|tool| agent_profile.allows_tool_id(tool.exposed_name.as_str()))
                .map(|tool| ProviderToolDefinition {
                    name: tool.exposed_name,
                    description: tool
                        .description
                        .unwrap_or_else(|| format!("MCP tool {}", tool.remote_name)),
                    parameters: tool.input_schema,
                }),
        );

        tools
    }

    fn compaction_policy(&self) -> CompactionPolicy {
        CompactionPolicy {
            min_messages: self.config.context_compaction_min_messages,
            keep_tail_messages: self.config.context_compaction_keep_tail_messages,
            max_output_tokens: self.config.context_compaction_max_output_tokens,
            max_summary_chars: self.config.context_compaction_max_summary_chars,
        }
    }

    fn resolve_context_window_tokens(
        &self,
        provider: &dyn ProviderDriver,
        model: Option<&str>,
    ) -> Option<u32> {
        self.config.context_window_tokens_override.or_else(|| {
            let resolved_model = model
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or(provider.descriptor().default_model.as_deref())?
                .trim()
                .to_ascii_lowercase();
            match (
                provider.descriptor().model_family.as_str(),
                resolved_model.as_str(),
            ) {
                ("zai", "glm-5-turbo") => Some(200_000),
                _ => None,
            }
        })
    }

    fn estimate_prompt_tokens(messages: &[ProviderMessage], instructions: Option<&str>) -> u32 {
        let instruction_tokens = instructions.map_or(0, approximate_token_count);
        instruction_tokens.saturating_add(
            messages
                .iter()
                .map(|message| approximate_token_count(&message.content))
                .sum::<u32>(),
        )
    }

    fn session_head_runtime(
        &self,
        provider: Option<&dyn ProviderDriver>,
        session: &Session,
        model: Option<&str>,
    ) -> SessionHeadRuntime {
        let resolved_model = model
            .map(str::to_string)
            .or_else(|| session.settings.model.clone())
            .or_else(|| provider.and_then(|provider| provider.descriptor().default_model.clone()));
        let context_window_tokens =
            provider.and_then(|provider| self.resolve_context_window_tokens(provider, model));
        let auto_compaction_trigger_ratio = Some(self.config.context_auto_compaction_trigger_ratio);
        SessionHeadRuntime {
            provider_name: provider.map(|provider| provider.descriptor().name.clone()),
            model: resolved_model,
            think_level: session.settings.think_level.clone(),
            context_window_tokens,
            auto_compaction_trigger_ratio,
            usable_context_tokens: SessionHeadRuntime::usable_context_tokens(
                context_window_tokens,
                auto_compaction_trigger_ratio,
            ),
            estimated_prompt_tokens: None,
        }
    }

    fn prompt_budget_context_window_tokens(
        &self,
        provider: Option<&dyn ProviderDriver>,
        session: &Session,
    ) -> Option<u32> {
        provider
            .and_then(|provider| {
                self.resolve_context_window_tokens(provider, session.settings.model.as_deref())
            })
            .or(self.config.context_window_tokens_override)
    }

    fn prompt_budget_read_output_for_session(
        &self,
        provider: Option<&dyn ProviderDriver>,
        session: &Session,
    ) -> PromptBudgetReadOutput {
        let context_window_tokens = self.prompt_budget_context_window_tokens(provider, session);
        let auto_compaction_trigger_ratio = Some(self.config.context_auto_compaction_trigger_ratio);
        let usable_context_tokens = SessionHeadRuntime::usable_context_tokens(
            context_window_tokens,
            auto_compaction_trigger_ratio,
        );
        let policy = &session.settings.prompt_budget;
        let target_tokens = |percent: u8| {
            usable_context_tokens
                .map(|tokens| ((u64::from(tokens) * u64::from(percent)) / 100) as u32)
        };
        let layer = |name: &str, percent: u8| PromptBudgetLayerOutput {
            layer: name.to_string(),
            percent,
            target_tokens: target_tokens(percent),
        };
        PromptBudgetReadOutput {
            session_id: session.id.clone(),
            source: if *policy == agent_runtime::session::PromptBudgetPolicy::default() {
                "runtime_default".to_string()
            } else {
                "session_override".to_string()
            },
            context_window_tokens,
            auto_compaction_trigger_basis_points: (self
                .config
                .context_auto_compaction_trigger_ratio
                * 10_000.0)
                .round()
                .max(0.0) as u32,
            usable_context_tokens,
            total_percent: policy.total_percent(),
            layers: vec![
                layer("system", policy.system),
                layer("agents", policy.agents),
                layer("active_skills", policy.active_skills),
                layer("session_head", policy.session_head),
                layer("autonomy_state", policy.autonomy_state),
                layer("plan", policy.plan),
                layer("context_summary", policy.context_summary),
                layer("offload_refs", policy.offload_refs),
                layer("recent_tool_activity", policy.recent_tool_activity),
                layer("transcript_tail", policy.transcript_tail),
            ],
        }
    }

    fn read_prompt_budget_policy(
        &self,
        store: &PersistenceStore,
        provider: Option<&dyn ProviderDriver>,
        session_id: &str,
    ) -> Result<PromptBudgetReadOutput, ExecutionError> {
        let session = Session::try_from(
            store
                .get_session(session_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingSession {
                    id: session_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        Ok(self.prompt_budget_read_output_for_session(provider, &session))
    }

    fn update_prompt_budget_policy(
        &self,
        store: &PersistenceStore,
        provider: Option<&dyn ProviderDriver>,
        session_id: &str,
        input: &agent_runtime::tool::PromptBudgetUpdateInput,
        now: i64,
    ) -> Result<PromptBudgetUpdateOutput, ExecutionError> {
        let mut session = Session::try_from(
            store
                .get_session(session_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingSession {
                    id: session_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let mut policy = if input.reset {
            agent_runtime::session::PromptBudgetPolicy::default()
        } else {
            session.settings.prompt_budget.clone()
        };
        if let Some(percentages) = &input.percentages {
            percentages.apply_to(&mut policy);
        }
        policy.validate().map_err(|source| {
            ExecutionError::Tool(agent_runtime::tool::ToolError::InvalidPlanWrite {
                reason: source.to_string(),
            })
        })?;
        session.settings.prompt_budget = policy;
        session.updated_at = now;
        let record = agent_persistence::SessionRecord::try_from(&session)
            .map_err(ExecutionError::RecordConversion)?;
        store.put_session(&record).map_err(ExecutionError::Store)?;
        let budget = self.prompt_budget_read_output_for_session(provider, &session);
        Ok(PromptBudgetUpdateOutput {
            session_id: session.id,
            reset: input.reset,
            reason: input.reason.clone(),
            budget,
        })
    }

    fn autonomy_state_for_session(
        &self,
        session: &Session,
        schedule: Option<agent_runtime::prompt::SessionHeadScheduleSummary>,
    ) -> Option<AutonomyState> {
        let turn_source = session
            .delegation_label
            .as_deref()
            .and_then(|label| {
                if label.starts_with("agent-schedule:") {
                    Some("schedule")
                } else if label.starts_with("agent-chain:") {
                    Some("agent2agent")
                } else {
                    None
                }
            })
            .or_else(|| session.parent_session_id.as_ref().map(|_| "subagent"))
            .map(str::to_string);
        if turn_source.is_none() && schedule.is_none() {
            return None;
        }
        Some(AutonomyState {
            turn_source,
            schedule,
            interagent_lines: Vec::new(),
        })
    }

    fn recent_tool_activity(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<Option<RecentToolActivity>, ExecutionError> {
        let mut failures = Vec::new();
        let mut successes = Vec::new();
        for record in store
            .list_tool_calls_for_session(session_id)
            .map_err(ExecutionError::Store)?
            .into_iter()
            .rev()
        {
            if record.status == "failed" && failures.len() < 8 {
                failures.push(recent_tool_activity_entry(record));
            } else if record.status == "completed"
                && (record.result_summary.is_some() || record.result_artifact_id.is_some())
                && successes.len() < 3
            {
                successes.push(recent_tool_activity_entry(record));
            }
            if failures.len() >= 8 && successes.len() >= 3 {
                break;
            }
        }
        failures.reverse();
        successes.reverse();
        let entries = failures.into_iter().chain(successes).collect::<Vec<_>>();
        if entries.is_empty() {
            return Ok(None);
        }
        Ok(Some(RecentToolActivity { entries }))
    }

    fn auto_compaction_decision(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        session_id: &str,
        model: Option<&str>,
        instructions: Option<&str>,
    ) -> Result<Option<AutoCompactionDecision>, ExecutionError> {
        let Some(context_window_tokens) = self.resolve_context_window_tokens(provider, model)
        else {
            return Ok(None);
        };
        let workspace = self.load_session_workspace(store, session_id)?;
        let prompt_messages = self.prompt_messages(
            store,
            Some(provider),
            session_id,
            &workspace,
            model,
            instructions,
        )?;
        let estimated_prompt_tokens =
            Self::estimate_prompt_tokens(&prompt_messages.messages, instructions);
        let trigger_threshold_tokens = ((context_window_tokens as f64)
            * self.config.context_auto_compaction_trigger_ratio)
            .floor() as u32;
        if estimated_prompt_tokens < trigger_threshold_tokens {
            return Ok(None);
        }
        Ok(Some(AutoCompactionDecision {
            estimated_prompt_tokens,
            trigger_threshold_tokens,
            context_window_tokens,
        }))
    }

    pub(crate) fn compact_session_at(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        session_id: &str,
        now: i64,
    ) -> Result<bool, ExecutionError> {
        let session = self.load_session(store, session_id)?;
        let transcripts = store
            .list_transcripts_for_session(session_id)
            .map_err(ExecutionError::Store)?;
        let policy = self.compaction_policy();
        if !policy.should_compact(transcripts.len()) {
            return Ok(false);
        }

        let covered_message_count = policy.covered_message_count(transcripts.len());
        let summary_messages = transcripts
            .iter()
            .take(covered_message_count)
            .map(|record| {
                let role = MessageRole::try_from(record.kind.as_str()).map_err(|_| {
                    ExecutionError::RecordConversion(RecordConversionError::InvalidMessageRole {
                        value: record.kind.clone(),
                    })
                })?;
                Ok::<ProviderMessage, ExecutionError>(ProviderMessage {
                    role,
                    content: record.content.clone(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let response = provider
            .complete(&ProviderRequest {
                model: session.settings.model.clone(),
                instructions: Some(crate::bootstrap::compaction_instructions()),
                messages: summary_messages,
                think_level: None,
                previous_response_id: None,
                continuation_messages: Vec::new(),
                tools: Vec::new(),
                tool_outputs: Vec::new(),
                max_output_tokens: Some(policy.max_output_tokens),
                stream: ProviderStreamMode::Disabled,
            })
            .map_err(ExecutionError::Provider)?;
        let summary_text = policy.trim_summary_text(&response.output_text);
        let context_summary = ContextSummary {
            session_id: session.id.clone(),
            summary_text: summary_text.clone(),
            covered_message_count: covered_message_count as u32,
            summary_token_estimate: approximate_token_count(&summary_text),
            updated_at: now,
        };
        store
            .put_context_summary(&agent_persistence::ContextSummaryRecord::from(
                &context_summary,
            ))
            .map_err(ExecutionError::Store)?;

        let mut updated_session = session;
        updated_session.settings.compactifications += 1;
        updated_session.updated_at = now;
        store
            .put_session(
                &agent_persistence::SessionRecord::try_from(&updated_session)
                    .map_err(ExecutionError::RecordConversion)?,
            )
            .map_err(ExecutionError::Store)?;
        Ok(true)
    }

    fn prompt_messages(
        &self,
        store: &PersistenceStore,
        provider: Option<&dyn ProviderDriver>,
        session_id: &str,
        workspace: &WorkspaceRef,
        model: Option<&str>,
        instructions: Option<&str>,
    ) -> Result<PromptMessages, ExecutionError> {
        let session = Session::try_from(
            store
                .get_session(session_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingSession {
                    id: session_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let transcripts = store
            .list_transcripts_for_session(session_id)
            .map_err(ExecutionError::Store)?;
        let transcript_messages = transcripts
            .iter()
            .map(|record| {
                let role = MessageRole::try_from(record.kind.as_str()).map_err(|_| {
                    ExecutionError::RecordConversion(RecordConversionError::InvalidMessageRole {
                        value: record.kind.clone(),
                    })
                })?;
                Ok(ProviderMessage {
                    role,
                    content: record.content.clone(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let context_summary = store
            .get_context_summary(session_id)
            .map_err(ExecutionError::Store)?
            .map(ContextSummary::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)?;
        let plan_snapshot = store
            .get_plan(session_id)
            .map_err(ExecutionError::Store)?
            .map(PlanSnapshot::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)?;
        let context_offload = store
            .get_context_offload(session_id)
            .map_err(ExecutionError::Store)?
            .map(ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)?;
        let runs = store
            .load_execution_state()
            .map_err(ExecutionError::Store)?
            .runs
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        let agent_name = store
            .get_agent_profile(&session.agent_profile_id)
            .map_err(ExecutionError::Store)?
            .map(|record| record.name)
            .unwrap_or_else(|| session.agent_profile_id.clone());
        let schedule = session
            .delegation_label
            .as_deref()
            .and_then(|label| label.strip_prefix("agent-schedule:"))
            .map(str::to_string)
            .map(|schedule_id| {
                store
                    .get_agent_schedule(&schedule_id)
                    .map_err(ExecutionError::Store)?
                    .map(agent_runtime::agent::AgentSchedule::try_from)
                    .transpose()
                    .map_err(ExecutionError::RecordConversion)
                    .map(|maybe| {
                        maybe
                            .map(crate::bootstrap::SessionScheduleSummary::from)
                            .as_ref()
                            .map(crate::bootstrap::session_head_schedule_summary)
                    })
            })
            .transpose()?
            .flatten();
        let agent_home = agents::agent_home(&self.config.data_dir, &session.agent_profile_id);
        let runtime = self.session_head_runtime(provider, &session, model);
        let schedule_for_autonomy = schedule.clone();
        let session_head = prompting::build_session_head(prompting::BuildSessionHeadInput {
            session: &session,
            agent_name: &agent_name,
            agent_home: Some(agent_home.as_path()),
            runtime: Some(runtime),
            schedule,
            transcripts: &transcripts,
            context_summary: context_summary.as_ref(),
            runs: &runs,
            workspace,
        });
        let system_prompt =
            prompting::load_system_prompt(&self.config.data_dir, &session.agent_profile_id);
        let agents_prompt =
            prompting::load_agents_prompt(&self.config.data_dir, &session.agent_profile_id);
        let transcripts_for_activation = transcripts
            .iter()
            .cloned()
            .map(TranscriptEntry::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        let agent_skills_dir =
            agents::agent_home(&self.config.data_dir, &session.agent_profile_id).join("skills");
        let skills_catalog = scan_skill_catalog_with_overrides(
            &self.config.skills_dir,
            Some(agent_skills_dir.as_path()),
        )
        .map_err(|source| ExecutionError::ProviderLoop {
            reason: format!(
                "failed to scan merged skills catalog at {} and {}: {source}",
                self.config.skills_dir.display(),
                agent_skills_dir.display()
            ),
        })?;
        let active_skill_status = resolve_session_skill_status(
            &skills_catalog,
            &session.settings,
            &session.title,
            &transcripts_for_activation,
        );
        let active_skill_prompts =
            prompting::load_active_skill_prompts(&skills_catalog, &active_skill_status);
        let autonomy_state = self.autonomy_state_for_session(&session, schedule_for_autonomy);
        let recent_tool_activity = self.recent_tool_activity(store, session_id)?;

        let mut input = PromptAssemblyInput {
            system_prompt: Some(system_prompt),
            agents_prompt,
            active_skill_prompts,
            session_head: Some(session_head),
            autonomy_state,
            plan_snapshot,
            context_summary,
            context_offload: context_offload.clone(),
            recent_tool_activity,
            transcript_messages,
        };
        let first_messages = PromptAssembly::build_messages(input.clone());
        if let Some(session_head) = input.session_head.as_mut() {
            session_head.estimated_prompt_tokens =
                Some(Self::estimate_prompt_tokens(&first_messages, instructions));
        }

        Ok(PromptMessages {
            messages: PromptAssembly::build_messages(input),
            context_offload,
        })
    }

    pub(crate) fn build_provider_request_preview(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        session_id: &str,
    ) -> Result<ProviderRequest, ExecutionError> {
        let session = Session::try_from(
            store
                .get_session(session_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingSession {
                    id: session_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let agent_profile = self.load_agent_profile(store, &session.agent_profile_id)?;
        let workspace = WorkspaceRef::new(&session.workspace_root);
        let prompt = self.prompt_messages(
            store,
            Some(provider),
            session_id,
            &workspace,
            session.settings.model.as_deref(),
            session
                .prompt_override
                .as_ref()
                .map(|override_| override_.as_str()),
        )?;
        let tools = self.automatic_provider_tools(
            provider,
            prompt.context_offload.as_ref(),
            &agent_profile,
        );
        let cursor = ProviderLoopCursor::new(provider, None, self.config.provider_max_tool_rounds);
        Ok(cursor.build_request(
            &prompt.messages,
            session.settings.model.as_deref(),
            session
                .prompt_override
                .as_ref()
                .map(|override_| override_.as_str()),
            session.settings.think_level.as_deref(),
            &tools,
            cursor.stream_mode(true),
            self.config.provider_max_output_tokens,
        ))
    }

    pub(super) fn persist_run(
        &self,
        store: &PersistenceStore,
        run: &RunEngine,
    ) -> Result<(), ExecutionError> {
        let record =
            RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?;
        retry_store_sync(
            SQLITE_LOCK_RETRY_ATTEMPTS,
            Duration::from_millis(SQLITE_LOCK_RETRY_DELAY_MS),
            || store.put_run(&record),
        )
        .map_err(ExecutionError::Store)
    }

    pub(crate) fn latest_active_session_run(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<Option<RunSnapshot>, ExecutionError> {
        let runs = store
            .load_execution_state()
            .map_err(ExecutionError::Store)?
            .runs
            .into_iter()
            .map(RunSnapshot::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        Ok(runs
            .into_iter()
            .filter(|run| run.session_id == session_id && !run.status.is_terminal())
            .max_by(|left, right| {
                left.updated_at
                    .cmp(&right.updated_at)
                    .then_with(|| left.started_at.cmp(&right.started_at))
                    .then_with(|| left.id.cmp(&right.id))
            }))
    }

    pub(crate) fn cancel_latest_session_run(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        now: i64,
    ) -> Result<Option<RunSnapshot>, ExecutionError> {
        let Some(snapshot) = self.latest_active_session_run(store, session_id)? else {
            return Ok(None);
        };

        for process in &snapshot.active_processes {
            self.terminate_pid_ref(process.pid_ref.as_str())?;
        }

        let mut run = RunEngine::from_snapshot(snapshot);
        run.cancel("operator stop", now)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, &run)?;
        Ok(Some(run.snapshot().clone()))
    }

    pub(crate) fn cancel_all_session_work(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        now: i64,
    ) -> Result<SessionWorkCancellationReport, ExecutionError> {
        let snapshot = store
            .load_execution_state()
            .map_err(ExecutionError::Store)?;
        if !snapshot
            .sessions
            .iter()
            .any(|record| record.id == session_id)
        {
            return Ok(SessionWorkCancellationReport::default());
        }

        let target_session_ids =
            self.collect_session_tree_session_ids(session_id, &snapshot.sessions);
        let mut report = SessionWorkCancellationReport {
            session_count: target_session_ids.len(),
            ..SessionWorkCancellationReport::default()
        };
        let mut cancelled_mission_ids = BTreeSet::new();

        for mission_record in snapshot.missions {
            let mut mission =
                MissionSpec::try_from(mission_record).map_err(ExecutionError::RecordConversion)?;
            if !target_session_ids.contains(&mission.session_id) {
                continue;
            }
            if !matches!(
                mission.status,
                MissionStatus::Ready | MissionStatus::Running
            ) {
                continue;
            }
            mission.status = MissionStatus::Cancelled;
            mission.updated_at = now;
            mission.completed_at = Some(now);
            cancelled_mission_ids.insert(mission.id.clone());
            store
                .put_mission(
                    &MissionRecord::try_from(&mission).map_err(ExecutionError::RecordConversion)?,
                )
                .map_err(ExecutionError::Store)?;
            report.mission_count += 1;
        }

        for session_record in snapshot.sessions {
            if !target_session_ids.contains(&session_record.id) {
                continue;
            }
            let clear_active_mission = session_record
                .active_mission_id
                .as_ref()
                .is_some_and(|id| cancelled_mission_ids.contains(id));
            if !clear_active_mission {
                continue;
            }
            let mut session =
                Session::try_from(session_record).map_err(ExecutionError::RecordConversion)?;
            session.active_mission_id = None;
            session.updated_at = now;
            store
                .put_session(
                    &agent_persistence::SessionRecord::try_from(&session)
                        .map_err(ExecutionError::RecordConversion)?,
                )
                .map_err(ExecutionError::Store)?;
        }

        for run_record in snapshot.runs {
            let snapshot =
                RunSnapshot::try_from(run_record).map_err(ExecutionError::RecordConversion)?;
            if !target_session_ids.contains(&snapshot.session_id) || snapshot.status.is_terminal() {
                continue;
            }
            for process in &snapshot.active_processes {
                self.terminate_pid_ref(process.pid_ref.as_str())?;
            }
            let mut run = RunEngine::from_snapshot(snapshot);
            run.cancel("operator cancelled all session work", now)
                .map_err(ExecutionError::RunTransition)?;
            self.persist_run(store, &run)?;
            report.run_count += 1;
        }

        for job_record in snapshot.jobs {
            let mut job =
                JobSpec::try_from(job_record).map_err(ExecutionError::RecordConversion)?;
            if !target_session_ids.contains(&job.session_id) || !job.status.is_active() {
                continue;
            }
            self.cancel_job_spec(store, &mut job, now, "operator cancelled all session work")?;
            report.job_count += 1;
        }

        for inbox_record in snapshot.inbox_events {
            if !target_session_ids.contains(&inbox_record.session_id) {
                continue;
            }
            let event = SessionInboxEvent::try_from(inbox_record)
                .map_err(ExecutionError::RecordConversion)?
                .mark_processed(now);
            store
                .put_session_inbox_event(
                    &agent_persistence::SessionInboxEventRecord::try_from(&event)
                        .map_err(ExecutionError::RecordConversion)?,
                )
                .map_err(ExecutionError::Store)?;
            report.inbox_event_count += 1;
        }

        Ok(report)
    }

    fn collect_session_tree_session_ids(
        &self,
        root_session_id: &str,
        sessions: &[agent_persistence::SessionRecord],
    ) -> BTreeSet<String> {
        let mut target_session_ids = BTreeSet::from([root_session_id.to_string()]);
        loop {
            let mut changed = false;
            for session in sessions {
                if target_session_ids.contains(&session.id) {
                    continue;
                }
                if session
                    .parent_session_id
                    .as_ref()
                    .is_some_and(|parent| target_session_ids.contains(parent))
                {
                    target_session_ids.insert(session.id.clone());
                    changed = true;
                }
            }
            if !changed {
                return target_session_ids;
            }
        }
    }

    pub(super) fn cancel_job_spec(
        &self,
        store: &PersistenceStore,
        job: &mut JobSpec,
        now: i64,
        reason: &str,
    ) -> Result<(), ExecutionError> {
        job.status = JobStatus::Cancelled;
        job.error = Some(reason.to_string());
        job.updated_at = now;
        job.finished_at = Some(now);
        job.cancel_requested_at = Some(now);
        job.lease_owner = None;
        job.lease_expires_at = None;
        job.heartbeat_at = Some(now);
        job.last_progress_message = Some(reason.to_string());
        let record = JobRecord::try_from(&*job).map_err(ExecutionError::RecordConversion)?;
        retry_store_sync(
            SQLITE_LOCK_RETRY_ATTEMPTS,
            Duration::from_millis(SQLITE_LOCK_RETRY_DELAY_MS),
            || store.put_job(&record),
        )
        .map_err(ExecutionError::Store)
    }

    pub(super) fn run_was_cancelled_by_operator(
        &self,
        store: &PersistenceStore,
        run_id: &str,
    ) -> Result<bool, ExecutionError> {
        let Some(run) = store.get_run(run_id).map_err(ExecutionError::Store)? else {
            return Ok(false);
        };
        let snapshot = RunSnapshot::try_from(run).map_err(ExecutionError::RecordConversion)?;
        Ok(snapshot.status == agent_runtime::run::RunStatus::Cancelled)
    }

    pub(super) fn job_was_cancelled_by_operator(
        &self,
        store: &PersistenceStore,
        job_id: &str,
    ) -> Result<bool, ExecutionError> {
        let Some(job) = store.get_job(job_id).map_err(ExecutionError::Store)? else {
            return Ok(false);
        };
        let job = JobSpec::try_from(job).map_err(ExecutionError::RecordConversion)?;
        Ok(job.status == JobStatus::Cancelled || job.cancel_requested_at.is_some())
    }

    fn terminate_pid_ref(&self, pid_ref: &str) -> Result<(), ExecutionError> {
        #[cfg(unix)]
        {
            let Some(pid_text) = pid_ref.strip_prefix("pid:") else {
                return Err(ExecutionError::ProviderLoop {
                    reason: format!("invalid pid_ref {pid_ref}"),
                });
            };
            let pid =
                pid_text
                    .parse::<libc::pid_t>()
                    .map_err(|_| ExecutionError::ProviderLoop {
                        reason: format!("invalid pid_ref {pid_ref}"),
                    })?;
            let rc = unsafe { libc::kill(pid, libc::SIGTERM) };
            if rc == 0 {
                return Ok(());
            }
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                return Ok(());
            }
            Err(ExecutionError::ProviderLoop {
                reason: format!("failed to stop process {pid_ref}: {error}"),
            })
        }

        #[cfg(not(unix))]
        {
            let _ = pid_ref;
            Err(ExecutionError::ProviderLoop {
                reason: "operator stop for active processes is not supported on this platform yet"
                    .to_string(),
            })
        }
    }

    pub(super) fn find_job_by_run_id(
        &self,
        store: &PersistenceStore,
        run_id: &str,
    ) -> Result<Option<JobSpec>, ExecutionError> {
        store
            .load_execution_state()
            .map_err(ExecutionError::Store)?
            .jobs
            .into_iter()
            .find(|record| record.run_id.as_deref() == Some(run_id))
            .map(JobSpec::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)
    }

    fn apply_provider_response(
        &self,
        run: &mut RunEngine,
        observed: &ObservedProviderResponse,
        now: i64,
    ) -> Result<(), ExecutionError> {
        run.begin_provider_stream(
            &observed.response.response_id,
            &observed.response.model,
            now,
        )
        .map_err(ExecutionError::RunTransition)?;
        for delta in &observed.reasoning_deltas {
            run.push_provider_reasoning(delta, now)
                .map_err(ExecutionError::RunTransition)?;
        }
        if !observed.text_deltas.is_empty() {
            for delta in &observed.text_deltas {
                run.push_provider_text(delta, now)
                    .map_err(ExecutionError::RunTransition)?;
            }
        } else if !observed.response.output_text.is_empty() {
            run.push_provider_text(&observed.response.output_text, now)
                .map_err(ExecutionError::RunTransition)?;
        }
        run.set_latest_provider_usage(observed.response.usage.clone(), now)
            .map_err(ExecutionError::RunTransition)?;
        run.finish_provider_stream(now)
            .map_err(ExecutionError::RunTransition)?;
        Ok(())
    }

    pub(super) fn emit_event(
        observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
        event: ChatExecutionEvent,
    ) {
        if let Some(observer) = observer.as_deref_mut() {
            observer(event);
        }
    }

    fn request_provider_response(
        &self,
        provider: &dyn ProviderDriver,
        request: &ProviderRequest,
        observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
    ) -> Result<ObservedProviderResponse, ExecutionError> {
        if matches!(request.stream, ProviderStreamMode::Enabled) {
            let mut stream = provider.stream(request).map_err(ExecutionError::Provider)?;
            let mut final_response = None;
            let mut reasoning_deltas = Vec::new();
            let mut text_deltas = Vec::new();
            while let Some(event) = stream.next_event().map_err(ExecutionError::Provider)? {
                match event {
                    ProviderStreamEvent::ReasoningDelta(delta) => {
                        Self::emit_event(
                            observer,
                            ChatExecutionEvent::ReasoningDelta(delta.clone()),
                        );
                        reasoning_deltas.push(delta);
                    }
                    ProviderStreamEvent::TextDelta(delta) => {
                        Self::emit_event(
                            observer,
                            ChatExecutionEvent::AssistantTextDelta(delta.clone()),
                        );
                        text_deltas.push(delta);
                    }
                    ProviderStreamEvent::Completed(response) => {
                        final_response = Some(response);
                        break;
                    }
                }
            }
            final_response
                .map(|response| ObservedProviderResponse {
                    response,
                    reasoning_deltas,
                    text_deltas,
                })
                .ok_or_else(|| ExecutionError::ProviderLoop {
                    reason: "provider stream ended without a final response".to_string(),
                })
        } else {
            provider
                .complete(request)
                .map(|response| ObservedProviderResponse {
                    response,
                    reasoning_deltas: Vec::new(),
                    text_deltas: Vec::new(),
                })
                .map_err(ExecutionError::Provider)
        }
    }

    fn note_transient_provider_retry(
        &self,
        store: &PersistenceStore,
        run: &mut RunEngine,
        error: &ProviderError,
        attempt: usize,
        at: i64,
    ) -> Result<(), ExecutionError> {
        let detail = format!(
            "provider retryable error: {}; retrying request ({}/{})",
            error.approval_summary(),
            attempt,
            MAX_TRANSIENT_PROVIDER_RETRIES
        );
        run.record_system_note(detail, at)
            .map_err(ExecutionError::RunTransition)?;
        self.persist_run(store, run)
    }

    fn request_provider_response_with_retries(
        &self,
        store: &PersistenceStore,
        run: &mut RunEngine,
        provider: &dyn ProviderDriver,
        request: &ProviderRequest,
        now: i64,
        observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
    ) -> Result<ObservedProviderResponse, ExecutionError> {
        let mut attempt = 0usize;
        loop {
            match self.request_provider_response(provider, request, observer) {
                Ok(observed) => return Ok(observed),
                Err(ExecutionError::Provider(error))
                    if error.is_transient() && attempt < MAX_TRANSIENT_PROVIDER_RETRIES =>
                {
                    attempt += 1;
                    self.note_transient_provider_retry(store, run, &error, attempt, now)?;
                    thread::sleep(
                        self.config
                            .runtime_timing
                            .provider_loop_transient_retry_delay(attempt),
                    );
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn resolve_provider_tool_call(
        &self,
        catalog: &ToolCatalog,
        tool_call: &ProviderToolCall,
    ) -> Result<(ToolCall, ToolDefinition), ExecutionError> {
        let parsed = ToolCall::from_openai_function(&tool_call.name, &tool_call.arguments)
            .map_err(|source| ExecutionError::ToolCallParse {
                name: tool_call.name.clone(),
                reason: source.to_string(),
            })?;
        if let ToolCall::McpCall(input) = &parsed {
            let discovered = self
                .mcp
                .list_discovered_tools()
                .into_iter()
                .find(|tool| tool.exposed_name == input.exposed_name)
                .ok_or_else(|| ExecutionError::ToolCallParse {
                    name: tool_call.name.clone(),
                    reason: format!("unknown MCP tool {}", input.exposed_name),
                })?;
            return Ok((
                parsed,
                ToolDefinition {
                    name: ToolName::McpCall,
                    family: ToolFamily::Mcp,
                    description: "invoke a discovered MCP tool",
                    policy: ToolPolicy {
                        read_only: discovered.read_only,
                        destructive: discovered.destructive,
                        requires_approval: discovered.destructive || !discovered.read_only,
                    },
                },
            ));
        }
        let definition = catalog
            .definition_for_call(&parsed)
            .ok_or_else(|| ExecutionError::ToolCallParse {
                name: tool_call.name.clone(),
                reason: "tool is not in the catalog".to_string(),
            })?
            .clone();
        Ok((parsed, definition))
    }

    pub(super) fn invoke_provider_tool_call(
        &self,
        context: ProviderToolExecutionContext<'_>,
        run: &mut RunEngine,
        tool_runtime: &mut ToolRuntime,
        invocation: ProviderToolCallInvocation<'_>,
        observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
    ) -> Result<String, ExecutionError> {
        let parsed = invocation.parsed;
        Self::record_tool_call_ledger(ToolCallLedgerUpdate {
            store: context.store,
            session_id: context.session_id,
            run_id: context.run_id,
            provider_tool_call_id: invocation.tool_call_id,
            tool_name: parsed.name().as_str(),
            arguments_json: invocation.arguments_json,
            summary: &parsed.summary(),
            status: ToolExecutionStatus::Running,
            error: None,
            now: context.now,
        })?;
        Self::emit_event(
            observer,
            ChatExecutionEvent::ToolStatus {
                tool_call_id: invocation.tool_call_id.to_string(),
                tool_name: parsed.name().as_str().to_string(),
                summary: parsed.summary(),
                status: ToolExecutionStatus::Running,
            },
        );
        let output = match self.execute_model_tool_call(
            context.store,
            Some(context.provider),
            context.session_id,
            tool_runtime,
            parsed,
            context.now,
        ) {
            Ok(output) => output,
            Err(source) => {
                Self::emit_event(
                    observer,
                    ChatExecutionEvent::ToolStatus {
                        tool_call_id: invocation.tool_call_id.to_string(),
                        tool_name: parsed.name().as_str().to_string(),
                        summary: parsed.summary(),
                        status: ToolExecutionStatus::Failed,
                    },
                );
                Self::record_tool_call_ledger(ToolCallLedgerUpdate {
                    store: context.store,
                    session_id: context.session_id,
                    run_id: context.run_id,
                    provider_tool_call_id: invocation.tool_call_id,
                    tool_name: parsed.name().as_str(),
                    arguments_json: invocation.arguments_json,
                    summary: &parsed.summary(),
                    status: ToolExecutionStatus::Failed,
                    error: Some(source.to_string()),
                    now: context.now,
                })?;
                return Err(source);
            }
        };
        match &output {
            ToolOutput::ProcessStart(start) => {
                run.track_active_process(
                    agent_runtime::run::ActiveProcess::new(
                        start.process_id.clone(),
                        super::tools::process_kind_label(start.kind),
                        start.pid_ref.clone(),
                        context.now,
                    )
                    .with_command_details(start.command_display.clone(), start.cwd.clone()),
                    context.now,
                )
                .map_err(ExecutionError::RunTransition)?;
            }
            ToolOutput::ProcessResult(result)
                if run
                    .snapshot()
                    .active_processes
                    .iter()
                    .any(|process| process.id == result.process_id) =>
            {
                run.finish_active_process(&result.process_id, result.exit_code, context.now)
                    .map_err(ExecutionError::RunTransition)?;
            }
            _ => {}
        }
        let output_summary = output.summary();
        let model_output = output.model_output();
        run.record_tool_completion(
            super::tools::completed_tool_step_detail(parsed, &output),
            context.now,
        )
        .map_err(ExecutionError::RunTransition)?;
        Self::emit_event(
            observer,
            ChatExecutionEvent::ToolStatus {
                tool_call_id: invocation.tool_call_id.to_string(),
                tool_name: parsed.name().as_str().to_string(),
                summary: output_summary.clone(),
                status: ToolExecutionStatus::Completed,
            },
        );
        Self::record_tool_call_ledger(ToolCallLedgerUpdate {
            store: context.store,
            session_id: context.session_id,
            run_id: context.run_id,
            provider_tool_call_id: invocation.tool_call_id,
            tool_name: parsed.name().as_str(),
            arguments_json: invocation.arguments_json,
            summary: &parsed.summary(),
            status: ToolExecutionStatus::Completed,
            error: None,
            now: context.now,
        })?;
        Self::record_tool_call_result(ToolCallResultLedgerUpdate {
            store: context.store,
            session_id: context.session_id,
            run_id: context.run_id,
            provider_tool_call_id: invocation.tool_call_id,
            tool_name: parsed.name().as_str(),
            result_summary: &output_summary,
            result_output: &model_output,
            now: context.now,
        })?;
        self.prepare_model_tool_output(
            context.store,
            context.session_id,
            invocation.tool_call_id,
            parsed,
            &output,
            model_output,
            context.now,
        )
    }

    pub(super) fn execute_model_tool_call(
        &self,
        store: &PersistenceStore,
        provider: Option<&dyn ProviderDriver>,
        session_id: &str,
        tool_runtime: &mut ToolRuntime,
        parsed: &ToolCall,
        now: i64,
    ) -> Result<ToolOutput, ExecutionError> {
        match parsed {
            ToolCall::PlanRead(_) => Ok(ToolOutput::PlanRead(
                self.read_plan_snapshot(store, session_id)?,
            )),
            ToolCall::PlanWrite(input) => Ok(ToolOutput::PlanWrite(
                self.write_plan_snapshot(store, session_id, input, now)?,
            )),
            ToolCall::InitPlan(input) => Ok(ToolOutput::InitPlan(self.init_plan_snapshot(
                store,
                session_id,
                input.goal.as_str(),
                now,
            )?)),
            ToolCall::AddTask(input) => Ok(ToolOutput::AddTask(self.add_plan_task(
                store,
                session_id,
                input.description.as_str(),
                input.depends_on.clone(),
                input.parent_task_id.clone(),
                now,
            )?)),
            ToolCall::SetTaskStatus(input) => {
                Ok(ToolOutput::SetTaskStatus(self.set_plan_task_status(
                    store,
                    session_id,
                    input.task_id.as_str(),
                    input.new_status.as_str(),
                    input.blocked_reason.clone(),
                    now,
                )?))
            }
            ToolCall::AddTaskNote(input) => Ok(ToolOutput::AddTaskNote(self.add_plan_task_note(
                store,
                session_id,
                input.task_id.as_str(),
                input.note.as_str(),
                now,
            )?)),
            ToolCall::EditTask(input) => Ok(ToolOutput::EditTask(self.edit_plan_task(
                store,
                session_id,
                input.task_id.as_str(),
                input.description.clone(),
                input.depends_on.clone(),
                input.parent_task_id.clone(),
                input.clear_parent_task,
                now,
            )?)),
            ToolCall::PlanSnapshot(_) => Ok(ToolOutput::PlanSnapshot(
                self.plan_snapshot_output(store, session_id)?,
            )),
            ToolCall::PlanLint(_) => Ok(ToolOutput::PlanLint(
                self.lint_plan_snapshot(store, session_id)?,
            )),
            ToolCall::PromptBudgetRead(_) => Ok(ToolOutput::PromptBudgetRead(
                self.read_prompt_budget_policy(store, provider, session_id)?,
            )),
            ToolCall::PromptBudgetUpdate(input) => Ok(ToolOutput::PromptBudgetUpdate(
                self.update_prompt_budget_policy(store, provider, session_id, input, now)?,
            )),
            ToolCall::ArtifactRead(input) => Ok(ToolOutput::ArtifactRead(
                self.read_context_offload_artifact(store, session_id, input.artifact_id.as_str())?,
            )),
            ToolCall::ArtifactSearch(input) => Ok(ToolOutput::ArtifactSearch(
                self.search_context_offload_artifacts(
                    store,
                    session_id,
                    input.query.as_str(),
                    input.limit,
                )?,
            )),
            ToolCall::KnowledgeSearch(input) => Ok(ToolOutput::KnowledgeSearch(
                self.search_knowledge(store, input)?,
            )),
            ToolCall::KnowledgeRead(input) => Ok(ToolOutput::KnowledgeRead(
                self.read_knowledge(store, input)?,
            )),
            ToolCall::McpCall(input) => Ok(ToolOutput::McpCall(
                self.mcp
                    .call_tool(&input.exposed_name, &input.arguments_json)
                    .map_err(|reason| {
                        ExecutionError::Tool(agent_runtime::tool::ToolError::InvalidMcpTool {
                            reason,
                        })
                    })?,
            )),
            ToolCall::McpSearchResources(input) => Ok(ToolOutput::McpSearchResources(
                self.search_mcp_resources(input),
            )),
            ToolCall::McpReadResource(input) => Ok(ToolOutput::McpReadResource(
                self.mcp
                    .read_resource(&input.connector_id, &input.uri)
                    .map_err(|reason| {
                        ExecutionError::Tool(agent_runtime::tool::ToolError::InvalidMcpTool {
                            reason,
                        })
                    })?,
            )),
            ToolCall::McpSearchPrompts(input) => {
                Ok(ToolOutput::McpSearchPrompts(self.search_mcp_prompts(input)))
            }
            ToolCall::McpGetPrompt(input) => Ok(ToolOutput::McpGetPrompt(
                self.mcp
                    .get_prompt(&input.connector_id, &input.name, input.arguments.clone())
                    .map_err(|reason| {
                        ExecutionError::Tool(agent_runtime::tool::ToolError::InvalidMcpTool {
                            reason,
                        })
                    })?,
            )),
            ToolCall::SessionSearch(input) => Ok(ToolOutput::SessionSearch(
                self.search_sessions(store, input)?,
            )),
            ToolCall::SessionRead(input) => {
                Ok(ToolOutput::SessionRead(self.read_session(store, input)?))
            }
            ToolCall::SessionWait(input) => {
                let Some(provider) = provider else {
                    return Err(ExecutionError::Tool(
                        agent_runtime::tool::ToolError::InvalidAgentTool {
                            reason:
                                "session_wait requires a provider-backed canonical session path"
                                    .to_string(),
                        },
                    ));
                };
                Ok(ToolOutput::SessionWait(
                    self.wait_for_session(store, provider, input, now)?,
                ))
            }
            ToolCall::AgentList(input) => {
                Ok(ToolOutput::AgentList(self.list_tool_agents(store, input)?))
            }
            ToolCall::AgentRead(input) => {
                Ok(ToolOutput::AgentRead(self.read_tool_agent(store, input)?))
            }
            ToolCall::AgentCreate(input) => Ok(ToolOutput::AgentCreate(
                self.create_tool_agent(store, session_id, input, now)?,
            )),
            ToolCall::ContinueLater(input) => Ok(ToolOutput::ContinueLater(
                self.continue_later_tool(store, session_id, input, now)?,
            )),
            ToolCall::ScheduleList(input) => Ok(ToolOutput::ScheduleList(
                self.list_tool_schedules(store, input)?,
            )),
            ToolCall::ScheduleRead(input) => Ok(ToolOutput::ScheduleRead(
                self.read_tool_schedule(store, input)?,
            )),
            ToolCall::ScheduleCreate(input) => Ok(ToolOutput::ScheduleCreate(
                self.create_tool_schedule(store, session_id, input, now)?,
            )),
            ToolCall::ScheduleUpdate(input) => Ok(ToolOutput::ScheduleUpdate(
                self.update_tool_schedule(store, session_id, input, now)?,
            )),
            ToolCall::ScheduleDelete(input) => Ok(ToolOutput::ScheduleDelete(
                self.delete_tool_schedule(store, input)?,
            )),
            ToolCall::MessageAgent(input) => Ok(ToolOutput::MessageAgent(
                self.queue_interagent_message(store, session_id, input, now)?,
            )),
            ToolCall::GrantAgentChainContinuation(input) => {
                Ok(ToolOutput::GrantAgentChainContinuation(
                    self.grant_agent_chain_continuation(store, input, now)?,
                ))
            }
            _ => tool_runtime
                .invoke(parsed.clone())
                .map_err(ExecutionError::Tool),
        }
    }

    fn search_mcp_resources(
        &self,
        input: &agent_runtime::tool::McpSearchResourcesInput,
    ) -> agent_runtime::tool::McpSearchResourcesOutput {
        let query = input
            .query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let query_lower = query.as_ref().map(|value| value.to_ascii_lowercase());

        let mut results = self
            .mcp
            .list_discovered_resources(input.connector_id.as_deref())
            .into_iter()
            .filter(|resource| {
                query_lower.as_ref().is_none_or(|needle| {
                    resource.uri.to_ascii_lowercase().contains(needle)
                        || resource.name.to_ascii_lowercase().contains(needle)
                        || resource
                            .title
                            .as_ref()
                            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                        || resource
                            .description
                            .as_ref()
                            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                        || resource
                            .mime_type
                            .as_ref()
                            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                })
            })
            .map(
                |resource| agent_runtime::tool::McpDiscoveredResourceOutput {
                    connector_id: resource.connector_id,
                    uri: resource.uri,
                    name: resource.name,
                    title: resource.title,
                    description: resource.description,
                    mime_type: resource.mime_type,
                },
            )
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            left.connector_id
                .cmp(&right.connector_id)
                .then_with(|| left.uri.cmp(&right.uri))
        });

        let (offset, limit, next_offset) = normalized_mcp_pagination(
            results.len(),
            input.offset,
            input.limit,
            self.config.runtime_limits.mcp_search_default_limit,
            self.config.runtime_limits.mcp_search_max_limit,
        );
        let end = offset.saturating_add(limit).min(results.len());
        let page = results[offset..end].to_vec();

        agent_runtime::tool::McpSearchResourcesOutput {
            connector_id: input.connector_id.clone(),
            query,
            results: page,
            truncated: next_offset.is_some(),
            offset,
            limit,
            total_results: results.len(),
            next_offset,
        }
    }

    fn search_mcp_prompts(
        &self,
        input: &agent_runtime::tool::McpSearchPromptsInput,
    ) -> agent_runtime::tool::McpSearchPromptsOutput {
        let query = input
            .query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let query_lower = query.as_ref().map(|value| value.to_ascii_lowercase());

        let mut results = self
            .mcp
            .list_discovered_prompts(input.connector_id.as_deref())
            .into_iter()
            .filter(|prompt| {
                query_lower.as_ref().is_none_or(|needle| {
                    prompt.name.to_ascii_lowercase().contains(needle)
                        || prompt
                            .title
                            .as_ref()
                            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                        || prompt
                            .description
                            .as_ref()
                            .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
                        || prompt.arguments.iter().any(|argument| {
                            argument.name.to_ascii_lowercase().contains(needle)
                                || argument.description.as_ref().is_some_and(|value| {
                                    value.to_ascii_lowercase().contains(needle)
                                })
                        })
                })
            })
            .map(|prompt| agent_runtime::tool::McpDiscoveredPromptOutput {
                connector_id: prompt.connector_id,
                name: prompt.name,
                title: prompt.title,
                description: prompt.description,
                arguments: prompt
                    .arguments
                    .into_iter()
                    .map(|argument| agent_runtime::tool::McpPromptArgumentOutput {
                        name: argument.name,
                        description: argument.description,
                        required: argument.required,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            left.connector_id
                .cmp(&right.connector_id)
                .then_with(|| left.name.cmp(&right.name))
        });

        let (offset, limit, next_offset) = normalized_mcp_pagination(
            results.len(),
            input.offset,
            input.limit,
            self.config.runtime_limits.mcp_search_default_limit,
            self.config.runtime_limits.mcp_search_max_limit,
        );
        let end = offset.saturating_add(limit).min(results.len());
        let page = results[offset..end].to_vec();

        agent_runtime::tool::McpSearchPromptsOutput {
            connector_id: input.connector_id.clone(),
            query,
            results: page,
            truncated: next_offset.is_some(),
            offset,
            limit,
            total_results: results.len(),
            next_offset,
        }
    }

    fn read_plan_snapshot(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<PlanReadOutput, ExecutionError> {
        let snapshot = self.load_plan_snapshot(store, session_id)?;

        Ok(PlanReadOutput {
            goal: snapshot.goal,
            items: snapshot.items,
        })
    }

    fn write_plan_snapshot(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &agent_runtime::tool::PlanWriteInput,
        now: i64,
    ) -> Result<PlanWriteOutput, ExecutionError> {
        let items = input
            .items
            .clone()
            .into_iter()
            .map(PlanItem::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| {
                ExecutionError::Tool(ToolError::InvalidPlanWrite {
                    reason: source.to_string(),
                })
            })?;
        let snapshot = PlanSnapshot {
            session_id: session_id.to_string(),
            goal: self.load_plan_snapshot(store, session_id)?.goal,
            items: items.clone(),
            updated_at: now,
        };
        self.persist_plan_snapshot(store, &snapshot)?;

        Ok(PlanWriteOutput {
            goal: snapshot.goal,
            items,
        })
    }

    fn init_plan_snapshot(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        goal: &str,
        now: i64,
    ) -> Result<InitPlanOutput, ExecutionError> {
        let mut snapshot = self.load_plan_snapshot(store, session_id)?;
        let goal = goal.trim();
        if goal.is_empty() {
            return Err(Self::invalid_plan_tool(
                agent_runtime::plan::PlanMutationError::EmptyGoal,
            ));
        }

        if snapshot.plan_exists() {
            if snapshot.goal.is_none() {
                snapshot.goal = Some(goal.to_string());
                snapshot.updated_at = now;
                self.persist_plan_snapshot(store, &snapshot)?;
            }
        } else {
            snapshot
                .initialize(goal, now)
                .map_err(Self::invalid_plan_tool)?;
            self.persist_plan_snapshot(store, &snapshot)?;
        }

        Ok(InitPlanOutput {
            goal: snapshot.goal.unwrap_or_else(|| goal.to_string()),
            item_count: snapshot.items.len(),
        })
    }

    fn add_plan_task(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        description: &str,
        depends_on: Vec<String>,
        parent_task_id: Option<String>,
        now: i64,
    ) -> Result<AddTaskOutput, ExecutionError> {
        let mut snapshot = self.load_plan_snapshot(store, session_id)?;
        let depends_on = Self::normalize_plan_task_references(&snapshot, depends_on);
        let parent_task_id =
            Self::normalize_optional_plan_task_reference(&snapshot, parent_task_id);
        let task = snapshot
            .add_task(description, depends_on, parent_task_id, now)
            .map_err(Self::invalid_plan_tool)?;
        self.persist_plan_snapshot(store, &snapshot)?;
        Ok(AddTaskOutput { task })
    }

    fn set_plan_task_status(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        task_id: &str,
        new_status: &str,
        blocked_reason: Option<String>,
        now: i64,
    ) -> Result<SetTaskStatusOutput, ExecutionError> {
        let mut snapshot = self.load_plan_snapshot(store, session_id)?;
        let task_id = Self::normalize_plan_task_reference(&snapshot, task_id);
        let status = PlanItemStatus::try_from(new_status).map_err(Self::invalid_plan_tool)?;
        let task = snapshot
            .set_task_status(&task_id, status, blocked_reason, now)
            .map_err(Self::invalid_plan_tool)?;
        self.persist_plan_snapshot(store, &snapshot)?;
        Ok(SetTaskStatusOutput { task })
    }

    fn add_plan_task_note(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        task_id: &str,
        note: &str,
        now: i64,
    ) -> Result<AddTaskNoteOutput, ExecutionError> {
        let mut snapshot = self.load_plan_snapshot(store, session_id)?;
        let task_id = Self::normalize_plan_task_reference(&snapshot, task_id);
        let task = snapshot
            .add_task_note(&task_id, note, now)
            .map_err(Self::invalid_plan_tool)?;
        self.persist_plan_snapshot(store, &snapshot)?;
        Ok(AddTaskNoteOutput { task })
    }

    #[allow(clippy::too_many_arguments)]
    fn edit_plan_task(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        task_id: &str,
        description: Option<String>,
        depends_on: Option<Vec<String>>,
        parent_task_id: Option<String>,
        clear_parent_task: bool,
        now: i64,
    ) -> Result<EditTaskOutput, ExecutionError> {
        let mut snapshot = self.load_plan_snapshot(store, session_id)?;
        let task_id = Self::normalize_plan_task_reference(&snapshot, task_id);
        let depends_on =
            depends_on.map(|items| Self::normalize_plan_task_references(&snapshot, items));
        let parent_task_id =
            Self::normalize_optional_plan_task_reference(&snapshot, parent_task_id);
        let task = snapshot
            .edit_task(
                &task_id,
                description,
                depends_on,
                parent_task_id,
                clear_parent_task,
                now,
            )
            .map_err(Self::invalid_plan_tool)?;
        self.persist_plan_snapshot(store, &snapshot)?;
        Ok(EditTaskOutput { task })
    }

    fn plan_snapshot_output(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<PlanSnapshotOutput, ExecutionError> {
        let snapshot = self.load_plan_snapshot(store, session_id)?;
        Ok(PlanSnapshotOutput {
            goal: snapshot.goal,
            items: snapshot.items,
        })
    }

    fn lint_plan_snapshot(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<PlanLintOutput, ExecutionError> {
        let snapshot = self.load_plan_snapshot(store, session_id)?;
        let issues = snapshot.lint();
        Ok(PlanLintOutput {
            ok: issues.is_empty(),
            issues,
        })
    }

    fn load_plan_snapshot(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<PlanSnapshot, ExecutionError> {
        let snapshot = store
            .get_plan(session_id)
            .map_err(ExecutionError::Store)?
            .map(PlanSnapshot::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)?
            .unwrap_or_else(|| PlanSnapshot {
                session_id: session_id.to_string(),
                goal: None,
                items: Vec::new(),
                updated_at: 0,
            });
        Ok(snapshot)
    }

    fn persist_plan_snapshot(
        &self,
        store: &PersistenceStore,
        snapshot: &PlanSnapshot,
    ) -> Result<(), ExecutionError> {
        let record = PlanRecord::try_from(snapshot).map_err(ExecutionError::RecordConversion)?;
        store.put_plan(&record).map_err(ExecutionError::Store)
    }

    fn invalid_plan_tool(source: impl std::fmt::Display) -> ExecutionError {
        ExecutionError::Tool(ToolError::InvalidPlanWrite {
            reason: source.to_string(),
        })
    }

    fn normalize_plan_task_references(
        snapshot: &PlanSnapshot,
        references: Vec<String>,
    ) -> Vec<String> {
        references
            .into_iter()
            .map(|reference| Self::normalize_plan_task_reference(snapshot, &reference))
            .collect()
    }

    fn normalize_optional_plan_task_reference(
        snapshot: &PlanSnapshot,
        reference: Option<String>,
    ) -> Option<String> {
        reference.map(|reference| Self::normalize_plan_task_reference(snapshot, &reference))
    }

    fn normalize_plan_task_reference(snapshot: &PlanSnapshot, reference: &str) -> String {
        let trimmed = reference.trim();
        if trimmed.is_empty() || snapshot.task(trimmed).is_some() {
            return trimmed.to_string();
        }

        let Some(index) = Self::plan_task_ordinal_index(trimmed) else {
            return trimmed.to_string();
        };

        snapshot
            .items
            .get(index)
            .map(|item| item.id.clone())
            .unwrap_or_else(|| trimmed.to_string())
    }

    fn plan_task_ordinal_index(reference: &str) -> Option<usize> {
        let normalized = reference.trim().to_ascii_lowercase();
        let ordinal = normalized.parse::<usize>().ok().or_else(|| {
            ["task-", "task_", "task ", "задача-", "задача_", "задача "]
                .into_iter()
                .find_map(|prefix| {
                    normalized
                        .strip_prefix(prefix)?
                        .trim()
                        .parse::<usize>()
                        .ok()
                })
        })?;
        ordinal.checked_sub(1)
    }

    pub(super) fn completion_gate_decision(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        run: &RunEngine,
        response: &ProviderResponse,
    ) -> Result<Option<CompletionGateDecision>, ExecutionError> {
        let session = Session::try_from(
            store
                .get_session(session_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingSession {
                    id: session_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        let Some(max_completion_nudges) = session.settings.completion_nudges else {
            return Ok(None);
        };

        if !run
            .snapshot()
            .recent_steps
            .iter()
            .any(|step| matches!(step.kind, RunStepKind::ToolCompleted))
        {
            return Ok(None);
        }

        let snapshot = self.load_plan_snapshot(store, session_id)?;
        let unfinished = snapshot
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item.status,
                    PlanItemStatus::Pending | PlanItemStatus::InProgress
                )
            })
            .collect::<Vec<_>>();
        if unfinished.is_empty() {
            return Ok(None);
        }

        Ok(Some(CompletionGateDecision {
            max_completion_nudges: max_completion_nudges as usize,
            nudge_message: self.build_completion_nudge_message(&snapshot, &unfinished, response),
        }))
    }

    fn session_auto_approve_enabled(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<bool, ExecutionError> {
        let session = Session::try_from(
            store
                .get_session(session_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingSession {
                    id: session_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        Ok(session.settings.auto_approve)
    }

    fn session_think_level(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<Option<String>, ExecutionError> {
        let session = Session::try_from(
            store
                .get_session(session_id)
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| ExecutionError::MissingSession {
                    id: session_id.to_string(),
                })?,
        )
        .map_err(ExecutionError::RecordConversion)?;
        Ok(session.settings.think_level)
    }

    fn build_completion_nudge_message(
        &self,
        snapshot: &PlanSnapshot,
        unfinished: &[&PlanItem],
        response: &ProviderResponse,
    ) -> String {
        let remaining = unfinished
            .iter()
            .take(3)
            .map(|item| format!("{} [{}]: {}", item.id, item.status.as_str(), item.content))
            .collect::<Vec<_>>()
            .join("; ");
        let remaining_suffix = if unfinished.len() > 3 {
            format!("; и ещё {} задач", unfinished.len() - 3)
        } else {
            String::new()
        };
        let goal = snapshot
            .goal
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!(" Цель плана: {value}."))
            .unwrap_or_default();
        let prior_reply = prompting::preview_text(response.output_text.trim(), 180);
        let prior_reply_suffix = if prior_reply.is_empty() {
            String::new()
        } else {
            format!(" Твой предыдущий ответ был промежуточным: {prior_reply}")
        };
        format!(
            "Ты остановился раньше времени.{goal} В плане остались незавершённые задачи: {remaining}{remaining_suffix}.{prior_reply_suffix} Не заканчивай ход промежуточным резюме. Продолжай работу в этой же сессии: вызывай нужные tools, обновляй план и заверши только когда задачи будут действительно доведены до конца или если нужен approval, blocker или background handoff."
        )
    }

    pub(super) fn completion_continuation_messages(
        &self,
        supports_previous_response_id: bool,
        response: &ProviderResponse,
        nudge_message: &str,
    ) -> Vec<ProviderMessage> {
        let mut messages = Vec::new();
        if !supports_previous_response_id && !response.output_text.trim().is_empty() {
            messages.push(ProviderMessage::new(
                MessageRole::Assistant,
                response.output_text.trim(),
            ));
        }
        messages.push(ProviderMessage::new(MessageRole::System, nudge_message));
        messages
    }

    fn empty_response_continuation_messages(
        &self,
        supports_previous_response_id: bool,
    ) -> Vec<ProviderMessage> {
        let mut messages = Vec::new();
        if !supports_previous_response_id {
            messages.push(ProviderMessage::new(
                MessageRole::Assistant,
                "Предыдущий ответ после результатов tools оказался пустым.",
            ));
        }
        messages.push(ProviderMessage::new(
            MessageRole::System,
            "Твой предыдущий ответ после результатов tools оказался пустым. Не возвращай пустой ответ. Продолжай работу в этой же сессии: либо вызови следующий нужный tool, либо дай обычный assistant reply с конкретным продолжением.",
        ));
        messages
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_model_tool_output(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        tool_call_id: &str,
        parsed: &ToolCall,
        output: &ToolOutput,
        inline_output: String,
        now: i64,
    ) -> Result<String, ExecutionError> {
        let Some((label, summary, payload_bytes, compact_output)) =
            self.offloadable_tool_output(parsed, output)?
        else {
            return Ok(inline_output);
        };
        let payload_text = String::from_utf8_lossy(&payload_bytes).to_string();
        let token_estimate = approximate_token_count(&payload_text);

        if token_estimate <= INLINE_TOOL_OUTPUT_TOKEN_LIMIT {
            return Ok(inline_output);
        }

        let mut snapshot = store
            .get_context_offload(session_id)
            .map_err(ExecutionError::Store)?
            .map(ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)?
            .unwrap_or_else(|| ContextOffloadSnapshot {
                session_id: session_id.to_string(),
                refs: Vec::new(),
                updated_at: 0,
            });

        let normalized_id = sanitize_identifier(tool_call_id);
        let artifact_id = format!("artifact-tool-offload-{session_id}-{normalized_id}");
        let ref_id = format!("tool-offload-{normalized_id}");
        let current_ref = ContextOffloadRef {
            id: ref_id.clone(),
            label,
            summary,
            artifact_id: artifact_id.clone(),
            token_estimate,
            message_count: 1,
            created_at: now,
        };
        snapshot.refs.push(current_ref.clone());
        snapshot.refs.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        snapshot.refs.truncate(MAX_CONTEXT_OFFLOAD_REFS);
        snapshot.updated_at = now;

        let mut retained_refs = Vec::with_capacity(snapshot.refs.len());
        let mut payloads = Vec::with_capacity(snapshot.refs.len());
        for reference in &snapshot.refs {
            if reference.artifact_id == artifact_id {
                retained_refs.push(reference.clone());
                payloads.push(ContextOffloadPayload {
                    artifact_id: artifact_id.clone(),
                    bytes: payload_bytes.clone(),
                });
                continue;
            }

            if let Some(payload) = self
                .load_context_offload_payload_for_refresh(store, reference.artifact_id.as_str())?
            {
                retained_refs.push(reference.clone());
                payloads.push(payload);
            }
        }
        snapshot.refs = retained_refs;

        let snapshot_record = agent_persistence::ContextOffloadRecord::try_from(&snapshot)
            .map_err(ExecutionError::RecordConversion)?;
        match store.put_context_offload(&snapshot_record, &payloads) {
            Ok(()) => {}
            Err(source) if Self::is_stale_context_offload_payload_error(&source) => {
                let fallback_snapshot = ContextOffloadSnapshot {
                    session_id: session_id.to_string(),
                    refs: vec![current_ref],
                    updated_at: now,
                };
                let fallback_payloads = vec![ContextOffloadPayload {
                    artifact_id: artifact_id.clone(),
                    bytes: payload_bytes,
                }];
                store
                    .put_context_offload(
                        &agent_persistence::ContextOffloadRecord::try_from(&fallback_snapshot)
                            .map_err(ExecutionError::RecordConversion)?,
                        &fallback_payloads,
                    )
                    .map_err(ExecutionError::Store)?;
            }
            Err(source) => return Err(ExecutionError::Store(source)),
        }

        Ok(compact_output
            .replace("__ARTIFACT_ID__", artifact_id.as_str())
            .replace("__REF_ID__", ref_id.as_str()))
    }

    fn offloadable_tool_output(
        &self,
        _parsed: &ToolCall,
        output: &ToolOutput,
    ) -> Result<Option<OffloadableToolOutput>, ExecutionError> {
        match output {
            ToolOutput::FsReadText(result) => {
                let payload = output.model_output().into_bytes();
                let preview = prompting::preview_text(result.content.as_str(), 240);
                Ok(Some((
                    format!("fs_read_text {}", result.path),
                    format!("Large file read from {}", result.path),
                    payload,
                    serde_json::json!({
                        "tool": "fs_read_text",
                        "path": result.path,
                        "offloaded": true,
                        "artifact_id": "__ARTIFACT_ID__",
                        "ref_id": "__REF_ID__",
                        "summary": format!("Large file read from {}", result.path),
                        "preview": preview,
                    })
                    .to_string(),
                )))
            }
            ToolOutput::FsReadLines(result) => {
                let payload = output.model_output().into_bytes();
                let preview = prompting::preview_text(result.content.as_str(), 240);
                Ok(Some((
                    format!("fs_read_lines {}", result.path),
                    format!(
                        "Large line-range read from {} ({}-{})",
                        result.path, result.start_line, result.end_line
                    ),
                    payload,
                    serde_json::json!({
                        "tool": "fs_read_lines",
                        "path": result.path,
                        "start_line": result.start_line,
                        "end_line": result.end_line,
                        "total_lines": result.total_lines,
                        "eof": result.eof,
                        "next_start_line": result.next_start_line,
                        "offloaded": true,
                        "artifact_id": "__ARTIFACT_ID__",
                        "ref_id": "__REF_ID__",
                        "summary": format!("Large line-range read from {} ({}-{})", result.path, result.start_line, result.end_line),
                        "preview": preview,
                    })
                    .to_string(),
                )))
            }
            ToolOutput::FsFindInFiles(result) => {
                let payload = output.model_output().into_bytes();
                let preview_matches = result
                    .matches
                    .iter()
                    .take(INLINE_FIND_IN_FILES_PREVIEW_LIMIT)
                    .map(|entry| {
                        serde_json::json!({
                            "path": entry.path,
                            "line_number": entry.line_number,
                            "line": entry.line,
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(Some((
                    "fs_find_in_files workspace search".to_string(),
                    format!("Large multi-file search result with {} matches", result.matches.len()),
                    payload,
                    serde_json::json!({
                        "tool": "fs_find_in_files",
                        "offloaded": true,
                        "artifact_id": "__ARTIFACT_ID__",
                        "ref_id": "__REF_ID__",
                        "summary": format!("Large multi-file search result with {} matches", result.matches.len()),
                        "match_count": result.matches.len(),
                        "preview_matches": preview_matches,
                    })
                    .to_string(),
                )))
            }
            ToolOutput::WebFetch(result) => {
                let payload = output.model_output().into_bytes();
                let preview = prompting::preview_text(result.body.as_str(), 240);
                let summary = if result.extracted_from_html {
                    format!("Large readable web fetch from {}", result.url)
                } else {
                    format!("Large web fetch response from {}", result.url)
                };
                Ok(Some((
                    format!("web_fetch {}", result.url),
                    summary.clone(),
                    payload,
                    serde_json::json!({
                        "tool": "web_fetch",
                        "url": result.url,
                        "status_code": result.status_code,
                        "content_type": result.content_type,
                        "title": result.title,
                        "extracted_from_html": result.extracted_from_html,
                        "offloaded": true,
                        "artifact_id": "__ARTIFACT_ID__",
                        "ref_id": "__REF_ID__",
                        "summary": summary,
                        "preview": preview,
                    })
                    .to_string(),
                )))
            }
            ToolOutput::ProcessResult(result) => {
                let payload = output.model_output().into_bytes();
                let stdout_preview = prompting::preview_text(result.stdout.as_str(), 180);
                let stderr_preview = prompting::preview_text(result.stderr.as_str(), 180);
                Ok(Some((
                    format!("exec_wait {}", result.process_id),
                    format!(
                        "Large process output for {} (exit_code={:?})",
                        result.process_id, result.exit_code
                    ),
                    payload,
                    serde_json::json!({
                        "tool": "process_result",
                        "process_id": result.process_id,
                        "status": format!("{:?}", result.status).to_lowercase(),
                        "exit_code": result.exit_code,
                        "offloaded": true,
                        "artifact_id": "__ARTIFACT_ID__",
                        "ref_id": "__REF_ID__",
                        "summary": format!("Large process output for {} (exit_code={:?})", result.process_id, result.exit_code),
                        "stdout_preview": stdout_preview,
                        "stderr_preview": stderr_preview,
                    })
                    .to_string(),
                )))
            }
            _ => Ok(None),
        }
    }

    fn read_context_offload_artifact(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        artifact_id: &str,
    ) -> Result<ArtifactReadOutput, ExecutionError> {
        let snapshot = self.require_context_offload_snapshot(store, session_id)?;
        let reference = snapshot
            .refs
            .into_iter()
            .find(|reference| reference.artifact_id == artifact_id)
            .ok_or_else(|| {
                ExecutionError::Tool(ToolError::InvalidArtifactTool {
                    reason: format!(
                        "artifact {} is not referenced by the current session offload snapshot",
                        artifact_id
                    ),
                })
            })?;
        let payload = self.load_context_offload_payload_for_tool(store, artifact_id)?;

        Ok(ArtifactReadOutput {
            ref_id: reference.id,
            artifact_id: reference.artifact_id,
            label: reference.label,
            summary: reference.summary,
            content: String::from_utf8_lossy(&payload.bytes).to_string(),
        })
    }

    fn search_context_offload_artifacts(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<ArtifactSearchOutput, ExecutionError> {
        let snapshot = self.require_context_offload_snapshot(store, session_id)?;
        let query = query.trim();
        if query.is_empty() {
            return Err(ExecutionError::Tool(ToolError::InvalidArtifactTool {
                reason: "artifact_search query must not be empty".to_string(),
            }));
        }
        let normalized_query = query.to_ascii_lowercase();
        let mut results = Vec::new();
        let effective_limit = limit.max(1);

        for reference in snapshot.refs {
            let payload =
                self.load_context_offload_payload_for_tool(store, reference.artifact_id.as_str())?;
            let content = String::from_utf8_lossy(&payload.bytes).to_string();
            let haystack = format!(
                "{}\n{}\n{}\n{}",
                reference.artifact_id, reference.label, reference.summary, content
            )
            .to_ascii_lowercase();
            if !haystack.contains(&normalized_query) {
                continue;
            }

            results.push(ArtifactSearchResult {
                ref_id: reference.id,
                artifact_id: reference.artifact_id,
                label: reference.label,
                summary: reference.summary,
                token_estimate: reference.token_estimate,
                message_count: reference.message_count,
                preview: prompting::preview_text(&content, 240),
            });
            if results.len() >= effective_limit {
                break;
            }
        }

        Ok(ArtifactSearchOutput {
            query: query.to_string(),
            results,
        })
    }

    fn require_context_offload_snapshot(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<ContextOffloadSnapshot, ExecutionError> {
        store
            .get_context_offload(session_id)
            .map_err(ExecutionError::Store)?
            .map(ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)?
            .filter(|snapshot| !snapshot.is_empty())
            .ok_or_else(|| {
                ExecutionError::Tool(ToolError::InvalidArtifactTool {
                    reason: "the current session has no offloaded context".to_string(),
                })
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_provider_turn_loop(
        &self,
        store: &PersistenceStore,
        provider: &dyn ProviderDriver,
        session_id: &str,
        model: Option<String>,
        instructions: Option<String>,
        run: &mut RunEngine,
        initial_loop_state: Option<ProviderLoopState>,
        auto_approve_override: Option<bool>,
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
    ) -> Result<ProviderResponse, ExecutionError> {
        if initial_loop_state.is_none()
            && let Some(decision) = self.auto_compaction_decision(
                store,
                provider,
                session_id,
                model.as_deref(),
                instructions.as_deref(),
            )?
            && self.compact_session_at(store, provider, session_id, now)?
        {
            run.record_system_note(
                format!(
                    "automatic context compaction triggered before provider turn: estimated_prompt_tokens={} threshold_tokens={} context_window_tokens={} ratio={:.2}",
                    decision.estimated_prompt_tokens,
                    decision.trigger_threshold_tokens,
                    decision.context_window_tokens,
                    self.config.context_auto_compaction_trigger_ratio
                ),
                now,
            )
            .map_err(ExecutionError::RunTransition)?;
            self.persist_run(store, run)?;
        }

        let workspace = self.load_session_workspace(store, session_id)?;
        let prompt_messages = self.prompt_messages(
            store,
            Some(provider),
            session_id,
            &workspace,
            model.as_deref(),
            instructions.as_deref(),
        )?;
        let catalog = ToolCatalog::default();
        let agent_profile = self.load_agent_profile_for_session(store, session_id)?;
        let tools = self.automatic_provider_tools(
            provider,
            prompt_messages.context_offload.as_ref(),
            &agent_profile,
        );
        let mut tool_runtime = self.tool_runtime_for_workspace(workspace);
        let auto_approve =
            auto_approve_override.unwrap_or(self.session_auto_approve_enabled(store, session_id)?);
        let think_level = self.session_think_level(store, session_id)?;
        let mut cursor = ProviderLoopCursor::new(
            provider,
            initial_loop_state,
            self.config.provider_max_tool_rounds,
        );

        loop {
            if self.run_was_cancelled_by_operator(store, run.snapshot().id.as_str())? {
                return Err(ExecutionError::CancelledByOperator);
            }
            if !cursor.has_round_budget() {
                if auto_approve {
                    cursor.reset_round_budget();
                    Self::emit_event(
                        observer,
                        ChatExecutionEvent::ProviderLoopProgress {
                            current_round: 1,
                            max_rounds: cursor.max_rounds,
                        },
                    );
                    continue;
                }
                let approval_id = format!(
                    "approval-{}-provider-loop-r{}-{}",
                    run.snapshot().id,
                    cursor.round,
                    run.snapshot().recent_steps.len()
                );
                let reason = format!(
                    "provider reached the tool-calling limit ({}/{}) before producing a final answer; approve to reset the tool round budget and continue",
                    cursor.round, cursor.max_rounds
                );
                let approval_state = cursor.loop_reset_approval_state(&approval_id);
                run.wait_for_approval(
                    ApprovalRequest::new(approval_id.clone(), "provider-loop", &reason, now),
                    now,
                )
                .map_err(ExecutionError::RunTransition)?;
                run.set_provider_loop_state(approval_state, now)
                    .map_err(ExecutionError::RunTransition)?;
                self.persist_run(store, run)?;
                return Err(ExecutionError::ApprovalRequired {
                    tool: "provider_loop".to_string(),
                    approval_id,
                    reason,
                });
            }
            let request = cursor.build_request(
                &prompt_messages.messages,
                model.as_deref(),
                instructions.as_deref(),
                think_level.as_deref(),
                &tools,
                cursor.stream_mode(observer.is_some()),
                self.config.provider_max_output_tokens,
            );
            let observed = match self.request_provider_response_with_retries(
                store, run, provider, &request, now, observer,
            ) {
                Ok(observed) => observed,
                Err(ExecutionError::Provider(ProviderError::ResponseMissingOutputText))
                    if cursor.can_recover_from_empty_response() =>
                {
                    cursor.record_empty_response_recovery();
                    run.record_system_note(
                        format!(
                            "provider returned an empty response after tool results; retrying with explicit continuation ({}/{})",
                            cursor.empty_response_recoveries_used,
                            MAX_EMPTY_RESPONSE_RECOVERIES
                        ),
                        now,
                    )
                    .map_err(ExecutionError::RunTransition)?;
                    cursor.queue_post_tool_continuation_messages(
                        self.empty_response_continuation_messages(
                            cursor.supports_previous_response_id,
                        ),
                    );
                    run.set_provider_loop_state(cursor.persistent_state(None), now)
                        .map_err(ExecutionError::RunTransition)?;
                    self.persist_run(store, run)?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            cursor.clear_continuation_input_messages();
            self.apply_provider_response(run, &observed, now)?;
            self.persist_run(store, run)?;
            let response = observed.response;

            if response.tool_calls.is_empty() {
                if let Some(decision) =
                    self.completion_gate_decision(store, session_id, run, &response)?
                {
                    if cursor.completion_nudges_used < decision.max_completion_nudges {
                        cursor.record_completion_nudge();
                        cursor.adopt_response_anchor(&response);
                        cursor.queue_continuation_input_messages(
                            self.completion_continuation_messages(
                                cursor.supports_previous_response_id,
                                &response,
                                decision.nudge_message.as_str(),
                            ),
                        );
                        run.set_provider_loop_state(cursor.persistent_state(None), now)
                            .map_err(ExecutionError::RunTransition)?;
                        self.persist_run(store, run)?;
                        continue;
                    }

                    if auto_approve {
                        cursor.adopt_response_anchor(&response);
                        cursor.queue_continuation_input_messages(
                            self.completion_continuation_messages(
                                cursor.supports_previous_response_id,
                                &response,
                                decision.nudge_message.as_str(),
                            ),
                        );
                        run.set_provider_loop_state(cursor.persistent_state(None), now)
                            .map_err(ExecutionError::RunTransition)?;
                        self.persist_run(store, run)?;
                        continue;
                    }

                    let approval_id = format!(
                        "approval-{}-completion-{}-{}",
                        run.snapshot().id,
                        cursor.completion_nudges_used,
                        run.snapshot().recent_steps.len()
                    );
                    let reason = format!(
                        "model stopped early with unfinished plan work after {} automatic continuation nudges; approve to continue this run",
                        cursor.completion_nudges_used
                    );
                    cursor.adopt_response_anchor(&response);
                    cursor.queue_continuation_input_messages(
                        self.completion_continuation_messages(
                            cursor.supports_previous_response_id,
                            &response,
                            decision.nudge_message.as_str(),
                        ),
                    );
                    let approval_state = cursor
                        .completion_approval_state(&approval_id, decision.max_completion_nudges);
                    run.wait_for_approval(
                        ApprovalRequest::new(approval_id.clone(), "completion-gate", &reason, now),
                        now,
                    )
                    .map_err(ExecutionError::RunTransition)?;
                    run.set_provider_loop_state(approval_state, now)
                        .map_err(ExecutionError::RunTransition)?;
                    self.persist_run(store, run)?;
                    return Err(ExecutionError::ApprovalRequired {
                        tool: "completion_gate".to_string(),
                        approval_id,
                        reason,
                    });
                }
                return Ok(response);
            }

            Self::emit_event(
                observer,
                ChatExecutionEvent::ProviderLoopProgress {
                    current_round: cursor.round.saturating_add(1),
                    max_rounds: cursor.max_rounds,
                },
            );
            cursor.remember_tool_signature(&response)?;
            cursor.note_assistant_tool_calls(&response);
            cursor.begin_tool_round();
            for tool_call in &response.tool_calls {
                let run_id = run.snapshot().id.clone();
                let (parsed, definition) =
                    match self.resolve_provider_tool_call(&catalog, tool_call) {
                        Ok(resolved) => resolved,
                        Err(ExecutionError::ToolCallParse { reason, .. }) => {
                            Self::record_tool_call_ledger(ToolCallLedgerUpdate {
                                store,
                                session_id,
                                run_id: &run_id,
                                provider_tool_call_id: &tool_call.call_id,
                                tool_name: &tool_call.name,
                                arguments_json: &tool_call.arguments,
                                summary: &tool_call.name,
                                status: ToolExecutionStatus::Failed,
                                error: Some(format!("invalid arguments: {reason}")),
                                now,
                            })?;
                            Self::emit_event(
                                observer,
                                ChatExecutionEvent::ToolStatus {
                                    tool_call_id: tool_call.call_id.clone(),
                                    tool_name: tool_call.name.clone(),
                                    summary: format!("invalid arguments: {reason}"),
                                    status: ToolExecutionStatus::Failed,
                                },
                            );
                            run.record_tool_completion(
                                format!("{} invalid arguments: {reason}", tool_call.name),
                                now,
                            )
                            .map_err(ExecutionError::RunTransition)?;
                            cursor.record_tool_output(
                                &tool_call.call_id,
                                Self::invalid_provider_tool_output(&tool_call.name, &reason),
                            );
                            continue;
                        }
                        Err(other) => return Err(other),
                    };
                Self::record_tool_call_ledger(ToolCallLedgerUpdate {
                    store,
                    session_id,
                    run_id: &run_id,
                    provider_tool_call_id: &tool_call.call_id,
                    tool_name: parsed.name().as_str(),
                    arguments_json: &tool_call.arguments,
                    summary: &parsed.summary(),
                    status: ToolExecutionStatus::Requested,
                    error: None,
                    now,
                })?;
                Self::emit_event(
                    observer,
                    ChatExecutionEvent::ToolStatus {
                        tool_call_id: tool_call.call_id.clone(),
                        tool_name: parsed.name().as_str().to_string(),
                        summary: parsed.summary(),
                        status: ToolExecutionStatus::Requested,
                    },
                );
                if let Err(error) = self.ensure_agent_tool_allowed(store, session_id, parsed.name())
                {
                    Self::record_tool_call_ledger(ToolCallLedgerUpdate {
                        store,
                        session_id,
                        run_id: &run_id,
                        provider_tool_call_id: &tool_call.call_id,
                        tool_name: parsed.name().as_str(),
                        arguments_json: &tool_call.arguments,
                        summary: &parsed.summary(),
                        status: ToolExecutionStatus::Failed,
                        error: Some(error.to_string()),
                        now,
                    })?;
                    Self::emit_event(
                        observer,
                        ChatExecutionEvent::ToolStatus {
                            tool_call_id: tool_call.call_id.clone(),
                            tool_name: parsed.name().as_str().to_string(),
                            summary: format!("tool error: {error}"),
                            status: ToolExecutionStatus::Failed,
                        },
                    );
                    run.record_tool_completion(
                        format!("{} tool error: {error}", parsed.summary()),
                        now,
                    )
                    .map_err(ExecutionError::RunTransition)?;
                    if let Some(model_output) =
                        self.recoverable_execution_error_output(&parsed, &error)
                    {
                        cursor.record_tool_output(&tool_call.call_id, model_output);
                        continue;
                    }
                    return Err(error);
                }
                let permission = self.permissions.resolve(&definition, &parsed);

                match permission.action {
                    PermissionAction::Allow => {}
                    PermissionAction::Deny => {
                        let reason = format!(
                            "tool {} denied by permission policy: {}",
                            parsed.name().as_str(),
                            permission.reason
                        );
                        Self::record_tool_call_ledger(ToolCallLedgerUpdate {
                            store,
                            session_id,
                            run_id: &run_id,
                            provider_tool_call_id: &tool_call.call_id,
                            tool_name: parsed.name().as_str(),
                            arguments_json: &tool_call.arguments,
                            summary: &parsed.summary(),
                            status: ToolExecutionStatus::Failed,
                            error: Some(reason.clone()),
                            now,
                        })?;
                        Self::emit_event(
                            observer,
                            ChatExecutionEvent::ToolStatus {
                                tool_call_id: tool_call.call_id.clone(),
                                tool_name: parsed.name().as_str().to_string(),
                                summary: parsed.summary(),
                                status: ToolExecutionStatus::Failed,
                            },
                        );
                        run.record_tool_completion(
                            format!("{} tool error: {reason}", parsed.summary()),
                            now,
                        )
                        .map_err(ExecutionError::RunTransition)?;
                        let model_output = self
                            .recoverable_execution_error_output(
                                &parsed,
                                &ExecutionError::PermissionDenied {
                                    tool: parsed.name().as_str().to_string(),
                                    reason,
                                },
                            )
                            .expect("permission denied should produce model-visible tool output");
                        cursor.record_tool_output(&tool_call.call_id, model_output);
                        if self.run_was_cancelled_by_operator(store, run.snapshot().id.as_str())? {
                            return Err(ExecutionError::CancelledByOperator);
                        }
                        if interrupt_after_tool_step.is_some_and(|flag| flag.load(Ordering::SeqCst))
                        {
                            run.interrupt("superseded by queued user input", now)
                                .map_err(ExecutionError::RunTransition)?;
                            self.persist_run(store, run)?;
                            return Err(ExecutionError::InterruptedByQueuedInput);
                        }
                        continue;
                    }
                    PermissionAction::Ask => {
                        if auto_approve {
                            Self::record_tool_call_ledger(ToolCallLedgerUpdate {
                                store,
                                session_id,
                                run_id: &run_id,
                                provider_tool_call_id: &tool_call.call_id,
                                tool_name: parsed.name().as_str(),
                                arguments_json: &tool_call.arguments,
                                summary: &parsed.summary(),
                                status: ToolExecutionStatus::Approved,
                                error: None,
                                now,
                            })?;
                            Self::emit_event(
                                observer,
                                ChatExecutionEvent::ToolStatus {
                                    tool_call_id: tool_call.call_id.clone(),
                                    tool_name: parsed.name().as_str().to_string(),
                                    summary: parsed.summary(),
                                    status: ToolExecutionStatus::Approved,
                                },
                            );
                        } else {
                            let approval_id = format!(
                                "approval-{}-{}",
                                run.snapshot().id,
                                parsed.name().as_str()
                            );
                            let reason = format!(
                                "tool {} requires approval: {} ({})",
                                parsed.name().as_str(),
                                parsed.summary(),
                                permission.reason
                            );
                            Self::record_tool_call_ledger(ToolCallLedgerUpdate {
                                store,
                                session_id,
                                run_id: &run_id,
                                provider_tool_call_id: &tool_call.call_id,
                                tool_name: parsed.name().as_str(),
                                arguments_json: &tool_call.arguments,
                                summary: &parsed.summary(),
                                status: ToolExecutionStatus::WaitingApproval,
                                error: Some(reason.clone()),
                                now,
                            })?;
                            Self::emit_event(
                                observer,
                                ChatExecutionEvent::ToolStatus {
                                    tool_call_id: tool_call.call_id.clone(),
                                    tool_name: parsed.name().as_str().to_string(),
                                    summary: parsed.summary(),
                                    status: ToolExecutionStatus::WaitingApproval,
                                },
                            );
                            let approval_state = cursor.pending_approval_state(
                                &response,
                                tool_call,
                                &parsed,
                                &approval_id,
                            );
                            run.wait_for_approval(
                                ApprovalRequest::new(
                                    approval_id.clone(),
                                    tool_call.call_id.as_str(),
                                    &reason,
                                    now,
                                ),
                                now,
                            )
                            .map_err(ExecutionError::RunTransition)?;
                            run.set_provider_loop_state(approval_state, now)
                                .map_err(ExecutionError::RunTransition)?;
                            self.persist_run(store, run)?;
                            return Err(ExecutionError::ApprovalRequired {
                                tool: parsed.name().as_str().to_string(),
                                approval_id,
                                reason,
                            });
                        }
                    }
                }

                let model_output = match self.invoke_provider_tool_call(
                    ProviderToolExecutionContext {
                        store,
                        provider,
                        session_id,
                        run_id: &run_id,
                        now,
                    },
                    run,
                    &mut tool_runtime,
                    ProviderToolCallInvocation {
                        tool_call_id: &tool_call.call_id,
                        arguments_json: &tool_call.arguments,
                        parsed: &parsed,
                    },
                    observer,
                ) {
                    Ok(model_output) => model_output,
                    Err(error) => {
                        if let Some(model_output) =
                            self.recoverable_execution_error_output(&parsed, &error)
                        {
                            Self::emit_event(
                                observer,
                                ChatExecutionEvent::ToolStatus {
                                    tool_call_id: tool_call.call_id.clone(),
                                    tool_name: parsed.name().as_str().to_string(),
                                    summary: format!("tool error: {error}"),
                                    status: ToolExecutionStatus::Failed,
                                },
                            );
                            run.record_tool_completion(
                                format!("{} tool error: {error}", parsed.summary()),
                                now,
                            )
                            .map_err(ExecutionError::RunTransition)?;
                            model_output
                        } else {
                            run.record_tool_completion(
                                format!("{} failed: {error}", parsed.summary()),
                                now,
                            )
                            .map_err(ExecutionError::RunTransition)?;
                            return Err(error);
                        }
                    }
                };
                if self.run_was_cancelled_by_operator(store, run.snapshot().id.as_str())? {
                    return Err(ExecutionError::CancelledByOperator);
                }
                cursor.record_tool_output(&tool_call.call_id, model_output);
                if interrupt_after_tool_step.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
                    run.interrupt("superseded by queued user input", now)
                        .map_err(ExecutionError::RunTransition)?;
                    self.persist_run(store, run)?;
                    return Err(ExecutionError::InterruptedByQueuedInput);
                }
            }

            cursor.advance_after_response(&response);
            self.persist_run(store, run)?;
        }
    }
}

fn sanitize_identifier(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = normalized
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if compact.is_empty() {
        "artifact".to_string()
    } else {
        compact
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_runtime::plan::PlanItemStatus;
    use agent_runtime::provider::{
        FinishReason, ModelCapabilities, ProviderDescriptor, ProviderError, ProviderRequest,
        ProviderResponse, ProviderResponseStream, ProviderToolCall,
    };
    use std::io;

    struct TestProviderDriver {
        descriptor: ProviderDescriptor,
    }

    impl ProviderDriver for TestProviderDriver {
        fn descriptor(&self) -> &ProviderDescriptor {
            &self.descriptor
        }

        fn complete(&self, _request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
            unreachable!("provider loop cursor tests do not call complete")
        }

        fn stream(
            &self,
            _request: &ProviderRequest,
        ) -> Result<Box<dyn ProviderResponseStream>, ProviderError> {
            unreachable!("provider loop cursor tests do not call stream")
        }
    }

    fn provider() -> TestProviderDriver {
        TestProviderDriver {
            descriptor: ProviderDescriptor {
                name: "test-provider".to_string(),
                model_family: "test".to_string(),
                default_model: Some("test-model".to_string()),
                capabilities: ModelCapabilities {
                    supports_streaming: true,
                    supports_text_input: true,
                    supports_tool_calls: true,
                    supports_previous_response_id: true,
                    supports_reasoning_summaries: true,
                },
            },
        }
    }

    fn response_with_tool_call(name: &str, arguments: &str) -> ProviderResponse {
        ProviderResponse {
            response_id: "resp-test".to_string(),
            model: "test-model".to_string(),
            output_text: String::new(),
            tool_calls: vec![ProviderToolCall {
                call_id: "call-1".to_string(),
                name: name.to_string(),
                arguments: arguments.to_string(),
            }],
            finish_reason: FinishReason::Completed,
            usage: None,
        }
    }

    #[test]
    fn build_request_omits_max_output_tokens_when_not_configured() {
        let provider = provider();
        let cursor = ProviderLoopCursor::new(&provider, None, 24);

        let request = cursor.build_request(
            &[ProviderMessage::new(MessageRole::User, "hello")],
            Some("test-model"),
            None,
            None,
            &[],
            ProviderStreamMode::Disabled,
            None,
        );

        assert_eq!(request.max_output_tokens, None);
    }

    #[test]
    fn build_request_uses_configured_max_output_tokens() {
        let provider = provider();
        let cursor = ProviderLoopCursor::new(&provider, None, 24);

        let request = cursor.build_request(
            &[ProviderMessage::new(MessageRole::User, "hello")],
            Some("test-model"),
            None,
            None,
            &[],
            ProviderStreamMode::Disabled,
            Some(8192),
        );

        assert_eq!(request.max_output_tokens, Some(8192));
    }

    #[test]
    fn remember_tool_signature_allows_repeated_session_wait_polling() {
        let provider = provider();
        let mut cursor = ProviderLoopCursor::new(&provider, None, 24);
        let response = response_with_tool_call(
            ToolName::SessionWait.as_str(),
            r#"{"max_bytes":8000,"max_items":10,"mode":"transcript","session_id":"session-agentmsg-1","wait_timeout_ms":0}"#,
        );

        cursor
            .remember_tool_signature(&response)
            .expect("first repeated session_wait should be tracked");
        cursor
            .remember_tool_signature(&response)
            .expect("second repeated session_wait should be tracked");
        cursor
            .remember_tool_signature(&response)
            .expect("third repeated session_wait should be allowed");
    }

    #[test]
    fn recoverable_tool_error_output_treats_process_io_as_retryable() {
        let service = ExecutionService::default();
        let parsed = ToolCall::ExecStart(agent_runtime::tool::ExecStartInput {
            executable: "missing-binary".to_string(),
            args: Vec::new(),
            cwd: None,
        });

        let output = service
            .recoverable_tool_error_output(
                &parsed,
                &ToolError::ProcessIo {
                    process_id: "missing-binary".to_string(),
                    source: io::Error::new(io::ErrorKind::NotFound, "No such file or directory"),
                },
            )
            .expect("recoverable output");

        assert!(output.contains("\"retryable\":true"));
        assert!(output.contains("missing-binary"));
        assert!(output.contains("No such file or directory"));
    }

    #[test]
    fn recoverable_tool_error_output_treats_invalid_workspace_paths_as_retryable() {
        let service = ExecutionService::default();
        let parsed = ToolCall::FsReadText(agent_runtime::tool::FsReadTextInput {
            path: "/home/user/.twcrc".to_string(),
        });

        let output = service
            .recoverable_tool_error_output(
                &parsed,
                &ToolError::Workspace(agent_runtime::workspace::WorkspaceError::InvalidPath {
                    path: "/home/user/.twcrc".to_string(),
                    reason: "must be relative to the workspace root",
                }),
            )
            .expect("recoverable output");

        assert!(output.contains("\"retryable\":true"));
        assert!(output.contains("/home/user/.twcrc"));
        assert!(output.contains("workspace_relative_only"));
    }

    #[test]
    fn record_tool_call_result_offloads_large_outputs_to_artifact() {
        use agent_persistence::{
            ArtifactRepository, RunRecord, RunRepository, SessionRecord, SessionRepository,
            ToolCallRepository,
        };

        let temp = tempfile::tempdir().expect("tempdir");
        let scaffold =
            agent_persistence::PersistenceScaffold::from_config(agent_persistence::AppConfig {
                data_dir: temp.path().join("state-root"),
                ..agent_persistence::AppConfig::default()
            });
        let store = PersistenceStore::open(&scaffold).expect("open store");
        store
            .put_session(&SessionRecord {
                id: "session-1".to_string(),
                title: "Tool output".to_string(),
                prompt_override: None,
                settings_json: "{}".to_string(),
                workspace_root: std::env::current_dir()
                    .expect("current dir")
                    .display()
                    .to_string(),
                agent_profile_id: "default".to_string(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: None,
                created_at: 1,
                updated_at: 1,
            })
            .expect("put session");
        store
            .put_run(&RunRecord {
                id: "run-1".to_string(),
                session_id: "session-1".to_string(),
                mission_id: None,
                status: "running".to_string(),
                error: None,
                result: None,
                provider_usage_json: "null".to_string(),
                active_processes_json: "[]".to_string(),
                recent_steps_json: "[]".to_string(),
                evidence_refs_json: "[]".to_string(),
                pending_approvals_json: "[]".to_string(),
                provider_loop_json: "null".to_string(),
                delegate_runs_json: "[]".to_string(),
                started_at: 2,
                updated_at: 2,
                finished_at: None,
            })
            .expect("put run");
        ExecutionService::record_tool_call_ledger(ToolCallLedgerUpdate {
            store: &store,
            session_id: "session-1",
            run_id: "run-1",
            provider_tool_call_id: "provider-call-1",
            tool_name: "exec_wait",
            arguments_json: "{\"process_id\":\"exec-1\"}",
            summary: "exec_wait process_id=exec-1",
            status: ToolExecutionStatus::Completed,
            error: None,
            now: 3,
        })
        .expect("record ledger");

        let output = "x".repeat(TOOL_RESULT_PREVIEW_CHAR_LIMIT + 32);
        ExecutionService::record_tool_call_result(ToolCallResultLedgerUpdate {
            store: &store,
            session_id: "session-1",
            run_id: "run-1",
            provider_tool_call_id: "provider-call-1",
            tool_name: "exec_wait",
            result_summary: "process_result process_id=exec-1 status=exited exit_code=Some(0)",
            result_output: &output,
            now: 4,
        })
        .expect("record result");

        let call = store
            .get_tool_call("toolcall-run-1-provider-call-1")
            .expect("get tool call")
            .expect("tool call exists");
        assert!(call.result_truncated);
        assert!(
            call.result_preview
                .as_deref()
                .is_some_and(|preview| preview.contains("<truncated"))
        );
        let artifact_id = call.result_artifact_id.expect("artifact id");
        let artifact = store
            .get_artifact(&artifact_id)
            .expect("get artifact")
            .expect("artifact exists");
        assert_eq!(artifact.kind, "tool_output");
        assert_eq!(artifact.bytes, output.as_bytes());
    }

    #[test]
    fn normalize_plan_task_reference_maps_task_ordinals_to_existing_ids() {
        let snapshot = PlanSnapshot {
            session_id: "session-1".to_string(),
            goal: Some("goal".to_string()),
            items: vec![
                PlanItem {
                    id: "python-pip".to_string(),
                    content: "Detect Python and pip".to_string(),
                    status: PlanItemStatus::Completed,
                    depends_on: Vec::new(),
                    notes: Vec::new(),
                    blocked_reason: None,
                    parent_task_id: None,
                },
                PlanItem {
                    id: "ansible-pip".to_string(),
                    content: "Install Ansible".to_string(),
                    status: PlanItemStatus::Pending,
                    depends_on: vec!["python-pip".to_string()],
                    notes: Vec::new(),
                    blocked_reason: None,
                    parent_task_id: None,
                },
            ],
            updated_at: 0,
        };

        assert_eq!(
            ExecutionService::normalize_plan_task_reference(&snapshot, "task-1"),
            "python-pip"
        );
        assert_eq!(
            ExecutionService::normalize_plan_task_reference(&snapshot, "2"),
            "ansible-pip"
        );
        assert_eq!(
            ExecutionService::normalize_plan_task_reference(&snapshot, "ansible-pip"),
            "ansible-pip"
        );
    }

    #[test]
    fn recoverable_tool_error_output_treats_invalid_plan_references_as_retryable() {
        let service = ExecutionService::default();
        let parsed = ToolCall::AddTask(agent_runtime::tool::AddTaskInput {
            description: "Install Ansible".to_string(),
            depends_on: vec!["task-1".to_string()],
            parent_task_id: None,
        });

        let output = service
            .recoverable_tool_error_output(
                &parsed,
                &ToolError::InvalidPlanWrite {
                    reason: "unknown dependency task-1".to_string(),
                },
            )
            .expect("recoverable output");

        assert!(output.contains("\"retryable\":true"));
        assert!(output.contains("unknown dependency task-1"));
        assert!(output.contains("canonical task_id values"));
    }

    #[test]
    fn recoverable_tool_error_output_treats_missing_workspace_paths_as_retryable() {
        let service = ExecutionService::default();
        let parsed = ToolCall::FsReadText(agent_runtime::tool::FsReadTextInput {
            path: "references/govc-guide.md".to_string(),
        });

        let output = service
            .recoverable_tool_error_output(
                &parsed,
                &ToolError::Workspace(agent_runtime::workspace::WorkspaceError::Io {
                    path: std::path::PathBuf::from("./references/govc-guide.md"),
                    source: std::io::Error::from(std::io::ErrorKind::NotFound),
                }),
            )
            .expect("recoverable output");

        assert!(output.contains("\"retryable\":true"));
        assert!(output.contains("workspace path not found"));
        assert!(output.contains("references/govc-guide.md"));
    }

    #[test]
    fn recoverable_tool_error_output_treats_directory_workspace_paths_as_retryable() {
        let service = ExecutionService::default();
        let parsed = ToolCall::FsReadText(agent_runtime::tool::FsReadTextInput {
            path: "skills/project-knowledge-layout".to_string(),
        });

        let output = service
            .recoverable_tool_error_output(
                &parsed,
                &ToolError::Workspace(agent_runtime::workspace::WorkspaceError::Io {
                    path: std::path::PathBuf::from("./skills/project-knowledge-layout"),
                    source: std::io::Error::from(std::io::ErrorKind::IsADirectory),
                }),
            )
            .expect("recoverable output");

        assert!(output.contains("\"retryable\":true"));
        assert!(output.contains("workspace path is not a regular file"));
        assert!(output.contains("skills/project-knowledge-layout"));
    }

    #[test]
    fn stale_context_offload_payload_error_treats_io_not_found_as_stale() {
        let error = agent_persistence::StoreError::Io {
            path: std::path::PathBuf::from(
                "/tmp/artifacts/artifact-tool-offload-session-1-call-1.bin",
            ),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        };

        assert!(ExecutionService::is_stale_context_offload_payload_error(
            &error
        ));
    }

    #[test]
    fn recoverable_tool_error_output_treats_patch_search_misses_as_retryable() {
        let service = ExecutionService::default();
        let parsed = ToolCall::FsPatchText(agent_runtime::tool::FsPatchTextInput {
            path: "skills/vsphere-govc/scripts/vm-list.sh".to_string(),
            search: "missing old text".to_string(),
            replace: "new text".to_string(),
        });

        let output = service
            .recoverable_tool_error_output(
                &parsed,
                &ToolError::InvalidPatch {
                    path: "skills/vsphere-govc/scripts/vm-list.sh".to_string(),
                    reason: "search text not found in file".to_string(),
                },
            )
            .expect("recoverable output");

        assert!(output.contains("\"retryable\":true"));
        assert!(output.contains("search text not found in file"));
        assert!(output.contains("skills/vsphere-govc/scripts/vm-list.sh"));
    }

    #[test]
    fn recoverable_tool_error_output_treats_invalid_web_requests_as_nonfatal() {
        let service = ExecutionService::default();
        let parsed = ToolCall::WebFetch(agent_runtime::tool::WebFetchInput {
            url: "not-a-url".to_string(),
        });

        let output = service
            .recoverable_tool_error_output(
                &parsed,
                &ToolError::InvalidWebRequest {
                    reason: "relative URL without a base".to_string(),
                },
            )
            .expect("recoverable output");

        assert!(output.contains("\"retryable\":false"));
        assert!(output.contains("invalid web request"));
        assert!(output.contains("relative URL without a base"));
    }
}
