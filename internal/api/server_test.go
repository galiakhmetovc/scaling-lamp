package api

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/artifacts"
	"teamd/internal/llmtrace"
	"teamd/internal/memory"
	"teamd/internal/provider"
	"teamd/internal/provider/zai"
	"teamd/internal/runtime"
)

type apiTestRunner struct {
	view runtime.RunView
	ok   bool
	err  error
	req  runtime.StartRunRequest
}

func (r *apiTestRunner) StartDetached(ctx context.Context, req runtime.StartRunRequest) (runtime.RunView, bool, error) {
	r.req = req
	return r.view, r.ok, r.err
}

type apiTestJobs struct {
	job  runtime.JobView
	logs []runtime.JobLogChunk
}

func (j apiTestJobs) StartDetached(ctx context.Context, req runtime.JobStartRequest) (runtime.JobView, error) {
	if j.job.JobID != "" {
		return j.job, nil
	}
	return runtime.JobView{
		JobID:     "job-1",
		ChatID:    req.ChatID,
		SessionID: req.SessionID,
		Command:   req.Command,
		Args:      req.Args,
		Status:    runtime.JobRunning,
		StartedAt: time.Now().UTC(),
		Active:    true,
	}, nil
}
func (j apiTestJobs) Job(jobID string) (runtime.JobView, bool, error) {
	if j.job.JobID == jobID {
		return j.job, true, nil
	}
	return runtime.JobView{}, false, nil
}
func (j apiTestJobs) List(limit int) ([]runtime.JobView, error) {
	if j.job.JobID == "" {
		return nil, nil
	}
	return []runtime.JobView{j.job}, nil
}
func (j apiTestJobs) Logs(query runtime.JobLogQuery) ([]runtime.JobLogChunk, error) {
	return j.logs, nil
}
func (j apiTestJobs) Cancel(jobID string) (bool, error) {
	return j.job.JobID == jobID, nil
}

type apiTestWorkers struct {
	worker  runtime.WorkerView
	wait    runtime.WorkerWaitResult
	handoff runtime.WorkerHandoff
}

type apiTestSessionActions struct {
	result runtime.SessionActionResult
}

type apiTestToolCatalog struct {
	items []provider.ToolDefinition
}

type apiTestToolExecutor struct {
	output string
	err    error
	call   provider.ToolCall
	chatID int64
	allowedTools []string
}

type apiTestPreviewer struct {
	request provider.PromptRequest
	metrics runtime.PromptBudgetMetrics
	err     error
	chatID  int64
	session string
	query   string
	config  provider.RequestConfig
	profile *runtime.DebugExecutionProfile
}

func (c apiTestToolCatalog) DebugToolCatalog(role string) ([]provider.ToolDefinition, error) {
	return append([]provider.ToolDefinition(nil), c.items...), nil
}

func (e *apiTestToolExecutor) ExecuteApprovedTool(_ context.Context, chatID int64, allowedTools []string, call provider.ToolCall) (string, error) {
	e.chatID = chatID
	e.allowedTools = append([]string(nil), allowedTools...)
	e.call = call
	return e.output, e.err
}

func (p *apiTestPreviewer) DebugProviderPreview(_ context.Context, chatID int64, sessionID, query string, runtimeConfig provider.RequestConfig, profile *runtime.DebugExecutionProfile) (provider.PromptRequest, runtime.PromptBudgetMetrics, error) {
	p.chatID = chatID
	p.session = sessionID
	p.query = query
	p.config = runtimeConfig
	p.profile = profile
	return p.request, p.metrics, p.err
}

func (a apiTestSessionActions) Execute(chatID int64, req runtime.SessionActionRequest) (runtime.SessionActionResult, error) {
	if a.result.ActiveSession == "" {
		return runtime.SessionActionResult{
			Action:        req.Action,
			ActiveSession: "deploy",
			Sessions:      []string{"default", "deploy"},
			MessageCount:  2,
		}, nil
	}
	return a.result, nil
}

func (w apiTestWorkers) Spawn(ctx context.Context, req runtime.WorkerSpawnRequest) (runtime.WorkerView, error) {
	if w.worker.WorkerID != "" {
		return w.worker, nil
	}
	return runtime.WorkerView{
		WorkerID:        "worker-1",
		ParentChatID:    req.ParentChatID,
		ParentSessionID: req.ParentSessionID,
		WorkerChatID:    -1,
		WorkerSessionID: "worker-1",
		Status:          runtime.WorkerIdle,
		CreatedAt:       time.Now().UTC(),
		UpdatedAt:       time.Now().UTC(),
	}, nil
}

func (w apiTestWorkers) Message(ctx context.Context, workerID string, req runtime.WorkerMessageRequest) (runtime.WorkerView, error) {
	return w.worker, nil
}

func (w apiTestWorkers) Wait(workerID string, afterCursor int, afterEventID int64, eventLimit int) (runtime.WorkerWaitResult, bool, error) {
	return w.wait, true, nil
}

func (w apiTestWorkers) Close(workerID string) (runtime.WorkerView, bool, error) {
	return w.worker, true, nil
}

func (w apiTestWorkers) Handoff(workerID string) (runtime.WorkerHandoff, bool, error) {
	return w.handoff, w.handoff.WorkerID == workerID, nil
}

func (w apiTestWorkers) Worker(workerID string) (runtime.WorkerView, bool, error) {
	return w.worker, w.worker.WorkerID == workerID, nil
}

func (w apiTestWorkers) List(query runtime.WorkerQuery) ([]runtime.WorkerView, error) {
	if w.worker.WorkerID == "" {
		return nil, nil
	}
	return []runtime.WorkerView{w.worker}, nil
}

type apiTestRunStore struct {
	run    runtime.RunRecord
	ok     bool
	events []runtime.RuntimeEvent
	plans  map[string]runtime.PlanRecord
	head   runtime.SessionHead
	headOK bool
}

func (s *apiTestRunStore) SaveRun(run runtime.RunRecord) error { s.run = run; s.ok = true; return nil }
func (s *apiTestRunStore) MarkCancelRequested(runID string) error {
	s.run.CancelRequested = true
	return nil
}
func (s *apiTestRunStore) Run(runID string) (runtime.RunRecord, bool, error) {
	if s.ok && s.run.RunID == runID {
		return s.run, true, nil
	}
	return runtime.RunRecord{}, false, nil
}
func (s *apiTestRunStore) ListRuns(query runtime.RunQuery) ([]runtime.RunRecord, error) {
	if !s.ok {
		return nil, nil
	}
	if query.HasChatID && s.run.ChatID != query.ChatID {
		return nil, nil
	}
	if query.SessionID != "" && s.run.SessionID != query.SessionID {
		return nil, nil
	}
	if query.HasStatus && s.run.Status != query.Status {
		return nil, nil
	}
	return []runtime.RunRecord{s.run}, nil
}
func (s *apiTestRunStore) ListSessions(query runtime.SessionQuery) ([]runtime.SessionRecord, error) {
	if !s.ok {
		return nil, nil
	}
	if query.HasChatID && s.run.ChatID != query.ChatID {
		return nil, nil
	}
	return []runtime.SessionRecord{{
		SessionID:      s.run.SessionID,
		LastActivityAt: s.run.StartedAt,
		HasOverrides:   false,
	}}, nil
}
func (s *apiTestRunStore) SaveEvent(runtime.RuntimeEvent) error { return nil }
func (s *apiTestRunStore) ListEvents(query runtime.EventQuery) ([]runtime.RuntimeEvent, error) {
	return append([]runtime.RuntimeEvent(nil), s.events...), nil
}
func (s *apiTestRunStore) RecoverInterruptedRuns(reason string) (int, error) { return 0, nil }
func (s *apiTestRunStore) SaveCheckpoint(runtime.Checkpoint) error           { return nil }
func (s *apiTestRunStore) Checkpoint(chatID int64, sessionID string) (runtime.Checkpoint, bool, error) {
	return runtime.Checkpoint{}, false, nil
}
func (s *apiTestRunStore) SaveContinuity(runtime.Continuity) error { return nil }
func (s *apiTestRunStore) Continuity(chatID int64, sessionID string) (runtime.Continuity, bool, error) {
	return runtime.Continuity{}, false, nil
}
func (s *apiTestRunStore) SaveSessionHead(head runtime.SessionHead) error {
	s.head = head
	s.headOK = true
	return nil
}
func (s *apiTestRunStore) SessionHead(chatID int64, sessionID string) (runtime.SessionHead, bool, error) {
	if s.headOK && s.head.ChatID == chatID && s.head.SessionID == sessionID {
		return s.head, true, nil
	}
	return runtime.SessionHead{}, false, nil
}
func (s *apiTestRunStore) TryMarkUpdate(chatID int64, updateID int64) (bool, error) { return true, nil }
func (s *apiTestRunStore) SaveSessionOverrides(overrides runtime.SessionOverrides) error {
	return nil
}
func (s *apiTestRunStore) SessionOverrides(sessionID string) (runtime.SessionOverrides, bool, error) {
	return runtime.SessionOverrides{}, false, nil
}
func (s *apiTestRunStore) ClearSessionOverrides(sessionID string) error { return nil }
func (s *apiTestRunStore) SaveApproval(record approvals.Record) error   { return nil }
func (s *apiTestRunStore) Approval(id string) (approvals.Record, bool, error) {
	return approvals.Record{}, false, nil
}
func (s *apiTestRunStore) PendingApprovals(sessionID string) ([]approvals.Record, error) {
	return nil, nil
}
func (s *apiTestRunStore) SaveHandledApprovalCallback(updateID string, record approvals.Record) error {
	return nil
}
func (s *apiTestRunStore) HandledApprovalCallback(updateID string) (approvals.Record, bool, error) {
	return approvals.Record{}, false, nil
}
func (s *apiTestRunStore) SaveApprovalContinuation(runtime.ApprovalContinuation) error { return nil }
func (s *apiTestRunStore) ApprovalContinuation(id string) (runtime.ApprovalContinuation, bool, error) {
	return runtime.ApprovalContinuation{}, false, nil
}
func (s *apiTestRunStore) DeleteApprovalContinuation(id string) error { return nil }
func (s *apiTestRunStore) SaveTimeoutDecision(runtime.TimeoutDecisionRecord) error {
	return nil
}
func (s *apiTestRunStore) TimeoutDecision(runID string) (runtime.TimeoutDecisionRecord, bool, error) {
	return runtime.TimeoutDecisionRecord{}, false, nil
}
func (s *apiTestRunStore) DeleteTimeoutDecision(runID string) error { return nil }
func (s *apiTestRunStore) SavePlan(plan runtime.PlanRecord) error {
	if s.plans == nil {
		s.plans = map[string]runtime.PlanRecord{}
	}
	s.plans[plan.PlanID] = plan
	return nil
}
func (s *apiTestRunStore) Plan(planID string) (runtime.PlanRecord, bool, error) {
	item, ok := s.plans[planID]
	return item, ok, nil
}
func (s *apiTestRunStore) ListPlans(query runtime.PlanQuery) ([]runtime.PlanRecord, error) {
	out := []runtime.PlanRecord{}
	for _, item := range s.plans {
		if query.OwnerType != "" && item.OwnerType != query.OwnerType {
			continue
		}
		if query.OwnerID != "" && item.OwnerID != query.OwnerID {
			continue
		}
		out = append(out, item)
	}
	return out, nil
}

