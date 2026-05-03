use agent_persistence::ToolCallRecord;
use agent_runtime::context::{ContextOffloadSnapshot, approximate_token_count};
use agent_runtime::prompt::{RecentToolActivityEntry, SessionHeadRuntime};
use agent_runtime::provider::ProviderMessage;
use agent_runtime::workspace::WorkspaceRef;

#[derive(Debug, Clone)]
pub(super) struct PromptMessages {
    pub(super) messages: Vec<ProviderMessage>,
    pub(super) context_offload: Option<ContextOffloadSnapshot>,
}

pub(super) struct PromptMessagesRequest<'a> {
    pub(super) session_id: &'a str,
    pub(super) workspace: &'a WorkspaceRef,
    pub(super) model: Option<&'a str>,
    pub(super) instructions: Option<&'a str>,
    pub(super) consume_next_turn_prompt_budget: bool,
    pub(super) include_memory_recall: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct AutoCompactionDecision {
    pub(super) estimated_prompt_tokens: u32,
    pub(super) trigger_threshold_tokens: u32,
    pub(super) context_window_tokens: u32,
}

pub(super) fn recent_tool_activity_entry(record: ToolCallRecord) -> RecentToolActivityEntry {
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

pub(super) fn estimate_prompt_tokens(
    messages: &[ProviderMessage],
    instructions: Option<&str>,
) -> u32 {
    let instruction_tokens = instructions.map_or(0, approximate_token_count);
    instruction_tokens.saturating_add(
        messages
            .iter()
            .map(|message| approximate_token_count(&message.content))
            .sum::<u32>(),
    )
}

pub(super) fn session_head_runtime(
    provider_name: Option<String>,
    default_model: Option<String>,
    session_model: Option<String>,
    model: Option<&str>,
    think_level: Option<String>,
    context_window_tokens: Option<u32>,
    auto_compaction_trigger_ratio: Option<f64>,
) -> SessionHeadRuntime {
    let resolved_model = model
        .map(str::to_string)
        .or(session_model)
        .or(default_model);
    SessionHeadRuntime {
        provider_name,
        model: resolved_model,
        think_level,
        context_window_tokens,
        auto_compaction_trigger_ratio,
        usable_context_tokens: SessionHeadRuntime::usable_context_tokens(
            context_window_tokens,
            auto_compaction_trigger_ratio,
        ),
        estimated_prompt_tokens: None,
    }
}
