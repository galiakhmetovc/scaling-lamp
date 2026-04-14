package runtime

import (
	"context"
	"strings"
	"testing"
	"time"

	"teamd/internal/compaction"
	"teamd/internal/provider"
	"teamd/internal/worker"
)

type scriptedProvider struct {
	responses []provider.PromptResponse
	index     int
}

func (p *scriptedProvider) Generate(ctx context.Context, req provider.PromptRequest) (provider.PromptResponse, error) {
	resp := p.responses[p.index]
	if p.index < len(p.responses)-1 {
		p.index++
	}
	return resp, nil
}

type conversationTestStore struct {
	messages []provider.Message
	checkpoint worker.Checkpoint
	savedCheckpoints int
}

func (s *conversationTestStore) Append(chatID int64, msg provider.Message) error {
	s.messages = append(s.messages, msg)
	return nil
}

func (s *conversationTestStore) Messages(chatID int64) ([]provider.Message, error) {
	if len(s.messages) > 0 {
		return append([]provider.Message(nil), s.messages...), nil
	}
	return []provider.Message{{Role: "user", Content: "inspect"}}, nil
}

func (s *conversationTestStore) Checkpoint(chatID int64) (worker.Checkpoint, bool, error) {
	if s.checkpoint.SessionID != "" || s.checkpoint.WhatHappened != "" || s.checkpoint.WhatMattersNow != "" {
		return s.checkpoint, true, nil
	}
	return worker.Checkpoint{}, false, nil
}

func (s *conversationTestStore) SaveCheckpoint(chatID int64, checkpoint worker.Checkpoint) error {
	s.savedCheckpoints++
	s.checkpoint = checkpoint
	return nil
}

func (s *conversationTestStore) ActiveSession(chatID int64) (string, error) {
	return "1001:default", nil
}

func TestExecuteConversationStoresShapedToolResult(t *testing.T) {
	store := &conversationTestStore{}
	prov := &scriptedProvider{
		responses: []provider.PromptResponse{
			{
				FinishReason: "tool_calls",
				ToolCalls: []provider.ToolCall{{
					ID:   "call-1",
					Name: "shell.exec",
					Arguments: map[string]any{
						"command": "printf hello",
					},
				}},
			},
			{
				FinishReason: "stop",
				Text:         "done",
			},
		},
	}

	_, err := ExecuteConversation(context.Background(), 1001, ConversationHooks{
		Provider: prov,
		Store:    store,
		Budget:   compaction.Budget{},
		Compactor: compaction.New(compaction.Deps{}),
		ProviderTools: func(role string) ([]provider.ToolDefinition, error) {
			return []provider.ToolDefinition{{Name: "shell.exec"}}, nil
		},
		RequestConfig: func(chatID int64) provider.RequestConfig { return provider.RequestConfig{} },
		InjectPromptContext: func(chatID int64, messages []provider.Message) ([]provider.Message, error) {
			return messages, nil
		},
		ExecuteTool: func(ctx context.Context, chatID int64, call provider.ToolCall) (string, error) {
			return "very large raw output", nil
		},
		ShapeToolResult: func(chatID int64, call provider.ToolCall, content string) (OffloadedToolResult, error) {
			return OffloadedToolResult{
				Content:     "tool output offloaded\nartifact_ref: artifact://tool-output-1",
				ArtifactRef: "artifact://tool-output-1",
				Offloaded:   true,
			}, nil
		},
		LastUserMessage: func(messages []provider.Message) string { return "inspect" },
		ToolCallSignature: func(call provider.ToolCall) string {
			return call.Name
		},
		SyntheticLoopOutput: func(call provider.ToolCall, repeatedCount int) string {
			return ""
		},
	})
	if err != nil {
		t.Fatalf("ExecuteConversation: %v", err)
	}

	var sawTool bool
	for _, msg := range store.messages {
		if msg.Role != "tool" {
			continue
		}
		sawTool = true
		if msg.Content != "tool output offloaded\nartifact_ref: artifact://tool-output-1" {
			t.Fatalf("unexpected shaped tool content: %q", msg.Content)
		}
	}
	if !sawTool {
		t.Fatal("expected tool message to be stored")
	}
}