func TestServerReturnsRuntimeSummary(t *testing.T) {
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{Profile: "conservative"}, runtime.ActionPolicy{ApprovalRequiredTools: []string{"shell.exec"}})
	req := httptest.NewRequest(http.MethodGet, "/api/runtime", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out RuntimeSummaryResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if out.MemoryPolicy.Profile != "conservative" || len(out.ActionPolicy.ApprovalRequiredTools) != 1 || out.Runtime.Model != "glm-5-turbo" {
		t.Fatalf("unexpected runtime summary: %+v", out)
	}
}

func TestServerReturnsDebugSessionAndRunViews(t *testing.T) {
	now := time.Now().UTC()
	store := &apiTestRunStore{
		run: runtime.RunRecord{
			RunID:         "run-1",
			ChatID:        1001,
			SessionID:     "1001:default",
			Query:         "hello",
			FinalResponse: "done",
			Status:        runtime.StatusCompleted,
			StartedAt:     now,
			PromptBudget: runtime.PromptBudgetMetrics{
				ContextWindowTokens: 200000,
				PromptBudgetTokens:  150000,
				FinalPromptTokens:   42000,
			},
		},
		ok: true,
		head: runtime.SessionHead{
			ChatID:             1001,
			SessionID:          "1001:default",
			LastCompletedRunID: "run-1",
			CurrentGoal:        "debug runtime",
			LastResultSummary:  "done",
			UpdatedAt:          now,
		},
		headOK: true,
		events: []runtime.RuntimeEvent{
			{EntityType: "run", EntityID: "run-1", ChatID: 1001, SessionID: "1001:default", RunID: "run-1", Kind: "prompt.assembled", CreatedAt: now},
		},
	}
	apiCore := runtime.NewRuntimeCore(
		runtime.NewAPI(store, runtime.NewActiveRegistry(), approvals.New(approvals.TestDeps())),
		nil, nil, nil, nil,
		provider.RequestConfig{Model: "glm-5"},
		runtime.MemoryPolicy{Profile: "conservative"},
		runtime.ActionPolicy{},
	)
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).WithCore(apiCore)

	req := httptest.NewRequest(http.MethodGet, "/api/debug/sessions/1001:default?chat_id=1001&event_limit=10", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected debug session status: %d body=%s", rec.Code, rec.Body.String())
	}
	var sessionOut map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &sessionOut); err != nil {
		t.Fatalf("decode debug session: %v", err)
	}
	if sessionOut["session"] == nil || sessionOut["control"] == nil {
		t.Fatalf("expected session and control in debug session response: %+v", sessionOut)
	}

	req = httptest.NewRequest(http.MethodGet, "/api/debug/runs/run-1?event_limit=10", nil)
	rec = httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected debug run status: %d body=%s", rec.Code, rec.Body.String())
	}
	var runOut map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &runOut); err != nil {
		t.Fatalf("decode debug run: %v", err)
	}
	if runOut["run"] == nil || runOut["replay"] == nil {
		t.Fatalf("expected run and replay in debug run response: %+v", runOut)
	}

	req = httptest.NewRequest(http.MethodGet, "/api/debug/runs/run-1/context-provenance", nil)
	rec = httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected debug provenance status: %d body=%s", rec.Code, rec.Body.String())
	}
	var provenanceOut map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &provenanceOut); err != nil {
		t.Fatalf("decode debug provenance: %v", err)
	}
	if provenanceOut["provenance"] == nil {
		t.Fatalf("expected provenance in debug provenance response: %+v", provenanceOut)
	}
}

