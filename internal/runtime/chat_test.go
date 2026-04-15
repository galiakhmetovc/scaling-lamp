package runtime_test

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/delegation"
	"teamd/internal/filesystem"
	"teamd/internal/provider"
	"teamd/internal/runtime"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
	"teamd/internal/tools"
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
		Config:        chatRuntimeConfigForTest(),
		Contracts:     chatContractsForTest(),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
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
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, tools.NewPlanToolExecutor(), filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)

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

func TestAgentResumeChatSessionAllowsEmptyPersistedSession(t *testing.T) {
	clock := time.Date(2026, 4, 15, 18, 40, 0, 0, time.UTC)
	idValues := []string{"session-chat-empty-1", "evt-session-created-1"}
	nextID := func(prefix string) string {
		if len(idValues) == 0 {
			t.Fatalf("unexpected id request for prefix %q", prefix)
		}
		id := idValues[0]
		idValues = idValues[1:]
		return id
	}

	agent := &runtime.Agent{
		Config:   runtimeConfigForSmokeTest(),
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
		},
		Now:   func() time.Time { return clock },
		NewID: nextID,
	}

	session, err := agent.CreateChatSession(context.Background())
	if err != nil {
		t.Fatalf("CreateChatSession returned error: %v", err)
	}

	resumed, err := agent.ResumeChatSession(context.Background(), session.SessionID)
	if err != nil {
		t.Fatalf("ResumeChatSession returned error: %v", err)
	}
	if resumed.SessionID != session.SessionID {
		t.Fatalf("resumed session id = %q, want %q", resumed.SessionID, session.SessionID)
	}
	if len(resumed.Messages) != 0 {
		t.Fatalf("resumed messages len = %d, want 0", len(resumed.Messages))
	}
}

func TestAgentChatTurnExecutesPlanToolCallsAndReturnsFinalAssistantMessage(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	clock := time.Date(2026, 4, 14, 18, 0, 0, 0, time.UTC)
	idValues := []string{
		"session-chat-1",
		"run-chat-1", "evt-session-1", "evt-msg-user-1", "evt-run-start-1",
		"evt-provider-request-1", "evt-transport-1", "evt-tool-call-started-1", "plan-1", "evt-plan-create-1", "evt-tool-call-completed-1",
		"evt-provider-request-2", "evt-transport-2", "evt-msg-assistant-1", "evt-run-complete-1",
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
	var secondRequestBody map[string]any
	agent := &runtime.Agent{
		Config:        chatRuntimeConfigForTest(),
		MaxToolRounds: 2,
		Contracts:     chatContractsForToolLoopTest(),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				call++
				if call == 2 {
					defer req.Body.Close()
					if err := json.NewDecoder(req.Body).Decode(&secondRequestBody); err != nil {
						t.Fatalf("decode second request body: %v", err)
					}
				}
				if call == 1 {
					return &http.Response{
						StatusCode: http.StatusOK,
						Header:     http.Header{"Content-Type": []string{"application/json"}},
						Body: io.NopCloser(bytes.NewBufferString(`{
  "id":"resp-tools-1",
  "model":"glm-5-turbo",
  "choices":[
    {
      "finish_reason":"tool_calls",
      "message":{
        "role":"assistant",
        "content":"",
        "tool_calls":[
          {
            "id":"call-1",
            "function":{
              "name":"init_plan",
              "arguments":{"goal":"Refactor auth"}
            }
          }
        ]
      }
    }
  ],
  "usage":{"prompt_tokens":8,"completion_tokens":2,"total_tokens":10}
}`)),
					}, nil
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body: io.NopCloser(bytes.NewBufferString(`{
  "id":"resp-final-1",
  "model":"glm-5-turbo",
  "choices":[
    {
      "finish_reason":"stop",
      "message":{"role":"assistant","content":"Plan initialized."}
    }
  ],
  "usage":{"prompt_tokens":12,"completion_tokens":4,"total_tokens":16}
}`)),
				}, nil
			},
		}),
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewRunProjection(),
			projections.NewTranscriptProjection(),
			projections.NewActivePlanProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewPlanArchiveProjection(),
		},
		Now:   func() time.Time { return clock },
		NewID: nextID,
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)

	session, err := agent.NewChatSession()
	if err != nil {
		t.Fatalf("NewChatSession returned error: %v", err)
	}
	result, err := agent.ChatTurn(context.Background(), session, runtime.ChatTurnInput{Prompt: "Plan auth refactor"})
	if err != nil {
		t.Fatalf("ChatTurn returned error: %v", err)
	}
	if result.Provider.Message.Content != "Plan initialized." {
		t.Fatalf("final response = %q, want Plan initialized.", result.Provider.Message.Content)
	}
	if call != 2 {
		t.Fatalf("transport call count = %d, want 2", call)
	}
	if secondRequestBody == nil {
		t.Fatal("second request body is nil")
	}
	messages, ok := secondRequestBody["messages"].([]any)
	if !ok || len(messages) < 3 {
		t.Fatalf("second request messages = %#v", secondRequestBody["messages"])
	}
	lastMessage, ok := messages[len(messages)-1].(map[string]any)
	if !ok || lastMessage["role"] != "tool" {
		t.Fatalf("last message = %#v, want tool message", messages[len(messages)-1])
	}

	activePlan := findActivePlanProjection(t, agent.Projections)
	if activePlan.SnapshotForSession(session.SessionID).Plan.Goal != "Refactor auth" {
		t.Fatalf("active plan goal = %q, want Refactor auth", activePlan.SnapshotForSession(session.SessionID).Plan.Goal)
	}
	if len(session.Messages) != 2 {
		t.Fatalf("session messages len = %d, want 2", len(session.Messages))
	}
	if session.Messages[1].Content != "Plan initialized." {
		t.Fatalf("assistant session message = %#v", session.Messages[1])
	}
}

