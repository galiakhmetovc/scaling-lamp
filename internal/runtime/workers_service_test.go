package runtime

import (
	"context"
	"encoding/json"
	"sync"
	"testing"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/provider"
)

type workerStoreStub struct {
	mu      sync.Mutex
	workers map[string]WorkerRecord
	handoffs map[string]WorkerHandoff
	events  []RuntimeEvent
}

type workerApprovalLookupStub struct {
	items []ApprovalView
}

func (s workerApprovalLookupStub) PendingApprovals(sessionID string) []ApprovalView {
	out := make([]ApprovalView, 0, len(s.items))
	for _, item := range s.items {
		if item.SessionID == sessionID {
			out = append(out, item)
		}
	}
	return out
}

func newWorkerStoreStub() *workerStoreStub {
	return &workerStoreStub{workers: make(map[string]WorkerRecord), handoffs: make(map[string]WorkerHandoff)}
}

func (s *workerStoreStub) SaveWorker(record WorkerRecord) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.workers[record.WorkerID] = record
	return nil
}

func (s *workerStoreStub) Worker(workerID string) (WorkerRecord, bool, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	record, ok := s.workers[workerID]
	return record, ok, nil
}

func (s *workerStoreStub) ListWorkers(query WorkerQuery) ([]WorkerRecord, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := []WorkerRecord{}
	for _, record := range s.workers {
		if query.HasParentChatID && record.ParentChatID != query.ParentChatID {
			continue
		}
		out = append(out, record)
	}
	return out, nil
}

func (s *workerStoreStub) SaveEvent(event RuntimeEvent) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	event.ID = int64(len(s.events) + 1)
	s.events = append(s.events, event)
	return nil
}

func (s *workerStoreStub) SaveWorkerHandoff(handoff WorkerHandoff) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.handoffs[handoff.WorkerID] = handoff
	return nil
}

func (s *workerStoreStub) WorkerHandoff(workerID string) (WorkerHandoff, bool, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	handoff, ok := s.handoffs[workerID]
	return handoff, ok, nil
}

func (s *workerStoreStub) ListEvents(query EventQuery) ([]RuntimeEvent, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := []RuntimeEvent{}
	for _, event := range s.events {
		if query.EntityType != "" && event.EntityType != query.EntityType {
			continue
		}
		if query.EntityID != "" && event.EntityID != query.EntityID {
			continue
		}
		if query.AfterID > 0 && event.ID <= query.AfterID {
			continue
		}
		out = append(out, event)
	}
	return out, nil
}

func (s *workerStoreStub) RecoverInterruptedWorkers(reason string) (int, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	count := 0
	for id, record := range s.workers {
		if record.Process.State == WorkerProcessRunning || record.Process.State == WorkerProcessStarting {
			record.Process.State = WorkerProcessFailed
			record.Process.ExitReason = reason
			record.UpdatedAt = time.Now().UTC()
			s.workers[id] = record
			count++
		}
	}
	return count, nil
}

type workerTranscriptStub struct {
	mu       sync.Mutex
	active   map[int64]string
	sessions map[int64]map[string][]provider.Message
}

func newWorkerTranscriptStub() *workerTranscriptStub {
	return &workerTranscriptStub{
		active:   make(map[int64]string),
		sessions: make(map[int64]map[string][]provider.Message),
	}
}

func (s *workerTranscriptStub) ensure(chatID int64) {
	if _, ok := s.sessions[chatID]; !ok {
		s.sessions[chatID] = map[string][]provider.Message{"default": nil}
	}
	if _, ok := s.active[chatID]; !ok {
		s.active[chatID] = "default"
	}
}

func (s *workerTranscriptStub) CreateSession(chatID int64, session string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.ensure(chatID)
	if _, ok := s.sessions[chatID][session]; !ok {
		s.sessions[chatID][session] = nil
	}
	return nil
}

func (s *workerTranscriptStub) UseSession(chatID int64, session string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.ensure(chatID)
	s.active[chatID] = session
	return nil
}

