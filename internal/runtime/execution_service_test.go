package runtime

import (
	"context"
	"encoding/json"
	"testing"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/compaction"
	"teamd/internal/provider"
	"teamd/internal/worker"
)

type executionTestStore struct {
	runtimeAPITestStore
	continuations    map[string]ApprovalContinuation
	timeoutDecisions map[string]TimeoutDecisionRecord
}

func (s *executionTestStore) SaveApproval(record approvals.Record) error                  { return nil }
func (s *executionTestStore) Approval(id string) (approvals.Record, bool, error)         { return approvals.Record{}, false, nil }
func (s *executionTestStore) PendingApprovals(sessionID string) ([]approvals.Record, error) { return nil, nil }
func (s *executionTestStore) SaveHandledApprovalCallback(updateID string, record approvals.Record) error {
	return nil
}
func (s *executionTestStore) HandledApprovalCallback(updateID string) (approvals.Record, bool, error) {
	return approvals.Record{}, false, nil
}
func (s *executionTestStore) SaveApprovalContinuation(cont ApprovalContinuation) error {
	if s.continuations == nil {
		s.continuations = map[string]ApprovalContinuation{}
	}
	s.continuations[cont.ApprovalID] = cont
	return nil
}
func (s *executionTestStore) ApprovalContinuation(id string) (ApprovalContinuation, bool, error) {
	cont, ok := s.continuations[id]
	return cont, ok, nil
}
func (s *executionTestStore) DeleteApprovalContinuation(id string) error {
	delete(s.continuations, id)
	return nil
}
func (s *executionTestStore) SaveTimeoutDecision(record TimeoutDecisionRecord) error {
	if s.timeoutDecisions == nil {
		s.timeoutDecisions = map[string]TimeoutDecisionRecord{}
	}
	s.timeoutDecisions[record.RunID] = record
	return nil
}
func (s *executionTestStore) TimeoutDecision(runID string) (TimeoutDecisionRecord, bool, error) {
	record, ok := s.timeoutDecisions[runID]
	return record, ok, nil
}
func (s *executionTestStore) DeleteTimeoutDecision(runID string) error {
	delete(s.timeoutDecisions, runID)
	return nil
}

type executionTestHooks struct {
	preparedReq        StartRunRequest
	preparedRun        PreparedRun
	preparedCont       ApprovalContinuation
	successChatID      int64
	successRunID       string
	successResp        provider.PromptResponse
	executedTool       provider.ToolCall
	finishedApprovalID string
	conversation       func(context.Context, int64) ConversationHooks
}

func (h *executionTestHooks) PrepareStart(ctx context.Context, req StartRunRequest, prepared PreparedRun) error {
	h.preparedReq = req
	h.preparedRun = prepared
	return nil
}

func (h *executionTestHooks) PrepareApprovalResume(ctx context.Context, cont ApprovalContinuation, prepared PreparedRun) error {
	h.preparedCont = cont
	h.preparedRun = prepared
	return nil
}

func (h *executionTestHooks) PrepareRunContext(ctx context.Context, runID string) context.Context {
	return ctx
}

func (h *executionTestHooks) ConversationHooks(ctx context.Context, chatID int64) ConversationHooks {
	if h.conversation != nil {
		return h.conversation(ctx, chatID)
	}
	return ConversationHooks{
		Provider:   provider.FakeProvider{},
		Store:      executionConversationStore{},
		Compactor:  compaction.New(compaction.Deps{}),
		Budget:     compaction.Budget{},
		ProviderTools: func(role string) ([]provider.ToolDefinition, error) { return nil, nil },
		RequestConfig: func(chatID int64) provider.RequestConfig { return provider.RequestConfig{} },
		InjectPromptContext: func(chatID int64, messages []provider.Message) ([]provider.Message, error) {
			return messages, nil
		},
		ExecuteTool: func(ctx context.Context, chatID int64, call provider.ToolCall) (string, error) {
			return "", nil
		},
		ShapeToolResult: func(chatID int64, call provider.ToolCall, content string) (OffloadedToolResult, error) {
			return OffloadedToolResult{Content: content}, nil
		},
		LastUserMessage: func(messages []provider.Message) string {
			if len(messages) == 0 {
				return ""
			}
			return messages[len(messages)-1].Content
		},
	}
}