func TestAgentBtwTurnUsesNoToolsAndDoesNotMutateTranscript(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	clock := time.Date(2026, 4, 15, 17, 0, 0, 0, time.UTC)
	idValues := []string{
		"session-chat-1",
		"run-chat-1", "evt-session-1", "evt-msg-user-1", "evt-run-start-1", "evt-provider-request-1", "evt-transport-1", "evt-msg-assistant-1", "evt-run-complete-1",
	}
	nextID := func(prefix string) string {
		if len(idValues) == 0 {
			t.Fatalf("unexpected id request for prefix %q", prefix)
		}
		id := idValues[0]
		idValues = idValues[1:]
		return id
	}

	var btwRequestBody map[string]any
	agent := &runtime.Agent{
		Config:        chatRuntimeConfigForTest(),
		Contracts:     chatContractsForToolLoopTest(),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				defer req.Body.Close()
				if err := json.NewDecoder(req.Body).Decode(&btwRequestBody); err != nil {
					t.Fatalf("decode btw request body: %v", err)
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body: io.NopCloser(bytes.NewBufferString(`{
  "id":"resp-btw-1",
  "model":"glm-5-turbo",
  "choices":[
    {
      "finish_reason":"stop",
      "message":{"role":"assistant","content":"Side answer."}
    }
  ],
  "usage":{"prompt_tokens":9,"completion_tokens":3,"total_tokens":12}
}`)),
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
	if _, err := agent.ChatTurn(context.Background(), session, runtime.ChatTurnInput{Prompt: "Ping"}); err != nil {
		t.Fatalf("ChatTurn returned error: %v", err)
	}
	before := len(session.Messages)

	result, err := agent.BtwTurn(context.Background(), session, runtime.BtwTurnInput{Prompt: "Quick side question"})
	if err != nil {
		t.Fatalf("BtwTurn returned error: %v", err)
	}
	if result.Provider.Message.Content != "Side answer." {
		t.Fatalf("btw response = %q, want Side answer.", result.Provider.Message.Content)
	}
	if len(session.Messages) != before {
		t.Fatalf("session messages len = %d, want %d", len(session.Messages), before)
	}

	if toolsRaw, ok := btwRequestBody["tools"]; ok {
		if tools, ok := toolsRaw.([]any); ok && len(tools) != 0 {
			t.Fatalf("btw tools = %#v, want none", toolsRaw)
		}
	}
	messages, ok := btwRequestBody["messages"].([]any)
	if !ok || len(messages) < 2 {
		t.Fatalf("btw request messages = %#v", btwRequestBody["messages"])
	}
	last, ok := messages[len(messages)-1].(map[string]any)
	if !ok || last["content"] != "Quick side question" {
		t.Fatalf("btw last message = %#v", last)
	}
}

func TestAgentChatTurnHonorsConfiguredMaxToolRounds(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	agent := &runtime.Agent{
		Config:        chatRuntimeConfigForTest(),
		MaxToolRounds: 1,
		Contracts:     chatContractsForToolLoopTest(),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body: io.NopCloser(bytes.NewBufferString(`{
  "id":"resp-tools-1",
  "model":"glm-5-turbo",
  "choices":[
    {
      "finish_reason":"tool_calls",
      "message":{
        "role":"assistant",
        "content":"",
        "tool_calls":[
          {
            "id":"call-1",
            "function":{
              "name":"init_plan",
              "arguments":{"goal":"Refactor auth"}
            }
          }
        ]
      }
    }
  ],
  "usage":{"prompt_tokens":8,"completion_tokens":2,"total_tokens":10}
}`)),
				}, nil
			},
		}),
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewRunProjection(),
			projections.NewTranscriptProjection(),
			projections.NewActivePlanProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewPlanArchiveProjection(),
		},
		Now:   func() time.Time { return time.Date(2026, 4, 14, 19, 20, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)

	session, err := agent.NewChatSession()
	if err != nil {
		t.Fatalf("NewChatSession returned error: %v", err)
	}

	_, err = agent.ChatTurn(context.Background(), session, runtime.ChatTurnInput{Prompt: "make a plan"})
	if err == nil {
		t.Fatal("ChatTurn returned nil error, want tool loop limit failure")
	}
	if !strings.Contains(err.Error(), "provider tool loop exceeded 1 rounds") {
		t.Fatalf("ChatTurn error = %v, want configured round limit in message", err)
	}
}

