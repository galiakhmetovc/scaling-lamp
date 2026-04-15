package runtime_test

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"testing"
	"time"

	"teamd/internal/contracts"
	"teamd/internal/delegation"
	"teamd/internal/filesystem"
	"teamd/internal/provider"
	"teamd/internal/runtime"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
	"teamd/internal/tools"
)

type delegateToolRuntimeStub struct{}

func (delegateToolRuntimeStub) Spawn(_ context.Context, req runtime.DelegateSpawnRequest) (runtime.DelegateView, error) {
	now := time.Date(2026, 4, 15, 15, 30, 0, 0, time.UTC)
	return runtime.DelegateView{
		DelegateID:     "delegate-1",
		Backend:        req.Backend,
		OwnerSessionID: req.OwnerSessionID,
		Status:         runtime.DelegateStatusRunning,
		PolicySnapshot: req.PolicySnapshot,
		CreatedAt:      now,
		UpdatedAt:      now,
	}, nil
}

func (delegateToolRuntimeStub) Message(_ context.Context, delegateID string, _ runtime.DelegateMessageRequest) (runtime.DelegateView, error) {
	now := time.Date(2026, 4, 15, 15, 30, 1, 0, time.UTC)
	return runtime.DelegateView{
		DelegateID: delegateID,
		Backend:    runtime.DelegateBackendLocalWorker,
		Status:     runtime.DelegateStatusRunning,
		CreatedAt:  now,
		UpdatedAt:  now,
	}, nil
}

func (delegateToolRuntimeStub) Wait(_ context.Context, req runtime.DelegateWaitRequest) (runtime.DelegateWaitResult, bool, error) {
	now := time.Date(2026, 4, 15, 15, 30, 2, 0, time.UTC)
	return runtime.DelegateWaitResult{
		Delegate: runtime.DelegateView{
			DelegateID:     req.DelegateID,
			Backend:        runtime.DelegateBackendLocalWorker,
			OwnerSessionID: "session-chat-delegate-1",
			Status:         runtime.DelegateStatusIdle,
			PolicySnapshot: map[string]any{"backend": "local_worker"},
			CreatedAt:      now,
			UpdatedAt:      now,
		},
		Handoff: &runtime.DelegateHandoff{
			DelegateID:          req.DelegateID,
			Backend:             runtime.DelegateBackendLocalWorker,
			Summary:             "Subagent result.",
			RecommendedNextStep: "review delegate output",
			CreatedAt:           now,
			UpdatedAt:           now,
		},
		Messages: []runtime.DelegateMessage{
			{Cursor: 1, Role: "assistant", Content: "Subagent result."},
		},
		Events:         []runtime.DelegateEventRef{{EventID: 7, Kind: "delegate.completed"}},
		NextCursor:     1,
		NextEventAfter: 7,
	}, true, nil
}

func (delegateToolRuntimeStub) Close(_ context.Context, delegateID string) (runtime.DelegateView, bool, error) {
	now := time.Date(2026, 4, 15, 15, 30, 3, 0, time.UTC)
	return runtime.DelegateView{
		DelegateID: delegateID,
		Backend:    runtime.DelegateBackendLocalWorker,
		Status:     runtime.DelegateStatusClosed,
		CreatedAt:  now,
		UpdatedAt:  now,
		ClosedAt:   &now,
	}, true, nil
}

func (delegateToolRuntimeStub) Handoff(_ context.Context, delegateID string) (runtime.DelegateHandoff, bool, error) {
	now := time.Date(2026, 4, 15, 15, 30, 4, 0, time.UTC)
	return runtime.DelegateHandoff{
		DelegateID:          delegateID,
		Backend:             runtime.DelegateBackendLocalWorker,
		Summary:             "Subagent result.",
		RecommendedNextStep: "review delegate output",
		CreatedAt:           now,
		UpdatedAt:           now,
	}, true, nil
}