func (s *workerTranscriptStub) Messages(chatID int64) ([]provider.Message, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.ensure(chatID)
	src := s.sessions[chatID][s.active[chatID]]
	out := make([]provider.Message, len(src))
	copy(out, src)
	return out, nil
}

func (s *workerTranscriptStub) append(chatID int64, msg provider.Message) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.ensure(chatID)
	session := s.active[chatID]
	s.sessions[chatID][session] = append(s.sessions[chatID][session], msg)
}

type workerRunControlStub struct {
	mu          sync.Mutex
	transcripts *workerTranscriptStub
	runs        map[string]RunView
}

func newWorkerRunControlStub(transcripts *workerTranscriptStub) *workerRunControlStub {
	return &workerRunControlStub{transcripts: transcripts, runs: make(map[string]RunView)}
}

func (s *workerRunControlStub) StartDetached(_ context.Context, req StartRunRequest) (RunView, bool, error) {
	s.transcripts.append(req.ChatID, provider.Message{Role: "user", Content: req.Query})
	run := RunView{
		RunID:        req.RunID,
		ChatID:       req.ChatID,
		SessionID:    req.SessionID,
		Query:        req.Query,
		ArtifactRefs: []string{"artifact://worker-output-1"},
		Status:       StatusCompleted,
		StartedAt:    time.Now().UTC(),
	}
	ended := time.Now().UTC()
	run.EndedAt = &ended
	s.mu.Lock()
	s.runs[req.RunID] = run
	s.mu.Unlock()
	s.transcripts.append(req.ChatID, provider.Message{Role: "assistant", Content: "worker reply: " + req.Query})
	return run, true, nil
}

func (s *workerRunControlStub) RunView(runID string) (RunView, bool, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	run, ok := s.runs[runID]
	return run, ok, nil
}

func (s *workerRunControlStub) CancelRunByID(runID string) (bool, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	run, ok := s.runs[runID]
	if ok {
		run.Status = StatusCancelled
		s.runs[runID] = run
	}
	return ok, nil
}

type contextCheckingRunControlStub struct{}

func (contextCheckingRunControlStub) StartDetached(ctx context.Context, req StartRunRequest) (RunView, bool, error) {
	if ctx.Err() != nil {
		return RunView{}, false, ctx.Err()
	}
	return RunView{RunID: req.RunID, ChatID: req.ChatID, SessionID: req.SessionID, Query: req.Query, Status: StatusRunning, StartedAt: time.Now().UTC()}, true, nil
}

func (contextCheckingRunControlStub) RunView(runID string) (RunView, bool, error) {
	return RunView{RunID: runID, Status: StatusRunning, StartedAt: time.Now().UTC()}, true, nil
}

func (contextCheckingRunControlStub) CancelRunByID(runID string) (bool, error) { return true, nil }

type workerSupervisorStub struct {
	started []string
	stopped []string
	runtime map[string]WorkerProcessRuntime
}

func (s *workerSupervisorStub) Start(_ context.Context, record WorkerRecord) (WorkerProcessRuntime, error) {
	s.started = append(s.started, record.WorkerID)
	now := time.Now().UTC()
	state := WorkerProcessRuntime{
		PID:             4242,
		State:           WorkerProcessRunning,
		StartedAt:       &now,
		LastHeartbeatAt: &now,
	}
	if s.runtime == nil {
		s.runtime = map[string]WorkerProcessRuntime{}
	}
	s.runtime[record.WorkerID] = state
	return state, nil
}

func (s *workerSupervisorStub) Stop(_ context.Context, workerID string, _ WorkerRecord) error {
	s.stopped = append(s.stopped, workerID)
	if s.runtime == nil {
		s.runtime = map[string]WorkerProcessRuntime{}
	}
	state := s.runtime[workerID]
	now := time.Now().UTC()
	state.State = WorkerProcessStopped
	state.ExitedAt = &now
	s.runtime[workerID] = state
	return nil
}

func (s *workerSupervisorStub) Runtime(workerID string) (WorkerProcessRuntime, bool) {
	if s.runtime == nil {
		return WorkerProcessRuntime{}, false
	}
	state, ok := s.runtime[workerID]
	return state, ok
}

