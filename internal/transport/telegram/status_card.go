package telegram

import (
	"fmt"
	"strconv"
	"strings"
	"time"
)

const defaultTracePageLimit = 3200
const maxStatusSteps = 12

func runKeyboard(run *RunState) map[string]any {
	if run == nil {
		return nil
	}
	if run.Completed || run.Failed {
		return map[string]any{
			"inline_keyboard": [][]map[string]string{
				{
					{"text": "Статус", "callback_data": "run:status"},
					{"text": "Удалить", "callback_data": "run:delete"},
				},
			},
		}
	}
	if run.WaitingOn == "provider-timeout" {
		return map[string]any{
			"inline_keyboard": [][]map[string]string{
				{
					{"text": "Продолжить", "callback_data": "timeout:continue:" + run.ID},
					{"text": "Повторить", "callback_data": "timeout:retry:" + run.ID},
				},
				{
					{"text": "Завершить ошибкой", "callback_data": "timeout:fail:" + run.ID},
					{"text": "Отменить", "callback_data": "timeout:cancel:" + run.ID},
				},
			},
		}
	}
	return map[string]any{
		"inline_keyboard": [][]map[string]string{
			{
				{"text": "Статус", "callback_data": "run:status"},
				{"text": "Отменить", "callback_data": "run:cancel"},
			},
		},
	}
}

func formatAck(elapsed time.Duration) string {
	return "✅ Запрос получен. Агент работает\nЖдём ответ: " + formatElapsed(elapsed)
}

func formatStatusCard(run *RunState) string {
	if run == nil {
		return "🧠 Агент не запущен"
	}
	lines := []string{
		"✅ Запрос получен. Агент работает",
		"🧠 Стадия: " + valueOrUnknown(run.Stage),
		"⏱️ Ждём ответ: " + formatElapsed(run.Elapsed(time.Now().UTC())),
	}
	if run.RoundIndex > 0 {
		lines = append(lines, fmt.Sprintf("🔁 Раунд: %d", run.RoundIndex))
	}
	if strings.TrimSpace(run.WaitingOn) != "" {
		lines = append(lines, "⏳ Ожидание: "+run.WaitingOn)
	}
	if run.CurrentTool != "" {
		lines = append(lines, "🛠️ Текущий инструмент: "+run.CurrentTool)
	}
	if run.ContextPercent > 0 {
		lines = append(lines, fmt.Sprintf("🧮 Контекст: %d%%", run.ContextPercent))
	}
	if run.PromptBudgetPercent > 0 {
		lines = append(lines, fmt.Sprintf("📦 Промпт: %d%%", run.PromptBudgetPercent))
	}
	if len(run.Steps) > 0 {
		lines = append(lines, "", "Вызов инструментов:")
		steps := run.Steps
		hidden := 0
		if len(steps) > maxStatusSteps {
			hidden = len(steps) - maxStatusSteps
			steps = steps[len(steps)-maxStatusSteps:]
		}
		if hidden > 0 {
			lines = append(lines, fmt.Sprintf("… скрыто шагов: %d", hidden))
		}
		for _, step := range steps {
			prefix := step.Icon
			if prefix == "" {
				prefix = "•"
			}
			line := fmt.Sprintf("%s %s", prefix, step.Title)
			if step.Detail != "" {
				line += " — " + step.Detail
			}
			lines = append(lines, line)
		}
	}
	if run.CancelRequested {
		lines = append(lines, "", "⚠️ Отмена запрошена")
	}
	if run.Failed {
		lines = append(lines, "", "❌ Ошибка: "+run.FailureText)
	}
	if run.Completed {
		lines = append(lines, "", "✅ Выполнение завершено")
	}
	return strings.Join(lines, "\n")
}