type executionEventStore struct {
	executionTestStore
	events []RuntimeEvent
}

func (s *executionEventStore) SaveEvent(event RuntimeEvent) error {
	s.events = append(s.events, event)
	return nil
}

func (h *executionTestHooks) HandleRunSuccess(ctx context.Context, chatID int64, runID string, interactive bool, resp provider.PromptResponse) error {
	h.successChatID = chatID
	h.successRunID = runID
	h.successResp = resp
	return nil
}

func (h *executionTestHooks) HandleRunError(ctx context.Context, chatID int64, interactive bool, err error) error {
	return err
}

func (h *executionTestHooks) HandleRunCancelled(ctx context.Context, chatID int64, interactive bool) error {
	return nil
}

func (h *executionTestHooks) ExecuteApprovedTool(ctx context.Context, chatID int64, _ []string, call provider.ToolCall) (string, error) {
	h.executedTool = call
	return "approved result", nil
}

func (h *executionTestHooks) FinishApprovalResume(id string) error {
	h.finishedApprovalID = id
	return nil
}

type executionConversationStore struct{}

func (executionConversationStore) Append(chatID int64, msg provider.Message) error { return nil }
func (executionConversationStore) Messages(chatID int64) ([]provider.Message, error) {
	return []provider.Message{{Role: "user", Content: "hello"}}, nil
}
func (executionConversationStore) Checkpoint(chatID int64) (worker.Checkpoint, bool, error) {
	return worker.Checkpoint{}, false, nil
}
func (executionConversationStore) SaveCheckpoint(chatID int64, checkpoint worker.Checkpoint) error {
	return nil
}
func (executionConversationStore) ActiveSession(chatID int64) (string, error) { return "default", nil }

type blockingProvider struct {
	started chan<- struct{}
	release <-chan struct{}
}

func (p blockingProvider) Generate(ctx context.Context, req provider.PromptRequest) (provider.PromptResponse, error) {
	select {
	case p.started <- struct{}{}:
	default:
	}
	select {
	case <-p.release:
		return provider.PromptResponse{FinishReason: "stop", Text: "detached reply"}, nil
	case <-ctx.Done():
		return provider.PromptResponse{}, ctx.Err()
	}
}

type timeoutProvider struct{}

func (timeoutProvider) Generate(ctx context.Context, req provider.PromptRequest) (provider.PromptResponse, error) {
	<-ctx.Done()
	return provider.PromptResponse{}, ctx.Err()
}

type timeoutOnceThenSuccessProvider struct {
	calls int
}

func (p *timeoutOnceThenSuccessProvider) Generate(ctx context.Context, req provider.PromptRequest) (provider.PromptResponse, error) {
	p.calls++
	if p.calls == 1 {
		<-ctx.Done()
		return provider.PromptResponse{}, ctx.Err()
	}
	return provider.PromptResponse{FinishReason: "stop", Text: "continued reply"}, nil
}

func TestExecutionServiceStartsRunThroughHooks(t *testing.T) {
	store := &executionTestStore{}
	svc := NewExecutionService(NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps())), &executionTestHooks{})

	view, ok, err := svc.StartAndWait(context.Background(), StartRunRequest{
		RunID:       "run-1",
		ChatID:      1001,
		SessionID:   "1001:default",
		Query:       "hello",
		Interactive: true,
	})
	if err != nil || !ok {
		t.Fatalf("start run: ok=%v err=%v", ok, err)
	}
	if view.RunID != "run-1" || view.ChatID != 1001 {
		t.Fatalf("unexpected run view: %+v", view)
	}
	hooks := svc.hooks.(*executionTestHooks)
	if hooks.successRunID != "run-1" || hooks.successChatID != 1001 {
		t.Fatalf("run was not executed through hooks: %+v", hooks)
	}
	head, ok, err := store.SessionHead(1001, "1001:default")
	if err != nil {
		t.Fatalf("session head: %v", err)
	}
	if !ok {
		t.Fatal("expected session head to be updated")
	}
	if head.LastCompletedRunID != "run-1" || head.CurrentGoal != "hello" {
		t.Fatalf("unexpected session head: %+v", head)
	}
	if head.LastResultSummary == "" {
		t.Fatalf("expected non-empty result summary: %+v", head)
	}
}

