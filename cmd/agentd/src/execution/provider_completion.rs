use crate::prompting;
use agent_runtime::plan::{PlanItem, PlanSnapshot};
use agent_runtime::provider::{ProviderMessage, ProviderResponse};
use agent_runtime::session::MessageRole;

#[derive(Debug, Clone)]
pub(super) struct CompletionGateDecision {
    pub(super) max_completion_nudges: usize,
    pub(super) nudge_message: String,
}

pub(super) fn build_completion_nudge_message(
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

pub(super) fn empty_response_continuation_messages(
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
