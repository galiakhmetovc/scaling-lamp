package compaction

import (
	"strings"
	"testing"

	"teamd/internal/provider"
	"teamd/internal/worker"
)

func TestCheckpointMessageIncludesArchiveAndArtifactRefs(t *testing.T) {
	checkpoint := worker.Checkpoint{
		SessionID:       "telegram:1/default",
		WhatHappened:    "summarized the session",
		WhatMattersNow:  "resume from the archived record",
		ArchiveRefs:     []string{"archive://chat/1001/session/default#messages-1-8"},
		SourceArtifacts: []string{"artifact://tool-output/1"},
	}

	msg, ok := CheckpointPromptMessage(checkpoint)
	if !ok {
		t.Fatal("expected checkpoint message")
	}
	if !strings.Contains(msg.Content, "Archive refs:") ||
		!strings.Contains(msg.Content, "archive://chat/1001/session/default#messages-1-8") ||
		!strings.Contains(msg.Content, "Artifact refs:") ||
		!strings.Contains(msg.Content, "artifact://tool-output/1") {
		t.Fatalf("checkpoint message missing structured refs: %q", msg.Content)
	}
}

func TestAssemblePromptUsesSummaryAndNewestTurnsWithinBudget(t *testing.T) {
	budget := Budget{
		PromptBudgetTokens:  60,
		MaxToolContextChars: 80,
	}

	checkpoint := worker.Checkpoint{
		SessionID:      "telegram:1/default",
		WhatHappened:   "Earlier the user established important deployment context.",
		WhatMattersNow: "Keep the deployment target and rollback requirement in mind.",
	}

	raw := []provider.Message{
		{Role: "user", Content: strings.Repeat("old-", 20)},
		{Role: "assistant", Content: strings.Repeat("older-", 20)},
		{Role: "user", Content: "recent question"},
		{Role: "assistant", Content: "recent answer"},
	}

	got := AssemblePrompt(budget, checkpoint, raw)
	if len(got) == 0 {
		t.Fatal("expected assembled messages")
	}
	if got[0].Role != "system" {
		t.Fatalf("expected checkpoint summary first, got %#v", got[0])
	}
	if got[len(got)-1].Content != "recent answer" {
		t.Fatalf("expected newest raw turn to survive, got %#v", got[len(got)-1])
	}
	for _, msg := range got {
		if msg.Content == strings.Repeat("old-", 20) {
			t.Fatalf("expected oldest message to be trimmed, got %#v", got)
		}
	}
}

func TestAssemblePromptReducesOversizedToolOutput(t *testing.T) {
	budget := Budget{
		PromptBudgetTokens:  200,
		MaxToolContextChars: 32,
	}

	raw := []provider.Message{
		{Role: "tool", Content: strings.Repeat("abcdef", 20), ToolCallID: "tool-1"},
	}

	got := AssemblePrompt(budget, worker.Checkpoint{}, raw)
	if len(got) != 1 {
		t.Fatalf("unexpected output: %#v", got)
	}
	if !strings.Contains(got[0].Content, "truncated") {
		t.Fatalf("expected truncation marker, got %q", got[0].Content)
	}
	if got[0].ToolCallID != "tool-1" {
		t.Fatalf("expected tool call id preserved, got %#v", got[0])
	}
}

func TestAssemblePromptPreservesActiveTurnFromLastUser(t *testing.T) {
	budget := Budget{
		PromptBudgetTokens:  55,
		MaxToolContextChars: 48,
	}

	raw := []provider.Message{
		{Role: "user", Content: strings.Repeat("old-", 24)},
		{Role: "assistant", Content: strings.Repeat("older-", 20)},
		{Role: "user", Content: "давай править AGENTS.md и найди лучшие практики SearXNG"},
		{
			Role:    "assistant",
			Content: "",
			ToolCalls: []provider.ToolCall{
				{ID: "call-1", Name: "shell_exec"},
			},
		},
		{Role: "tool", ToolCallID: "call-1", Content: strings.Repeat("tool-output-", 20)},
		{
			Role:    "assistant",
			Content: "",
			ToolCalls: []provider.ToolCall{
				{ID: "call-2", Name: "filesystem_read_file"},
			},
		},
		{Role: "tool", ToolCallID: "call-2", Content: strings.Repeat("file-output-", 20)},
	}

	got := AssemblePrompt(budget, worker.Checkpoint{}, raw)
	if len(got) != 5 {
		t.Fatalf("expected only active turn to survive, got %#v", got)
	}
	if got[0].Role != "user" || got[0].Content != raw[2].Content {
		t.Fatalf("expected assembled prompt to start from last user, got %#v", got)
	}
	if got[1].Role != "assistant" || len(got[1].ToolCalls) != 1 || got[1].ToolCalls[0].ID != "call-1" {
		t.Fatalf("expected first tool call preserved, got %#v", got[1])
	}
	if got[2].Role != "tool" || got[2].ToolCallID != "call-1" {
		t.Fatalf("expected first tool result preserved, got %#v", got[2])
	}
	if got[3].Role != "assistant" || len(got[3].ToolCalls) != 1 || got[3].ToolCalls[0].ID != "call-2" {
		t.Fatalf("expected second tool call preserved, got %#v", got[3])
	}
	if got[4].Role != "tool" || got[4].ToolCallID != "call-2" {
		t.Fatalf("expected second tool result preserved, got %#v", got[4])
	}
	if !strings.Contains(got[2].Content, "truncated") && !strings.Contains(got[2].Content, "omitted") {
		t.Fatalf("expected reduced tool output marker, got %q", got[2].Content)
	}
}

