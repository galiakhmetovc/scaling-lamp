use super::*;
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
    ProviderContinuationMessage, ProviderMessage, ProviderRequest, ProviderResponse,
    ProviderStreamEvent, ProviderStreamMode, ProviderToolCall, ProviderToolDefinition,
    ProviderToolOutput,
};
use agent_runtime::run::{ApprovalRequest, PendingToolApproval, ProviderLoopState};
use agent_runtime::session::{MessageRole, TranscriptEntry};
use agent_runtime::skills::{resolve_session_skill_status, scan_skill_catalog};
use agent_runtime::tool::{
    AddTaskNoteOutput, AddTaskOutput, ArtifactReadOutput, ArtifactSearchOutput,
    ArtifactSearchResult, EditTaskOutput, InitPlanOutput, PlanLintOutput, PlanReadOutput,
    PlanSnapshotOutput, PlanWriteOutput, SetTaskStatusOutput, ToolCatalog, ToolDefinition,
    ToolName, ToolOutput, ToolRuntime,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_PROVIDER_TOOL_ROUNDS: usize = 8;
const MAX_CONTEXT_OFFLOAD_REFS: usize = 16;
const INLINE_TOOL_OUTPUT_TOKEN_LIMIT: u32 = 512;
const INLINE_FIND_IN_FILES_PREVIEW_LIMIT: usize = 6;

type OffloadableToolOutput = (String, String, Vec<u8>, String);

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
struct ProviderLoopCursor {
    round: usize,
    pending_tool_outputs: Vec<ProviderToolOutput>,
    continuation_messages: Vec<ProviderContinuationMessage>,
    previous_response_id: Option<String>,
    seen_tool_signatures: BTreeMap<String, usize>,
    supports_previous_response_id: bool,
    supports_streaming: bool,
}

impl ProviderLoopCursor {
    fn new(provider: &dyn ProviderDriver, initial_loop_state: Option<ProviderLoopState>) -> Self {
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
        let previous_response_id = initial_loop_state
            .as_ref()
            .and_then(|state| state.previous_response_id.clone());
        let seen_tool_signatures = initial_loop_state
            .as_ref()
            .map(|state| {
                state
                    .seen_tool_signatures
                    .iter()
                    .enumerate()
                    .map(|(index, signature)| (signature.clone(), index))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();

        Self {
            round,
            pending_tool_outputs,
            continuation_messages,
            previous_response_id,
            seen_tool_signatures,
            supports_previous_response_id,
            supports_streaming,
        }
    }

    fn has_round_budget(&self) -> bool {
        self.round < MAX_PROVIDER_TOOL_ROUNDS
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
        ProviderRequest {
            model: model.map(str::to_string),
            instructions: instructions.map(str::to_string),
            messages: if self.supports_previous_response_id && self.previous_response_id.is_some() {
                Vec::new()
            } else {
                base_messages.to_vec()
            },
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
        if let Some(first_seen_round) = self
            .seen_tool_signatures
            .insert(signature.clone(), self.round)
        {
            return Err(ExecutionError::ProviderLoop {
                reason: format!(
                    "provider repeated tool-call signature from round {}: {}",
                    first_seen_round + 1,
                    signature
                ),
            });
        }
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
            seen_tool_signatures: self.seen_tool_signatures.keys().cloned().collect(),
            pending_approval: Some(PendingToolApproval::new(
                approval_id.to_string(),
                tool_call.call_id.clone(),
                parsed.name().as_str().to_string(),
                tool_call.arguments.clone(),
            )),
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

    fn exhausted_rounds_error(&self) -> ExecutionError {
        ExecutionError::ProviderLoop {
            reason: format!(
                "provider exceeded {} tool-calling rounds without producing a final answer",
                MAX_PROVIDER_TOOL_ROUNDS
            ),
        }
    }
}

impl ExecutionService {
    fn automatic_provider_tools(
        &self,
        provider: &dyn ProviderDriver,
        context_offload: Option<&ContextOffloadSnapshot>,
    ) -> Vec<ProviderToolDefinition> {
        if !provider.descriptor().capabilities.supports_tool_calls {
            return Vec::new();
        }

        let has_context_offload = context_offload.is_some_and(|snapshot| !snapshot.is_empty());
        ToolCatalog::default()
            .automatic_model_definitions()
            .into_iter()
            .filter(|definition| {
                has_context_offload
                    || !matches!(
                        definition.name,
                        ToolName::ArtifactRead | ToolName::ArtifactSearch
                    )
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
        let system_prompt = prompting::load_system_prompt(&self.workspace);
        let agents_prompt = prompting::load_agents_prompt(&self.workspace);
        let transcripts_for_activation = transcripts
            .iter()
            .cloned()
            .map(TranscriptEntry::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        let skills_catalog = scan_skill_catalog(&self.skills_dir).map_err(|source| {
            ExecutionError::ProviderLoop {
                reason: format!(
                    "failed to scan skills catalog at {}: {source}",
                    self.skills_dir.display()
                ),
            }
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
        response: &ProviderResponse,
        now: i64,
    ) -> Result<(), ExecutionError> {
        run.begin_provider_stream(&response.response_id, &response.model, now)
            .map_err(ExecutionError::RunTransition)?;
        if !response.output_text.is_empty() {
            run.push_provider_text(&response.output_text, now)
                .map_err(ExecutionError::RunTransition)?;
        }
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
    ) -> Result<ProviderResponse, ExecutionError> {
        if matches!(request.stream, ProviderStreamMode::Enabled) {
            let mut stream = provider.stream(request).map_err(ExecutionError::Provider)?;
            let mut final_response = None;
            while let Some(event) = stream.next_event().map_err(ExecutionError::Provider)? {
                match event {
                    ProviderStreamEvent::ReasoningDelta(delta) => {
                        Self::emit_event(observer, ChatExecutionEvent::ReasoningDelta(delta));
                    }
                    ProviderStreamEvent::TextDelta(delta) => {
                        Self::emit_event(observer, ChatExecutionEvent::AssistantTextDelta(delta));
                    }
                    ProviderStreamEvent::Completed(response) => {
                        final_response = Some(response);
                        break;
                    }
                }
            }
            final_response.ok_or_else(|| ExecutionError::ProviderLoop {
                reason: "provider stream ended without a final response".to_string(),
            })
        } else {
            provider.complete(request).map_err(ExecutionError::Provider)
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
                        status: ToolExecutionStatus::Failed,
                    },
                );
                return Err(source);
            }
        };
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
        snapshot
            .initialize(goal, now)
            .map_err(Self::invalid_plan_tool)?;
        self.persist_plan_snapshot(store, &snapshot)?;

        Ok(InitPlanOutput {
            goal: snapshot.goal.unwrap_or_default(),
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
        let status = PlanItemStatus::try_from(new_status).map_err(Self::invalid_plan_tool)?;
        let task = snapshot
            .set_task_status(task_id, status, blocked_reason, now)
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
        let task = snapshot
            .add_task_note(task_id, note, now)
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
        let task = snapshot
            .edit_task(
                task_id,
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

        let payloads = snapshot
            .refs
            .iter()
            .map(|reference| {
                if reference.artifact_id == artifact_id {
                    Ok(ContextOffloadPayload {
                        artifact_id: artifact_id.clone(),
                        bytes: payload_bytes.clone(),
                    })
                } else {
                    Ok(store
                        .get_context_offload_payload(reference.artifact_id.as_str())
                        .map_err(ExecutionError::Store)?
                        .ok_or_else(|| {
                            ExecutionError::Tool(ToolError::InvalidArtifactTool {
                                reason: format!(
                                    "artifact {} is missing from context offload storage",
                                    reference.artifact_id
                                ),
                            })
                        })?)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

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
        let payload = store
            .get_context_offload_payload(artifact_id)
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| {
                ExecutionError::Tool(ToolError::InvalidArtifactTool {
                    reason: format!(
                        "artifact {} is missing from context offload storage",
                        artifact_id
                    ),
                })
            })?;

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
            let payload = store
                .get_context_offload_payload(reference.artifact_id.as_str())
                .map_err(ExecutionError::Store)?
                .ok_or_else(|| {
                    ExecutionError::Tool(ToolError::InvalidArtifactTool {
                        reason: format!(
                            "artifact {} is missing from context offload storage",
                            reference.artifact_id
                        ),
                    })
                })?;
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
        now: i64,
        interrupt_after_tool_step: Option<&AtomicBool>,
        observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
    ) -> Result<ProviderResponse, ExecutionError> {
        let prompt_messages = self.prompt_messages(store, session_id)?;
        let catalog = ToolCatalog::default();
        let tools =
            self.automatic_provider_tools(provider, prompt_messages.context_offload.as_ref());
        let mut tool_runtime = self.tool_runtime();
        let mut cursor = ProviderLoopCursor::new(provider, initial_loop_state);

        while cursor.has_round_budget() {
            let request = cursor.build_request(
                &prompt_messages.messages,
                model.as_deref(),
                instructions.as_deref(),
                &tools,
                cursor.stream_mode(observer.is_some()),
                self.provider_max_output_tokens,
            );
            let response = self.request_provider_response(provider, &request, observer)?;
            self.apply_provider_response(run, &response, now)?;
            self.persist_run(store, run)?;

            if response.tool_calls.is_empty() {
                return Ok(response);
            }

            cursor.remember_tool_signature(&response)?;
            cursor.note_assistant_tool_calls(&response);
            cursor.begin_tool_round();
            for tool_call in &response.tool_calls {
                let (parsed, definition) = self.resolve_provider_tool_call(&catalog, tool_call)?;
                Self::emit_event(
                    observer,
                    ChatExecutionEvent::ToolStatus {
                        tool_name: parsed.name().as_str().to_string(),
                        status: ToolExecutionStatus::Requested,
                    },
                );
                let permission = self.permissions.resolve(definition, &parsed);

                match permission.action {
                    PermissionAction::Allow => {}
                    PermissionAction::Deny => {
                        Self::emit_event(
                            observer,
                            ChatExecutionEvent::ToolStatus {
                                tool_name: parsed.name().as_str().to_string(),
                                status: ToolExecutionStatus::Failed,
                            },
                        );
                        let reason = format!(
                            "tool {} denied by permission policy: {}",
                            parsed.name().as_str(),
                            permission.reason
                        );
                        run.fail(reason.clone(), now)
                            .map_err(ExecutionError::RunTransition)?;
                        self.persist_run(store, run)?;
                        return Err(ExecutionError::PermissionDenied {
                            tool: parsed.name().as_str().to_string(),
                            reason,
                        });
                    }
                    PermissionAction::Ask => {
                        Self::emit_event(
                            observer,
                            ChatExecutionEvent::ToolStatus {
                                tool_name: parsed.name().as_str().to_string(),
                                status: ToolExecutionStatus::WaitingApproval,
                            },
                        );
                        let approval_id =
                            format!("approval-{}-{}", run.snapshot().id, parsed.name().as_str());
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

                let model_output = self.invoke_provider_tool_call(
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
                )?;
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

        Err(cursor.exhausted_rounds_error())
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
    use agent_runtime::provider::{
        ModelCapabilities, ProviderDescriptor, ProviderError, ProviderRequest, ProviderResponse,
        ProviderResponseStream,
    };

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
        let cursor = ProviderLoopCursor::new(&provider, None);

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
        let cursor = ProviderLoopCursor::new(&provider, None);

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
}
