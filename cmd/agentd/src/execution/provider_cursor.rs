use agent_runtime::provider::{
    ProviderContinuationMessage, ProviderDriver, ProviderMessage, ProviderRequest,
    ProviderResponse, ProviderStreamMode, ProviderToolCall, ProviderToolDefinition,
    ProviderToolOutput,
};
use agent_runtime::run::{
    PendingLoopResetApproval, PendingProviderApproval, PendingToolApproval, ProviderLoopState,
};
use agent_runtime::tool::ToolName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ToolSignatureObservation {
    Accepted,
    RepeatedSuppressed {
        consecutive_repeats: usize,
        signature: String,
    },
}

#[derive(Debug, Clone)]
pub(super) struct ProviderLoopCursor {
    pub(super) max_rounds: usize,
    pub(super) round: usize,
    pending_tool_outputs: Vec<ProviderToolOutput>,
    continuation_messages: Vec<ProviderContinuationMessage>,
    continuation_input_messages: Vec<ProviderMessage>,
    previous_response_id: Option<String>,
    seen_tool_signatures: Vec<String>,
    pub(super) completion_nudges_used: usize,
    pub(super) empty_response_recoveries_used: usize,
    pub(super) supports_previous_response_id: bool,
    supports_streaming: bool,
}

impl ProviderLoopCursor {
    fn permits_repeated_tool_signature(response: &ProviderResponse) -> bool {
        !response.tool_calls.is_empty()
            && response.tool_calls.iter().all(|tool_call| {
                matches!(
                    tool_call.name.as_str(),
                    name if name == ToolName::ExecReadOutput.as_str()
                )
            })
    }

    pub(super) fn new(
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

    pub(super) fn has_round_budget(&self) -> bool {
        self.round < self.max_rounds
    }

    pub(super) fn reset_round_budget(&mut self) {
        self.round = 0;
    }

    pub(super) fn stream_mode(&self, has_observer: bool) -> ProviderStreamMode {
        if has_observer && self.supports_streaming {
            ProviderStreamMode::Enabled
        } else {
            ProviderStreamMode::Disabled
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_request(
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

    pub(super) fn persistent_state(
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

    pub(super) fn remember_tool_signature(
        &mut self,
        response: &ProviderResponse,
        max_consecutive_identical_tool_signatures: usize,
    ) -> ToolSignatureObservation {
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
        self.seen_tool_signatures.push(signature.clone());
        if consecutive_repeats >= max_consecutive_identical_tool_signatures.max(1)
            && !Self::permits_repeated_tool_signature(response)
        {
            return ToolSignatureObservation::RepeatedSuppressed {
                consecutive_repeats,
                signature,
            };
        }
        ToolSignatureObservation::Accepted
    }

    pub(super) fn note_assistant_tool_calls(&mut self, response: &ProviderResponse) {
        if !self.supports_previous_response_id {
            self.continuation_messages
                .push(ProviderContinuationMessage::AssistantToolCalls {
                    tool_calls: response.tool_calls.clone(),
                });
        }
    }

    pub(super) fn begin_tool_round(&mut self) {
        if self.supports_previous_response_id {
            self.pending_tool_outputs.clear();
        }
    }

    pub(super) fn record_tool_output(&mut self, tool_call_id: &str, model_output: String) {
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

    pub(super) fn pending_approval_state(
        &self,
        response: &ProviderResponse,
        tool_call: &ProviderToolCall,
        parsed: &agent_runtime::tool::ToolCall,
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

    pub(super) fn loop_reset_approval_state(&self, approval_id: &str) -> ProviderLoopState {
        let mut state = self.persistent_state(Some(PendingProviderApproval::LoopReset(
            PendingLoopResetApproval::new(approval_id.to_string(), self.round, self.max_rounds),
        )));
        state.continuation_input_messages.clear();
        state
    }

    pub(super) fn completion_approval_state(
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

    pub(super) fn queue_continuation_input_messages(&mut self, messages: Vec<ProviderMessage>) {
        self.continuation_input_messages.clear();
        self.continuation_input_messages.extend(messages);
    }

    pub(super) fn queue_post_tool_continuation_messages(&mut self, messages: Vec<ProviderMessage>) {
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

    pub(super) fn clear_continuation_input_messages(&mut self) {
        self.continuation_input_messages.clear();
    }

    pub(super) fn record_completion_nudge(&mut self) {
        self.completion_nudges_used += 1;
    }

    pub(super) fn can_recover_from_empty_response(
        &self,
        max_empty_response_recoveries: usize,
    ) -> bool {
        self.empty_response_recoveries_used < max_empty_response_recoveries
            && (!self.pending_tool_outputs.is_empty() || !self.continuation_messages.is_empty())
    }

    pub(super) fn record_empty_response_recovery(&mut self) {
        self.empty_response_recoveries_used += 1;
    }

    pub(super) fn adopt_response_anchor(&mut self, response: &ProviderResponse) {
        if self.supports_previous_response_id {
            self.previous_response_id = Some(response.response_id.clone());
            self.pending_tool_outputs.clear();
        }
    }

    pub(super) fn advance_after_response(&mut self, response: &ProviderResponse) {
        if self.supports_previous_response_id {
            self.previous_response_id = Some(response.response_id.clone());
        } else {
            self.previous_response_id = None;
            self.pending_tool_outputs.clear();
        }
        self.round += 1;
    }
}
