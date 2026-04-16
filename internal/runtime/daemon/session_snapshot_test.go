package daemon

import (
	"context"
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
