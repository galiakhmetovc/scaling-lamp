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

func TestAgentChatTurnAndResumeSession(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	clock := time.Date(2026, 4, 14, 16, 10, 0, 0, time.UTC)
	idValues := []string{
		"session-chat-1",
		"run-chat-1", "evt-session-1", "evt-msg-user-1", "evt-run-start-1", "evt-provider-request-1", "evt-transport-1", "evt-msg-assistant-1", "evt-run-complete-1",
		"run-chat-2", "evt-msg-user-2", "evt-run-start-2", "evt-provider-request-2", "evt-transport-2", "evt-msg-assistant-2", "evt-run-complete-2",
	}
	nextID := func(prefix string) string {
		if len(idValues) == 0 {
			t.Fatalf("unexpected id request for prefix %q", prefix)
		}
		id := idValues[0]
		idValues = idValues[1:]
		return id
	}

	call := 0
	agent := &runtime.Agent{
		Config:      chatRuntimeConfigForTest(),
		Contracts:   chatContractsForTest(),
		PromptAssets: provider.NewPromptAssetExecutor(),
		RequestShape: provider.NewRequestShapeExecutor(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				call++
				body := `data: {"id":"resp-1","model":"glm-5-turbo","choices":[{"delta":{"role":"assistant","content":"Po"},"finish_reason":""}]}` + "\n\n" +
					`data: {"id":"resp-1","model":"glm-5-turbo","choices":[{"delta":{"content":"ng"},"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}}` + "\n\n" +
					"data: [DONE]\n\n"
				if call == 2 {
					body = `data: {"id":"resp-2","model":"glm-5-turbo","choices":[{"delta":{"role":"assistant","content":"Pa"},"finish_reason":""}]}` + "\n\n" +
						`data: {"id":"resp-2","model":"glm-5-turbo","choices":[{"delta":{"content":"th"},"finish_reason":"stop"}],"usage":{"prompt_tokens":18,"completion_tokens":4,"total_tokens":22}}` + "\n\n" +
						"data: [DONE]\n\n"
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"text/event-stream"}},
					Body:       io.NopCloser(bytes.NewBufferString(body)),
				}, nil
			},
		}),
		EventLog:    runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{projections.NewSessionProjection(), projections.NewRunProjection()},
		Now:         func() time.Time { return clock },
		NewID:       nextID,
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.Transport)

	session, err := agent.NewChatSession()
	if err != nil {
		t.Fatalf("NewChatSession returned error: %v", err)
	}
	if session.SessionID != "session-chat-1" {
		t.Fatalf("session id = %q, want session-chat-1", session.SessionID)
	}

	var deltas []string
	first, err := agent.ChatTurn(context.Background(), session, runtime.ChatTurnInput{
		Prompt: "Ping",
		StreamObserver: func(event provider.StreamEvent) {
			if event.Kind == provider.StreamEventText {
				deltas = append(deltas, event.Text)
			}
		},
	})
	if err != nil {
		t.Fatalf("first ChatTurn returned error: %v", err)
	}
	if first.Provider.Message.Content != "Pong" {
		t.Fatalf("first response = %q, want Pong", first.Provider.Message.Content)
	}
	if len(deltas) != 2 || deltas[0] != "Po" || deltas[1] != "ng" {
		t.Fatalf("deltas = %#v, want [Po ng]", deltas)
	}

	resumed, err := agent.ResumeChatSession(context.Background(), session.SessionID)
	if err != nil {
		t.Fatalf("ResumeChatSession returned error: %v", err)
	}
	if len(resumed.Messages) != 2 {
		t.Fatalf("resumed messages len = %d, want 2", len(resumed.Messages))
	}
	if resumed.Messages[0].Role != "user" || resumed.Messages[0].Content != "Ping" {
		t.Fatalf("resumed first message = %#v", resumed.Messages[0])
	}
	if resumed.Messages[1].Role != "assistant" || resumed.Messages[1].Content != "Pong" {
		t.Fatalf("resumed second message = %#v", resumed.Messages[1])
	}

	second, err := agent.ChatTurn(context.Background(), resumed, runtime.ChatTurnInput{Prompt: "Again"})
	if err != nil {
		t.Fatalf("second ChatTurn returned error: %v", err)
	}
	if second.Provider.Message.Content != "Path" {
		t.Fatalf("second response = %q, want Path", second.Provider.Message.Content)
	}
	if len(resumed.Messages) != 4 {
		t.Fatalf("resumed messages len after second turn = %d, want 4", len(resumed.Messages))
	}

	sessionEvents, err := agent.EventLog.ListByAggregate(context.Background(), eventing.AggregateSession, session.SessionID)
	if err != nil {
		t.Fatalf("ListByAggregate session returned error: %v", err)
	}
	if len(sessionEvents) != 5 {
		t.Fatalf("session events len = %d, want 5", len(sessionEvents))
	}
	if sessionEvents[1].Kind != eventing.EventMessageRecorded || sessionEvents[2].Kind != eventing.EventMessageRecorded {
		t.Fatalf("session message events = %#v", sessionEvents)
	}
	runEvents, err := agent.EventLog.ListByAggregate(context.Background(), eventing.AggregateRun, "run-chat-1")
	if err != nil {
		t.Fatalf("ListByAggregate run returned error: %v", err)
	}
	if len(runEvents) != 4 {
		t.Fatalf("run events len = %d, want 4", len(runEvents))
	}
	if runEvents[1].Kind != eventing.EventProviderRequestCaptured {
		t.Fatalf("second run event kind = %q, want %q", runEvents[1].Kind, eventing.EventProviderRequestCaptured)
	}
	requestPayload, ok := runEvents[1].Payload["request_payload"].(map[string]any)
	if !ok {
		t.Fatalf("captured request payload = %#v, want map", runEvents[1].Payload["request_payload"])
	}
	messages, ok := requestPayload["messages"].([]any)
	if !ok || len(messages) != 1 {
		t.Fatalf("captured request messages = %#v", requestPayload["messages"])
	}
}