func TestServerStartsRunViaDebugSessionMessageSubmit(t *testing.T) {
	runner := &apiTestRunner{
		view: runtime.RunView{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:debug",
			Query:     "hello from web",
			Status:    runtime.StatusRunning,
			StartedAt: time.Now().UTC(),
			Active:    true,
		},
		ok: true,
	}
	server := NewServer(
		nil,
		nil,
		nil,
		runner,
		nil,
		nil,
		provider.RequestConfig{},
		runtime.MemoryPolicy{},
		runtime.ActionPolicy{},
	)
	body := strings.NewReader(`{"chat_id":1001,"query":"hello from web"}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/sessions/1001:debug/messages", body)
	req.Header.Set("Content-Type", "application/json")
	rec := httptest.NewRecorder()

	server.Handler().ServeHTTP(rec, req)

	if rec.Code != http.StatusAccepted {
		t.Fatalf("unexpected debug message status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out CreateRunResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if !out.Accepted || out.Run.RunID != "run-1" || out.Run.SessionID != "1001:debug" {
		t.Fatalf("unexpected debug message response: %+v", out)
	}
	if runner.req.Interactive {
		t.Fatalf("expected debug web submit to start non-interactive run, got %+v", runner.req)
	}
}

func TestServerServesWebTestBenchShellAndAssets(t *testing.T) {
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})

	req := httptest.NewRequest(http.MethodGet, "/debug/test-bench", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected shell status: %d body=%s", rec.Code, rec.Body.String())
	}
	if got := rec.Header().Get("Cache-Control"); !strings.Contains(got, "no-store") {
		t.Fatalf("expected no-store cache control for shell, got %q", got)
	}
	if !strings.Contains(rec.Body.String(), "teamD Web Session Test Bench") {
		t.Fatalf("expected shell title, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `id="new-session-form"`) {
		t.Fatalf("expected new session form, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `id="chat-form"`) {
		t.Fatalf("expected chat form, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `id="mode-display"`) || !strings.Contains(rec.Body.String(), `value="Raw Conversation"`) {
		t.Fatalf("expected raw-conversation-only shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `id="model-select"`) || !strings.Contains(rec.Body.String(), `id="temperature-input"`) {
		t.Fatalf("expected model and parameter controls in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `id="reasoning-mode-select"`) || !strings.Contains(rec.Body.String(), `id="clear-thinking-input"`) {
		t.Fatalf("expected reasoning controls in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `id="top-p-input"`) || !strings.Contains(rec.Body.String(), `id="max-tokens-input"`) {
		t.Fatalf("expected sampling controls in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `id="do-sample-input"`) || !strings.Contains(rec.Body.String(), `id="response-format-select"`) {
		t.Fatalf("expected advanced provider controls in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "Session Setup") || !strings.Contains(rec.Body.String(), "Provider Runtime") {
		t.Fatalf("expected grouped launch rail sections, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "Thinking") || !strings.Contains(rec.Body.String(), "Sampling") || !strings.Contains(rec.Body.String(), "Response Shape") {
		t.Fatalf("expected grouped provider config sections, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "clear_thinking") {
		t.Fatalf("expected explicit clear_thinking label, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "<h3>Tools</h3>") || !strings.Contains(rec.Body.String(), `id="tool-picker"`) {
		t.Fatalf("expected tool picker in raw-conversation shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `id="system-prompt-input"`) || !strings.Contains(rec.Body.String(), `id="include-system-prompt-input"`) {
		t.Fatalf("expected system prompt controls in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `id="auto-approve-tools-input"`) || !strings.Contains(rec.Body.String(), `id="tool-picker-shell"`) {
		t.Fatalf("expected tool execution controls in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `id="offload-old-tools-input"`) {
		t.Fatalf("expected offload old tool outputs control in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `id="submit-status"`) {
		t.Fatalf("expected submit status container in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), `id="pending-tool-banner"`) {
		t.Fatalf("expected pending tool banner container in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "Request Preview") {
		t.Fatalf("expected request preview panel in shell, got %q", rec.Body.String())
	}

	req = httptest.NewRequest(http.MethodGet, "/debug/assets/app.js", nil)
	rec = httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected js status: %d body=%s", rec.Code, rec.Body.String())
	}
	if got := rec.Header().Get("Cache-Control"); !strings.Contains(got, "no-store") {
		t.Fatalf("expected no-store cache control for js, got %q", got)
	}
	if !strings.Contains(rec.Body.String(), "loadSessions") {
		t.Fatalf("expected app bootstrap, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "renderRequestPreview") {
		t.Fatalf("expected request preview renderer in shell, got %q", rec.Body.String())
	}
	if strings.Contains(rec.Body.String(), "/provider-preview") || strings.Contains(rec.Body.String(), "Ignored Session Controls") {
		t.Fatalf("expected session-only preview wiring to be removed from raw-conversation shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "createSession") {
		t.Fatalf("expected createSession handler, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "submitMessage") {
		t.Fatalf("expected submitMessage handler, got %q", rec.Body.String())
	}
	if strings.Contains(rec.Body.String(), "loadContextProvenance") || strings.Contains(rec.Body.String(), "scheduleSessionRefresh") {
		t.Fatalf("expected session-only handlers to be removed from raw-conversation shell, got %q", rec.Body.String())
	}
	if strings.Contains(rec.Body.String(), "selectedView.session") {
		t.Fatalf("expected session wrapper access to be removed from raw-conversation shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "latest_answer") {
		t.Fatalf("expected raw conversation summary in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "http-line-label") || !strings.Contains(rec.Body.String(), "Provider Payload") {
		t.Fatalf("expected raw provider payload sections, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "/api/debug/raw-network") {
		t.Fatalf("expected raw network endpoint usage in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "provider_url") || !strings.Contains(rec.Body.String(), "provider_request_headers") {
		t.Fatalf("expected snake_case raw trace fields in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "Run Metrics") || !strings.Contains(rec.Body.String(), "Usage") {
		t.Fatalf("expected run metrics usage sections in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "Session Head") || !strings.Contains(rec.Body.String(), "renderSessionHead") {
		t.Fatalf("expected SessionHead inspector in shell, got %q", rec.Body.String())
	}
	if strings.Contains(rec.Body.String(), "run-diff-left") || strings.Contains(rec.Body.String(), "run-diff-right") {
		t.Fatalf("expected run diff controls to be removed from raw-conversation shell, got %q", rec.Body.String())
	}
	if strings.Contains(rec.Body.String(), `data-mode-target="control"`) || strings.Contains(rec.Body.String(), `data-mode-target="inspector"`) {
		t.Fatalf("expected readable/raw inspector toggles to be removed from raw-conversation shell, got %q", rec.Body.String())
	}
	if strings.Contains(rec.Body.String(), "renderControlReadable") || strings.Contains(rec.Body.String(), "renderProvenanceReadable") {
		t.Fatalf("expected session inspector renderers to be removed from raw-conversation shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "readRequestConfigFromForm") || !strings.Contains(rec.Body.String(), "window.__TEAMD_RUNTIME_DEFAULTS__") {
		t.Fatalf("expected request config bootstrap and reader in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "tool-toggle") || !strings.Contains(rec.Body.String(), "Tool Picker") {
		t.Fatalf("expected checkbox-based tool picker in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "includeSystemPrompt") || !strings.Contains(rec.Body.String(), "autoApproveToolsEnabled") {
		t.Fatalf("expected raw conversation system prompt and auto-approve JS hooks in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "offloadOldToolOutputsEnabled") || !strings.Contains(rec.Body.String(), "normalizeProviderMessages") {
		t.Fatalf("expected raw conversation offload helpers in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "/api/debug/raw-conversations/") {
		t.Fatalf("expected raw conversation hydrate endpoint in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "remote.turns.length > localTurns") {
		t.Fatalf("expected server raw history to override stale localStorage when longer, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "/api/sessions/") || !strings.Contains(rec.Body.String(), "state.sessionState") {
		t.Fatalf("expected session state fetch and storage in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "Tool confirmation required") || !strings.Contains(rec.Body.String(), "waiting for confirmation") {
		t.Fatalf("expected explicit tool confirmation UI text in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "toggleTopLevelCollapse") {
		t.Fatalf("expected top-level turn/run collapse helper in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "collectDiffs") || !strings.Contains(rec.Body.String(), "normalizeForDiff") {
		t.Fatalf("expected raw turn diff helpers in shell, got %q", rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "DISPLAY_MESSAGE_LIMIT") || !strings.Contains(rec.Body.String(), "Display-only preview: showing last") {
		t.Fatalf("expected display-only truncation helpers in shell, got %q", rec.Body.String())
	}
}

func TestServerDebugToolsReturnsCatalog(t *testing.T) {
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).
		WithToolCatalog(apiTestToolCatalog{items: []provider.ToolDefinition{
			{Name: "shell_exec", Description: "run shell command"},
			{Name: "filesystem_read_file", Description: "read file"},
		}})

	req := httptest.NewRequest(http.MethodGet, "/api/debug/tools?role=telegram", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected debug tools status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out ToolCatalogResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(out.Items) != 2 || out.Items[0].Name != "shell_exec" {
		t.Fatalf("unexpected tool catalog: %+v", out)
	}
}

func TestServerDebugToolsReturnsCatalogWithVFS(t *testing.T) {
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).
		WithToolCatalog(apiTestToolCatalog{items: []provider.ToolDefinition{{Name: "shell_exec", Description: "run shell command"}}}).
		WithRawVFSRootDir(t.TempDir())

	req := httptest.NewRequest(http.MethodGet, "/api/debug/tools?role=telegram", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected tool catalog status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out ToolCatalogResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode tool catalog response: %v", err)
	}
	names := make([]string, 0, len(out.Items))
	for _, item := range out.Items {
		names = append(names, item.Name)
	}
	joined := strings.Join(names, ",")
	if !strings.Contains(joined, "shell_exec") || !strings.Contains(joined, "vfs_path") || !strings.Contains(joined, "vfs_patch") || !strings.Contains(joined, "vfs_tree") || !strings.Contains(joined, "vfs_read_file") {
		t.Fatalf("expected mixed runtime+vfs catalog, got %+v", names)
	}
}

func TestServerReturnsRawNetworkDebugTrace(t *testing.T) {
	providerServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/chat/completions" {
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"raw answer","reasoning_content":"raw chain"}}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}`))
	}))
	defer providerServer.Close()

	rawProvider := llmtrace.TracingProvider{Base: zai.NewClient(providerServer.URL, "test-key")}
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).WithRawProvider(rawProvider)

	body := bytes.NewBufferString(`{"chat_id":1001,"query":"hello raw"}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/raw-network", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected raw-network status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out DebugRawNetworkResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw-network response: %v", err)
	}
	if len(out.Request.Messages) != 1 || out.Request.Messages[0].Content != "hello raw" {
		t.Fatalf("unexpected parsed request: %+v", out.Request)
	}
	if out.Response.Text != "raw answer" {
		t.Fatalf("unexpected parsed response: %+v", out.Response)
	}
	if !strings.Contains(out.Trace.ProviderRequestBody, `"content":"hello raw"`) {
		t.Fatalf("unexpected provider request body: %q", out.Trace.ProviderRequestBody)
	}
	if out.Trace.ProviderRequestHeaders["Authorization"][0] != "Bearer test-key" {
		t.Fatalf("unexpected provider request headers: %#v", out.Trace.ProviderRequestHeaders)
	}
	if !strings.Contains(out.Trace.ProviderResponseBody, `"content":"raw answer"`) {
		t.Fatalf("unexpected provider response body: %q", out.Trace.ProviderResponseBody)
	}
	if !strings.Contains(strings.Join(out.Trace.ProviderResponseHeaders["Content-Type"], ","), "application/json") {
		t.Fatalf("unexpected provider response headers: %#v", out.Trace.ProviderResponseHeaders)
	}
	if out.Trace.ProviderStatusCode != http.StatusOK {
		t.Fatalf("unexpected provider status code: %d", out.Trace.ProviderStatusCode)
	}
}

func TestServerReturnsRawConversationTraceFromExplicitMessages(t *testing.T) {
	providerServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request: %v", err)
		}
		messages, ok := body["messages"].([]any)
		if !ok || len(messages) != 3 {
			t.Fatalf("unexpected messages payload: %#v", body["messages"])
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"third reply"}}],"usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7}}`))
	}))
	defer providerServer.Close()

	rawProvider := llmtrace.TracingProvider{Base: zai.NewClient(providerServer.URL, "test-key")}
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).WithRawProvider(rawProvider)

	body := bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:raw","messages":[{"role":"user","content":"first"},{"role":"assistant","content":"second"},{"role":"user","content":"third"}]}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/raw-network", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected raw-conversation status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out DebugRawNetworkResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw-conversation response: %v", err)
	}
	if len(out.Request.Messages) != 3 {
		t.Fatalf("unexpected parsed request messages: %+v", out.Request)
	}
	if out.Request.Messages[2].Content != "third" {
		t.Fatalf("unexpected last message in request: %+v", out.Request.Messages)
	}
	if out.Response.Text != "third reply" {
		t.Fatalf("unexpected response text: %+v", out.Response)
	}
}

func TestServerReturnsRawConversationTraceWithSelectedTools(t *testing.T) {
	providerServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request: %v", err)
		}
		tools, ok := body["tools"].([]any)
		if !ok || len(tools) != 1 {
			t.Fatalf("unexpected tools payload: %#v", body["tools"])
		}
		tool, ok := tools[0].(map[string]any)
		if !ok {
			t.Fatalf("unexpected tool definition: %#v", tools[0])
		}
		function, ok := tool["function"].(map[string]any)
		if !ok || function["name"] != "shell_exec" {
			t.Fatalf("unexpected tool definition: %#v", tools[0])
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"tool reply"}}],"usage":{"prompt_tokens":8,"completion_tokens":2,"total_tokens":10}}`))
	}))
	defer providerServer.Close()

	rawProvider := llmtrace.TracingProvider{Base: zai.NewClient(providerServer.URL, "test-key")}
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).
		WithRawProvider(rawProvider).
		WithToolCatalog(apiTestToolCatalog{items: []provider.ToolDefinition{
			{Name: "shell_exec", Description: "run shell command"},
			{Name: "filesystem_read_file", Description: "read file"},
		}})

	body := bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:raw","messages":[{"role":"user","content":"first turn"}],"tools":["shell_exec"]}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/raw-network", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected raw-conversation tool status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out DebugRawNetworkResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw-conversation tool response: %v", err)
	}
	if len(out.Request.Tools) != 1 || out.Request.Tools[0].Name != "shell_exec" {
		t.Fatalf("unexpected parsed request tools: %+v", out.Request.Tools)
	}
	if !strings.Contains(out.Trace.ProviderRequestBody, `"tools"`) || !strings.Contains(out.Trace.ProviderRequestBody, `"shell_exec"`) {
		t.Fatalf("expected provider request body to contain selected tool, got %q", out.Trace.ProviderRequestBody)
	}
}

func TestServerReturnsRawConversationTraceWithSystemPrompt(t *testing.T) {
	providerServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request: %v", err)
		}
		messages, ok := body["messages"].([]any)
		if !ok || len(messages) != 2 {
			t.Fatalf("unexpected messages payload: %#v", body["messages"])
		}
		first, ok := messages[0].(map[string]any)
		if !ok || first["role"] != "system" || first["content"] != "You are terse." {
			t.Fatalf("unexpected first system message: %#v", messages[0])
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":4,"completion_tokens":1,"total_tokens":5}}`))
	}))
	defer providerServer.Close()

	rawProvider := llmtrace.TracingProvider{Base: zai.NewClient(providerServer.URL, "test-key")}
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).
		WithRawProvider(rawProvider)

	body := bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:raw","query":"hello","system_prompt":"You are terse.","include_system_prompt":true}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/raw-network", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected raw-conversation system prompt status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out DebugRawNetworkResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw-conversation system prompt response: %v", err)
	}
	if len(out.Request.Messages) != 2 || out.Request.Messages[0].Role != "system" || out.Request.Messages[0].Content != "You are terse." {
		t.Fatalf("unexpected parsed request messages: %+v", out.Request.Messages)
	}
	if !strings.Contains(out.Trace.ProviderRequestBody, `"role":"system"`) || !strings.Contains(out.Trace.ProviderRequestBody, `You are terse.`) {
		t.Fatalf("expected provider request body to contain system prompt, got %q", out.Trace.ProviderRequestBody)
	}
}

