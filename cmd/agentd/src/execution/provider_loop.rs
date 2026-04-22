use super::*;
use crate::agents;
use crate::prompting;
use agent_persistence::ContextOffloadRepository;
use agent_runtime::context::{
    ContextOffloadPayload, ContextOffloadRef, ContextOffloadSnapshot, ContextSummary,
    approximate_token_count,
};
use agent_runtime::permission::PermissionAction;
use agent_runtime::plan::{PlanItem, PlanItemStatus, PlanSnapshot};
use agent_runtime::prompt::{PromptAssembly, PromptAssemblyInput};
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
    PlanSnapshotOutput, PlanWriteOutput, SetTaskStatusOutput, ToolCatalog, ToolDefinition,
    ToolName, ToolOutput, ToolRuntime,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

const MAX_CONTEXT_OFFLOAD_REFS: usize = 16;
const INLINE_TOOL_OUTPUT_TOKEN_LIMIT: u32 = 512;
const INLINE_FIND_IN_FILES_PREVIEW_LIMIT: usize = 6;
const MAX_CONSECUTIVE_IDENTICAL_TOOL_SIGNATURES: usize = 3;
const MAX_TRANSIENT_PROVIDER_RETRIES: usize = 3;

type OffloadableToolOutput = (String, String, Vec<u8>, String);

#[derive(Debug, Clone)]
pub(super) struct CompletionGateDecision {
    pub(super) max_completion_nudges: usize,
    pub(super) nudge_message: String,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ProviderToolExecutionContext<'a> {
    pub(super) store: &'a PersistenceStore,
    pub(super) session_id: &'a str,
    pub(super) now: i64,
}

#[derive(Debug, Clone)]
struct PromptMessages {
    messages: Vec<ProviderMessage>,
    context_offload: Option<ContextOffloadSnapshot>,
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
    supports_previous_response_id: bool,
    supports_streaming: bool,
}

impl ProviderLoopCursor {
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

