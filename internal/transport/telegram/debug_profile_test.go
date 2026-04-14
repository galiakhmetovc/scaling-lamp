package telegram

import (
	"context"
	"os"
	"strings"
	"testing"

	"teamd/internal/mcp"
	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
	"teamd/internal/worker"
)

type debugProfileStoreStub struct {
	messages            []provider.Message
	checkpoint          worker.Checkpoint
	checkpointOK        bool
	saveCheckpointCalls int
}

func (s *debugProfileStoreStub) Append(chatID int64, msg provider.Message) error { return nil }
func (s *debugProfileStoreStub) Messages(chatID int64) ([]provider.Message, error) {
	out := make([]provider.Message, len(s.messages))
	copy(out, s.messages)
	return out, nil
}
func (s *debugProfileStoreStub) Reset(chatID int64) error { return nil }
func (s *debugProfileStoreStub) Checkpoint(chatID int64) (worker.Checkpoint, bool, error) {
	return s.checkpoint, s.checkpointOK, nil
}
func (s *debugProfileStoreStub) SaveCheckpoint(chatID int64, checkpoint worker.Checkpoint) error {
	s.saveCheckpointCalls++
	s.checkpoint = checkpoint
	s.checkpointOK = true
	return nil
}
func (s *debugProfileStoreStub) ActiveSession(chatID int64) (string, error) { return "1001:debug", nil }
func (s *debugProfileStoreStub) CreateSession(chatID int64, session string) error {
	return nil
}
func (s *debugProfileStoreStub) UseSession(chatID int64, session string) error { return nil }
func (s *debugProfileStoreStub) ListSessions(chatID int64) ([]string, error) {
	return []string{"1001:debug"}, nil
}

func TestDebugProfileConversationStoreTrimsToActiveTurnAndSuppressesCheckpoint(t *testing.T) {
	base := &debugProfileStoreStub{
		messages: []provider.Message{
			{Role: "user", Content: "first"},
			{Role: "assistant", Content: "first answer"},
			{Role: "user", Content: "second"},
			{Role: "assistant", Content: "tool call"},
			{Role: "tool", Name: "shell.exec", Content: "tool output"},
		},
		checkpoint:   worker.Checkpoint{WhatHappened: "older summary"},
		checkpointOK: true,
	}
	store := debugProfileConversationStore{
		base: base,
		profile: runtimex.DebugExecutionProfile{
			Transcript: false,
			Checkpoint: false,
		},
	}

	gotMessages, err := store.Messages(1001)
	if err != nil {
		t.Fatalf("messages: %v", err)
	}
	if len(gotMessages) != 3 {
		t.Fatalf("expected active turn only, got %#v", gotMessages)
	}
	if gotMessages[0].Role != "user" || gotMessages[0].Content != "second" {
		t.Fatalf("expected active turn to start from last user, got %#v", gotMessages)
	}
	if _, ok, err := store.Checkpoint(1001); err != nil || ok {
		t.Fatalf("expected checkpoint to be hidden, ok=%v err=%v", ok, err)
	}
	if err := store.SaveCheckpoint(1001, worker.Checkpoint{WhatHappened: "new"}); err != nil {
		t.Fatalf("save checkpoint: %v", err)
	}
	if base.saveCheckpointCalls != 0 {
		t.Fatalf("expected checkpoint save suppression, got %d calls", base.saveCheckpointCalls)
	}
}

func TestConversationHooksRespectDebugProfileForPromptLayersAndTools(t *testing.T) {
	workspaceDir := t.TempDir()
	if err := os.WriteFile(workspaceDir+"/AGENTS.md", []byte("Agent rule: stay concise."), 0o644); err != nil {
		t.Fatalf("write AGENTS: %v", err)
	}

	adapter := New(Deps{
		WorkspaceRoot: workspaceDir,
		Tools: &fakeToolRuntime{
			tools: []mcp.Tool{
				{Name: "shell.exec", Description: "exec"},
			},
		},
	})
	profile := runtimex.DefaultDebugExecutionProfile()
	profile.Workspace = false
	profile.SessionHead = false
	profile.RecentWork = false
	profile.MemoryRecall = false
	profile.Skills = false
	profile.Tools = false
	adapter.rememberDebugProfile("run-1", &profile)

	ctx := withDebugRunID(context.Background(), "run-1")
	hooks := adapter.conversationHooks(ctx, 1001)
	tools, err := hooks.ProviderTools("telegram")
	if err != nil {
		t.Fatalf("provider tools: %v", err)
	}
	if len(tools) != 0 {
		t.Fatalf("expected tools to be disabled, got %#v", tools)
	}
	build, err := hooks.BuildPromptContext(1001, []provider.Message{{Role: "user", Content: "hello"}})
	if err != nil {
		t.Fatalf("build prompt context: %v", err)
	}
	if len(build.Messages) != 1 || build.Messages[0].Role != "user" {
		t.Fatalf("expected only user message, got %#v", build.Messages)
	}
	if len(build.Layers) != 0 {
		t.Fatalf("expected no prompt layers, got %#v", build.Layers)
	}
}

func TestConversationHooksRespectDebugProfileAllowedToolsAndWorkspaceFiles(t *testing.T) {
	workspaceDir := t.TempDir()
	if err := os.WriteFile(workspaceDir+"/AGENTS.md", []byte("Agent rule: stay concise."), 0o644); err != nil {
		t.Fatalf("write AGENTS: %v", err)
	}
	if err := os.MkdirAll(workspaceDir+"/docs", 0o755); err != nil {
		t.Fatalf("mkdir docs: %v", err)
	}
	if err := os.WriteFile(workspaceDir+"/docs/guide.md", []byte("Guide body"), 0o644); err != nil {
		t.Fatalf("write guide: %v", err)
	}

	adapter := New(Deps{
		WorkspaceRoot: workspaceDir,
		Tools: &fakeToolRuntime{
			tools: []mcp.Tool{
				{Name: "shell.exec", Description: "exec"},
				{Name: "filesystem.read_file", Description: "read"},
			},
		},
	})
	profile := runtimex.DefaultDebugExecutionProfile()
	profile.AllowedTools = []string{"shell.exec"}
	profile.WorkspaceFiles = []string{"docs/guide.md"}
	profile.SessionHead = false
	profile.RecentWork = false
	profile.MemoryRecall = false
	profile.Skills = false
	adapter.rememberDebugProfile("run-2", &profile)

	ctx := withDebugRunID(context.Background(), "run-2")
	hooks := adapter.conversationHooks(ctx, 1001)
	tools, err := hooks.ProviderTools("telegram")
	if err != nil {
		t.Fatalf("provider tools: %v", err)
	}
	if len(tools) != 1 || tools[0].Name != providerToolName("shell.exec") {
		t.Fatalf("expected shell.exec only, got %#v", tools)
	}
	build, err := hooks.BuildPromptContext(1001, []provider.Message{{Role: "user", Content: "hello"}})
	if err != nil {
		t.Fatalf("build prompt context: %v", err)
	}
	prompt := joinPromptContents(build.Messages)
	if !strings.Contains(prompt, "guide.md") || !strings.Contains(prompt, "Guide body") {
		t.Fatalf("expected selected workspace file in prompt, got %#v", build.Messages)
	}
	if strings.Contains(prompt, "Agent rule: stay concise.") {
		t.Fatalf("expected AGENTS.md to be excluded when workspace files are selected, got %#v", build.Messages)
	}
}
