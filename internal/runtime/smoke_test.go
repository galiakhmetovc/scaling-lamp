package runtime_test

import (
	"bytes"
	"context"
	"io"
	"net/http"
	"testing"
	"time"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/provider"
	"teamd/internal/runtime"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestAgentSmokeExecutesProviderClientAndRecordsEvents(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	clock := time.Date(2026, 4, 14, 14, 0, 0, 0, time.UTC)
	idValues := []string{"run-smoke-1", "evt-session-1", "evt-run-start-1", "evt-transport-1", "evt-run-complete-1"}
	nextID := func(prefix string) string {
		if len(idValues) == 0 {
			t.Fatalf("unexpected id request for prefix %q", prefix)
		}
		id := idValues[0]
		idValues = idValues[1:]
		return id
	}

	agent := &runtime.Agent{
		Config: runtimeConfigForSmokeTest(),
		Contracts: smokeContractsForTest(),
		PromptAssets: provider.NewPromptAssetExecutor(),
		RequestShape: provider.NewRequestShapeExecutor(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"X-Test": []string{"ok"}},
					Body: io.NopCloser(bytes.NewBufferString(`{
  "id":"resp-1",
  "model":"glm-5-turbo",
  "choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"pong"}}],
  "usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}
}`)),
				}, nil
			},
		}),
		EventLog:    runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{projections.NewSessionProjection(), projections.NewRunProjection()},
		Now:         func() time.Time { return clock },
		NewID:       nextID,
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.Transport)

	result, err := agent.Smoke(context.Background(), runtime.SmokeInput{Prompt: "ping"})
	if err != nil {
		t.Fatalf("Smoke returned error: %v", err)
	}

	if result.Provider.Message.Content != "pong" {
		t.Fatalf("provider message content = %q, want pong", result.Provider.Message.Content)
	}

	sessionProjection, ok := agent.Projections[0].(*projections.SessionProjection)
	if !ok {
		t.Fatalf("projection type = %T, want *SessionProjection", agent.Projections[0])
	}
	if sessionProjection.Snapshot().SessionID != "smoke:agent-smoke-test" {
		t.Fatalf("session ID = %q, want %q", sessionProjection.Snapshot().SessionID, "smoke:agent-smoke-test")
	}

	runProjection, ok := agent.Projections[1].(*projections.RunProjection)
	if !ok {
		t.Fatalf("projection type = %T, want *RunProjection", agent.Projections[1])
	}
	if runProjection.Snapshot().RunID != "run-smoke-1" {
		t.Fatalf("run ID = %q, want %q", runProjection.Snapshot().RunID, "run-smoke-1")
	}
	if runProjection.Snapshot().Status != projections.RunStatusCompleted {
		t.Fatalf("run status = %q, want %q", runProjection.Snapshot().Status, projections.RunStatusCompleted)
	}

	sessionEvents, err := agent.EventLog.ListByAggregate(context.Background(), eventing.AggregateSession, "smoke:agent-smoke-test")
	if err != nil {
		t.Fatalf("ListByAggregate session returned error: %v", err)
	}
	if len(sessionEvents) != 1 || sessionEvents[0].Kind != eventing.EventSessionCreated {
		t.Fatalf("session events = %#v, want single session.created", sessionEvents)
	}

	runEvents, err := agent.EventLog.ListByAggregate(context.Background(), eventing.AggregateRun, "run-smoke-1")
	if err != nil {
		t.Fatalf("ListByAggregate run returned error: %v", err)
	}
	if len(runEvents) != 3 {
		t.Fatalf("run events len = %d, want 3", len(runEvents))
	}
	if runEvents[0].Kind != eventing.EventRunStarted {
		t.Fatalf("first run event kind = %q, want %q", runEvents[0].Kind, eventing.EventRunStarted)
	}
	if runEvents[1].Kind != eventing.EventTransportAttemptCompleted {
		t.Fatalf("second run event kind = %q, want %q", runEvents[1].Kind, eventing.EventTransportAttemptCompleted)
	}
	if runEvents[2].Kind != eventing.EventRunCompleted {
		t.Fatalf("third run event kind = %q, want %q", runEvents[2].Kind, eventing.EventRunCompleted)
	}
}

func runtimeConfigForSmokeTest() config.AgentConfig {
	return config.AgentConfig{
		ID: "agent-smoke-test",
	}
}

func smokeContractsForTest() contracts.ResolvedContracts {
	return contracts.ResolvedContracts{
		ProviderRequest: contracts.ProviderRequestContract{
			Transport: contracts.TransportContract{
				ID: "transport-smoke",
				Endpoint: contracts.EndpointPolicy{
					Enabled: true,
					Strategy: "static",
					Params: contracts.EndpointParams{
						BaseURL: "https://api.z.ai/api/coding/paas/v4",
						Path: "/chat/completions",
						Method: http.MethodPost,
					},
				},
				Auth: contracts.AuthPolicy{
					Enabled: true,
					Strategy: "bearer_token",
					Params: contracts.AuthParams{
						Header: "Authorization",
						Prefix: "Bearer",
						ValueEnvVar: "TEAMD_ZAI_API_KEY",
					},
				},
			},
			RequestShape: contracts.RequestShapeContract{
				ID: "request-shape-smoke",
				Model: contracts.ModelPolicy{Enabled: true, Strategy: "static_model", Params: contracts.ModelParams{Model: "glm-5-turbo"}},
				Messages: contracts.MessagePolicy{Enabled: true, Strategy: "raw_messages"},
				Tools: contracts.ToolPolicy{Enabled: true, Strategy: "tools_inline"},
				Streaming: contracts.StreamingPolicy{Enabled: true, Strategy: "static_stream", Params: contracts.StreamingParams{Stream: false}},
			},
		},
		PromptAssets: contracts.PromptAssetsContract{
			ID: "prompt-assets-smoke",
			PromptAsset: contracts.PromptAssetPolicy{
				Enabled: true,
				Strategy: "inline_assets",
				Params: contracts.PromptAssetParams{Assets: []contracts.PromptAsset{}},
			},
		},
	}
}

type fakeDoer struct {
	do func(req *http.Request) (*http.Response, error)
}

func (f fakeDoer) Do(req *http.Request) (*http.Response, error) {
	return f.do(req)
}