func TestAgentChatTurnExecutesStreamedPlanToolCallsAndReturnsFinalAssistantMessage(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	clock := time.Date(2026, 4, 14, 18, 30, 0, 0, time.UTC)
	idValues := []string{
		"session-chat-stream-1",
		"run-chat-stream-1", "evt-session-stream-1", "evt-msg-user-stream-1", "evt-run-start-stream-1",
		"evt-provider-request-stream-1", "evt-transport-stream-1", "evt-tool-call-started-stream-1", "plan-stream-1", "evt-plan-create-stream-1", "evt-tool-call-completed-stream-1",
		"evt-provider-request-stream-2", "evt-transport-stream-2", "evt-msg-assistant-stream-1", "evt-run-complete-stream-1",
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
		Config:        chatRuntimeConfigForTest(),
		Contracts:     chatContractsForToolLoopStreamTest(),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				call++
				if call == 1 {
					return &http.Response{
						StatusCode: http.StatusOK,
						Header:     http.Header{"Content-Type": []string{"text/event-stream"}},
						Body: io.NopCloser(bytes.NewBufferString(strings.Join([]string{
							`data: {"id":"resp-tools-1","model":"glm-5-turbo","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"id":"call-1","index":0,"type":"function","function":{"name":"init_plan","arguments":"{\"goal\":\"Refactor auth\"}"}}]}}]}`,
							"",
							`data: {"id":"resp-tools-1","model":"glm-5-turbo","choices":[{"index":0,"finish_reason":"tool_calls","delta":{"content":""}}],"usage":{"prompt_tokens":8,"completion_tokens":2,"total_tokens":10}}`,
							"",
							`data: [DONE]`,
							"",
						}, "\n"))),
					}, nil
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"text/event-stream"}},
					Body: io.NopCloser(bytes.NewBufferString(strings.Join([]string{
						`data: {"id":"resp-final-1","model":"glm-5-turbo","choices":[{"index":0,"delta":{"role":"assistant","content":"Plan "},"finish_reason":""}]}`,
						"",
						`data: {"id":"resp-final-1","model":"glm-5-turbo","choices":[{"index":0,"delta":{"content":"initialized."},"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":4,"total_tokens":16}}`,
						"",
						`data: [DONE]`,
						"",
					}, "\n"))),
				}, nil
			},
		}),
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewRunProjection(),
			projections.NewTranscriptProjection(),
			projections.NewActivePlanProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewPlanArchiveProjection(),
		},
		Now:   func() time.Time { return clock },
		NewID: nextID,
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)

	session, err := agent.NewChatSession()
	if err != nil {
		t.Fatalf("NewChatSession returned error: %v", err)
	}
	result, err := agent.ChatTurn(context.Background(), session, runtime.ChatTurnInput{Prompt: "Plan auth refactor"})
	if err != nil {
		t.Fatalf("ChatTurn returned error: %v", err)
	}
	if result.Provider.Message.Content != "Plan initialized." {
		t.Fatalf("final response = %q, want Plan initialized.", result.Provider.Message.Content)
	}
	if call != 2 {
		t.Fatalf("transport call count = %d, want 2", call)
	}
	activePlan := findActivePlanProjection(t, agent.Projections)
	if activePlan.SnapshotForSession(session.SessionID).Plan.Goal != "Refactor auth" {
		t.Fatalf("active plan goal = %q, want Refactor auth", activePlan.SnapshotForSession(session.SessionID).Plan.Goal)
	}
}