func chatRuntimeConfigForTest() config.AgentConfig {
	return config.AgentConfig{ID: "agent-chat-test"}
}

func chatContractsForTest() contracts.ResolvedContracts {
	return contracts.ResolvedContracts{
		ProviderRequest: contracts.ProviderRequestContract{
			Transport: contracts.TransportContract{
				ID: "transport-chat",
				Endpoint: contracts.EndpointPolicy{
					Enabled:  true,
					Strategy: "static",
					Params: contracts.EndpointParams{
						BaseURL: "https://api.z.ai/api/coding/paas/v4",
						Path:    "/chat/completions",
						Method:  http.MethodPost,
					},
				},
				Auth: contracts.AuthPolicy{
					Enabled:  true,
					Strategy: "bearer_token",
					Params: contracts.AuthParams{
						Header:      "Authorization",
						Prefix:      "Bearer",
						ValueEnvVar: "TEAMD_ZAI_API_KEY",
					},
				},
			},
			RequestShape: contracts.RequestShapeContract{
				ID:        "request-shape-chat",
				Model:     contracts.ModelPolicy{Enabled: true, Strategy: "static_model", Params: contracts.ModelParams{Model: "glm-5-turbo"}},
				Messages:  contracts.MessagePolicy{Enabled: true, Strategy: "raw_messages"},
				Tools:     contracts.ToolPolicy{Enabled: true, Strategy: "tools_inline"},
				Streaming: contracts.StreamingPolicy{Enabled: true, Strategy: "static_stream", Params: contracts.StreamingParams{Stream: true}},
			},
		},
		PromptAssets: contracts.PromptAssetsContract{
			ID: "prompt-assets-chat",
			PromptAsset: contracts.PromptAssetPolicy{
				Enabled: true,
				Strategy: "inline_assets",
				Params: contracts.PromptAssetParams{Assets: []contracts.PromptAsset{}},
			},
		},
		ProviderTrace: contracts.ProviderTraceContract{
			ID: "provider-trace-chat",
			Request: contracts.ProviderTracePolicy{
				Enabled:  true,
				Strategy: "inline_request",
				Params: contracts.ProviderTraceParams{
					IncludeRawBody:       true,
					IncludeDecodedPayload: true,
				},
			},
		},
	}
}