func TestWorkersServiceSpawnMessageWaitAndClose(t *testing.T) {
	store := newWorkerStoreStub()
	transcripts := newWorkerTranscriptStub()
	runs := newWorkerRunControlStub(transcripts)
	supervisor := &workerSupervisorStub{}
	service := NewWorkersService(store, transcripts, runs, nil, supervisor)

	worker, err := service.Spawn(context.Background(), WorkerSpawnRequest{
		ParentChatID:    1001,
		ParentSessionID: "1001:default",
	})
	if err != nil {
		t.Fatalf("spawn: %v", err)
	}
	if worker.Status != WorkerIdle {
		t.Fatalf("unexpected initial worker status: %s", worker.Status)
	}
	if worker.WorkerChatID >= 0 {
		t.Fatalf("expected negative synthetic worker chat id, got %d", worker.WorkerChatID)
	}
	if worker.Process.PID != 4242 || worker.Process.State != WorkerProcessRunning {
		t.Fatalf("expected running worker process, got %+v", worker.Process)
	}
	if len(supervisor.started) != 1 || supervisor.started[0] != worker.WorkerID {
		t.Fatalf("expected supervisor start, got %+v", supervisor.started)
	}

	worker, err = service.Message(context.Background(), worker.WorkerID, WorkerMessageRequest{Content: "inspect deployment"})
	if err != nil {
		t.Fatalf("message: %v", err)
	}
	if worker.LastRunID == "" {
		t.Fatalf("expected worker run id")
	}

	waited, ok, err := service.Wait(worker.WorkerID, 0, 0, 20)
	if err != nil || !ok {
		t.Fatalf("wait: ok=%v err=%v", ok, err)
	}
	if waited.Worker.Status != WorkerIdle {
		t.Fatalf("unexpected worker status after wait: %s", waited.Worker.Status)
	}
	if len(waited.Worker.ArtifactRefs) != 1 || waited.Worker.ArtifactRefs[0] != "artifact://worker-output-1" {
		t.Fatalf("expected worker artifact refs, got %+v", waited.Worker.ArtifactRefs)
	}
	if waited.Handoff == nil || waited.Handoff.Summary != "worker reply: inspect deployment" {
		t.Fatalf("expected worker handoff, got %+v", waited.Handoff)
	}
	if len(waited.Handoff.Artifacts) != 1 || waited.Handoff.Artifacts[0] != "artifact://worker-output-1" {
		t.Fatalf("expected handoff artifacts, got %+v", waited.Handoff)
	}
	if len(waited.Messages) != 2 {
		t.Fatalf("expected 2 worker messages, got %d", len(waited.Messages))
	}
	if waited.Messages[0].Role != "user" || waited.Messages[1].Role != "assistant" {
		t.Fatalf("unexpected worker messages: %+v", waited.Messages)
	}
	if len(waited.Events) == 0 {
		t.Fatalf("expected worker events")
	}
	foundHandoffEvent := false
	for _, event := range waited.Events {
		if event.Kind == "worker.handoff_created" {
			foundHandoffEvent = true
		}
	}
	if !foundHandoffEvent {
		t.Fatalf("expected worker.handoff_created event, got %+v", waited.Events)
	}

	closed, ok, err := service.Close(worker.WorkerID)
	if err != nil || !ok {
		t.Fatalf("close: ok=%v err=%v", ok, err)
	}
	if closed.Status != WorkerClosed {
		t.Fatalf("unexpected closed status: %s", closed.Status)
	}
	if len(supervisor.stopped) != 1 || supervisor.stopped[0] != worker.WorkerID {
		t.Fatalf("expected supervisor stop, got %+v", supervisor.stopped)
	}
}