func TestExecutionServiceEmitsTranscriptAndSessionHeadDebugEvents(t *testing.T) {
	store := &executionEventStore{}
	svc := NewExecutionService(NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps())), &executionTestHooks{})

	_, ok, err := svc.StartAndWait(context.Background(), StartRunRequest{
		RunID:       "run-1",
		ChatID:      1001,
		SessionID:   "1001:default",
		Query:       "hello",
		Interactive: true,
	})
	if err != nil || !ok {
		t.Fatalf("start run: ok=%v err=%v", ok, err)
	}

	var sawTranscript, sawSessionHead bool
	for _, event := range store.events {
		switch event.Kind {
		case "transcript.appended":
			sawTranscript = true
			var payload map[string]any
			if err := json.Unmarshal(event.Payload, &payload); err != nil {
				t.Fatalf("unmarshal transcript payload: %v", err)
			}
			if payload["role"] == "" {
				t.Fatalf("expected role in transcript payload, got %v", payload)
			}
		case "session_head.updated":
			sawSessionHead = true
		}
	}
	if !sawTranscript {
		t.Fatalf("expected transcript.appended event, got %+v", store.events)
	}
	if !sawSessionHead {
		t.Fatalf("expected session_head.updated event, got %+v", store.events)
	}
}

func TestExecutionServiceEmitsPromptAssembledDebugEvent(t *testing.T) {
	store := &executionEventStore{}
	svc := NewExecutionService(NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps())), &executionTestHooks{})

	_, ok, err := svc.StartAndWait(context.Background(), StartRunRequest{
		RunID:       "run-1",
		ChatID:      1001,
		SessionID:   "1001:default",
		Query:       "hello",
		Interactive: true,
	})
	if err != nil || !ok {
		t.Fatalf("start run: ok=%v err=%v", ok, err)
	}

	for _, event := range store.events {
		if event.Kind != "prompt.assembled" {
			continue
		}
		var payload map[string]any
		if err := json.Unmarshal(event.Payload, &payload); err != nil {
			t.Fatalf("unmarshal prompt payload: %v", err)
		}
		if payload["final_prompt_tokens"] == nil {
			t.Fatalf("expected final_prompt_tokens in prompt event, got %v", payload)
		}
		return
	}
	t.Fatalf("expected prompt.assembled event, got %+v", store.events)
}

