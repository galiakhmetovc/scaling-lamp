package promptassembly_test

import (
	"os"
	"path/filepath"
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/promptassembly"
	"teamd/internal/runtime/projections"
)

func TestExecutorBuildPlacesSessionHeadAtMessageZeroAndLoadsSystemPromptFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	systemPromptPath := filepath.Join(dir, "system.md")
	mustWriteFile(t, systemPromptPath, "You are the system prompt.\n")

	executor := promptassembly.NewExecutor()
	got, err := executor.Build(contracts.PromptAssemblyContract{
		ID: "prompt-assembly-main",
		SystemPrompt: contracts.SystemPromptPolicy{
			Enabled:  true,
			Strategy: "file_static",
			Params: contracts.SystemPromptParams{
				Path:                   systemPromptPath,
				Role:                   "system",
				Required:               true,
				TrimTrailingWhitespace: true,
			},
		},
		SessionHead: contracts.SessionHeadPolicy{
			Enabled:  true,
			Strategy: "projection_summary",
			Params: contracts.SessionHeadParams{
				Placement:                   "message0",
				Title:                       "Session head",
				IncludeSessionID:            true,
				IncludeLastUserMessage:      true,
				IncludeLastAssistantMessage: true,
			},
		},
	}, promptassembly.Input{
		SessionID: "session-123",
		Transcript: projections.TranscriptSnapshot{
			Sessions: map[string][]contracts.Message{
				"session-123": {
					{Role: "user", Content: "hello"},
					{Role: "assistant", Content: "hi"},
				},
			},
		},
		RawMessages: []contracts.Message{
			{Role: "user", Content: "current prompt"},
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if len(got) != 3 {
		t.Fatalf("message count = %d, want 3", len(got))
	}
	if got[0].Role != "system" || got[0].Content == "" {
		t.Fatalf("messages[0] = %#v, want non-empty session head", got[0])
	}
	if got[0].Content != "Session head\nsession_id: session-123\nlast_user: hello\nlast_assistant: hi" {
		t.Fatalf("session head = %q", got[0].Content)
	}
	if got[1].Content != "You are the system prompt." {
		t.Fatalf("system prompt = %q, want trimmed file content", got[1].Content)
	}
	if got[2].Content != "current prompt" || got[2].Role != "user" {
		t.Fatalf("raw messages = %#v", got[2:])
	}
}

func TestExecutorBuildUsesTranscriptForSummaryButDoesNotReplayTranscriptAsOutboundMessages(t *testing.T) {
	t.Parallel()

	executor := promptassembly.NewExecutor()
	got, err := executor.Build(contracts.PromptAssemblyContract{
		SessionHead: contracts.SessionHeadPolicy{
			Enabled:  true,
			Strategy: "projection_summary",
			Params: contracts.SessionHeadParams{
				Placement:              "message0",
				Title:                  "Session head",
				IncludeLastUserMessage: true,
			},
		},
	}, promptassembly.Input{
		SessionID: "session-123",
		Transcript: projections.TranscriptSnapshot{
			Sessions: map[string][]contracts.Message{
				"session-123": {
					{Role: "user", Content: "older"},
				},
			},
		},
		RawMessages: []contracts.Message{
			{Role: "user", Content: "latest"},
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if len(got) != 2 {
		t.Fatalf("message count = %d, want 2", len(got))
	}
	if got[0].Content != "Session head\nlast_user: older" {
		t.Fatalf("session head = %q", got[0].Content)
	}
	if got[1].Content != "latest" {
		t.Fatalf("outbound raw message = %q, want latest", got[1].Content)
	}
}

func TestExecutorBuildIncludesCompactPlanSummaryInSessionHead(t *testing.T) {
	t.Parallel()

	executor := promptassembly.NewExecutor()
	got, err := executor.Build(contracts.PromptAssemblyContract{
		SessionHead: contracts.SessionHeadPolicy{
			Enabled:  true,
			Strategy: "projection_summary",
			Params: contracts.SessionHeadParams{
				Placement:                   "message0",
				Title:                       "Session head",
				IncludeSessionID:            true,
				IncludeLastUserMessage:      true,
				IncludeLastAssistantMessage: true,
			},
		},
	}, promptassembly.Input{
		SessionID: "session-123",
		Transcript: projections.TranscriptSnapshot{
			Sessions: map[string][]contracts.Message{
				"session-123": {
					{Role: "user", Content: "older"},
					{Role: "assistant", Content: "done"},
				},
			},
		},
		PlanHead: projections.PlanHeadSnapshot{
			Plan: projections.PlanView{
				ID:     "plan-1",
				Goal:   "Refactor auth",
				Status: "active",
			},
			Tasks: map[string]projections.PlanTaskView{
				"t1": {ID: "t1", Description: "Design schema", Status: "done", Order: 1},
				"t2": {ID: "t2", Description: "Write middleware", Status: "todo", Order: 2},
				"t3": {ID: "t3", Description: "Write tests", Status: "blocked", Order: 3, BlockedReason: "waiting for Vasya"},
			},
			Ready:                 map[string]bool{"t2": true},
			WaitingOnDependencies: map[string]bool{},
			Blocked:               map[string]string{"t3": "waiting for Vasya"},
			Notes:                 map[string][]string{"t2": {"Roles are still cached."}},
		},
		RawMessages: []contracts.Message{
			{Role: "user", Content: "latest"},
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if got[0].Content != "Session head\nsession_id: session-123\nlast_user: older\nlast_assistant: done\n🎯 Цель: Refactor auth\n✅ [t1] Design schema\n⬜ [t2] Write middleware\n📝 Roles are still cached.\n🚫 [t3] Write tests (Blocked: waiting for Vasya)" {
		t.Fatalf("session head with plan = %q", got[0].Content)
	}
}

func TestExecutorBuildTrimsLastUserAndAssistantMessagesPerPolicy(t *testing.T) {
	t.Parallel()

	executor := promptassembly.NewExecutor()
	got, err := executor.Build(contracts.PromptAssemblyContract{
		SessionHead: contracts.SessionHeadPolicy{
			Enabled:  true,
			Strategy: "projection_summary",
			Params: contracts.SessionHeadParams{
				Placement:                   "message0",
				Title:                       "Session head",
				IncludeLastUserMessage:      true,
				IncludeLastAssistantMessage: true,
				MaxUserChars:                8,
				MaxAssistantChars:           10,
			},
		},
	}, promptassembly.Input{
		SessionID: "session-123",
		Transcript: projections.TranscriptSnapshot{
			Sessions: map[string][]contracts.Message{
				"session-123": {
					{Role: "user", Content: "1234567890"},
					{Role: "assistant", Content: "abcdefghijklm"},
				},
			},
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if got[0].Content != "Session head\nlast_user: 12345678…\nlast_assistant: abcdefghij…" {
		t.Fatalf("trimmed session head = %q", got[0].Content)
	}
}

func TestExecutorBuildUsesCompactPlanSummaryWhenEnabled(t *testing.T) {
	t.Parallel()

	executor := promptassembly.NewExecutor()
	got, err := executor.Build(contracts.PromptAssemblyContract{
		SessionHead: contracts.SessionHeadPolicy{
			Enabled:  true,
			Strategy: "projection_summary",
			Params: contracts.SessionHeadParams{
				Placement:                   "message0",
				Title:                       "Session head",
				IncludeSessionID:            true,
				IncludeLastUserMessage:      true,
				IncludeLastAssistantMessage: true,
				CompactPlan:                 true,
			},
		},
	}, promptassembly.Input{
		SessionID: "session-123",
		Transcript: projections.TranscriptSnapshot{
			Sessions: map[string][]contracts.Message{
				"session-123": {
					{Role: "user", Content: "older"},
					{Role: "assistant", Content: "done"},
				},
			},
		},
		PlanHead: projections.PlanHeadSnapshot{
			Plan: projections.PlanView{
				ID:     "plan-1",
				Goal:   "Refactor auth",
				Status: "active",
			},
			Tasks: map[string]projections.PlanTaskView{
				"t1": {ID: "t1", Description: "Design schema", Status: "done", Order: 1},
				"t2": {ID: "t2", Description: "Write middleware", Status: "todo", Order: 2},
				"t3": {ID: "t3", Description: "Write tests", Status: "blocked", Order: 3, BlockedReason: "waiting for Vasya"},
				"t4": {ID: "t4", Description: "Review rollout", Status: "in_progress", Order: 4},
			},
			Ready:                 map[string]bool{"t2": true},
			WaitingOnDependencies: map[string]bool{},
			Blocked:               map[string]string{"t3": "waiting for Vasya"},
			Notes:                 map[string][]string{"t2": {"Roles are still cached."}},
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if got[0].Content != "Session head\nsession_id: session-123\nlast_user: older\nlast_assistant: done\n🎯 Цель: Refactor auth\n📊 Прогресс: 1 todo | 1 in_progress | 1 done | 1 blocked\n🏃 Текущая: [t4] Review rollout\n⚠️ Blocked: [t3] waiting for Vasya" {
		t.Fatalf("compact session head = %q", got[0].Content)
	}
}

func TestExecutorBuildFailsWhenRequiredSystemPromptFileIsMissing(t *testing.T) {
	t.Parallel()

	executor := promptassembly.NewExecutor()
	_, err := executor.Build(contracts.PromptAssemblyContract{
		SystemPrompt: contracts.SystemPromptPolicy{
			Enabled:  true,
			Strategy: "file_static",
			Params: contracts.SystemPromptParams{
				Path:     "/does/not/exist/system.md",
				Required: true,
			},
		},
	}, promptassembly.Input{})
	if err == nil {
		t.Fatal("Build error = nil, want missing required prompt file error")
	}
}

func mustWriteFile(t *testing.T, path, body string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("WriteFile(%q): %v", path, err)
	}
}