func TestServerOffloadsOldToolOutputsFromRawConversation(t *testing.T) {
	vfsRoot := t.TempDir()
	runStore := &apiTestRunStore{
		head: runtime.SessionHead{
			ChatID:    1001,
			SessionID: "1001:raw",
		},
		headOK: true,
	}
	providerServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request: %v", err)
		}
		messages, ok := body["messages"].([]any)
		if !ok || len(messages) != 4 {
			t.Fatalf("unexpected messages payload: %#v", body["messages"])
		}
		tool, ok := messages[1].(map[string]any)
		if !ok || tool["role"] != "tool" {
			t.Fatalf("unexpected tool message: %#v", messages[1])
		}
		content := fmt.Sprint(tool["content"])
		if len(content) > 600 {
			t.Fatalf("expected old tool output to be offloaded, got full content in provider payload")
		}
		if !strings.Contains(content, ".agent/memory/") || !strings.Contains(content, "Artifact offloaded") {
			t.Fatalf("expected offload reference in provider payload, got %q", content)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"continued"}}],"usage":{"prompt_tokens":10,"completion_tokens":2,"total_tokens":12}}`))
	}))
	defer providerServer.Close()

	rawProvider := llmtrace.TracingProvider{Base: zai.NewClient(providerServer.URL, "test-key")}
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).
		WithRawProvider(rawProvider).
		WithSessionHeadStore(runStore).
		WithRawVFSRootDir(vfsRoot)

	longTool := "BEGIN-LONG-TOOL-OUTPUT\n" + strings.Repeat("0123456789abcdef", 375)
	payload, err := json.Marshal(map[string]any{
		"chat_id":                  1001,
		"session_id":               "1001:raw",
		"offload_old_tool_outputs": true,
		"messages": []map[string]any{
			{"role": "user", "content": "first"},
			{"role": "tool", "tool_call_id": "call_1", "name": "shell_exec", "content": longTool},
			{"role": "assistant", "content": "second"},
			{"role": "user", "content": "third"},
		},
	})
	if err != nil {
		t.Fatalf("marshal payload: %v", err)
	}
	body := bytes.NewBuffer(payload)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/raw-network", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected raw-conversation offload status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out DebugRawNetworkResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw-conversation offload response: %v", err)
	}
	if len(out.Request.Messages) != 4 {
		t.Fatalf("unexpected parsed request messages: %+v", out.Request.Messages)
	}
	if strings.Contains(out.Request.Messages[1].Content, longTool) {
		t.Fatalf("expected offloaded tool content in parsed request, got %q", out.Request.Messages[1].Content)
	}
	if !strings.Contains(out.Request.Messages[1].Content, ".agent/memory/") {
		t.Fatalf("expected artifact path in parsed request, got %q", out.Request.Messages[1].Content)
	}
	matches, err := filepath.Glob(filepath.Join(vfsRoot, "1001-raw", ".agent", "memory", "*.txt"))
	if err != nil {
		t.Fatalf("glob offload artifacts: %v", err)
	}
	if len(matches) != 1 {
		t.Fatalf("expected one offload artifact, got %d (%v)", len(matches), matches)
	}
	data, err := os.ReadFile(matches[0])
	if err != nil {
		t.Fatalf("read offload artifact: %v", err)
	}
	if string(data) != longTool {
		t.Fatalf("unexpected offload artifact content size=%d", len(data))
	}
	head, ok, err := runStore.SessionHead(1001, "1001:raw")
	if err != nil || !ok {
		t.Fatalf("expected session head after offload, ok=%v err=%v", ok, err)
	}
	if len(head.RecentArtifactRefs) != 1 || !strings.Contains(head.RecentArtifactRefs[0], ".agent/memory/") {
		t.Fatalf("expected offloaded artifact ref in session head, got %#v", head.RecentArtifactRefs)
	}
}

func TestServerAppendsRawConversationSessionLog(t *testing.T) {
	providerServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"logged reply"}}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}`))
	}))
	defer providerServer.Close()

	rawProvider := llmtrace.TracingProvider{Base: zai.NewClient(providerServer.URL, "test-key")}
	logDir := t.TempDir()
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).
		WithRawProvider(rawProvider).
		WithRawSessionLogDir(logDir)

	body := bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:rawlog","query":"hello logged","system_prompt":"Log this.","include_system_prompt":true}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/raw-network", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected raw-network log status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out DebugRawNetworkResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw-network log response: %v", err)
	}
	if strings.TrimSpace(out.LogPath) == "" {
		t.Fatalf("expected log path in response, got %+v", out)
	}
	data, err := os.ReadFile(out.LogPath)
	if err != nil {
		t.Fatalf("read raw session log: %v", err)
	}
	lines := strings.Split(strings.TrimSpace(string(data)), "\n")
	if len(lines) != 1 {
		t.Fatalf("expected one log line, got %d in %q", len(lines), string(data))
	}
	var entry map[string]any
	if err := json.Unmarshal([]byte(lines[0]), &entry); err != nil {
		t.Fatalf("decode log line: %v", err)
	}
	if entry["session_id"] != "1001:rawlog" || entry["query"] != "hello logged" {
		t.Fatalf("unexpected log entry identity: %#v", entry)
	}
	if entry["system_prompt"] != "Log this." {
		t.Fatalf("unexpected log system prompt: %#v", entry["system_prompt"])
	}
	if _, err := os.Stat(filepath.Dir(out.LogPath)); err != nil {
		t.Fatalf("expected log directory to exist: %v", err)
	}
}

func TestServerExecutesRawToolStep(t *testing.T) {
	executor := &apiTestToolExecutor{output: "tool output"}
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).
		WithToolExecutor(executor)

	body := bytes.NewBufferString(`{"chat_id":1001,"tools":["shell_exec"],"call":{"id":"call_1","name":"shell_exec","arguments":{"command":"echo hi"}}}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/raw-tool-exec", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected raw-tool-exec status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out DebugRawToolExecResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw-tool-exec response: %v", err)
	}
	if executor.chatID != 1001 || executor.call.Name != "shell_exec" {
		t.Fatalf("unexpected executed tool call: chat=%d call=%+v", executor.chatID, executor.call)
	}
	if len(executor.allowedTools) != 1 || executor.allowedTools[0] != "shell_exec" {
		t.Fatalf("unexpected raw allowed tools: %#v", executor.allowedTools)
	}
	if !out.Success || out.Output != "tool output" || out.Call.Name != "shell_exec" {
		t.Fatalf("unexpected raw-tool-exec response: %+v", out)
	}
}

func TestServerReturnsStructuredRawToolError(t *testing.T) {
	executor := &apiTestToolExecutor{err: fmt.Errorf("old text not found in file.md")}
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).
		WithToolExecutor(executor)

	body := bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:err","call":{"id":"call_1","name":"shell_exec","arguments":{"command":"echo hi"}}}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/raw-tool-exec", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("expected structured tool error, got status %d body=%s", rec.Code, rec.Body.String())
	}
	var out DebugRawToolExecResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw-tool-exec response: %v", err)
	}
	if out.Success {
		t.Fatalf("expected unsuccessful tool result, got %+v", out)
	}
	if out.ErrorCode != "tool_execution_error" || !strings.Contains(out.ErrorMessage, "old text not found") {
		t.Fatalf("unexpected structured tool error: %+v", out)
	}
	if !strings.Contains(out.Output, "tool execution error: old text not found") {
		t.Fatalf("unexpected tool error output: %+v", out)
	}
}

func TestServerExecutesRawVFSToolStep(t *testing.T) {
	vfsRoot := t.TempDir()
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).
		WithRawVFSRootDir(vfsRoot)

	body := bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:vfs","call":{"id":"call_1","name":"vfs_write_file","arguments":{"path":"notes/a.txt","content":"hello vfs"}}}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/raw-tool-exec", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected vfs_write_file status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out DebugRawToolExecResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw vfs write response: %v", err)
	}
	if !strings.Contains(out.Output, "wrote notes/a.txt") {
		t.Fatalf("unexpected raw vfs write output: %+v", out)
	}
	data, err := os.ReadFile(filepath.Join(vfsRoot, "1001-vfs", "notes", "a.txt"))
	if err != nil {
		t.Fatalf("read vfs file: %v", err)
	}
	if string(data) != "hello vfs" {
		t.Fatalf("unexpected vfs file content: %q", string(data))
	}

	body = bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:vfs","call":{"id":"call_2","name":"vfs_read_file","arguments":{"path":"notes/a.txt"}}}`)
	req = httptest.NewRequest(http.MethodPost, "/api/debug/raw-tool-exec", body)
	rec = httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected vfs_read_file status: %d body=%s", rec.Code, rec.Body.String())
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw vfs read response: %v", err)
	}
	if !strings.Contains(out.Output, "hello vfs") {
		t.Fatalf("unexpected raw vfs read output: %+v", out)
	}

	body = bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:vfs","call":{"id":"call_3","name":"vfs_path","arguments":{"path":"notes/a.txt"}}}`)
	req = httptest.NewRequest(http.MethodPost, "/api/debug/raw-tool-exec", body)
	rec = httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected vfs_path status: %d body=%s", rec.Code, rec.Body.String())
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw vfs path response: %v", err)
	}
	if !strings.Contains(out.Output, "relative_path: notes/a.txt") || !strings.Contains(out.Output, "exists: true") {
		t.Fatalf("unexpected raw vfs path output: %+v", out)
	}

	body = bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:vfs","call":{"id":"call_4","name":"vfs_patch","arguments":{"path":"notes/a.txt","old":"hello","new":"HELLO"}}}`)
	req = httptest.NewRequest(http.MethodPost, "/api/debug/raw-tool-exec", body)
	rec = httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected vfs_patch status: %d body=%s", rec.Code, rec.Body.String())
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw vfs patch response: %v", err)
	}
	if !out.Success || !strings.Contains(out.Output, "patched notes/a.txt") || !strings.Contains(out.Output, "+HELLO vfs") {
		t.Fatalf("unexpected raw vfs patch output: %+v", out)
	}

	body = bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:vfs","call":{"id":"call_5","name":"vfs_patch","arguments":{"path":"notes/a.txt","old":"missing","new":"HELLO"}}}`)
	req = httptest.NewRequest(http.MethodPost, "/api/debug/raw-tool-exec", body)
	rec = httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("expected structured vfs patch error, got status %d body=%s", rec.Code, rec.Body.String())
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw vfs patch error response: %v", err)
	}
	if out.Success || out.ErrorCode != "tool_execution_error" {
		t.Fatalf("unexpected structured vfs patch error: %+v", out)
	}
	if !strings.Contains(out.Output, "tool execution error: old text not found") {
		t.Fatalf("unexpected vfs patch error output: %+v", out)
	}
}