        Self {
            max_rounds,
            round,
            pending_tool_outputs,
            continuation_messages,
            continuation_input_messages,
            previous_response_id,
            seen_tool_signatures,
            completion_nudges_used,
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

    fn build_request(
        &self,
        base_messages: &[ProviderMessage],
        model: Option<&str>,
        instructions: Option<&str>,
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
        if consecutive_repeats >= MAX_CONSECUTIVE_IDENTICAL_TOOL_SIGNATURES {
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

    fn clear_continuation_input_messages(&mut self) {
        self.continuation_input_messages.clear();
    }

    fn record_completion_nudge(&mut self) {
        self.completion_nudges_used += 1;
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

impl ExecutionService {
    fn is_stale_context_offload_payload_error(error: &agent_persistence::StoreError) -> bool {
        matches!(
            error,
            agent_persistence::StoreError::MissingPayload { .. }
                | agent_persistence::StoreError::IntegrityMismatch { .. }
        )
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
        ToolCatalog::default()
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
            .collect()
    }

    fn prompt_messages(
        &self,
        store: &PersistenceStore,
        session_id: &str,
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
        let session_head = prompting::build_session_head(
            &session,
            &transcripts,
            context_summary.as_ref(),
            &runs,
            &self.workspace,
        );
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

        Ok(PromptMessages {
            messages: PromptAssembly::build_messages(PromptAssemblyInput {
                system_prompt: Some(system_prompt),
                agents_prompt,
                active_skill_prompts,
                session_head: Some(session_head),
                plan_snapshot,
                context_summary,
                context_offload: context_offload.clone(),
                transcript_messages,
            }),
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
        let prompt = self.prompt_messages(store, session_id)?;
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
        store
            .put_run(
                &RunRecord::try_from(run.snapshot()).map_err(ExecutionError::RecordConversion)?,
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

    fn run_was_cancelled_by_operator(
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

    fn transient_provider_retry_delay(attempt: usize) -> Duration {
        Duration::from_millis((attempt as u64) * 100)
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
                    thread::sleep(Self::transient_provider_retry_delay(attempt));
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn resolve_provider_tool_call<'a>(
        &self,
        catalog: &'a ToolCatalog,
        tool_call: &ProviderToolCall,
    ) -> Result<(ToolCall, &'a ToolDefinition), ExecutionError> {
        let parsed = ToolCall::from_openai_function(&tool_call.name, &tool_call.arguments)
            .map_err(|source| ExecutionError::ToolCallParse {
                name: tool_call.name.clone(),
                reason: source.to_string(),
            })?;
        let definition =
            catalog
                .definition_for_call(&parsed)
                .ok_or_else(|| ExecutionError::ToolCallParse {
                    name: tool_call.name.clone(),
                    reason: "tool is not in the catalog".to_string(),
                })?;
        Ok((parsed, definition))
    }

    pub(super) fn invoke_provider_tool_call(
        &self,
        context: ProviderToolExecutionContext<'_>,
        run: &mut RunEngine,
        tool_runtime: &mut ToolRuntime,
        tool_call_id: &str,
        parsed: &ToolCall,
        observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
    ) -> Result<String, ExecutionError> {
        Self::emit_event(
            observer,
            ChatExecutionEvent::ToolStatus {
                tool_name: parsed.name().as_str().to_string(),
                summary: parsed.summary(),
                status: ToolExecutionStatus::Running,
            },
        );
        let output = match self.execute_model_tool_call(
            context.store,
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
                        tool_name: parsed.name().as_str().to_string(),
                        summary: parsed.summary(),
                        status: ToolExecutionStatus::Failed,
                    },
                );
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
                tool_name: parsed.name().as_str().to_string(),
                summary: output_summary,
                status: ToolExecutionStatus::Completed,
            },
        );
        self.prepare_model_tool_output(
            context.store,
            context.session_id,
            tool_call_id,
            parsed,
            &output,
            model_output,
            context.now,
        )
    }

    pub(super) fn execute_model_tool_call(
        &self,
        store: &PersistenceStore,
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
        snapshot.refs.push(ContextOffloadRef {
            id: ref_id.clone(),
            label,
            summary,
            artifact_id: artifact_id.clone(),
            token_estimate,
            message_count: 1,
            created_at: now,
        });
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

        store
            .put_context_offload(
                &agent_persistence::ContextOffloadRecord::try_from(&snapshot)
                    .map_err(ExecutionError::RecordConversion)?,
                &payloads,
            )
            .map_err(ExecutionError::Store)?;

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
        let prompt_messages = self.prompt_messages(store, session_id)?;
        let catalog = ToolCatalog::default();
        let agent_profile = self.load_agent_profile_for_session(store, session_id)?;
        let tools = self.automatic_provider_tools(
            provider,
            prompt_messages.context_offload.as_ref(),
            &agent_profile,
        );
        let mut tool_runtime = self.tool_runtime();
        let auto_approve =
            auto_approve_override.unwrap_or(self.session_auto_approve_enabled(store, session_id)?);
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
                &tools,
                cursor.stream_mode(observer.is_some()),
                self.config.provider_max_output_tokens,
            );
            let observed = self.request_provider_response_with_retries(
                store, run, provider, &request, now, observer,
            )?;
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
                let (parsed, definition) =
                    match self.resolve_provider_tool_call(&catalog, tool_call) {
                        Ok(resolved) => resolved,
                        Err(ExecutionError::ToolCallParse { reason, .. }) => {
                            Self::emit_event(
                                observer,
                                ChatExecutionEvent::ToolStatus {
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
                Self::emit_event(
                    observer,
                    ChatExecutionEvent::ToolStatus {
                        tool_name: parsed.name().as_str().to_string(),
                        summary: parsed.summary(),
                        status: ToolExecutionStatus::Requested,
                    },
                );
                if let Err(error) = self.ensure_agent_tool_allowed(store, session_id, parsed.name())
                {
                    Self::emit_event(
                        observer,
                        ChatExecutionEvent::ToolStatus {
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
                let permission = self.permissions.resolve(definition, &parsed);

                match permission.action {
                    PermissionAction::Allow => {}
                    PermissionAction::Deny => {
                        Self::emit_event(
                            observer,
                            ChatExecutionEvent::ToolStatus {
                                tool_name: parsed.name().as_str().to_string(),
                                summary: parsed.summary(),
                                status: ToolExecutionStatus::Failed,
                            },
                        );
                        let reason = format!(
                            "tool {} denied by permission policy: {}",
                            parsed.name().as_str(),
                            permission.reason
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
                            Self::emit_event(
                                observer,
                                ChatExecutionEvent::ToolStatus {
                                    tool_name: parsed.name().as_str().to_string(),
                                    summary: parsed.summary(),
                                    status: ToolExecutionStatus::Approved,
                                },
                            );
                        } else {
                            Self::emit_event(
                                observer,
                                ChatExecutionEvent::ToolStatus {
                                    tool_name: parsed.name().as_str().to_string(),
                                    summary: parsed.summary(),
                                    status: ToolExecutionStatus::WaitingApproval,
                                },
                            );
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
                        session_id,
                        now,
                    },
                    run,
                    &mut tool_runtime,
                    &tool_call.call_id,
                    &parsed,
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
        ModelCapabilities, ProviderDescriptor, ProviderError, ProviderRequest, ProviderResponse,
        ProviderResponseStream,
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

    #[test]
    fn build_request_omits_max_output_tokens_when_not_configured() {
        let provider = provider();
        let cursor = ProviderLoopCursor::new(&provider, None, 24);

        let request = cursor.build_request(
            &[ProviderMessage::new(MessageRole::User, "hello")],
            Some("test-model"),
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
            &[],
            ProviderStreamMode::Disabled,
            Some(8192),
        );

        assert_eq!(request.max_output_tokens, Some(8192));
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