func TestAssemblePromptSelectsOlderUserSignalOverNoisyNewerPrefix(t *testing.T) {
	budget := Budget{
		PromptBudgetTokens:  90,
		MaxToolContextChars: 48,
	}
	raw := []provider.Message{
		{Role: "user", Content: "important constraint: never touch production gateway"},
		{Role: "tool", Name: "shell.exec", Content: strings.Repeat("0123456789", 40)},
		{Role: "assistant", Content: "recent summary"},
		{Role: "user", Content: "current task"},
		{Role: "assistant", Content: "current answer"},
	}

	got := AssemblePrompt(budget, worker.Checkpoint{}, raw)
	joined := make([]string, 0, len(got))
	for _, msg := range got {
		joined = append(joined, msg.Content)
	}
	text := strings.Join(joined, "\n")
	if !strings.Contains(text, "important constraint: never touch production gateway") {
		t.Fatalf("expected older high-signal user message to survive: %q", text)
	}
	if strings.Contains(text, strings.Repeat("0123456789", 40)) {
		t.Fatalf("expected noisy older tool output to lose residency: %q", text)
	}
}

func TestReduceForCompactionOmitsNoisyToolOutput(t *testing.T) {
	msg := provider.Message{
		Role:    "tool",
		Name:    "shell_exec",
		Content: "Session checkpoint.\nWhat happened: %s\nWhat matters now: %scrypto/elliptic: ScalarMult was called on an invalid pointx509: authority key identifier incorrectly marked criticalhttp2: Transport received Server's graceful shutdown GOAWAYruntime: mmap: too much locked memory",
	}

	got := ReduceForCompaction(msg, 64)
	if strings.Contains(got.Content, "crypto/elliptic") || strings.Contains(got.Content, "http2:") {
		t.Fatalf("expected noisy binary-like output to be omitted, got %q", got.Content)
	}
	if !strings.Contains(got.Content, "tool output omitted") {
		t.Fatalf("expected omission marker, got %q", got.Content)
	}
}

func TestReduceForCompactionDoesNotRewriteNoisyUserInput(t *testing.T) {
	tokenLike := "eyJhbGciOiJSUzUxMiIsInR5cCI6IkpXVCJ9.PpWVJgIXTywtQ4s1uRML5HVLH1Zptsv6U6ipTXY4FafQ4NTUDjD9I8dkLXNH_fT_qwv-kSF37LxmNZBLKUmnEZVnADQAEEzh2kCD5TKvc5OER0tLQ6zKygSBcqDNZYcY6Uq5e78sq0An79FT-yGP1IEdz1zjqKzstPMBm1KQYsJnB8_6UyyCqR2LD8LfMOJ0RiIndwRTKAZv-Aw_dflv6ZoMvnW3oh_SC8izGoUsDzjcEqA0GawxZcL3bOjgdNBqXy1csV8cD_2wEH5tlN_RZpuXEwDqNUfci5tns4g-7w1Xe8slowmzvMh6mIDf_M-hiL7uGys3wVGqovhPXtkSXL8IuUHZa"
	msg := provider.Message{
		Role:    "user",
		Content: "Вот токен\n\n" + tokenLike + "\n\nЭто для timeweb\n\nСохрани его",
	}

	got := ReduceForCompaction(msg, 64)
	if got.Content != msg.Content {
		t.Fatalf("expected user message to survive unchanged, got %q", got.Content)
	}
	if strings.Contains(got.Content, "tool output omitted") {
		t.Fatalf("unexpected omission marker in user content: %q", got.Content)
	}
}