func TestServerReturnsRawNetworkTraceWithConfigOverrides(t *testing.T) {
	providerServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request: %v", err)
		}
		if body["model"] != "glm-5-plus" {
			t.Fatalf("unexpected model override: %#v", body["model"])
		}
		if body["temperature"] != 0.25 {
			t.Fatalf("unexpected temperature override: %#v", body["temperature"])
		}
		if body["top_p"] != 0.8 {
			t.Fatalf("unexpected top_p override: %#v", body["top_p"])
		}
		if body["max_tokens"] != float64(256) {
			t.Fatalf("unexpected max_tokens override: %#v", body["max_tokens"])
		}
		if body["do_sample"] != true {
			t.Fatalf("unexpected do_sample override: %#v", body["do_sample"])
		}
		responseFormat, ok := body["response_format"].(map[string]any)
		if !ok || responseFormat["type"] != "json_object" {
			t.Fatalf("unexpected response_format override: %#v", body["response_format"])
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"choices":[{"message":{"content":"raw answer"}}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}`))
	}))
	defer providerServer.Close()

	rawProvider := llmtrace.TracingProvider{Base: zai.NewClient(providerServer.URL, "test-key")}
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).WithRawProvider(rawProvider)

	body := bytes.NewBufferString(`{"chat_id":1001,"query":"hello raw","config":{"model":"glm-5-plus","temperature":0.25,"top_p":0.8,"max_tokens":256,"do_sample":true,"response_format":"json_object"}}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/raw-network", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected raw-network override status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out DebugRawNetworkResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw-network override response: %v", err)
	}
	if out.Request.Config.Model != "glm-5-plus" || out.Request.Config.ResponseFormat != "json_object" {
		t.Fatalf("unexpected parsed request config: %+v", out.Request.Config)
	}
	if out.Request.Config.DoSample == nil || !*out.Request.Config.DoSample {
		t.Fatalf("unexpected do_sample in parsed request config: %+v", out.Request.Config)
	}
}

func TestServerDebugSessionMessageMergesRequestConfig(t *testing.T) {
	runner := &apiTestRunner{
		view: runtime.RunView{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:web",
			Query:     "hello",
			Status:    runtime.StatusRunning,
			StartedAt: time.Now().UTC(),
			Active:    true,
		},
		ok: true,
	}
	server := NewServer(nil, nil, nil, runner, nil, nil, provider.RequestConfig{
		Model:         "glm-5-turbo",
		ReasoningMode: "enabled",
	}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})

	body := bytes.NewBufferString(`{"chat_id":1001,"query":"hello","config":{"model":"glm-5-plus","reasoning_mode":"disabled","temperature":0.1,"do_sample":true,"response_format":"json_object"}}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/sessions/1001:web/messages", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusAccepted {
		t.Fatalf("unexpected debug session submit status: %d body=%s", rec.Code, rec.Body.String())
	}
	if runner.req.PolicySnapshot.Runtime.Model != "glm-5-plus" {
		t.Fatalf("expected merged model override, got %+v", runner.req.PolicySnapshot.Runtime)
	}
	if runner.req.PolicySnapshot.Runtime.ReasoningMode != "disabled" {
		t.Fatalf("expected merged reasoning override, got %+v", runner.req.PolicySnapshot.Runtime)
	}
	if runner.req.PolicySnapshot.Runtime.Temperature == nil || *runner.req.PolicySnapshot.Runtime.Temperature != 0.1 {
		t.Fatalf("expected merged temperature override, got %+v", runner.req.PolicySnapshot.Runtime)
	}
	if runner.req.PolicySnapshot.Runtime.DoSample == nil || !*runner.req.PolicySnapshot.Runtime.DoSample {
		t.Fatalf("expected merged do_sample override, got %+v", runner.req.PolicySnapshot.Runtime)
	}
	if runner.req.PolicySnapshot.Runtime.ResponseFormat != "json_object" {
		t.Fatalf("expected merged response_format override, got %+v", runner.req.PolicySnapshot.Runtime)
	}
}

func TestServerDebugSessionMessagePassesContextInputsAsDebugProfile(t *testing.T) {
	runner := &apiTestRunner{
		view: runtime.RunView{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:web",
			Query:     "hello",
			Status:    runtime.StatusRunning,
			StartedAt: time.Now().UTC(),
			Active:    true,
		},
		ok: true,
	}
	server := NewServer(nil, nil, nil, runner, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})

	body := bytes.NewBufferString(`{"chat_id":1001,"query":"hello","context_inputs":{"transcript":false,"session_head":false,"recent_work":false,"memory_recall":false,"checkpoint":false,"workspace":true,"skills":false,"tools":true,"allowed_tools":["shell.exec"],"workspace_files":["AGENTS.md","docs/guide.md"]}}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/sessions/1001:web/messages", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusAccepted {
		t.Fatalf("unexpected debug session submit status: %d body=%s", rec.Code, rec.Body.String())
	}
	if runner.req.DebugProfile == nil {
		t.Fatalf("expected debug profile to be forwarded, got nil")
	}
	if runner.req.DebugProfile.Transcript {
		t.Fatalf("expected transcript=false, got %+v", *runner.req.DebugProfile)
	}
	if runner.req.DebugProfile.Tools != true {
		t.Fatalf("expected tools=true, got %+v", *runner.req.DebugProfile)
	}
	if len(runner.req.DebugProfile.AllowedTools) != 1 || runner.req.DebugProfile.AllowedTools[0] != "shell.exec" {
		t.Fatalf("expected allowed tools to be forwarded, got %+v", *runner.req.DebugProfile)
	}
	if len(runner.req.DebugProfile.WorkspaceFiles) != 2 || runner.req.DebugProfile.WorkspaceFiles[1] != "docs/guide.md" {
		t.Fatalf("expected workspace files to be forwarded, got %+v", *runner.req.DebugProfile)
	}
}

func TestServerDebugSessionProviderPreviewReturnsAssembledRequest(t *testing.T) {
	previewer := &apiTestPreviewer{
		request: provider.PromptRequest{
			WorkerID: "telegram:1001",
			Messages: []provider.Message{
				{Role: "system", Content: "ctx"},
				{Role: "user", Content: "hello"},
			},
			Tools: []provider.ToolDefinition{
				{Name: "shell_exec", Description: "run shell"},
			},
			Config: provider.RequestConfig{Model: "glm-5-turbo"},
		},
		metrics: runtime.PromptBudgetMetrics{FinalPromptTokens: 123},
	}
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).
		WithProviderPreviewer(previewer)

	body := bytes.NewBufferString(`{"chat_id":1001,"query":"hello","config":{"model":"glm-5-plus"},"context_inputs":{"tools":true,"allowed_tools":["shell.exec"]}}`)
	req := httptest.NewRequest(http.MethodPost, "/api/debug/sessions/1001:web/provider-preview", body)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected preview status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out DebugProviderPreviewResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode preview response: %v", err)
	}
	if out.Request.WorkerID != "telegram:1001" || len(out.Request.Tools) != 1 {
		t.Fatalf("unexpected preview request: %+v", out.Request)
	}
	if out.Metrics.FinalPromptTokens != 123 {
		t.Fatalf("unexpected preview metrics: %+v", out.Metrics)
	}
	if previewer.session != "1001:web" || previewer.query != "hello" || previewer.config.Model != "glm-5-plus" {
		t.Fatalf("unexpected preview invocation: %+v", previewer)
	}
	if previewer.profile == nil || len(previewer.profile.AllowedTools) != 1 || previewer.profile.AllowedTools[0] != "shell.exec" {
		t.Fatalf("unexpected preview profile: %+v", previewer.profile)
	}
}

func TestServerServesWebTestBenchShellWithOperatorToken(t *testing.T) {
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).WithOperatorToken("operator-secret")

	req := httptest.NewRequest(http.MethodGet, "/debug/test-bench", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected shell status with operator token: %d body=%s", rec.Code, rec.Body.String())
	}
	if !strings.Contains(rec.Body.String(), "window.__TEAMD_OPERATOR_TOKEN__ = \"operator-secret\";") {
		t.Fatalf("expected embedded operator token bootstrap, got %q", rec.Body.String())
	}

	req = httptest.NewRequest(http.MethodGet, "/api/sessions", nil)
	rec = httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("expected api auth to stay enforced, got %d body=%s", rec.Code, rec.Body.String())
	}
}

func TestWebTestBenchAppJSSyntax(t *testing.T) {
	if _, err := exec.LookPath("node"); err != nil {
		t.Skip("node is not available")
	}
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})
	req := httptest.NewRequest(http.MethodGet, "/debug/assets/app.js", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected js status: %d body=%s", rec.Code, rec.Body.String())
	}
	tmp, err := os.CreateTemp("", "teamd-app-*.js")
	if err != nil {
		t.Fatalf("create temp js: %v", err)
	}
	defer os.Remove(tmp.Name())
	if _, err := tmp.Write(rec.Body.Bytes()); err != nil {
		t.Fatalf("write temp js: %v", err)
	}
	if err := tmp.Close(); err != nil {
		t.Fatalf("close temp js: %v", err)
	}
	cmd := exec.Command("node", "--check", tmp.Name())
	if output, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("app.js syntax check failed: %v\n%s", err, output)
	}
}

