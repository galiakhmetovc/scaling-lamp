package daemon

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/runtime"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestSessionSnapshotIncludesMainRunMetadata(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 4, 15, 21, 36, 0, 0, time.UTC)
	agent := &runtime.Agent{
		Config: config.AgentConfig{ID: "daemon-chat"},
		Contracts: contracts.ResolvedContracts{
			ProviderRequest: contracts.ProviderRequestContract{
				Transport: contracts.TransportContract{
					ID: "provider_client",
					Endpoint: contracts.EndpointPolicy{
						Params: contracts.EndpointParams{BaseURL: "https://provider.example.test"},
					},
				},
				RequestShape: contracts.RequestShapeContract{
					Model: contracts.ModelPolicy{Params: contracts.ModelParams{Model: "glm-5-turbo"}},
				},
			},
		},
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewContextBudgetProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewShellCommandProjection(),
			projections.NewDelegateProjection(),
		},
		Now:   func() time.Time { return now },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	server := &Server{
		agent:          agent,
		sessionRuntime: map[string]*sessionRuntimeState{},
		daemonBus:      newDaemonBus(),
	}

	if err := agent.RecordEvent(context.Background(), eventing.Event{
		ID:               "evt-session-created",
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       now,
		AggregateID:      "session-1",
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		Payload:          map[string]any{"session_id": "session-1"},
	}); err != nil {
		t.Fatalf("record session created: %v", err)
	}

	if !server.startMainRun("session-1") {
		t.Fatalf("startMainRun returned false, want true")
	}

	running, err := server.buildSessionSnapshot("session-1")
	if err != nil {
		t.Fatalf("build running snapshot: %v", err)
	}
	if !running.MainRunActive || !running.MainRun.Active {
		t.Fatalf("running main run flags = %+v", running.MainRun)
	}
	if running.MainRun.Provider != "provider.example.test" {
		t.Fatalf("running provider = %q, want provider.example.test", running.MainRun.Provider)
	}
	if running.MainRun.Model != "glm-5-turbo" {
		t.Fatalf("running model = %q, want glm-5-turbo", running.MainRun.Model)
	}
	if running.MainRun.StartedAt.IsZero() {
		t.Fatalf("running started_at is zero")
	}

	server.finishMainRun("session-1", &providerResultPayload{
		Provider:     "provider.example.test",
		Model:        "glm-5-turbo",
		InputTokens:  8,
		OutputTokens: 4,
		TotalTokens:  12,
		Content:      "pong",
	})

	completed, err := server.buildSessionSnapshot("session-1")
	if err != nil {
		t.Fatalf("build completed snapshot: %v", err)
	}
	if completed.MainRunActive || completed.MainRun.Active {
		t.Fatalf("completed main run flags = %+v, want inactive", completed.MainRun)
	}
	if completed.MainRun.InputTokens != 8 || completed.MainRun.OutputTokens != 4 || completed.MainRun.TotalTokens != 12 {
		t.Fatalf("completed token usage = %+v", completed.MainRun)
	}
	if completed.ContextBudget.LastTotalTokens != 12 {
		t.Fatalf("context budget last_total_tokens = %d, want 12", completed.ContextBudget.LastTotalTokens)
	}
}