func TestExecutionServiceStartDetachedIgnoresRequestCancellation(t *testing.T) {
	store := &executionTestStore{}
	started := make(chan struct{}, 1)
	allowFinish := make(chan struct{})
	hooks := &executionTestHooks{
		conversation: func(ctx context.Context, chatID int64) ConversationHooks {
			return ConversationHooks{
				Provider: blockingProvider{started: started, release: allowFinish},
				Store:     executionConversationStore{},
				Compactor: compaction.New(compaction.Deps{}),
				Budget:    compaction.Budget{},
				ProviderTools: func(role string) ([]provider.ToolDefinition, error) { return nil, nil },
				RequestConfig: func(chatID int64) provider.RequestConfig { return provider.RequestConfig{} },
				InjectPromptContext: func(chatID int64, messages []provider.Message) ([]provider.Message, error) {
					return messages, nil
				},
				ExecuteTool: func(ctx context.Context, chatID int64, call provider.ToolCall) (string, error) {
					return "", nil
				},
				LastUserMessage: func(messages []provider.Message) string { return "hello" },
			}
		},
	}
	svc := NewExecutionService(NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps())), hooks)

	reqCtx, cancel := context.WithCancel(context.Background())
	view, ok, err := svc.StartDetached(reqCtx, StartRunRequest{
		RunID:     "run-1",
		ChatID:    1001,
		SessionID: "1001:default",
		Query:     "hello",
	})
	if err != nil || !ok {
		t.Fatalf("start detached: ok=%v err=%v", ok, err)
	}
	if view.RunID != "run-1" {
		t.Fatalf("unexpected run view: %+v", view)
	}
	<-started
	cancel()
	close(allowFinish)

	deadline := time.Now().Add(time.Second)
	for time.Now().Before(deadline) {
		if hooks.successRunID == "run-1" {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("detached run was cancelled by request context: %+v", hooks)
}

func TestExecutionServiceResumesApprovalContinuation(t *testing.T) {
	store := &executionTestStore{
		continuations: map[string]ApprovalContinuation{
			"approval-1": {
				ApprovalID:  "approval-1",
				RunID:       "run-1",
				ChatID:      1001,
				SessionID:   "1001:default",
				Query:       "hello",
				ToolCallID:  "call-1",
				ToolName:    "shell.exec",
				RequestedAt: time.Now().UTC(),
			},
		},
	}
	hooks := &executionTestHooks{}
	svc := NewExecutionService(NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps())), hooks)

	ok, err := svc.ResumeApprovalContinuation(context.Background(), "approval-1")
	if err != nil || !ok {
		t.Fatalf("resume continuation: ok=%v err=%v", ok, err)
	}
	deadline := time.Now().Add(time.Second)
	for time.Now().Before(deadline) {
		if hooks.finishedApprovalID == "approval-1" && hooks.executedTool.Name == "shell.exec" {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("approval continuation was not executed: %+v", hooks)
}

func TestExecutionServiceCreatesTimeoutDecisionOnFirstProviderTimeout(t *testing.T) {
	store := &executionTestStore{}
	testProvider := &timeoutOnceThenSuccessProvider{}
	hooks := &executionTestHooks{
		conversation: func(ctx context.Context, chatID int64) ConversationHooks {
			return ConversationHooks{
				Provider:             testProvider,
				Store:                executionConversationStore{},
				Compactor:            compaction.New(compaction.Deps{}),
				Budget:               compaction.Budget{},
				ProviderRoundTimeout: 5 * time.Millisecond,
				ProviderTools:        func(role string) ([]provider.ToolDefinition, error) { return nil, nil },
				RequestConfig:        func(chatID int64) provider.RequestConfig { return provider.RequestConfig{} },
				InjectPromptContext: func(chatID int64, messages []provider.Message) ([]provider.Message, error) {
					return messages, nil
				},
				ExecuteTool:     func(ctx context.Context, chatID int64, call provider.ToolCall) (string, error) { return "", nil },
				LastUserMessage: func(messages []provider.Message) string { return "hello" },
			}
		},
	}
	svc := NewExecutionService(NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps())), hooks)
	svc.timeoutDecisionWait = 20 * time.Millisecond

	view, ok, err := svc.StartAndWait(context.Background(), StartRunRequest{
		RunID:     "run-timeout-1",
		ChatID:    1001,
		SessionID: "1001:default",
		Query:     "hello",
	})
	if !ok {
		t.Fatalf("expected run to start")
	}
	if err != nil {
		t.Fatalf("expected timeout decision flow instead of fatal error, got %v", err)
	}
	if view.RunID != "run-timeout-1" {
		t.Fatalf("unexpected run view: %+v", view)
	}
	decision, ok, err := store.TimeoutDecision("run-timeout-1")
	if err != nil {
		t.Fatalf("timeout decision lookup: %v", err)
	}
	if !ok {
		t.Fatal("expected timeout decision to be stored")
	}
	if decision.Status != TimeoutDecisionContinued {
		t.Fatalf("unexpected timeout decision status: %+v", decision)
	}
	run, ok, err := store.Run("run-timeout-1")
	if err != nil {
		t.Fatalf("run lookup: %v", err)
	}
	if !ok {
		t.Fatal("expected run record")
	}
	if run.Status != StatusCompleted {
		t.Fatalf("expected run to complete after auto-continue, got %s", run.Status)
	}
	if hooks.successResp.Text != "continued reply" {
		t.Fatalf("expected run to auto-continue and succeed, got %+v", hooks.successResp)
	}
}