func TestServerRequiresBearerTokenForProtectedEndpoints(t *testing.T) {
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{Profile: "conservative"}, runtime.ActionPolicy{}).WithOperatorToken("operator-secret")
	req := httptest.NewRequest(http.MethodGet, "/api/sessions", nil)
	rec := httptest.NewRecorder()

	server.Handler().ServeHTTP(rec, req)

	if rec.Code != http.StatusUnauthorized {
		t.Fatalf("unexpected status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out ErrorResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if out.Error.Code != "unauthorized" {
		t.Fatalf("unexpected error: %+v", out.Error)
	}
}

func TestServerAllowsRuntimeSummaryWithoutBearerToken(t *testing.T) {
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{Profile: "conservative"}, runtime.ActionPolicy{}).WithOperatorToken("operator-secret")
	req := httptest.NewRequest(http.MethodGet, "/api/runtime", nil)
	rec := httptest.NewRecorder()

	server.Handler().ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", rec.Code, rec.Body.String())
	}
}

func TestServerReturnsRunStatus(t *testing.T) {
	store := &apiTestRunStore{
		run: runtime.RunRecord{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Query:     "hello",
			Status:    runtime.StatusCompleted,
			StartedAt: time.Now().UTC(),
		},
		ok: true,
	}
	rt := runtime.NewAPI(store, runtime.NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	server := NewServer(rt, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})
	req := httptest.NewRequest(http.MethodGet, "/api/runs/run-1", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out RunStatusResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if out.Run.RunID != "run-1" || out.Run.Status != runtime.StatusCompleted {
		t.Fatalf("unexpected run view: %+v", out.Run)
	}
}

func TestServerListsAndDecidesApprovals(t *testing.T) {
	svc := approvals.New(approvals.TestDeps())
	record, err := svc.Create(approvals.Request{
		WorkerID:   "shell.exec",
		SessionID:  "1001:default",
		Payload:    "{}",
		Reason:     "shell.exec requires approval by action policy",
		TargetType: "run",
		TargetID:   "run-1",
	})
	if err != nil {
		t.Fatalf("create approval: %v", err)
	}
	rt := runtime.NewAPI(&apiTestRunStore{}, runtime.NewActiveRegistry(), svc)
	server := NewServer(rt, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})

	listReq := httptest.NewRequest(http.MethodGet, "/api/approvals?session_id=1001:default", nil)
	listRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(listRec, listReq)
	if listRec.Code != http.StatusOK {
		t.Fatalf("list status: %d body=%s", listRec.Code, listRec.Body.String())
	}
	var list []ApprovalRecordResponse
	if err := json.Unmarshal(listRec.Body.Bytes(), &list); err != nil {
		t.Fatalf("decode list: %v", err)
	}
	if len(list) != 1 || list[0].ID != record.ID {
		t.Fatalf("unexpected approvals list: %+v", list)
	}
	if list[0].Reason == "" || list[0].TargetType != "run" || list[0].TargetID != "run-1" || list[0].RequestedAt.IsZero() {
		t.Fatalf("missing approval audit fields in list response: %+v", list[0])
	}

	approveReq := httptest.NewRequest(http.MethodPost, "/api/approvals/"+record.ID+"/approve", nil)
	approveReq.Header.Set("X-Update-ID", "api-1")
	approveRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(approveRec, approveReq)
	if approveRec.Code != http.StatusOK {
		t.Fatalf("approve status: %d body=%s", approveRec.Code, approveRec.Body.String())
	}
	var decided ApprovalRecordResponse
	if err := json.Unmarshal(approveRec.Body.Bytes(), &decided); err != nil {
		t.Fatalf("decode approval: %v", err)
	}
	if decided.Status != approvals.StatusApproved {
		t.Fatalf("unexpected approval status: %+v", decided)
	}
	if decided.DecisionUpdateID != "api-1" || decided.DecidedAt == nil {
		t.Fatalf("missing approval decision audit fields: %+v", decided)
	}
}

func TestServerStartsAndCancelsRuns(t *testing.T) {
	store := &apiTestRunStore{
		run: runtime.RunRecord{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Query:     "hello",
			Status:    runtime.StatusRunning,
			StartedAt: time.Now().UTC(),
		},
		ok: true,
	}
	rt := runtime.NewAPI(store, runtime.NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	server := NewServer(rt, nil, nil, &apiTestRunner{
		view: runtime.RunView{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Query:     "hello",
			Status:    runtime.StatusRunning,
			StartedAt: time.Now().UTC(),
			Active:    true,
		},
		ok: true,
	}, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})

	startReq := httptest.NewRequest(http.MethodPost, "/api/runs", bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:default","query":"hello"}`))
	startRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(startRec, startReq)
	if startRec.Code != http.StatusAccepted {
		t.Fatalf("start status: %d body=%s", startRec.Code, startRec.Body.String())
	}

	_, _, _ = rt.PrepareRun(context.Background(), "run-1", 1001, "1001:default", "hello", runtime.PolicySnapshot{})
	cancelReq := httptest.NewRequest(http.MethodPost, "/api/runs/run-1/cancel", nil)
	cancelRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(cancelRec, cancelReq)
	if cancelRec.Code != http.StatusOK {
		t.Fatalf("cancel status: %d body=%s", cancelRec.Code, cancelRec.Body.String())
	}
}

func TestServerListsEvents(t *testing.T) {
	store := &apiTestRunStore{
		events: []runtime.RuntimeEvent{
			{ID: 1, EntityType: "run", EntityID: "run-1", SessionID: "1001:default", Kind: "run.started", CreatedAt: time.Now().UTC()},
		},
	}
	rt := runtime.NewAPI(store, runtime.NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	server := NewServer(rt, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})
	req := httptest.NewRequest(http.MethodGet, "/api/events?entity_type=run&entity_id=run-1&limit=10", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out EventListResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(out.Items) != 1 || out.Items[0].Kind != "run.started" {
		t.Fatalf("unexpected event response: %+v", out)
	}
}

func TestServerStreamsEvents(t *testing.T) {
	store := &apiTestRunStore{
		events: []runtime.RuntimeEvent{
			{ID: 1, EntityType: "run", EntityID: "run-1", SessionID: "1001:default", Kind: "run.started", CreatedAt: time.Now().UTC()},
		},
	}
	rt := runtime.NewAPI(store, runtime.NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	server := NewServer(rt, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})
	httpServer := httptest.NewServer(server.Handler())
	defer httpServer.Close()

	req, err := http.NewRequest(http.MethodGet, httpServer.URL+"/api/events/stream?entity_type=run&entity_id=run-1&limit=10", nil)
	if err != nil {
		t.Fatalf("new request: %v", err)
	}
	req.Header.Set("Accept", "text/event-stream")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatalf("do request: %v", err)
	}
	defer resp.Body.Close()

	if got := resp.Header.Get("Content-Type"); !strings.Contains(got, "text/event-stream") {
		t.Fatalf("unexpected content type: %q", got)
	}
	reader := bufio.NewReader(resp.Body)
	body, err := reader.ReadString('\n')
	if err != nil {
		t.Fatalf("read event line: %v", err)
	}
	dataLine, err := reader.ReadString('\n')
	if err != nil {
		t.Fatalf("read data line: %v", err)
	}
	text := body + dataLine
	if !strings.Contains(text, "event: runtime") || !strings.Contains(text, "\"Kind\":\"run.started\"") {
		t.Fatalf("unexpected stream body: %q", text)
	}
}

func TestServerCreatesShowsAndUpdatesPlans(t *testing.T) {
	store := &apiTestRunStore{
		plans: map[string]runtime.PlanRecord{
			"plan-seeded": {
				PlanID:    "plan-seeded",
				OwnerType: "run",
				OwnerID:   "run-1",
				Title:     "Investigate rollout",
			},
		},
	}
	rt := runtime.NewAPI(store, runtime.NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	server := NewServer(rt, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})

	createReq := httptest.NewRequest(http.MethodPost, "/api/plans", bytes.NewBufferString(`{"owner_type":"run","owner_id":"run-2","title":"Inspect runtime"}`))
	createRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(createRec, createReq)
	if createRec.Code != http.StatusCreated {
		t.Fatalf("create status: %d body=%s", createRec.Code, createRec.Body.String())
	}

	listReq := httptest.NewRequest(http.MethodGet, "/api/plans?owner_type=run&owner_id=run-1", nil)
	listRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(listRec, listReq)
	if listRec.Code != http.StatusOK {
		t.Fatalf("list status: %d body=%s", listRec.Code, listRec.Body.String())
	}
	var list PlanListResponse
	if err := json.Unmarshal(listRec.Body.Bytes(), &list); err != nil {
		t.Fatalf("decode list: %v", err)
	}
	if len(list.Items) != 1 || list.Items[0].PlanID != "plan-seeded" {
		t.Fatalf("unexpected plan list: %+v", list.Items)
	}

	replaceReq := httptest.NewRequest(http.MethodPut, "/api/plans/plan-seeded/items", bytes.NewBufferString(`{"items":[{"content":"Inspect runtime events"},{"content":"Verify CLI output"}]}`))
	replaceRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(replaceRec, replaceReq)
	if replaceRec.Code != http.StatusOK {
		t.Fatalf("replace status: %d body=%s", replaceRec.Code, replaceRec.Body.String())
	}

	noteReq := httptest.NewRequest(http.MethodPost, "/api/plans/plan-seeded/notes", bytes.NewBufferString(`{"note":"Focus on runtime-owned state."}`))
	noteRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(noteRec, noteReq)
	if noteRec.Code != http.StatusOK {
		t.Fatalf("note status: %d body=%s", noteRec.Code, noteRec.Body.String())
	}

	showReq := httptest.NewRequest(http.MethodGet, "/api/plans/plan-seeded", nil)
	showRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(showRec, showReq)
	if showRec.Code != http.StatusOK {
		t.Fatalf("show status: %d body=%s", showRec.Code, showRec.Body.String())
	}
	var out PlanResponse
	if err := json.Unmarshal(showRec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode show: %v", err)
	}
	if len(out.Plan.Items) != 2 || len(out.Plan.Notes) != 1 {
		t.Fatalf("unexpected plan response: %+v", out.Plan)
	}
}

func TestServerReadsArtifacts(t *testing.T) {
	store := artifacts.NewInMemoryStore()
	_, err := store.Save("run", "run-1", "tool-output-1", []byte("full artifact body"))
	if err != nil {
		t.Fatalf("save artifact: %v", err)
	}
	server := NewServer(nil, nil, store, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})

	metaReq := httptest.NewRequest(http.MethodGet, "/api/artifacts/artifact:%2F%2Ftool-output-1", nil)
	metaRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(metaRec, metaReq)
	if metaRec.Code != http.StatusOK {
		t.Fatalf("unexpected metadata status: %d body=%s", metaRec.Code, metaRec.Body.String())
	}

	contentReq := httptest.NewRequest(http.MethodGet, "/api/artifacts/artifact:%2F%2Ftool-output-1/content", nil)
	contentRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(contentRec, contentReq)
	if contentRec.Code != http.StatusOK {
		t.Fatalf("unexpected content status: %d body=%s", contentRec.Code, contentRec.Body.String())
	}
	if contentRec.Body.String() == "" || contentRec.Body.String() != "full artifact body" {
		t.Fatalf("unexpected artifact content: %q", contentRec.Body.String())
	}
}