func formatStatusDetails(run *RunState) string {
	if run == nil {
		return "Статус недоступен"
	}
	return strings.Join([]string{
		"📊 Детальный статус",
		"Токены запроса: " + intOrUnknown(run.PromptTokens),
		"Токены ответа: " + intOrUnknown(run.CompletionTokens),
		"Вызовы инструментов: " + intWithDelta(run.ToolCalls, run.ToolCallsDelta),
		"Объём вывода инструментов: " + intWithDelta(run.ToolOutputChars, run.ToolOutputCharsDelta) + " симв.",
		"Время на инструменты: " + durationWithDelta(run.ToolDuration, run.ToolDurationDelta),
		"Оценка контекста: " + intOrUnknown(run.ContextEstimate),
		"Окно контекста: " + percentWithDelta(run.ContextPercent, run.ContextPercentDelta),
		"Prompt budget: " + percentWithDelta(run.PromptBudgetPercent, run.PromptBudgetPercentDelta),
		"System overhead: " + intOrUnknown(run.SystemOverheadTokens),
	}, "\n")
}

func formatTracePages(run *RunState, limit int) []string {
	if run == nil {
		return []string{"Trace недоступен"}
	}
	if limit <= 0 {
		limit = defaultTracePageLimit
	}

	sections := []string{
		"🔎 Mesh trace",
		"Запрос: " + valueOrUnknown(run.Query),
		"Стадия: " + valueOrUnknown(run.Stage),
	}
	for _, entry := range run.Trace {
		block := []string{
			"",
			"[" + valueOrUnknown(entry.Section) + "] " + valueOrUnknown(entry.Summary),
		}
		if strings.TrimSpace(entry.Payload) != "" {
			block = append(block, entry.Payload)
		}
		sections = append(sections, strings.Join(block, "\n"))
	}

	var pages []string
	current := ""
	for _, section := range sections {
		candidate := section
		if current != "" {
			candidate = current + "\n" + section
		}
		if len(candidate) <= limit {
			current = candidate
			continue
		}
		if current != "" {
			pages = append(pages, current)
			current = ""
		}
		if len(section) <= limit {
			current = section
			continue
		}
		chunks := splitTraceSection(section, limit)
		pages = append(pages, chunks[:len(chunks)-1]...)
		current = chunks[len(chunks)-1]
	}
	if current != "" {
		pages = append(pages, current)
	}
	return pages
}

func formatStatusReportPages(run *RunState, limit int) []string {
	pages := []string{formatStatusDetails(run)}
	pages = append(pages, formatTracePages(run, limit)...)
	return pages
}

func splitTraceSection(section string, limit int) []string {
	lines := strings.Split(section, "\n")
	var pages []string
	current := ""
	for _, line := range lines {
		candidate := line
		if current != "" {
			candidate = current + "\n" + line
		}
		if len(candidate) <= limit {
			current = candidate
			continue
		}
		if current != "" {
			pages = append(pages, current)
			current = ""
		}
		for len(line) > limit {
			pages = append(pages, line[:limit])
			line = line[limit:]
		}
		current = line
	}
	if current != "" {
		pages = append(pages, current)
	}
	return pages
}

func formatElapsed(d time.Duration) string {
	if d < 0 {
		d = 0
	}
	total := int(d.Seconds())
	return fmt.Sprintf("%02d:%02d", total/60, total%60)
}

func intWithDelta(total, delta int) string {
	if total <= 0 {
		return "unknown"
	}
	if delta <= 0 {
		return strconv.Itoa(total)
	}
	return fmt.Sprintf("%d (+%d)", total, delta)
}

func percentWithDelta(total, delta int) string {
	if total <= 0 {
		return "unknown"
	}
	if delta <= 0 {
		return fmt.Sprintf("%d%%", total)
	}
	return fmt.Sprintf("%d%% (+%d%%)", total, delta)
}

func durationWithDelta(total, delta time.Duration) string {
	if total <= 0 {
		return "unknown"
	}
	if delta <= 0 {
		return total.Round(time.Millisecond).String()
	}
	return fmt.Sprintf("%s (+%s)", total.Round(time.Millisecond), delta.Round(time.Millisecond))
}
