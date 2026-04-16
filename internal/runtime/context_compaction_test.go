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

func TestAgentChatTurnCompactsPromptWithRollingSummaryWhenBudgetExceeded(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	clock := time.Date(2026, 4, 16, 13, 30, 0, 0, time.UTC)
	idValues := []string{
		"session-chat-summary-1", "evt-session-summary-1",
		"evt-msg-seed-1", "evt-msg-seed-2", "evt-msg-seed-3", "evt-msg-seed-4",
		"run-chat-summary-1", "evt-msg-user-summary-1", "evt-run-start-summary-1",
		"evt-provider-request-summary-1", "evt-transport-summary-1", "evt-context-summary-1",
		"evt-provider-request-summary-2", "evt-transport-summary-2", "evt-msg-assistant-summary-1", "evt-run-complete-summary-1",
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
	var summaryRequest map[string]any
	var mainRequest map[string]any
	agent := &runtime.Agent{
		Config:        config.AgentConfig{ID: "summary-test"},
		ConfigPath:    t.TempDir() + "/agent.yaml",
		MaxToolRounds: 2,
		Contracts:     chatContractsForSummaryCompactionTest(),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				call++
				defer req.Body.Close()
				switch call {
				case 1:
					if err := json.NewDecoder(req.Body).Decode(&summaryRequest); err != nil {
						t.Fatalf("decode summary request body: %v", err)
					}
					return &http.Response{
						StatusCode: http.StatusOK,
						Header:     http.Header{"Content-Type": []string{"application/json"}},
						Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-summary-1","model":"glm-5-turbo","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Earlier work: auth middleware was audited, settings quick controls were added, and shell approval regressions were fixed."}}],"usage":{"prompt_tokens":140,"completion_tokens":28,"total_tokens":168}}`)),
					}, nil
				case 2:
					if err := json.NewDecoder(req.Body).Decode(&mainRequest); err != nil {
						t.Fatalf("decode main request body: %v", err)
					}
					return &http.Response{
						StatusCode: http.StatusOK,
						Header:     http.Header{"Content-Type": []string{"application/json"}},
						Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-main-1","model":"glm-5-turbo","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Continuing from compacted context."}}],"usage":{"prompt_tokens":180,"completion_tokens":12,"total_tokens":192}}`)),
					}, nil
				default:
					t.Fatalf("unexpected provider call %d", call)
					return nil, nil
				}
			},
		}),
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewRunProjection(),
			projections.NewTranscriptProjection(),
			projections.NewContextBudgetProjection(),
			projections.NewContextSummaryProjection(),
		},
		Now:   func() time.Time { return clock },
		NewID: nextID,
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)

	session, err := agent.CreateChatSession(context.Background())
	if err != nil {
		t.Fatalf("CreateChatSession returned error: %v", err)
	}
	seedMessages := []contracts.Message{
		{Role: "user", Content: strings.Repeat("audit auth middleware ", 16)},
		{Role: "assistant", Content: strings.Repeat("audited auth middleware and traced approval flow ", 12)},
		{Role: "user", Content: strings.Repeat("now inspect daemon websocket status handling ", 12)},
		{Role: "assistant", Content: strings.Repeat("daemon websocket reconnection and status handling were inspected ", 10)},
	}
	for i, message := range seedMessages {
		if err := agent.RecordEvent(context.Background(), eventing.Event{
			ID:            nextID("evt-msg-seed"),
			Kind:          eventing.EventMessageRecorded,
			OccurredAt:    clock,
			AggregateID:   session.SessionID,
			AggregateType: eventing.AggregateSession,
			CorrelationID: session.SessionID,
			Source:        "runtime.test",
			ActorID:       agent.Config.ID,
			ActorType:     "agent",
			TraceSummary:  "seed transcript",
			Payload: map[string]any{
				"session_id": session.SessionID,
				"role":       message.Role,
				"content":    message.Content,
				"index":      i,
			},
		}); err != nil {
			t.Fatalf("RecordEvent seed message %d: %v", i, err)
		}
		session.Messages = append(session.Messages, message)
	}

	result, err := agent.ChatTurn(context.Background(), session, runtime.ChatTurnInput{
		Prompt: strings.Repeat("continue with the same task and check follow-up diffs ", 10),
	})
	if err != nil {
		t.Fatalf("ChatTurn returned error: %v", err)
	}
	if result.Provider.Message.Content != "Continuing from compacted context." {
		t.Fatalf("assistant response = %q", result.Provider.Message.Content)
	}

	summaryMessages, ok := summaryRequest["messages"].([]any)
	if !ok || len(summaryMessages) == 0 {
		t.Fatalf("summary request messages = %#v", summaryRequest["messages"])
	}
	mainMessages, ok := mainRequest["messages"].([]any)
	if !ok || len(mainMessages) == 0 {
		t.Fatalf("main request messages = %#v", mainRequest["messages"])
	}
	var sawSummary bool
	for _, raw := range mainMessages {
		msg, ok := raw.(map[string]any)
		if !ok {
			continue
		}
		content, _ := msg["content"].(string)
		if strings.Contains(content, "Conversation summary covering earlier context") {
			sawSummary = true
		}
		if strings.Contains(content, "audit auth middleware audit auth middleware") {
			t.Fatalf("main request still contains oldest raw transcript chunk: %q", content)
		}
	}
	if !sawSummary {
		t.Fatalf("main request missing rolling summary message: %#v", mainMessages)
	}

	budget := agent.CurrentContextBudget(session.SessionID)
	if budget.SummarizationCount != 1 {
		t.Fatalf("summarization_count = %d, want 1", budget.SummarizationCount)
	}
	if budget.SummaryTokens <= 0 {
		t.Fatalf("summary_tokens = %d, want > 0", budget.SummaryTokens)
	}
}

