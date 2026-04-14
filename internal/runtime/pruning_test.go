package runtime

import (
	"strings"
	"testing"

	"teamd/internal/provider"
)

func TestPrunePromptResidencyPreservesActiveTail(t *testing.T) {
	raw := []provider.Message{
		{Role: "tool", Name: "shell.exec", Content: strings.Repeat("noise-", 40)},
		{Role: "assistant", Content: "older answer"},
		{Role: "user", Content: "current task"},
		{Role: "tool", Name: "shell.exec", Content: "fresh output"},
	}
	pruned := PrunePromptResidency(raw, 48)
	if pruned[2].Content != "current task" || pruned[3].Content != "fresh output" {
		t.Fatalf("expected active tail to remain untouched: %+v", pruned)
	}
	if pruned[0].Content == raw[0].Content {
		t.Fatalf("expected old tool output to be pruned: %+v", pruned)
	}
	if raw[0].Content != strings.Repeat("noise-", 40) {
		t.Fatalf("expected original transcript message to remain unchanged: %+v", raw)
	}
}

func TestPrunePromptResidencyCollapsesOldAssistantToolWrapper(t *testing.T) {
	raw := []provider.Message{
		{Role: "assistant", ToolCalls: []provider.ToolCall{{ID: "call-1", Name: "shell.exec"}}},
		{Role: "user", Content: "current task"},
	}
	pruned := PrunePromptResidency(raw, 64)
	if !strings.Contains(pruned[0].Content, "assistant requested tools: shell.exec") {
		t.Fatalf("expected old assistant tool wrapper to collapse: %+v", pruned[0])
	}
	if len(pruned[0].ToolCalls) != 0 {
		t.Fatalf("expected tool calls removed from residency copy: %+v", pruned[0])
	}
}