func TestAgentChatTurnExecutesDelegationLifecycleAndReturnsFinalAssistantMessage(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	clock := time.Date(2026, 4, 15, 15, 30, 0, 0, time.UTC)
	idValues := []string{
		"session-chat-delegate-1",
		"run-chat-delegate-1", "evt-session-delegate-1", "evt-msg-user-delegate-1", "evt-run-start-delegate-1",
		"evt-provider-request-delegate-1", "evt-transport-delegate-1", "evt-tool-call-started-delegate-1", "evt-tool-call-completed-delegate-1",
		"evt-provider-request-delegate-2", "evt-transport-delegate-2", "evt-tool-call-started-delegate-2", "evt-tool-call-completed-delegate-2",
		"evt-provider-request-delegate-3", "evt-transport-delegate-3", "evt-msg-assistant-delegate-1", "evt-run-complete-delegate-1",
	}
	nextID := func(prefix string) string {
		if len(idValues) == 0 {
			t.Fatalf("unexpected id request for prefix %q", prefix)
		}
		id := idValues[0]
		idValues = idValues[1:]
		return id
	}

	var thirdRequest map[string]any
	call := 0
	agent := &runtime.Agent{
		Config:          chatRuntimeConfigForTest(),
		MaxToolRounds:   4,
		Contracts:       chatContractsForDelegationToolLoopTest(),
		PromptAssets:    provider.NewPromptAssetExecutor(),
		RequestShape:    provider.NewRequestShapeExecutor(),
		PlanTools:       tools.NewPlanToolExecutor(),
		ToolCatalog:     tools.NewCatalogExecutor(),
		ToolExecution:   tools.NewExecutionGate(),
		DelegateRuntime: delegateToolRuntimeStub{},
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				call++
				if call == 1 {
					return &http.Response{
						StatusCode: http.StatusOK,
						Header:     http.Header{"Content-Type": []string{"application/json"}},
						Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-delegate-1","model":"glm-5-turbo","choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":"","tool_calls":[{"id":"call-delegate-1","function":{"name":"delegate_spawn","arguments":{"prompt":"Investigate tests"}}}]}}]}`)),
					}, nil
				}
				if call == 2 {
					return &http.Response{
						StatusCode: http.StatusOK,
						Header:     http.Header{"Content-Type": []string{"application/json"}},
						Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-delegate-2","model":"glm-5-turbo","choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":"","tool_calls":[{"id":"call-delegate-2","function":{"name":"delegate_wait","arguments":{"delegate_id":"delegate-1","after_cursor":0,"after_event_id":0}}}]}}]}`)),
					}, nil
				}
				defer req.Body.Close()
				if err := json.NewDecoder(req.Body).Decode(&thirdRequest); err != nil {
					t.Fatalf("decode third request body: %v", err)
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-delegate-3","model":"glm-5-turbo","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Delegation done."}}]}`)),
				}, nil
			},
		}),
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewRunProjection(),
			projections.NewTranscriptProjection(),
		},
		Now:   func() time.Time { return clock },
		NewID: nextID,
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)

	session, err := agent.NewChatSession()
	if err != nil {
		t.Fatalf("NewChatSession returned error: %v", err)
	}
	result, err := agent.ChatTurn(context.Background(), session, runtime.ChatTurnInput{Prompt: "delegate this"})
	if err != nil {
		t.Fatalf("ChatTurn returned error: %v", err)
	}
	if result.Provider.Message.Content != "Delegation done." {
		t.Fatalf("assistant response = %q, want Delegation done.", result.Provider.Message.Content)
	}
	messages, ok := thirdRequest["messages"].([]any)
	if !ok {
		t.Fatalf("third request messages = %#v", thirdRequest["messages"])
	}
	foundWaitTool := false
	for _, raw := range messages {
		msg, ok := raw.(map[string]any)
		if !ok {
			continue
		}
		if msg["role"] == "tool" && msg["name"] == "delegate_wait" {
			content, _ := msg["content"].(string)
			if strings.Contains(content, `"delegate_id":"delegate-1"`) && strings.Contains(content, `"summary":"Subagent result."`) {
				foundWaitTool = true
				break
			}
		}
	}
	if !foundWaitTool {
		t.Fatalf("third request missing delegate_wait tool result: %#v", messages)
	}
}

func chatContractsForDelegationToolLoopTest() contracts.ResolvedContracts {
	out := chatContractsForTest()
	out.ProviderRequest.RequestShape.Streaming = contracts.StreamingPolicy{
		Enabled:  true,
		Strategy: "static_stream",
		Params:   contracts.StreamingParams{Stream: false},
	}
	out.Tools = contracts.ToolContract{
		Catalog: contracts.ToolCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ToolCatalogParams{
				ToolIDs: []string{"delegate_spawn", "delegate_wait"},
			},
		},
		Serialization: contracts.ToolSerializationPolicy{
			Enabled:  true,
			Strategy: "openai_function_tools",
			Params:   contracts.ToolSerializationParams{IncludeDescriptions: true},
		},
	}
	out.DelegationTools = contracts.DelegationToolContract{
		Catalog: contracts.DelegationCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.DelegationCatalogParams{
				ToolIDs: []string{"delegate_spawn", "delegate_wait"},
			},
		},
		Description: contracts.DelegationDescriptionPolicy{
			Enabled:  true,
			Strategy: "static_builtin_descriptions",
			Params: contracts.DelegationDescriptionParams{
				IncludeExamples:       true,
				IncludeBackendHints:   true,
				IncludeLifecycleNotes: true,
			},
		},
	}
	out.DelegationExecution = contracts.DelegationExecutionContract{
		Backend: contracts.DelegationBackendPolicy{
			Enabled:  true,
			Strategy: "backend_allowlist",
			Params: contracts.DelegationBackendParams{
				AllowedBackends: []string{"local_worker"},
				DefaultBackend:  "local_worker",
			},
		},
		Result: contracts.DelegationResultPolicy{
			Enabled:  true,
			Strategy: "bounded_wait_results",
			Params: contracts.DelegationResultParams{
				IncludeEvents:         true,
				IncludeArtifacts:      true,
				IncludePolicySnapshot: true,
				DefaultEventLimit:     25,
				MaxEventLimit:         100,
			},
		},
	}
	out.ToolExecution = contracts.ToolExecutionContract{
		Access: contracts.ToolAccessPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ToolAccessParams{
				ToolIDs: []string{"delegate_spawn", "delegate_wait"},
			},
		},
		Approval: contracts.ToolApprovalPolicy{
			Enabled:  true,
			Strategy: "always_allow",
		},
		Sandbox: contracts.ToolSandboxPolicy{
			Enabled:  true,
			Strategy: "default_runtime",
		},
	}
	return out
}