func TestAgentChatTurnInjectsFiftyPercentGuardMessage(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	clock := time.Date(2026, 4, 16, 14, 0, 0, 0, time.UTC)
	idValues := []string{
		"session-chat-guard-50", "evt-session-guard-50",
		"evt-msg-seed-50-1", "evt-msg-seed-50-2",
		"run-chat-guard-50", "evt-msg-user-guard-50", "evt-run-start-guard-50",
		"evt-context-guard-50", "evt-provider-request-guard-50", "evt-transport-guard-50",
		"evt-msg-assistant-guard-50", "evt-run-complete-guard-50",
	}
	nextID := func(prefix string) string {
		if len(idValues) == 0 {
			t.Fatalf("unexpected id request for prefix %q", prefix)
		}
		id := idValues[0]
		idValues = idValues[1:]
		return id
	}

	var requestBody map[string]any
	agent := &runtime.Agent{
		Config:        config.AgentConfig{ID: "guard-50-test"},
		ConfigPath:    t.TempDir() + "/agent.yaml",
		MaxToolRounds: 2,
		Contracts:     chatContractsForGuardBandTest(180),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				defer req.Body.Close()
				if err := json.NewDecoder(req.Body).Decode(&requestBody); err != nil {
					t.Fatalf("decode request body: %v", err)
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-guard-50","model":"glm-5-turbo","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Guard noted."}}],"usage":{"prompt_tokens":80,"completion_tokens":8,"total_tokens":88}}`)),
				}, nil
			},
		}),
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewRunProjection(),
			projections.NewTranscriptProjection(),
			projections.NewContextBudgetProjection(),
			projections.NewContextSummaryProjection(),
		},
		Now:   func() time.Time { return clock },
		NewID: nextID,
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)

	session, err := agent.CreateChatSession(context.Background())
	if err != nil {
		t.Fatalf("CreateChatSession returned error: %v", err)
	}
	seedMessages := []contracts.Message{
		{Role: "user", Content: strings.Repeat("audit middleware ", 10)},
		{Role: "assistant", Content: strings.Repeat("middleware audited ", 8)},
	}
	for _, message := range seedMessages {
		if err := agent.RecordEvent(context.Background(), eventing.Event{
			ID:            nextID("evt-msg-seed"),
			Kind:          eventing.EventMessageRecorded,
			OccurredAt:    clock,
			AggregateID:   session.SessionID,
			AggregateType: eventing.AggregateSession,
			CorrelationID: session.SessionID,
			Source:        "runtime.test",
			ActorID:       agent.Config.ID,
			ActorType:     "agent",
			TraceSummary:  "seed transcript",
			Payload: map[string]any{
				"session_id": session.SessionID,
				"role":       message.Role,
				"content":    message.Content,
			},
		}); err != nil {
			t.Fatalf("RecordEvent seed message: %v", err)
		}
		session.Messages = append(session.Messages, message)
	}

	if _, err := agent.ChatTurn(context.Background(), session, runtime.ChatTurnInput{Prompt: strings.Repeat("continue ", 6)}); err != nil {
		t.Fatalf("ChatTurn returned error: %v", err)
	}

	messages := requestBody["messages"].([]any)
	found := false
	for _, raw := range messages {
		msg, ok := raw.(map[string]any)
		if !ok {
			continue
		}
		content, _ := msg["content"].(string)
		if strings.Contains(content, "Write a concise running summary of completed work") {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("request missing 50%% guard message: %#v", messages)
	}
}