func TestSessionSnapshotIncludesPromptOverride(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 4, 17, 9, 0, 0, 0, time.UTC)
	dir := t.TempDir()
	promptPath := filepath.Join(dir, "system.txt")
	if err := os.WriteFile(promptPath, []byte("default system prompt"), 0o644); err != nil {
		t.Fatalf("write prompt file: %v", err)
	}

	agent := &runtime.Agent{
		Config: config.AgentConfig{ID: "daemon-chat"},
		Contracts: contracts.ResolvedContracts{
			PromptAssembly: contracts.PromptAssemblyContract{
				SystemPrompt: contracts.SystemPromptPolicy{
					Enabled:  true,
					Strategy: "file_static",
					Params: contracts.SystemPromptParams{
						Path:     promptPath,
						Role:     "system",
						Required: true,
					},
				},
			},
		},
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewSessionPromptProjection(),
		},
		Now:   func() time.Time { return now },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	server := &Server{
		agent:          agent,
		sessionRuntime: map[string]*sessionRuntimeState{},
		daemonBus:      newDaemonBus(),
	}

	if err := agent.RecordEvent(context.Background(), eventing.Event{
		ID:               "evt-session-created",
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       now,
		AggregateID:      "session-1",
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		Payload:          map[string]any{"session_id": "session-1"},
	}); err != nil {
		t.Fatalf("record session created: %v", err)
	}
	if err := agent.SetSessionPromptOverride(context.Background(), "session-1", "session override prompt"); err != nil {
		t.Fatalf("SetSessionPromptOverride: %v", err)
	}

	snapshot, err := server.buildSessionSnapshot("session-1")
	if err != nil {
		t.Fatalf("build session snapshot: %v", err)
	}
	if snapshot.Prompt.Default != "default system prompt" {
		t.Fatalf("prompt default = %q, want default system prompt", snapshot.Prompt.Default)
	}
	if snapshot.Prompt.Override != "session override prompt" {
		t.Fatalf("prompt override = %q, want session override prompt", snapshot.Prompt.Override)
	}
	if snapshot.Prompt.Effective != "session override prompt" {
		t.Fatalf("prompt effective = %q, want session override prompt", snapshot.Prompt.Effective)
	}
	if !snapshot.Prompt.HasOverride {
		t.Fatalf("prompt has_override = false, want true")
	}
}

func TestSessionSnapshotIncludesToolGovernance(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 4, 17, 11, 0, 0, 0, time.UTC)
	agent := &runtime.Agent{
		Config: config.AgentConfig{ID: "daemon-chat"},
		Contracts: contracts.ResolvedContracts{
			ToolExecution: contracts.ToolExecutionContract{
				Access: contracts.ToolAccessPolicy{
					Enabled:  true,
					Strategy: "static_allowlist",
					Params:   contracts.ToolAccessParams{ToolIDs: []string{"fs_read_text", "shell_exec"}},
				},
				Approval: contracts.ToolApprovalPolicy{
					Enabled:  true,
					Strategy: "require_for_destructive",
					Params:   contracts.ToolApprovalParams{DestructiveToolIDs: []string{"shell_exec"}},
				},
			},
			ShellExecution: contracts.ShellExecutionContract{
				Approval: contracts.ShellApprovalPolicy{
					Enabled:  true,
					Strategy: "always_require",
					Params: contracts.ShellApprovalParams{
						AllowPrefixes: []string{"go test"},
						DenyPrefixes:  []string{"go env"},
					},
				},
				Runtime: contracts.ShellRuntimePolicy{
					Enabled:  true,
					Strategy: "workspace_write",
					Params: contracts.ShellRuntimeParams{
						Timeout:        "5s",
						MaxOutputBytes: 4096,
						AllowNetwork:   true,
					},
				},
			},
		},
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewShellCommandProjection(),
			projections.NewDelegateProjection(),
		},
		Now:   func() time.Time { return now },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	server := &Server{
		agent:          agent,
		sessionRuntime: map[string]*sessionRuntimeState{},
		daemonBus:      newDaemonBus(),
	}
	if err := agent.RecordEvent(context.Background(), eventing.Event{
		ID:               "evt-session-created",
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       now,
		AggregateID:      "session-1",
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		Payload:          map[string]any{"session_id": "session-1"},
	}); err != nil {
		t.Fatalf("record session created: %v", err)
	}

	snapshot, err := server.buildSessionSnapshot("session-1")
	if err != nil {
		t.Fatalf("build session snapshot: %v", err)
	}
	if snapshot.ToolGovernance.ApprovalMode != "require_for_destructive" {
		t.Fatalf("approval_mode = %q, want require_for_destructive", snapshot.ToolGovernance.ApprovalMode)
	}
	if len(snapshot.ToolGovernance.AllowedTools) != 2 {
		t.Fatalf("allowed_tools = %#v", snapshot.ToolGovernance.AllowedTools)
	}
	if len(snapshot.ToolGovernance.ShellAllowPrefixes) != 1 || snapshot.ToolGovernance.ShellAllowPrefixes[0] != "go test" {
		t.Fatalf("shell allow prefixes = %#v", snapshot.ToolGovernance.ShellAllowPrefixes)
	}
	if len(snapshot.ToolGovernance.ShellDenyPrefixes) != 1 || snapshot.ToolGovernance.ShellDenyPrefixes[0] != "go env" {
		t.Fatalf("shell deny prefixes = %#v", snapshot.ToolGovernance.ShellDenyPrefixes)
	}
}