func TestAgentChatTurnExecutesFilesystemToolCallAndReturnsFinalAssistantMessage(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	dir := t.TempDir()
	clock := time.Date(2026, 4, 14, 19, 0, 0, 0, time.UTC)
	idValues := []string{
		"session-chat-fs-1",
		"run-chat-fs-1", "evt-session-fs-1", "evt-msg-user-fs-1", "evt-run-start-fs-1",
		"evt-provider-request-fs-1", "evt-transport-fs-1", "evt-tool-call-started-fs-1", "evt-tool-call-completed-fs-1",
		"evt-provider-request-fs-2", "evt-transport-fs-2", "evt-msg-assistant-fs-1", "evt-run-complete-fs-1",
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
		Config:        chatRuntimeConfigForTest(),
		Contracts:     chatContractsForFilesystemToolLoopTest(dir),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				call++
				if call == 1 {
					return &http.Response{
						StatusCode: http.StatusOK,
						Header:     http.Header{"Content-Type": []string{"application/json"}},
						Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-fs-1","model":"glm-5-turbo","choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":"","tool_calls":[{"id":"call-fs-1","function":{"name":"fs_write_text","arguments":{"path":"notes/plan.txt","content":"hello from tool"}}}]}}]}`)),
					}, nil
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-fs-2","model":"glm-5-turbo","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"File written."}}]}`)),
				}, nil
			},
		}),
		EventLog:    runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{projections.NewSessionProjection(), projections.NewRunProjection()},
		Now:         func() time.Time { return clock },
		NewID:       nextID,
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)

	session, err := agent.NewChatSession()
	if err != nil {
		t.Fatalf("NewChatSession returned error: %v", err)
	}
	result, err := agent.ChatTurn(context.Background(), session, runtime.ChatTurnInput{Prompt: "write file"})
	if err != nil {
		t.Fatalf("ChatTurn returned error: %v", err)
	}
	if result.Provider.Message.Content != "File written." {
		t.Fatalf("assistant response = %q, want File written.", result.Provider.Message.Content)
	}
	data, err := os.ReadFile(filepath.Join(dir, "notes", "plan.txt"))
	if err != nil {
		t.Fatalf("ReadFile returned error: %v", err)
	}
	if string(data) != "hello from tool" {
		t.Fatalf("file content = %q, want hello from tool", string(data))
	}
}

func TestAgentChatTurnExecutesShellToolCallAndReturnsFinalAssistantMessage(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	dir := t.TempDir()
	clock := time.Date(2026, 4, 14, 19, 5, 0, 0, time.UTC)
	idValues := []string{
		"session-chat-shell-1",
		"run-chat-shell-1", "evt-session-shell-1", "evt-msg-user-shell-1", "evt-run-start-shell-1",
		"evt-provider-request-shell-1", "evt-transport-shell-1", "evt-tool-call-started-shell-1", "evt-tool-call-completed-shell-1",
		"evt-provider-request-shell-2", "evt-transport-shell-2", "evt-msg-assistant-shell-1", "evt-run-complete-shell-1",
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
	var secondRequest map[string]any
	agent := &runtime.Agent{
		Config:        chatRuntimeConfigForTest(),
		Contracts:     chatContractsForShellToolLoopTest(dir),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				call++
				if call == 1 {
					return &http.Response{
						StatusCode: http.StatusOK,
						Header:     http.Header{"Content-Type": []string{"application/json"}},
						Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-shell-1","model":"glm-5-turbo","choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":"","tool_calls":[{"id":"call-shell-1","function":{"name":"shell_exec","arguments":{"command":"pwd"}}}]}}]}`)),
					}, nil
				}
				defer req.Body.Close()
				if err := json.NewDecoder(req.Body).Decode(&secondRequest); err != nil {
					t.Fatalf("decode second request body: %v", err)
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-shell-2","model":"glm-5-turbo","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Shell done."}}]}`)),
				}, nil
			},
		}),
		EventLog:    runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{projections.NewSessionProjection(), projections.NewRunProjection()},
		Now:         func() time.Time { return clock },
		NewID:       nextID,
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)

	session, err := agent.NewChatSession()
	if err != nil {
		t.Fatalf("NewChatSession returned error: %v", err)
	}
	result, err := agent.ChatTurn(context.Background(), session, runtime.ChatTurnInput{Prompt: "run pwd"})
	if err != nil {
		t.Fatalf("ChatTurn returned error: %v", err)
	}
	if result.Provider.Message.Content != "Shell done." {
		t.Fatalf("assistant response = %q, want Shell done.", result.Provider.Message.Content)
	}
	messages, ok := secondRequest["messages"].([]any)
	if !ok {
		t.Fatalf("second request messages = %#v", secondRequest["messages"])
	}
	foundTool := false
	for _, raw := range messages {
		msg, ok := raw.(map[string]any)
		if !ok {
			continue
		}
		if msg["role"] == "tool" && msg["name"] == "shell_exec" {
			foundTool = true
			break
		}
	}
	if !foundTool {
		t.Fatalf("second request missing shell tool result: %#v", messages)
	}
}

