use crate::execution::{ChatExecutionEvent, ToolExecutionStatus};
use std::collections::BTreeMap;

const TELEGRAM_STATUS_DETAIL_CHAR_CAP: usize = 700;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TelegramProgressState {
    phase: TelegramProgressPhase,
    current_round: Option<(usize, usize)>,
    current_tool_name: Option<String>,
    current_tool_status: Option<ToolExecutionStatus>,
    current_tool_summary: Option<String>,
    total_tool_calls: usize,
    failed_tool_calls: usize,
}

impl Default for TelegramProgressState {
    fn default() -> Self {
        Self {
            phase: TelegramProgressPhase::Starting,
            current_round: None,
            current_tool_name: None,
            current_tool_status: None,
            current_tool_summary: None,
            total_tool_calls: 0,
            failed_tool_calls: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TelegramProgressPhase {
    Starting,
    Thinking,
    Drafting,
    Continuation,
    Tool,
}

#[derive(Debug, Default)]
pub(super) struct TelegramProgressTracker {
    state: TelegramProgressState,
    tool_calls: BTreeMap<String, ToolExecutionStatus>,
}

impl TelegramProgressTracker {
    pub(super) fn state(&self) -> &TelegramProgressState {
        &self.state
    }

    pub(super) fn apply(&mut self, event: &ChatExecutionEvent) -> bool {
        let previous = self.state.clone();
        match event {
            ChatExecutionEvent::ReasoningDelta(_) => {
                self.state.phase = TelegramProgressPhase::Thinking;
            }
            ChatExecutionEvent::AssistantTextDelta(_) => {
                self.state.phase = TelegramProgressPhase::Drafting;
            }
            ChatExecutionEvent::ProviderLoopProgress {
                current_round,
                max_rounds,
            } => {
                self.state.phase = TelegramProgressPhase::Continuation;
                self.state.current_round = Some((*current_round, *max_rounds));
            }
            ChatExecutionEvent::ToolStatus {
                tool_call_id,
                tool_name,
                summary,
                status,
            } => {
                self.state.phase = TelegramProgressPhase::Tool;
                self.state.current_tool_name = Some(tool_name.clone());
                self.state.current_tool_status = Some(status.clone());
                self.state.current_tool_summary = if summary.trim().is_empty() {
                    None
                } else {
                    Some(summary.clone())
                };
                let previous_status = self.tool_calls.insert(tool_call_id.clone(), status.clone());
                if previous_status.is_none() {
                    self.state.total_tool_calls += 1;
                }
                if !matches!(previous_status, Some(ToolExecutionStatus::Failed))
                    && matches!(status, ToolExecutionStatus::Failed)
                {
                    self.state.failed_tool_calls += 1;
                }
            }
        }
        self.state != previous
    }
}

pub(super) fn render_temporary_status_html(state: &TelegramProgressState) -> String {
    let (title, phase_label) = match (&state.phase, state.current_tool_status.as_ref()) {
        (TelegramProgressPhase::Starting, _) => ("⏳ Работаю", "запуск"),
        (TelegramProgressPhase::Thinking, _) => ("🧠 Анализирую", "анализ"),
        (TelegramProgressPhase::Drafting, _) => ("✍️ Пишу ответ", "черновик ответа"),
        (TelegramProgressPhase::Continuation, _) => ("🔁 Продолжаю", "продолжение"),
        (TelegramProgressPhase::Tool, Some(ToolExecutionStatus::WaitingApproval)) => {
            ("🛂 Жду подтверждение", "ожидаю апрув")
        }
        (TelegramProgressPhase::Tool, Some(ToolExecutionStatus::Failed)) => {
            ("⚠️ Ошибка инструмента", "инструменты")
        }
        (TelegramProgressPhase::Tool, _) => ("🔧 Работаю с инструментами", "инструменты"),
    };

    let mut lines = vec![
        format!("<b>{title}</b>"),
        format!("Стадия: {phase_label}"),
        format!(
            "Вызовы: {} · Ошибки: {}",
            state.total_tool_calls, state.failed_tool_calls
        ),
    ];

    if let Some((current_round, max_rounds)) = state.current_round {
        lines.push(format!("Раунд: {current_round}/{max_rounds}"));
    }
    if let Some(tool_name) = state.current_tool_name.as_deref() {
        lines.push(format!(
            "Инструмент: <code>{}</code>",
            escape_telegram_html(tool_name)
        ));
    }
    if let Some(status) = state.current_tool_status.as_ref() {
        lines.push(format!("Статус: {}", render_tool_status_label(status)));
    }
    if let Some(summary) = state.current_tool_summary.as_deref() {
        lines.push(format!("Деталь: {}", render_status_detail(summary)));
    }

    lines.join("\n")
}

pub(super) fn render_failed_temporary_status_html(error: &str) -> String {
    [
        "<b>❌ Ошибка</b>".to_string(),
        "Стадия: выполнение не завершилось".to_string(),
        format!("Деталь: {}", escape_telegram_html(error)),
    ]
    .join("\n")
}

fn render_tool_status_label(status: &ToolExecutionStatus) -> &'static str {
    match status {
        ToolExecutionStatus::Requested => "запрошен",
        ToolExecutionStatus::WaitingApproval => "ожидает апрув",
        ToolExecutionStatus::Approved => "подтверждён",
        ToolExecutionStatus::Running => "выполняется",
        ToolExecutionStatus::Completed => "завершён",
        ToolExecutionStatus::Failed => "ошибка",
    }
}

fn render_status_detail(summary: &str) -> String {
    let compact = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    let total_chars = compact.chars().count();
    if total_chars <= TELEGRAM_STATUS_DETAIL_CHAR_CAP {
        return escape_telegram_html(&compact);
    }

    let visible: String = compact
        .chars()
        .take(TELEGRAM_STATUS_DETAIL_CHAR_CAP)
        .collect();
    format!(
        "{}… (обрезано, ещё {} симв.)",
        escape_telegram_html(&visible),
        total_chars.saturating_sub(TELEGRAM_STATUS_DETAIL_CHAR_CAP)
    )
}

fn escape_telegram_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) fn render_file_delivery_failed_html(file_name: &str, error: &str) -> String {
    [
        "<b>⚠️ Файл не отправлен</b>".to_string(),
        "Не удалось отправить файл через Telegram.".to_string(),
        format!(
            "Файл: <code>{}</code>",
            escape_telegram_html(file_name.trim())
        ),
        format!("Деталь: {}", render_status_detail(error)),
        "Запрос доставки помечен как failed; файл остался artifact'ом текущей session.".to_string(),
    ]
    .join("\n")
}