func TestServerSearchesArtifactsByScopeWithPreview(t *testing.T) {
	store := artifacts.NewInMemoryStore()
	_, err := store.Save("run", "run-1", "tool-output-1", []byte("alpha\nbeta\ngamma\ndelta"))
	if err != nil {
		t.Fatalf("save artifact: %v", err)
	}
	_, err = store.Save("run", "run-2", "tool-output-2", []byte("alpha\nother\npayload"))
	if err != nil {
		t.Fatalf("save artifact: %v", err)
	}
	server := NewServer(nil, nil, store, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})

	req := httptest.NewRequest(http.MethodGet, "/api/artifacts/search?owner_type=run&owner_id=run-1&query=beta&limit=5", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out ArtifactSearchResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(out.Items) != 1 {
		t.Fatalf("unexpected search result count: %+v", out.Items)
	}
	item := out.Items[0]
	if item.Ref != "artifact://tool-output-1" || item.OwnerType != "run" || item.OwnerID != "run-1" {
		t.Fatalf("unexpected search hit: %+v", item)
	}
	if !strings.Contains(item.Preview, "alpha") || !strings.Contains(item.Preview, "beta") {
		t.Fatalf("expected preview snippet, got %+v", item)
	}

	globalReq := httptest.NewRequest(http.MethodGet, "/api/artifacts/search?global=true&query=alpha&limit=10", nil)
	globalRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(globalRec, globalReq)
	if globalRec.Code != http.StatusOK {
		t.Fatalf("global search status: %d body=%s", globalRec.Code, globalRec.Body.String())
	}
	var globalOut ArtifactSearchResponse
	if err := json.Unmarshal(globalRec.Body.Bytes(), &globalOut); err != nil {
		t.Fatalf("decode global search: %v", err)
	}
	if len(globalOut.Items) != 2 {
		t.Fatalf("unexpected global search result count: %+v", globalOut.Items)
	}
}

func TestServerListsRuns(t *testing.T) {
	store := &apiTestRunStore{
		run: runtime.RunRecord{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Query:     "hello",
			Status:    runtime.StatusRunning,
			StartedAt: time.Now().UTC(),
		},
		ok: true,
	}
	rt := runtime.NewAPI(store, runtime.NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	server := NewServer(rt, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})
	req := httptest.NewRequest(http.MethodGet, "/api/runs?session_id=1001:default", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("list status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out RunListResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(out.Items) != 1 || out.Items[0].RunID != "run-1" {
		t.Fatalf("unexpected runs: %+v", out.Items)
	}
}

func TestServerUpdatesSessionOverrides(t *testing.T) {
	store := NewMemoryOverrideStore()
	rt := runtime.NewAPI(store, runtime.NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	server := NewServer(rt, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{Profile: "conservative"}, runtime.ActionPolicy{ApprovalRequiredTools: []string{"shell.exec"}})

	req := httptest.NewRequest(http.MethodPatch, "/api/runtime/sessions/1001:default", bytes.NewBufferString(`{"runtime":{"model":"glm-5.1"},"memory_policy":{"profile":"standard"}}`))
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("patch status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out RuntimeSummaryResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode patch response: %v", err)
	}
	if out.Runtime.Model != "glm-5.1" || out.MemoryPolicy.Profile != "standard" || !out.HasOverrides || out.Overrides == nil {
		t.Fatalf("unexpected patched summary: %+v", out)
	}
}

func TestServerListsAndShowsSessions(t *testing.T) {
	store := NewMemoryOverrideStore()
	store.run = runtime.RunRecord{
		RunID:     "run-1",
		ChatID:    1001,
		SessionID: "1001:default",
		Query:     "hello",
		Status:    runtime.StatusCompleted,
		StartedAt: time.Now().UTC(),
	}
	store.ok = true
	rt := runtime.NewAPI(store, runtime.NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	server := NewServer(rt, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{Profile: "conservative"}, runtime.ActionPolicy{})

	listReq := httptest.NewRequest(http.MethodGet, "/api/sessions?chat_id=1001", nil)
	listRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(listRec, listReq)
	if listRec.Code != http.StatusOK {
		t.Fatalf("list status: %d body=%s", listRec.Code, listRec.Body.String())
	}
	var list SessionListResponse
	if err := json.Unmarshal(listRec.Body.Bytes(), &list); err != nil {
		t.Fatalf("decode list: %v", err)
	}
	if len(list.Items) != 1 || list.Items[0].SessionID != "1001:default" {
		t.Fatalf("unexpected session list: %+v", list.Items)
	}

	showReq := httptest.NewRequest(http.MethodGet, "/api/sessions/1001:default", nil)
	showRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(showRec, showReq)
	if showRec.Code != http.StatusOK {
		t.Fatalf("show status: %d body=%s", showRec.Code, showRec.Body.String())
	}
	var out SessionStateResponse
	if err := json.Unmarshal(showRec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode session: %v", err)
	}
	if out.Session.SessionID != "1001:default" || out.Session.RuntimeSummary.Runtime.Model != "glm-5-turbo" {
		t.Fatalf("unexpected session state: %+v", out.Session)
	}

	controlReq := httptest.NewRequest(http.MethodGet, "/api/control/1001:default", nil)
	controlRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(controlRec, controlReq)
	if controlRec.Code != http.StatusOK {
		t.Fatalf("control status: %d body=%s", controlRec.Code, controlRec.Body.String())
	}
	var control ControlStateResponse
	if err := json.Unmarshal(controlRec.Body.Bytes(), &control); err != nil {
		t.Fatalf("decode control: %v", err)
	}
	if control.Control.Session.SessionID != "1001:default" {
		t.Fatalf("unexpected control state: %+v", control.Control)
	}
}

func TestServerListsRawOnlySessionsFromLogs(t *testing.T) {
	logDir := t.TempDir()
	rawDir := filepath.Join(logDir, "1001-6565")
	if err := os.MkdirAll(rawDir, 0o755); err != nil {
		t.Fatalf("mkdir raw dir: %v", err)
	}
	entry := rawSessionLogEntry{
		Timestamp: time.Now().UTC(),
		Kind:      "provider_turn",
		ChatID:    1001,
		SessionID: "1001:6565",
		Query:     "continue",
		Request: provider.PromptRequest{
			Messages: []provider.Message{{Role: "user", Content: "continue"}},
		},
		Response: provider.PromptResponse{Text: "ok"},
	}
	body, err := json.Marshal(entry)
	if err != nil {
		t.Fatalf("marshal entry: %v", err)
	}
	if err := os.WriteFile(filepath.Join(rawDir, "session.jsonl"), append(body, '\n'), 0o644); err != nil {
		t.Fatalf("write session log: %v", err)
	}

	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).
		WithRawSessionLogDir(logDir)

	listReq := httptest.NewRequest(http.MethodGet, "/api/sessions?chat_id=1001", nil)
	listRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(listRec, listReq)
	if listRec.Code != http.StatusOK {
		t.Fatalf("list status: %d body=%s", listRec.Code, listRec.Body.String())
	}
	var list SessionListResponse
	if err := json.Unmarshal(listRec.Body.Bytes(), &list); err != nil {
		t.Fatalf("decode list: %v", err)
	}
	if len(list.Items) != 1 || list.Items[0].SessionID != "1001:6565" {
		t.Fatalf("unexpected raw session list: %+v", list.Items)
	}
}

func TestServerLoadsRawConversationFromLogs(t *testing.T) {
	logDir := t.TempDir()
	rawDir := filepath.Join(logDir, "1001-6565")
	if err := os.MkdirAll(rawDir, 0o755); err != nil {
		t.Fatalf("mkdir raw dir: %v", err)
	}
	entry := rawSessionLogEntry{
		Timestamp:           time.Now().UTC(),
		Kind:                "provider_turn",
		ChatID:              1001,
		SessionID:           "1001:6565",
		Query:               "continue",
		SystemPrompt:        "be terse",
		IncludeSystemPrompt: true,
		Request: provider.PromptRequest{
			Messages: []provider.Message{
				{Role: "system", Content: "be terse"},
				{Role: "user", Content: "continue"},
			},
		},
		Response: provider.PromptResponse{Text: "ok"},
	}
	body, err := json.Marshal(entry)
	if err != nil {
		t.Fatalf("marshal entry: %v", err)
	}
	if err := os.WriteFile(filepath.Join(rawDir, "session.jsonl"), append(body, '\n'), 0o644); err != nil {
		t.Fatalf("write session log: %v", err)
	}

	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).
		WithRawSessionLogDir(logDir)

	req := httptest.NewRequest(http.MethodGet, "/api/debug/raw-conversations/1001:6565", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("raw conversation status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out DebugRawConversationResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode raw conversation: %v", err)
	}
	if out.SessionID != "1001:6565" || len(out.Turns) != 1 {
		t.Fatalf("unexpected raw conversation payload: %+v", out)
	}
	if len(out.Messages) != 2 || out.Messages[0].Role != "user" || out.Messages[1].Role != "assistant" {
		t.Fatalf("unexpected hydrated messages: %+v", out.Messages)
	}
}

