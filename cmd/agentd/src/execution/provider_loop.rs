use super::*;
use crate::prompting;
use agent_runtime::permission::PermissionAction;
use agent_runtime::plan::PlanItem;
use agent_runtime::prompt::{PromptAssembly, PromptAssemblyInput};
use agent_runtime::provider::{
    ProviderContinuationMessage, ProviderMessage, ProviderRequest, ProviderResponse,
    ProviderStreamEvent, ProviderStreamMode, ProviderToolCall, ProviderToolDefinition,
    ProviderToolOutput,
};
use agent_runtime::run::{ApprovalRequest, PendingToolApproval, ProviderLoopState};
use agent_runtime::session::MessageRole;
use agent_runtime::tool::{
    PlanReadOutput, PlanWriteOutput, ToolCatalog, ToolDefinition, ToolOutput, ToolRuntime,
};
use std::collections::BTreeMap;

const MAX_PROVIDER_TOOL_ROUNDS: usize = 8;

#[derive(Debug, Clone, Copy)]
pub(super) struct ProviderToolExecutionContext<'a> {
    pub(super) store: &'a PersistenceStore,
    pub(super) session_id: &'a str,
    pub(super) now: i64,
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
            max_output_tokens: Some(512),
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
    ) -> Vec<ProviderToolDefinition> {
        if !provider.descriptor().capabilities.supports_tool_calls {
            return Vec::new();
        }

        ToolCatalog::default()
            .automatic_model_definitions()
            .into_iter()
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
    ) -> Result<Vec<ProviderMessage>, ExecutionError> {
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

        Ok(PromptAssembly::build_messages(PromptAssemblyInput {
            session_head: Some(session_head),
            plan_snapshot,
            context_summary,
            transcript_messages,
        }))
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
        Ok(model_output)
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
        let snapshot = store
            .get_plan(session_id)
            .map_err(ExecutionError::Store)?
            .map(PlanSnapshot::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)?
            .unwrap_or_else(|| PlanSnapshot {
                session_id: session_id.to_string(),
                items: Vec::new(),
                updated_at: 0,
            });

        Ok(PlanReadOutput {
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
            items: items.clone(),
            updated_at: now,
        };
        let record = PlanRecord::try_from(&snapshot).map_err(ExecutionError::RecordConversion)?;
        store.put_plan(&record).map_err(ExecutionError::Store)?;

        Ok(PlanWriteOutput { items })
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
        observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
    ) -> Result<ProviderResponse, ExecutionError> {
        let base_messages = self.prompt_messages(store, session_id)?;
        let catalog = ToolCatalog::default();
        let tools = self.automatic_provider_tools(provider);
        let mut tool_runtime = ToolRuntime::new(self.workspace.clone());
        let mut cursor = ProviderLoopCursor::new(provider, initial_loop_state);

        while cursor.has_round_budget() {
            let request = cursor.build_request(
                &base_messages,
                model.as_deref(),
                instructions.as_deref(),
                &tools,
                cursor.stream_mode(observer.is_some()),
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
                    &parsed,
                    observer,
                )?;
                cursor.record_tool_output(&tool_call.call_id, model_output);
            }

            cursor.advance_after_response(&response);
            self.persist_run(store, run)?;
        }

        Err(cursor.exhausted_rounds_error())
    }
}