func TestExecutionServiceEmitsArtifactOffloadedEvent(t *testing.T) {
	store := &executionEventStore{}
	hooks := &executionTestHooks{
		conversation: func(ctx context.Context, chatID int64) ConversationHooks {
			return ConversationHooks{
				Provider: &scriptedProvider{
					responses: []provider.PromptResponse{
						{
							FinishReason: "tool_calls",
							ToolCalls: []provider.ToolCall{{
								ID:   "call-1",
								Name: "shell.exec",
							}},
						},
						{
							FinishReason: "stop",
							Text:         "done",
						},
					},
				},
				Store:     executionConversationStore{},
				Compactor: compaction.New(compaction.Deps{}),
				Budget:    compaction.Budget{},
				ProviderTools: func(role string) ([]provider.ToolDefinition, error) {
					return []provider.ToolDefinition{{Name: "shell.exec"}}, nil
				},
				RequestConfig: func(chatID int64) provider.RequestConfig { return provider.RequestConfig{} },
				InjectPromptContext: func(chatID int64, messages []provider.Message) ([]provider.Message, error) {
					return messages, nil
				},
				ExecuteTool: func(ctx context.Context, chatID int64, call provider.ToolCall) (string, error) {
					return "large output", nil
				},
				ShapeToolResult: func(chatID int64, call provider.ToolCall, content string) (OffloadedToolResult, error) {
					return OffloadedToolResult{
						Content:     "tool output offloaded\nartifact_ref: artifact://tool-output-1",
						ArtifactRef: "artifact://tool-output-1",
						Offloaded:   true,
					}, nil
				},
				LastUserMessage: func(messages []provider.Message) string { return "hello" },
			}
		},
	}
	svc := NewExecutionService(NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps())), hooks)

	_, ok, err := svc.StartAndWait(context.Background(), StartRunRequest{
		RunID:       "run-1",
		ChatID:      1001,
		SessionID:   "1001:default",
		Query:       "hello",
		Interactive: false,
	})
	if err != nil || !ok {
		t.Fatalf("start run: ok=%v err=%v", ok, err)
	}
	for _, event := range store.events {
		if event.Kind != "artifact.offloaded" {
			continue
		}
		if event.EntityType != "run" || event.EntityID != "run-1" {
			t.Fatalf("unexpected artifact event target: %+v", event)
		}
		var payload map[string]any
		if err := json.Unmarshal(event.Payload, &payload); err != nil {
			t.Fatalf("decode payload: %v", err)
		}
		if payload["artifact_ref"] != "artifact://tool-output-1" {
			t.Fatalf("unexpected payload: %+v", payload)
		}
		return
	}
	t.Fatalf("expected artifact.offloaded event, got %+v", store.events)
}

func TestExecutionServiceEmitsAssistantFinalEvent(t *testing.T) {
	store := &executionEventStore{}
	hooks := &executionTestHooks{
		conversation: func(ctx context.Context, chatID int64) ConversationHooks {
			return ConversationHooks{
				Provider: &scriptedProvider{
					responses: []provider.PromptResponse{
						{
							FinishReason: "stop",
							Text:         "final answer",
							Model:        "fake-model",
						},
					},
				},
				Store:     executionConversationStore{},
				Compactor: compaction.New(compaction.Deps{}),
				Budget:    compaction.Budget{},
				ProviderTools: func(role string) ([]provider.ToolDefinition, error) {
					return nil, nil
				},
				RequestConfig: func(chatID int64) provider.RequestConfig { return provider.RequestConfig{} },
				InjectPromptContext: func(chatID int64, messages []provider.Message) ([]provider.Message, error) {
					return messages, nil
				},
				ExecuteTool: func(ctx context.Context, chatID int64, call provider.ToolCall) (string, error) {
					return "", nil
				},
				LastUserMessage: func(messages []provider.Message) string { return "hello" },
			}
		},
	}
	svc := NewExecutionService(NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps())), hooks)

	_, ok, err := svc.StartAndWait(context.Background(), StartRunRequest{
		RunID:       "run-1",
		ChatID:      1001,
		SessionID:   "1001:default",
		Query:       "hello",
		Interactive: false,
	})
	if err != nil || !ok {
		t.Fatalf("start run: ok=%v err=%v", ok, err)
	}
	for _, event := range store.events {
		if event.Kind != "assistant.final" {
			continue
		}
		if event.EntityType != "run" || event.EntityID != "run-1" {
			t.Fatalf("unexpected assistant event target: %+v", event)
		}
		var payload map[string]any
		if err := json.Unmarshal(event.Payload, &payload); err != nil {
			t.Fatalf("decode payload: %v", err)
		}
		if payload["text"] != "final answer" {
			t.Fatalf("unexpected payload: %+v", payload)
		}
		return
	}
	t.Fatalf("expected assistant.final event, got %+v", store.events)
}