func TestServerExecutesControlAction(t *testing.T) {
	store := &apiTestRunStore{
		run: runtime.RunRecord{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Query:     "hello",
			Status:    runtime.StatusRunning,
			StartedAt: time.Now().UTC(),
		},
		ok: true,
	}
	registry := runtime.NewActiveRegistry()
	registry.TryStart(runtime.ActiveRun{RunID: "run-1", ChatID: 1001, SessionID: "1001:default", Query: "hello", StartedAt: time.Now().UTC()})
	rt := runtime.NewAPI(store, registry, approvals.New(approvals.TestDeps()))
	server := NewServer(rt, nil, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, runtime.MemoryPolicy{Profile: "conservative"}, runtime.ActionPolicy{})

	req := httptest.NewRequest(http.MethodPost, "/api/control/1001:default/actions", bytes.NewBufferString(`{"action":"run.cancel","chat_id":1001}`))
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out ControlActionResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if out.Result.Message != "Отмена запрошена" || !out.Result.Control.Session.LatestRun.CancelRequested {
		t.Fatalf("unexpected control action result: %+v", out.Result)
	}
}

func TestServerExecutesSessionAction(t *testing.T) {
	server := NewServer(nil, nil, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{}).WithSessionActions(apiTestSessionActions{})
	req := httptest.NewRequest(http.MethodPost, "/api/session-actions", bytes.NewBufferString(`{"chat_id":1001,"action":"session.stats"}`))
	rec := httptest.NewRecorder()

	server.Handler().ServeHTTP(rec, req)

	if rec.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out SessionActionResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if out.Result.ActiveSession != "deploy" || out.Result.MessageCount != 2 {
		t.Fatalf("unexpected session action result: %+v", out.Result)
	}
}

func TestServerSearchesAndReadsMemory(t *testing.T) {
	mem := memoryTestStore{
		items: []memory.RecallItem{{DocKey: "continuity:1", Kind: "continuity", Title: "Test", Body: "remembered", Score: 0.9}},
		doc:   memory.Document{DocKey: "continuity:1", Kind: "continuity", Title: "Test", Body: "remembered"},
	}
	server := NewServer(nil, mem, nil, nil, nil, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})

	searchReq := httptest.NewRequest(http.MethodGet, "/api/memory/search?query=test&session_id=1001:default", nil)
	searchRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(searchRec, searchReq)
	if searchRec.Code != http.StatusOK {
		t.Fatalf("search status: %d body=%s", searchRec.Code, searchRec.Body.String())
	}
	readReq := httptest.NewRequest(http.MethodGet, "/api/memory/continuity:1", nil)
	readRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(readRec, readReq)
	if readRec.Code != http.StatusOK {
		t.Fatalf("read status: %d body=%s", readRec.Code, readRec.Body.String())
	}
}

type memoryOverrideStore struct {
	apiTestRunStore
	overrides map[string]runtime.SessionOverrides
}

type memoryTestStore struct {
	items []memory.RecallItem
	doc   memory.Document
}

func (m memoryTestStore) UpsertDocument(memory.Document) error { return nil }
func (m memoryTestStore) Search(memory.RecallQuery) ([]memory.RecallItem, error) {
	return m.items, nil
}
func (m memoryTestStore) Get(docKey string) (memory.Document, bool, error) {
	if m.doc.DocKey == docKey {
		return m.doc, true, nil
	}
	return memory.Document{}, false, nil
}

func NewMemoryOverrideStore() *memoryOverrideStore {
	return &memoryOverrideStore{overrides: map[string]runtime.SessionOverrides{}}
}

func (s *memoryOverrideStore) SaveSessionOverrides(overrides runtime.SessionOverrides) error {
	s.overrides[overrides.SessionID] = overrides
	return nil
}

func (s *memoryOverrideStore) SessionOverrides(sessionID string) (runtime.SessionOverrides, bool, error) {
	out, ok := s.overrides[sessionID]
	return out, ok, nil
}

func (s *memoryOverrideStore) ClearSessionOverrides(sessionID string) error {
	delete(s.overrides, sessionID)
	return nil
}

func TestServerStartsShowsAndCancelsJobs(t *testing.T) {
	jobs := apiTestJobs{
		job: runtime.JobView{
			JobID:     "job-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Command:   "echo",
			Args:      []string{"hello"},
			Status:    runtime.JobRunning,
			StartedAt: time.Now().UTC(),
			Active:    true,
		},
		logs: []runtime.JobLogChunk{{ID: 1, JobID: "job-1", Stream: "stdout", Content: "hello", CreatedAt: time.Now().UTC()}},
	}
	server := NewServer(nil, nil, nil, nil, jobs, nil, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})

	startReq := httptest.NewRequest(http.MethodPost, "/api/jobs", bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:default","command":"echo","args":["hello"]}`))
	startRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(startRec, startReq)
	if startRec.Code != http.StatusAccepted {
		t.Fatalf("start job status: %d body=%s", startRec.Code, startRec.Body.String())
	}

	showReq := httptest.NewRequest(http.MethodGet, "/api/jobs/job-1", nil)
	showRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(showRec, showReq)
	if showRec.Code != http.StatusOK {
		t.Fatalf("show job status: %d body=%s", showRec.Code, showRec.Body.String())
	}

	listReq := httptest.NewRequest(http.MethodGet, "/api/jobs", nil)
	listRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(listRec, listReq)
	if listRec.Code != http.StatusOK {
		t.Fatalf("list jobs status: %d body=%s", listRec.Code, listRec.Body.String())
	}

	logsReq := httptest.NewRequest(http.MethodGet, "/api/jobs/job-1/logs", nil)
	logsRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(logsRec, logsReq)
	if logsRec.Code != http.StatusOK {
		t.Fatalf("job logs status: %d body=%s", logsRec.Code, logsRec.Body.String())
	}

	cancelReq := httptest.NewRequest(http.MethodPost, "/api/jobs/job-1/cancel", nil)
	cancelRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(cancelRec, cancelReq)
	if cancelRec.Code != http.StatusOK {
		t.Fatalf("cancel job status: %d body=%s", cancelRec.Code, cancelRec.Body.String())
	}
}

func TestServerShowsWorkerHandoff(t *testing.T) {
	workers := apiTestWorkers{
		worker: runtime.WorkerView{
			WorkerID:        "worker-1",
			ParentSessionID: "1001:default",
			WorkerSessionID: "worker-1",
			Status:          runtime.WorkerIdle,
		},
		handoff: runtime.WorkerHandoff{
			WorkerID:  "worker-1",
			LastRunID: "worker-1-run-1",
			Summary:   "worker reply: inspect deployment",
			Artifacts: []string{"artifact://worker-output-1"},
			CreatedAt: time.Now().UTC(),
			UpdatedAt: time.Now().UTC(),
		},
	}
	server := NewServer(nil, nil, nil, nil, nil, workers, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})
	req := httptest.NewRequest(http.MethodGet, "/api/workers/worker-1/handoff", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("handoff status: %d body=%s", rec.Code, rec.Body.String())
	}
	var out WorkerHandoffResponse
	if err := json.Unmarshal(rec.Body.Bytes(), &out); err != nil {
		t.Fatalf("decode handoff: %v", err)
	}
	if out.Handoff.WorkerID != "worker-1" || len(out.Handoff.Artifacts) != 1 {
		t.Fatalf("unexpected handoff response: %+v", out.Handoff)
	}
}

func TestServerSpawnsMessagesWaitsAndClosesWorkers(t *testing.T) {
	workers := apiTestWorkers{
		worker: runtime.WorkerView{
			WorkerID:        "worker-1",
			ParentChatID:    1001,
			ParentSessionID: "1001:default",
			WorkerChatID:    -1,
			WorkerSessionID: "worker-1",
			Status:          runtime.WorkerIdle,
			CreatedAt:       time.Now().UTC(),
			UpdatedAt:       time.Now().UTC(),
		},
		wait: runtime.WorkerWaitResult{
			Worker: runtime.WorkerView{
				WorkerID:        "worker-1",
				ParentChatID:    1001,
				ParentSessionID: "1001:default",
				WorkerChatID:    -1,
				WorkerSessionID: "worker-1",
				Status:          runtime.WorkerIdle,
				CreatedAt:       time.Now().UTC(),
				UpdatedAt:       time.Now().UTC(),
			},
			Messages:       []runtime.WorkerMessage{{Cursor: 1, Role: "assistant", Content: "done"}},
			Events:         []runtime.RuntimeEvent{{ID: 1, EntityType: "worker", EntityID: "worker-1", Kind: "worker.spawned", CreatedAt: time.Now().UTC()}},
			NextCursor:     1,
			NextEventAfter: 1,
		},
	}
	server := NewServer(nil, nil, nil, nil, nil, workers, provider.RequestConfig{}, runtime.MemoryPolicy{}, runtime.ActionPolicy{})

	startReq := httptest.NewRequest(http.MethodPost, "/api/workers", bytes.NewBufferString(`{"chat_id":1001,"session_id":"1001:default","prompt":"hello"}`))
	startRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(startRec, startReq)
	if startRec.Code != http.StatusAccepted {
		t.Fatalf("start worker status: %d body=%s", startRec.Code, startRec.Body.String())
	}

	showReq := httptest.NewRequest(http.MethodGet, "/api/workers/worker-1", nil)
	showRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(showRec, showReq)
	if showRec.Code != http.StatusOK {
		t.Fatalf("show worker status: %d body=%s", showRec.Code, showRec.Body.String())
	}

	listReq := httptest.NewRequest(http.MethodGet, "/api/workers", nil)
	listRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(listRec, listReq)
	if listRec.Code != http.StatusOK {
		t.Fatalf("list workers status: %d body=%s", listRec.Code, listRec.Body.String())
	}

	msgReq := httptest.NewRequest(http.MethodPost, "/api/workers/worker-1/messages", bytes.NewBufferString(`{"content":"do it"}`))
	msgRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(msgRec, msgReq)
	if msgRec.Code != http.StatusAccepted {
		t.Fatalf("worker message status: %d body=%s", msgRec.Code, msgRec.Body.String())
	}

	waitReq := httptest.NewRequest(http.MethodGet, "/api/workers/worker-1/wait?after_cursor=0&after_event_id=0", nil)
	waitRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(waitRec, waitReq)
	if waitRec.Code != http.StatusOK {
		t.Fatalf("worker wait status: %d body=%s", waitRec.Code, waitRec.Body.String())
	}

	closeReq := httptest.NewRequest(http.MethodPost, "/api/workers/worker-1/close", nil)
	closeRec := httptest.NewRecorder()
	server.Handler().ServeHTTP(closeRec, closeReq)
	if closeRec.Code != http.StatusOK {
		t.Fatalf("worker close status: %d body=%s", closeRec.Code, closeRec.Body.String())
	}
}