func TestAgentChatTurnContinuesAfterShellToolError(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	dir := t.TempDir()
	clock := time.Date(2026, 4, 14, 19, 10, 0, 0, time.UTC)
	idValues := []string{
		"session-chat-shell-err-1",
		"run-chat-shell-err-1", "evt-session-shell-err-1", "evt-msg-user-shell-err-1", "evt-run-start-shell-err-1",
		"evt-provider-request-shell-err-1", "evt-transport-shell-err-1", "evt-tool-call-started-shell-err-1", "evt-tool-call-completed-shell-err-1",
		"evt-provider-request-shell-err-2", "evt-transport-shell-err-2", "evt-msg-assistant-shell-err-1", "evt-run-complete-shell-err-1",
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
	var secondRequest map[string]any
	agent := &runtime.Agent{
		Config:        chatRuntimeConfigForTest(),
		Contracts:     chatContractsForShellToolErrorLoopTest(dir),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				call++
				if call == 1 {
					return &http.Response{
						StatusCode: http.StatusOK,
						Header:     http.Header{"Content-Type": []string{"application/json"}},
						Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-shell-err-1","model":"glm-5-turbo","choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":"","tool_calls":[{"id":"call-shell-err-1","function":{"name":"shell_exec","arguments":{"command":"missing-binary"}}}]}}]}`)),
					}, nil
				}
				defer req.Body.Close()
				if err := json.NewDecoder(req.Body).Decode(&secondRequest); err != nil {
					t.Fatalf("decode second request body: %v", err)
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-shell-err-2","model":"glm-5-turbo","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Shell failed, trying fallback."}}]}`)),
				}, nil
			},
		}),
		EventLog:    runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{projections.NewSessionProjection(), projections.NewRunProjection()},
		Now:         func() time.Time { return clock },
		NewID:       nextID,
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)

	session, err := agent.NewChatSession()
	if err != nil {
		t.Fatalf("NewChatSession returned error: %v", err)
	}
	result, err := agent.ChatTurn(context.Background(), session, runtime.ChatTurnInput{Prompt: "run missing binary"})
	if err != nil {
		t.Fatalf("ChatTurn returned error: %v", err)
	}
	if result.Provider.Message.Content != "Shell failed, trying fallback." {
		t.Fatalf("assistant response = %q, want fallback reply", result.Provider.Message.Content)
	}
	messages, ok := secondRequest["messages"].([]any)
	if !ok {
		t.Fatalf("second request messages = %#v", secondRequest["messages"])
	}
	foundToolError := false
	for _, raw := range messages {
		msg, ok := raw.(map[string]any)
		if !ok {
			continue
		}
		if msg["role"] == "tool" && msg["name"] == "shell_exec" {
			content, _ := msg["content"].(string)
			if strings.Contains(content, `"status":"error"`) && strings.Contains(content, `"tool":"shell_exec"`) {
				foundToolError = true
				break
			}
		}
	}
	if !foundToolError {
		t.Fatalf("second request missing shell tool error result: %#v", messages)
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
				Enabled:  true,
				Strategy: "inline_assets",
				Params:   contracts.PromptAssetParams{Assets: []contracts.PromptAsset{}},
			},
		},
		ProviderTrace: contracts.ProviderTraceContract{
			ID: "provider-trace-chat",
			Request: contracts.ProviderTracePolicy{
				Enabled:  true,
				Strategy: "inline_request",
				Params: contracts.ProviderTraceParams{
					IncludeRawBody:        true,
					IncludeDecodedPayload: true,
				},
			},
		},
	}
}

