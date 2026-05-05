use crate::execution::{ChatExecutionEvent, ToolExecutionStatus};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TelegramProgressState {
    phase: TelegramProgressPhase,
    current_round: Option<(usize, usize)>,
    current_tool_name: Option<String>,
    current_tool_status: Option<ToolExecutionStatus>,
    current_tool_summary: Option<String>,
    current_context_summary: Option<String>,
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
            current_context_summary: None,
            total_tool_calls: 0,
            failed_tool_calls: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TelegramProgressPhase {
    Starting,
    Thinking,
    Context,
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
            ChatExecutionEvent::ContextStatus { summary, .. } => {
                self.state.phase = TelegramProgressPhase::Context;
                self.state.current_context_summary = if summary.trim().is_empty() {
                    None
                } else {
                    Some(summary.clone())
                };
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

pub(super) fn render_temporary_status_html(
    state: &TelegramProgressState,
    detail_char_cap: usize,
) -> String {
    let (title, phase_label) = match (&state.phase, state.current_tool_status.as_ref()) {
        (TelegramProgressPhase::Starting, _) => ("⏳ Работаю", "запуск"),
        (TelegramProgressPhase::Thinking, _) => ("🧠 Анализирую", "анализ"),
        (TelegramProgressPhase::Context, _) => ("🔎 Собираю контекст", "контекст"),
        (TelegramProgressPhase::Drafting, _) => ("✍️ Пишу ответ", "черновик ответа"),
        (TelegramProgressPhase::Continuation, _) => ("🔁 Продолжаю", "продолжение"),
        (TelegramProgressPhase::Tool, Some(ToolExecutionStatus::WaitingApproval)) => {
            ("🛂 Жду подтверждение", "ожидаю апрув")
        }
        (TelegramProgressPhase::Tool, Some(ToolExecutionStatus::Failed)) => {
            ("⚠️ Ошибка инструмента", "инструменты")
        }
        (TelegramProgressPhase::Tool, _) => (
            render_tool_phase_title(state.current_tool_name.as_deref()),
            "инструменты",
        ),
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
    if let Some(context) = state.current_context_summary.as_deref() {
        lines.push(format!(
            "Контекст: {}",
            render_status_detail(context, detail_char_cap)
        ));
    }
    if let Some(summary) = state.current_tool_summary.as_deref() {
        lines.push(format!(
            "Деталь: {}",
            render_status_detail(summary, detail_char_cap)
        ));
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

fn render_tool_phase_title(tool_name: Option<&str>) -> &'static str {
    let Some(tool_name) = tool_name else {
        return "🔧 Работаю с инструментами";
    };
    if tool_name.starts_with("memory_") {
        return "🧠 Работаю с памятью";
    }
    if tool_name.starts_with("kv_") {
        return "🗃️ Работаю с KV";
    }
    if tool_name.contains("silverbullet") {
        return "📝 Работаю с SilverBullet";
    }
    if tool_name.starts_with("skill_") {
        return "🧩 Работаю со skills";
    }
    if tool_name.starts_with("browser_") || tool_name.starts_with("web_") {
        return "🌐 Работаю с вебом";
    }
    if tool_name == "deliver_file" {
        return "📎 Готовлю файл";
    }
    "🔧 Работаю с инструментами"
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

fn render_status_detail(summary: &str, detail_char_cap: usize) -> String {
    let compact = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    let total_chars = compact.chars().count();
    if total_chars <= detail_char_cap {
        return escape_telegram_html(&compact);
    }

    let visible: String = compact.chars().take(detail_char_cap).collect();
    format!(
        "{}… (обрезано, ещё {} симв.)",
        escape_telegram_html(&visible),
        total_chars.saturating_sub(detail_char_cap)
    )
}

fn escape_telegram_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) fn render_file_delivery_failed_html(
    file_name: &str,
    error: &str,
    detail_char_cap: usize,
) -> String {
    [
        "<b>⚠️ Файл не отправлен</b>".to_string(),
        "Не удалось отправить файл через Telegram.".to_string(),
        format!(
            "Файл: <code>{}</code>",
            escape_telegram_html(file_name.trim())
        ),
        format!("Деталь: {}", render_status_detail(error, detail_char_cap)),
        "Запрос доставки помечен как failed; файл остался artifact'ом текущей session.".to_string(),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_status_renders_context_events() {
        let mut tracker = TelegramProgressTracker::default();
        assert!(tracker.apply(&ChatExecutionEvent::ContextStatus {
            label: "skills".to_string(),
            summary: "Использую skills: mem0-memory (auto)".to_string(),
        }));

        let rendered = render_temporary_status_html(tracker.state(), 700);

        assert!(rendered.contains("Собираю контекст"));
        assert!(rendered.contains("Контекст: Использую skills"));
        assert!(rendered.contains("Вызовы: 0"));
    }

    #[test]
    fn progress_status_classifies_memory_and_kv_tools() {
        let mut tracker = TelegramProgressTracker::default();
        tracker.apply(&ChatExecutionEvent::ToolStatus {
            tool_call_id: "call-1".to_string(),
            tool_name: "memory_search".to_string(),
            summary: "memory_search query=погода".to_string(),
            status: ToolExecutionStatus::Running,
        });
        let rendered_memory = render_temporary_status_html(tracker.state(), 700);
        assert!(rendered_memory.contains("Работаю с памятью"));

        tracker.apply(&ChatExecutionEvent::ToolStatus {
            tool_call_id: "call-2".to_string(),
            tool_name: "kv_get".to_string(),
            summary: "kv_get scope=operator key=selected_agent".to_string(),
            status: ToolExecutionStatus::Running,
        });
        let rendered_kv = render_temporary_status_html(tracker.state(), 700);
        assert!(rendered_kv.contains("Работаю с KV"));
        assert!(rendered_kv.contains("Вызовы: 2"));
    }
}
