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

func TestRunVersionCutover(t *testing.T) {
	t.Parallel()

	t.Run("explicit V2 creation", func(t *testing.T) {
		t.Parallel()

		server, agent := newRunVersionTestServer(t)
		mustCreateRunVersionSession(t, agent, "session-v2-create")

		if !server.startMainRunV2("session-v2-create") {
			t.Fatal("startMainRunV2 returned false, want true for explicit V2 creation")
		}

		snapshot, err := server.buildSessionSnapshot("session-v2-create")
		if err != nil {
			t.Fatalf("build session snapshot: %v", err)
		}
		if snapshot.ExecutionVersion != string(executionVersionV2) {
			t.Fatalf("snapshot execution version = %q, want %q", snapshot.ExecutionVersion, executionVersionV2)
		}
		if snapshot.MainRun.ExecutionVersion != string(executionVersionV2) {
			t.Fatalf("main run execution version = %q, want %q", snapshot.MainRun.ExecutionVersion, executionVersionV2)
		}
	})

	t.Run("existing V1 session remains V1 until migrated", func(t *testing.T) {
		t.Parallel()

		server, agent := newRunVersionTestServer(t)
		mustCreateRunVersionSession(t, agent, "session-v1")

		if !server.startMainRun("session-v1") {
			t.Fatal("startMainRun returned false, want true")
		}
		server.finishMainRun("session-v1", nil)

		snapshot, err := server.buildSessionSnapshot("session-v1")
		if err != nil {
			t.Fatalf("build session snapshot: %v", err)
		}
		if snapshot.ExecutionVersion != string(executionVersionV1) {
			t.Fatalf("snapshot execution version = %q, want %q", snapshot.ExecutionVersion, executionVersionV1)
		}

		if server.startMainRunV2("session-v1") {
			t.Fatal("startMainRunV2 returned true for V1 session without explicit migration")
		}
		if got := server.sessionExecutionVersion("session-v1"); got != executionVersionV1 {
			t.Fatalf("session execution version = %q, want %q", got, executionVersionV1)
		}

		server.migrateSessionExecutionVersion("session-v1", executionVersionV2)
		if !server.startMainRunV2("session-v1") {
			t.Fatal("startMainRunV2 returned false after explicit migration to V2")
		}
	})
}

func TestNoMixedRunVersion(t *testing.T) {
	t.Parallel()

	t.Run("V2 run rejects V1 handler", func(t *testing.T) {
		t.Parallel()

		server, agent := newRunVersionTestServer(t)
		mustCreateRunVersionSession(t, agent, "session-mixed-v2")

		if !server.startMainRunV2("session-mixed-v2") {
			t.Fatal("startMainRunV2 returned false, want true")
		}
		if server.startMainRun("session-mixed-v2") {
			t.Fatal("startMainRun returned true for session already owned by V2")
		}

		snapshot, err := server.buildSessionSnapshot("session-mixed-v2")
		if err != nil {
			t.Fatalf("build session snapshot: %v", err)
		}
		if snapshot.ExecutionVersion != string(executionVersionV2) {
			t.Fatalf("snapshot execution version = %q, want %q", snapshot.ExecutionVersion, executionVersionV2)
		}
	})

	t.Run("V1 run rejects V2 handler", func(t *testing.T) {
		t.Parallel()

		server, agent := newRunVersionTestServer(t)
		mustCreateRunVersionSession(t, agent, "session-mixed-v1")

		if !server.startMainRun("session-mixed-v1") {
			t.Fatal("startMainRun returned false, want true")
		}
		if server.startMainRunV2("session-mixed-v1") {
			t.Fatal("startMainRunV2 returned true for session already owned by V1")
		}

		snapshot, err := server.buildSessionSnapshot("session-mixed-v1")
		if err != nil {
			t.Fatalf("build session snapshot: %v", err)
		}
		if snapshot.ExecutionVersion != string(executionVersionV1) {
			t.Fatalf("snapshot execution version = %q, want %q", snapshot.ExecutionVersion, executionVersionV1)
		}
	})
}

func newRunVersionTestServer(t *testing.T) (*Server, *runtime.Agent) {
	t.Helper()

	now := time.Date(2026, 4, 18, 20, 0, 0, 0, time.UTC)
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
	return server, agent
}

func mustCreateRunVersionSession(t *testing.T, agent *runtime.Agent, sessionID string) {
	t.Helper()

	if err := agent.RecordEvent(context.Background(), eventing.Event{
		ID:               "evt-" + sessionID,
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       time.Date(2026, 4, 18, 20, 0, 0, 0, time.UTC),
		AggregateID:      sessionID,
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		Payload:          map[string]any{"session_id": sessionID},
	}); err != nil {
		t.Fatalf("record session created: %v", err)
	}
}