func chatContractsForToolLoopTest() contracts.ResolvedContracts {
	out := chatContractsForTest()
	out.ProviderRequest.RequestShape.Streaming = contracts.StreamingPolicy{
		Enabled:  true,
		Strategy: "static_stream",
		Params: contracts.StreamingParams{
			Stream: false,
		},
	}
	out.Tools = contracts.ToolContract{
		Catalog: contracts.ToolCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ToolCatalogParams{
				ToolIDs:    []string{"init_plan"},
				AllowEmpty: false,
				Dedupe:     true,
			},
		},
		Serialization: contracts.ToolSerializationPolicy{
			Enabled:  true,
			Strategy: "openai_function_tools",
			Params: contracts.ToolSerializationParams{
				IncludeDescriptions: true,
			},
		},
	}
	out.PlanTools = contracts.PlanToolContract{
		PlanTool: contracts.PlanToolPolicy{
			Enabled:  true,
			Strategy: "default_plan_tools",
			Params: contracts.PlanToolParams{
				ToolIDs: []string{"init_plan"},
			},
		},
	}
	out.ToolExecution = contracts.ToolExecutionContract{
		Access: contracts.ToolAccessPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ToolAccessParams{
				ToolIDs: []string{"init_plan"},
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

func chatContractsForToolLoopStreamTest() contracts.ResolvedContracts {
	out := chatContractsForToolLoopTest()
	out.ProviderRequest.RequestShape.Streaming = contracts.StreamingPolicy{
		Enabled:  true,
		Strategy: "static_stream",
		Params: contracts.StreamingParams{
			Stream: true,
		},
	}
	return out
}

func chatContractsForFilesystemToolLoopTest(root string) contracts.ResolvedContracts {
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
			Params:   contracts.ToolCatalogParams{ToolIDs: []string{"fs_write_text"}},
		},
		Serialization: contracts.ToolSerializationPolicy{
			Enabled:  true,
			Strategy: "openai_function_tools",
			Params:   contracts.ToolSerializationParams{IncludeDescriptions: true},
		},
	}
	out.ToolExecution = contracts.ToolExecutionContract{
		Access: contracts.ToolAccessPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ToolAccessParams{ToolIDs: []string{"fs_write_text"}},
		},
		Approval: contracts.ToolApprovalPolicy{Enabled: true, Strategy: "always_allow"},
		Sandbox:  contracts.ToolSandboxPolicy{Enabled: true, Strategy: "workspace_write"},
	}
	out.FilesystemTools = contracts.FilesystemToolContract{
		Catalog: contracts.FilesystemCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.FilesystemCatalogParams{ToolIDs: []string{"fs_write_text"}},
		},
		Description: contracts.FilesystemDescriptionPolicy{
			Enabled:  true,
			Strategy: "static_builtin_descriptions",
		},
	}
	out.FilesystemExecution = contracts.FilesystemExecutionContract{
		Scope: contracts.FilesystemScopePolicy{
			Enabled:  true,
			Strategy: "workspace_only",
			Params: contracts.FilesystemScopeParams{
				RootPath:      root,
				WriteSubpaths: []string{"notes"},
			},
		},
		Mutation: contracts.FilesystemMutationPolicy{
			Enabled:  true,
			Strategy: "allow_writes",
			Params:   contracts.FilesystemMutationParams{AllowWrite: true},
		},
		IO: contracts.FilesystemIOPolicy{
			Enabled:  true,
			Strategy: "bounded_text_io",
			Params:   contracts.FilesystemIOParams{MaxWriteBytes: 1024, Encoding: "utf-8"},
		},
	}
	return out
}