func TestAgentChatTurnInjectsSeventyPercentGuardMessageWithoutRefreshingSummary(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	clock := time.Date(2026, 4, 16, 14, 10, 0, 0, time.UTC)
	idValues := []string{
		"session-chat-guard-70", "evt-session-guard-70",
		"evt-msg-seed-70-1", "evt-msg-seed-70-2", "evt-msg-seed-70-3",
		"run-chat-guard-70", "evt-msg-user-guard-70", "evt-run-start-guard-70",
		"evt-context-guard-70", "evt-provider-request-guard-70", "evt-transport-guard-70",
		"evt-msg-assistant-guard-70", "evt-run-complete-guard-70",
	}
	nextID := func(prefix string) string {
		if len(idValues) == 0 {
			t.Fatalf("unexpected id request for prefix %q", prefix)
		}
		id := idValues[0]
		idValues = idValues[1:]
		return id
	}

	callCount := 0
	var requestBody map[string]any
	agent := &runtime.Agent{
		Config:        config.AgentConfig{ID: "guard-70-test"},
		ConfigPath:    t.TempDir() + "/agent.yaml",
		MaxToolRounds: 2,
		Contracts:     chatContractsForGuardBandTest(280),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				callCount++
				defer req.Body.Close()
				if err := json.NewDecoder(req.Body).Decode(&requestBody); err != nil {
					t.Fatalf("decode request body: %v", err)
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-guard-70","model":"glm-5-turbo","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Guard noted."}}],"usage":{"prompt_tokens":92,"completion_tokens":7,"total_tokens":99}}`)),
				}, nil
			},
		}),
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewRunProjection(),
			projections.NewTranscriptProjection(),
			projections.NewContextBudgetProjection(),
			projections.NewContextSummaryProjection(),
		},
		Now:   func() time.Time { return clock },
		NewID: nextID,
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)

	session, err := agent.CreateChatSession(context.Background())
	if err != nil {
		t.Fatalf("CreateChatSession returned error: %v", err)
	}
	seedMessages := []contracts.Message{
		{Role: "user", Content: strings.Repeat("audit middleware ", 14)},
		{Role: "assistant", Content: strings.Repeat("middleware audited ", 12)},
		{Role: "user", Content: strings.Repeat("check websocket status handling ", 10)},
	}
	for _, message := range seedMessages {
		if err := agent.RecordEvent(context.Background(), eventing.Event{
			ID:            nextID("evt-msg-seed"),
			Kind:          eventing.EventMessageRecorded,
			OccurredAt:    clock,
			AggregateID:   session.SessionID,
			AggregateType: eventing.AggregateSession,
			CorrelationID: session.SessionID,
			Source:        "runtime.test",
			ActorID:       agent.Config.ID,
			ActorType:     "agent",
			TraceSummary:  "seed transcript",
			Payload: map[string]any{
				"session_id": session.SessionID,
				"role":       message.Role,
				"content":    message.Content,
			},
		}); err != nil {
			t.Fatalf("RecordEvent seed message: %v", err)
		}
		session.Messages = append(session.Messages, message)
	}

	if _, err := agent.ChatTurn(context.Background(), session, runtime.ChatTurnInput{Prompt: strings.Repeat("continue ", 8)}); err != nil {
		t.Fatalf("ChatTurn returned error: %v", err)
	}
	if callCount != 1 {
		t.Fatalf("provider call count = %d, want 1 without summary refresh", callCount)
	}
	messages := requestBody["messages"].([]any)
	found := false
	for _, raw := range messages {
		msg, ok := raw.(map[string]any)
		if !ok {
			continue
		}
		content, _ := msg["content"].(string)
		if strings.Contains(content, "Context is getting tight") {
			found = true
		}
		if strings.Contains(content, "Conversation summary covering earlier context") {
			t.Fatalf("unexpected summary refresh in 70%% guard path: %#v", messages)
		}
	}
	if !found {
		t.Fatalf("request missing 70%% guard message: %#v", messages)
	}
}