func TestWorkersServiceEmitsWorkerApprovalRequestedEvent(t *testing.T) {
	store := newWorkerStoreStub()
	transcripts := newWorkerTranscriptStub()
	runs := newWorkerRunControlStub(transcripts)
	service := NewWorkersService(store, transcripts, runs, nil, nil)

	worker, err := service.Spawn(context.Background(), WorkerSpawnRequest{
		ParentChatID:    1001,
		ParentSessionID: "1001:default",
	})
	if err != nil {
		t.Fatalf("spawn: %v", err)
	}
	worker, err = service.Message(context.Background(), worker.WorkerID, WorkerMessageRequest{Content: "inspect deployment"})
	if err != nil {
		t.Fatalf("message: %v", err)
	}
	runs.mu.Lock()
	run := runs.runs[worker.LastRunID]
	run.Status = StatusWaitingApproval
	runs.runs[worker.LastRunID] = run
	runs.mu.Unlock()

	service.approvals = workerApprovalLookupStub{items: []ApprovalView{{
		ID:         "approval-1",
		WorkerID:   "shell.exec",
		SessionID:  workerSessionKey(WorkerRecord{WorkerChatID: worker.WorkerChatID, WorkerSessionID: worker.WorkerSessionID}),
		Status:     approvals.StatusPending,
		Reason:     "shell.exec requires approval",
		TargetType: "run",
		TargetID:   worker.LastRunID,
	}}}

	waited, ok, err := service.Wait(worker.WorkerID, 0, 0, 20)
	if err != nil || !ok {
		t.Fatalf("wait: ok=%v err=%v", ok, err)
	}
	if waited.Worker.Status != WorkerWaitingApproval {
		t.Fatalf("expected waiting approval worker status, got %s", waited.Worker.Status)
	}
	found := false
	for _, event := range waited.Events {
		if event.Kind != "worker.approval_requested" {
			continue
		}
		payload := map[string]any{}
		if err := json.Unmarshal(event.Payload, &payload); err != nil {
			t.Fatalf("unmarshal payload: %v", err)
		}
		if payload["approval_id"] != "approval-1" || payload["run_id"] != worker.LastRunID {
			t.Fatalf("unexpected approval payload: %+v", payload)
		}
		found = true
	}
	if !found {
		t.Fatalf("expected worker.approval_requested event, got %+v", waited.Events)
	}
}

func TestWorkersServiceMessageUsesDetachedContext(t *testing.T) {
	store := newWorkerStoreStub()
	transcripts := newWorkerTranscriptStub()
	service := NewWorkersService(store, transcripts, contextCheckingRunControlStub{}, nil, nil)

	worker, err := service.Spawn(context.Background(), WorkerSpawnRequest{
		ParentChatID:    1001,
		ParentSessionID: "1001:default",
	})
	if err != nil {
		t.Fatalf("spawn: %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := service.Message(ctx, worker.WorkerID, WorkerMessageRequest{Content: "still run"}); err != nil {
		t.Fatalf("message should ignore request cancellation for detached worker runs: %v", err)
	}
}

func TestWorkersServicePersistsPolicySnapshot(t *testing.T) {
	store := newWorkerStoreStub()
	transcripts := newWorkerTranscriptStub()
	runs := newWorkerRunControlStub(transcripts)
	service := NewWorkersService(store, transcripts, runs, nil, nil)

	worker, err := service.Spawn(context.Background(), WorkerSpawnRequest{
		ParentChatID:    1001,
		ParentSessionID: "1001:default",
		PolicySnapshot: PolicySnapshot{
			Runtime:      provider.RequestConfig{Model: "glm-5.1"},
			MemoryPolicy: MemoryPolicy{Profile: "standard"},
			ActionPolicy: ActionPolicy{ApprovalRequiredTools: []string{"shell.exec"}},
		},
	})
	if err != nil {
		t.Fatalf("spawn: %v", err)
	}
	if worker.PolicySnapshot.Runtime.Model != "glm-5.1" {
		t.Fatalf("missing worker snapshot in view: %+v", worker.PolicySnapshot)
	}
	record, ok, err := store.Worker(worker.WorkerID)
	if err != nil || !ok {
		t.Fatalf("worker lookup: ok=%v err=%v", ok, err)
	}
	if record.PolicySnapshot.MemoryPolicy.Profile != "standard" {
		t.Fatalf("missing worker snapshot in record: %+v", record.PolicySnapshot)
	}
}