func chatContractsForShellToolLoopTest(root string) contracts.ResolvedContracts {
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
			Params:   contracts.ToolCatalogParams{ToolIDs: []string{"shell_exec"}},
		},
		Serialization: contracts.ToolSerializationPolicy{
			Enabled:  true,
			Strategy: "openai_function_tools",
			Params:   contracts.ToolSerializationParams{IncludeDescriptions: true},
		},
	}
	out.ToolExecution = contracts.ToolExecutionContract{
		Access: contracts.ToolAccessPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ToolAccessParams{ToolIDs: []string{"shell_exec"}},
		},
		Approval: contracts.ToolApprovalPolicy{Enabled: true, Strategy: "always_allow"},
		Sandbox:  contracts.ToolSandboxPolicy{Enabled: true, Strategy: "workspace_write"},
	}
	out.ShellTools = contracts.ShellToolContract{
		Catalog: contracts.ShellCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCatalogParams{ToolIDs: []string{"shell_exec"}},
		},
		Description: contracts.ShellDescriptionPolicy{
			Enabled:  true,
			Strategy: "static_builtin_descriptions",
		},
	}
	out.ShellExecution = contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"pwd"}},
		},
		Approval: contracts.ShellApprovalPolicy{Enabled: true, Strategy: "always_allow"},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            root,
				Timeout:        "5s",
				MaxOutputBytes: 4096,
			},
		},
	}
	return out
}

func chatContractsForShellToolErrorLoopTest(root string) contracts.ResolvedContracts {
	out := chatContractsForShellToolLoopTest(root)
	out.ShellExecution.Command.Params.AllowedCommands = []string{"missing-binary"}
	return out
}

func findActivePlanProjection(t *testing.T, projectionsList []projections.Projection) *projections.ActivePlanProjection {
	t.Helper()
	for _, projection := range projectionsList {
		active, ok := projection.(*projections.ActivePlanProjection)
		if ok {
			return active
		}
	}
	t.Fatal("active plan projection not found")
	return nil
}