func chatContractsForSummaryCompactionTest() contracts.ResolvedContracts {
	out := chatContractsForTest()
	out.ProviderRequest.RequestShape.Streaming = contracts.StreamingPolicy{
		Enabled:  true,
		Strategy: "static_stream",
		Params:   contracts.StreamingParams{Stream: false},
	}
	out.Memory = contracts.MemoryContract{
		ID: "memory-summary",
		Offload: contracts.OffloadPolicy{
			Enabled:  true,
			Strategy: "artifact_store",
			Params: contracts.OffloadParams{
				MaxChars:             4096,
				PreviewChars:         240,
				ExposeRetrievalTools: true,
			},
		},
	}
	out.ContextBudget = contracts.ContextBudgetContract{
		ID: "context-budget-summary",
		Accounting: contracts.ContextBudgetAccountingPolicy{
			Enabled:  true,
			Strategy: "provider_usage_v1",
			Params: contracts.ContextBudgetAccountingParams{
				TrustInputTokens:  true,
				TrustOutputTokens: true,
				TrustTotalTokens:  true,
			},
		},
		Estimation: contracts.ContextBudgetEstimationPolicy{
			Enabled:  true,
			Strategy: "chars_div4",
			Params: contracts.ContextBudgetEstimationParams{
				CharsPerToken: 4,
				IncludeDrafts: true,
				IncludeQueue:  true,
			},
		},
		Compaction: contracts.ContextBudgetCompactionPolicy{
			Enabled:  true,
			Strategy: "rolling_summary_v1",
			Params: contracts.ContextBudgetCompactionParams{
				WarningTokens:          80,
				CompactionTokens:       120,
				KeepRecentMessages:     1,
				MinMessagesToSummarize: 4,
				RefreshEveryMessages:   1,
				MaxSummaryChars:        400,
				Instructions:           "Summarize earlier conversation faithfully for continued coding work. Keep decisions, changed files, open risks, and tool findings.",
				StoreArtifacts:         true,
			},
		},
		SummaryDisplay: contracts.ContextBudgetSummaryDisplayPolicy{
			Enabled:  true,
			Strategy: "counter_only",
			Params: contracts.ContextBudgetSummaryDisplayParams{IncludeSummaryCount: true},
		},
	}
	return out
}

func chatContractsForGuardBandTest(maxContextTokens int) contracts.ResolvedContracts {
	out := chatContractsForTest()
	out.ProviderRequest.RequestShape.Streaming = contracts.StreamingPolicy{
		Enabled:  true,
		Strategy: "static_stream",
		Params:   contracts.StreamingParams{Stream: false},
	}
	out.ContextBudget = contracts.ContextBudgetContract{
		ID: "context-budget-guard",
		Accounting: contracts.ContextBudgetAccountingPolicy{
			Enabled:  true,
			Strategy: "provider_usage_v1",
			Params: contracts.ContextBudgetAccountingParams{
				TrustInputTokens:  true,
				TrustOutputTokens: true,
				TrustTotalTokens:  true,
			},
		},
		Estimation: contracts.ContextBudgetEstimationPolicy{
			Enabled:  true,
			Strategy: "chars_div4",
			Params: contracts.ContextBudgetEstimationParams{
				CharsPerToken: 4,
			},
		},
		Compaction: contracts.ContextBudgetCompactionPolicy{
			Enabled:  true,
			Strategy: "rolling_summary_v1",
			Params: contracts.ContextBudgetCompactionParams{
				MaxContextTokens:       maxContextTokens,
				KeepRecentMessages:     2,
				MinMessagesToSummarize: 6,
				RefreshEveryMessages:   1,
				MaxSummaryChars:        300,
				Instructions:           "Summarize earlier conversation faithfully for continued coding work.",
				Guards: []contracts.ContextBudgetGuardRule{
					{Percent: 50, Action: "advisory", Message: "Write a concise running summary of completed work, lessons learned, and useful intermediate findings before context gets tight.", OncePerSummaryCycle: true},
					{Percent: 70, Action: "warning", Message: "Context is getting tight. Optimize for brevity and avoid replaying large details unless needed.", OncePerSummaryCycle: true},
					{Percent: 85, Action: "refresh_summary", Message: "Refresh the rolling summary now and continue from the compacted context.", OncePerSummaryCycle: true},
				},
			},
		},
		SummaryDisplay: contracts.ContextBudgetSummaryDisplayPolicy{
			Enabled:  true,
			Strategy: "counter_only",
			Params:   contracts.ContextBudgetSummaryDisplayParams{IncludeSummaryCount: true},
		},
	}
	return out
}
