package telegram

import (
	"fmt"
	"strings"
	"testing"
	"time"
)

func TestFormatStatusCardIncludesStageAndSteps(t *testing.T) {
	text := formatStatusCard(&RunState{
		Stage:          "Читаю логи",
		StartedAt:      time.Now().Add(-5 * time.Second),
		Steps:          []RunStep{{Title: "shell.exec", Detail: "command=journalctl -u nginx", Icon: "🖥️"}},
		CurrentTool:    "shell.exec",
		WaitingOn:      "tool",
		RoundIndex:     2,
		LastProgressAt: time.Now().Add(-2 * time.Second),
		ContextPercent: 41,
		PromptBudgetPercent: 55,
	})
	if !strings.Contains(text, "Читаю логи") || !strings.Contains(text, "Вызов инструментов") || !strings.Contains(text, "Раунд: 2") || !strings.Contains(text, "Ожидание: tool") || !strings.Contains(text, "Промпт: 55%") || strings.Contains(text, "1.") {
		t.Fatalf("unexpected status card: %q", text)
	}
}

func TestFormatStatusCardCapsVisibleSteps(t *testing.T) {
	steps := make([]RunStep, 0, maxStatusSteps+3)
	for i := 0; i < maxStatusSteps+3; i++ {
		steps = append(steps, RunStep{Title: fmt.Sprintf("tool-%d", i), Detail: "detail", Icon: "🖥️"})
	}

	text := formatStatusCard(&RunState{
		Stage:      "Выполняю инструмент",
		StartedAt:  time.Now().Add(-5 * time.Second),
		Steps:      steps,
		CurrentTool:"shell.exec",
	})

	if !strings.Contains(text, "… скрыто шагов: 3") {
		t.Fatalf("expected hidden steps marker, got %q", text)
	}
	if strings.Contains(text, "tool-0") {
		t.Fatalf("expected oldest steps to be dropped from live card, got %q", text)
	}
	if !strings.Contains(text, fmt.Sprintf("tool-%d", maxStatusSteps+2)) {
		t.Fatalf("expected newest step to stay visible, got %q", text)
	}
}

func TestFormatStatusDetailsIncludesBreakdown(t *testing.T) {
	text := formatStatusDetails(&RunState{
		PromptTokens:           120,
		CompletionTokens:       40,
		ToolCalls:              6,
		ToolCallsDelta:         6,
		ToolOutputChars:        300,
		ToolOutputCharsDelta:   300,
		ContextEstimate:        200,
		ContextPercent:         41,
		ContextPercentDelta:    12,
		PromptBudgetPercent:    55,
		PromptBudgetPercentDelta: 7,
		SystemOverheadTokens:   48,
		ToolDuration:           3 * time.Second,
		ToolDurationDelta:      2 * time.Second,
	})
	if !strings.Contains(text, "Токены запроса: 120") || !strings.Contains(text, "Вызовы инструментов: 6 (+6)") || !strings.Contains(text, "Окно контекста: 41% (+12%)") || !strings.Contains(text, "Prompt budget: 55% (+7%)") || !strings.Contains(text, "System overhead: 48") {
		t.Fatalf("unexpected details: %q", text)
	}
}

func TestFormatStatusCardShowsFinalStateAfterReply(t *testing.T) {
	text := formatStatusCard(&RunState{
		Stage:     "Ответ отправлен",
		StartedAt: time.Now().Add(-8 * time.Second),
		Completed: true,
	})
	if !strings.Contains(text, "Ответ отправлен") || !strings.Contains(text, "✅ Выполнение завершено") {
		t.Fatalf("unexpected final status card: %q", text)
	}
}

func TestRunKeyboardShowsDeleteForCompletedRun(t *testing.T) {
	keyboard := runKeyboard(&RunState{Completed: true})
	serialized := fmt.Sprintf("%v", keyboard)
	if !strings.Contains(serialized, "run:delete") || strings.Contains(serialized, "run:cancel") {
		t.Fatalf("unexpected final keyboard: %v", keyboard)
	}
}

func TestFormatTracePagesSplitsLongTrace(t *testing.T) {
	run := &RunState{
		Query: "проверь память",
		Trace: []TraceEntry{
			{Section: "Clarification", Summary: "raw prompt", Payload: strings.Repeat("a", 220)},
			{Section: "Proposal", Summary: "peer-a", Payload: strings.Repeat("b", 220)},
			{Section: "Execution", Summary: "winner", Payload: strings.Repeat("c", 220)},
		},
	}

	pages := formatTracePages(run, 300)
	if len(pages) < 2 {
		t.Fatalf("expected trace to split into multiple pages, got %#v", pages)
	}
	for _, page := range pages {
		if len(page) > 300 {
			t.Fatalf("expected page under limit, got %d chars: %q", len(page), page)
		}
	}
}

func TestFormatStatusReportPagesIncludesDetailsAndTrace(t *testing.T) {
	run := &RunState{
		PromptTokens:     10,
		CompletionTokens: 20,
		Query:            "проверь память",
		Stage:            "Выполняю инструмент",
		Trace: []TraceEntry{
			{Section: "Proposal", Summary: "owner", Payload: "proposal text"},
		},
	}
	pages := formatStatusReportPages(run, 500)
	if len(pages) < 2 {
		t.Fatalf("expected details page plus trace page, got %#v", pages)
	}
	if !strings.Contains(pages[0], "📊 Детальный статус") {
		t.Fatalf("unexpected first page: %q", pages[0])
	}
	if !strings.Contains(pages[1], "🔎 Mesh trace") {
		t.Fatalf("unexpected trace page: %q", pages[1])
	}
}