func TestExecuteConversationPassesOffloadedResultToHook(t *testing.T) {
	store := &conversationTestStore{}
	prov := &scriptedProvider{
		responses: []provider.PromptResponse{
			{
				FinishReason: "tool_calls",
				ToolCalls: []provider.ToolCall{{
					ID:   "call-1",
					Name: "shell.exec",
				}},
			},
			{FinishReason: "stop", Text: "done"},
		},
	}

	var got OffloadedToolResult
	_, err := ExecuteConversation(context.Background(), 1001, ConversationHooks{
		Provider:  prov,
		Store:     store,
		Budget:    compaction.Budget{},
		Compactor: compaction.New(compaction.Deps{}),
		ProviderTools: func(role string) ([]provider.ToolDefinition, error) {
			return []provider.ToolDefinition{{Name: "shell.exec"}}, nil
		},
		RequestConfig: func(chatID int64) provider.RequestConfig { return provider.RequestConfig{} },
		InjectPromptContext: func(chatID int64, messages []provider.Message) ([]provider.Message, error) {
			return messages, nil
		},
		ExecuteTool: func(ctx context.Context, chatID int64, call provider.ToolCall) (string, error) {
			return "very large raw output", nil
		},
		ShapeToolResult: func(chatID int64, call provider.ToolCall, content string) (OffloadedToolResult, error) {
			return OffloadedToolResult{
				Content:     "tool output offloaded\nartifact_ref: artifact://tool-output-1",
				ArtifactRef: "artifact://tool-output-1",
				Offloaded:   true,
			}, nil
		},
		OnToolResult: func(chatID int64, call provider.ToolCall, result OffloadedToolResult, elapsed time.Duration) error {
			got = result
			return nil
		},
		LastUserMessage: func(messages []provider.Message) string { return "inspect" },
		ToolCallSignature: func(call provider.ToolCall) string { return call.Name },
		SyntheticLoopOutput: func(call provider.ToolCall, repeatedCount int) string { return "" },
	})
	if err != nil {
		t.Fatalf("ExecuteConversation: %v", err)
	}
	if got.ArtifactRef != "artifact://tool-output-1" || !got.Offloaded {
		t.Fatalf("unexpected offloaded result: %+v", got)
	}
}

func TestPrepareConversationRoundCompactsOnProjectedPromptBudget(t *testing.T) {
	store := &conversationTestStore{
		messages: []provider.Message{
			{Role: "user", Content: strings.Repeat("u", 80)},
			{Role: "assistant", Content: strings.Repeat("a", 80)},
			{Role: "user", Content: "recent task"},
			{Role: "assistant", Content: "recent answer"},
		},
	}
	hooks := ConversationHooks{
		Store:     store,
		Budget:    compaction.Budget{ContextWindowTokens: 1000, PromptBudgetTokens: 240, CompactionTriggerTokens: 120, MaxToolContextChars: 256},
		Compactor: compaction.New(compaction.Deps{}),
		BuildPromptContext: func(chatID int64, messages []provider.Message) (PromptContextBuild, error) {
			assembler := PromptContextAssembler{
				WorkspaceContext: func(chatID int64) string { return strings.Repeat("workspace ", 12) },
				SessionHead:      func(chatID int64) (string, error) { return strings.Repeat("sessionhead ", 10), nil },
				MemoryRecall: func(chatID int64, messages []provider.Message) (string, error) {
					return strings.Repeat("recall ", 12), nil
				},
				SkillsCatalog: func() (string, error) { return strings.Repeat("skills ", 10), nil },
				ActiveSkills:  func(chatID int64) (string, error) { return strings.Repeat("active ", 10), nil },
			}
			return assembler.Build(chatID, messages)
		},
		InjectPromptContext: func(chatID int64, messages []provider.Message) ([]provider.Message, error) {
			return messages, nil
		},
		LastUserMessage: func(messages []provider.Message) string { return "recent task" },
	}

	_, _, metrics, err := prepareConversationRound(context.Background(), 1001, hooks)
	if err != nil {
		t.Fatalf("prepareConversationRound: %v", err)
	}
	if store.savedCheckpoints == 0 {
		t.Fatal("expected projected prompt budget to trigger compaction")
	}
	if metrics.FinalPromptTokens <= metrics.CompactionTriggerTokens {
		t.Fatalf("expected projected final prompt to exceed trigger: %+v", metrics)
	}
	if metrics.RawTranscriptTokens >= metrics.CompactionTriggerTokens {
		t.Fatalf("expected raw transcript to stay below trigger in this test: %+v", metrics)
	}
}
