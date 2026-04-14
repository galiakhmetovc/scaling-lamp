package integration

import (
	"context"
	"os"
	"path/filepath"
	"testing"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/artifacts"
	"teamd/internal/compaction"
	"teamd/internal/config"
	"teamd/internal/coordinator"
	"teamd/internal/events"
	mcptools "teamd/internal/mcp/tools"
	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
	"teamd/internal/skills"
	"teamd/internal/transport/telegram"
	"teamd/internal/worker"
)

func TestCoordinatorBootsWithEmptyConfig(t *testing.T) {
	cfg := config.TestConfig()
	svc, err := coordinator.New(cfg)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if svc == nil {
		t.Fatal("expected coordinator service")
	}
}

func TestCoordinatorRoutesInboundEvent(t *testing.T) {
	cfg := config.TestConfig()
	svc, err := coordinator.New(cfg)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	evt := events.InboundEvent{Source: "test", SessionID: "s1", Text: "hello"}

	result, err := svc.HandleInbound(context.Background(), evt)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.SessionID != "s1" {
		t.Fatalf("expected session s1, got %q", result.SessionID)
	}
}

func TestWorkerLifecycleTransitions(t *testing.T) {
	runtime := worker.NewRuntime(worker.TestDeps())
	id, err := runtime.Start(context.Background(), worker.Spec{Role: "supervisor"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	state := runtime.State(id)
	if state != worker.StateRunning {
		t.Fatalf("expected running state, got %v", state)
	}
}

func TestCompactionProducesStructuredCheckpoint(t *testing.T) {
	svc := compaction.New(compaction.TestDeps())
	out, err := svc.Compact(context.Background(), compaction.Input{
		SessionID:  "s1",
		Transcript: []string{"user: ping", "agent: pong"},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if out.WhatMattersNow == "" {
		t.Fatal("expected structured summary field")
	}
}

func TestWorkerCallsProvider(t *testing.T) {
	runtime := worker.NewRuntime(worker.TestDepsWithProvider(provider.FakeProvider{}))
	reply, err := runtime.RunPrompt(context.Background(), worker.PromptInput{
		WorkerID: "w1",
		Messages: []provider.Message{{Role: "user", Content: "hello"}},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if reply.Text == "" {
		t.Fatal("expected provider reply")
	}
}

func TestTelegramAdapterNormalizesUpdate(t *testing.T) {
	adapter := telegram.New(telegram.TestDeps())
	evt, err := adapter.Normalize(telegram.TestMessageUpdate("hello"))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if evt.Text != "hello" {
		t.Fatalf("expected hello, got %q", evt.Text)
	}
}

func TestTelegramApprovalCallbackFlow(t *testing.T) {
	svc := approvals.New(approvals.TestDeps())
	record, err := svc.Create(approvals.Request{
		WorkerID:  "w1",
		SessionID: "s1",
		Payload:   "approve deploy",
	})
	if err != nil {
		t.Fatalf("unexpected error creating approval: %v", err)
	}

	result, err := svc.HandleCallback(approvals.Callback{
		ApprovalID: record.ID,
		Action:     approvals.ActionApprove,
		UpdateID:   "cb-1",
	})
	if err != nil {
		t.Fatalf("unexpected error handling callback: %v", err)
	}
	if result.Status != approvals.StatusApproved {
		t.Fatalf("expected approved status, got %v", result.Status)
	}
}

func TestWorkerHydratesSkillsAndMCP(t *testing.T) {
	runtime := worker.NewRuntime(worker.TestDepsWithCapabilities(
		skills.StaticLoader{
			Bundles: []skills.Bundle{{Name: "research", Prompt: "be useful"}},
		},
		mcptools.NewRuntimeWithLocalTools(t.TempDir()),
	))
	id, err := runtime.Start(context.Background(), worker.Spec{Role: "researcher"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	snap := runtime.Snapshot(id)
	if len(snap.Skills) == 0 || len(snap.MCPServers) == 0 {
		t.Fatal("expected hydrated skills and MCP context")
	}
}

func TestWorkerCallsFilesystemTool(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "note.txt")
	if err := os.WriteFile(path, []byte("hello"), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	runtime := worker.NewRuntime(worker.TestDepsWithCapabilities(
		skills.StaticLoader{},
		mcptools.NewRuntimeWithLocalTools(dir),
	))

	out, err := runtime.CallTool(context.Background(), "filesystem.read_file", map[string]any{
		"path": path,
	})
	if err != nil {
		t.Fatalf("call tool: %v", err)
	}
	if out.Content != "hello" {
		t.Fatalf("unexpected tool output: %q", out.Content)
	}
}

func TestSupervisorDelegatesToSpecialistWorkers(t *testing.T) {
	cfg := config.TestConfig()
	svc, err := coordinator.New(cfg)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	result, err := svc.RunGoal(context.Background(), "research and summarize")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.WorkerCount < 2 {
		t.Fatalf("expected supervisor plus specialists, got %d", result.WorkerCount)
	}
}

func TestCompactionLinksArtifactReferences(t *testing.T) {
	svc := compaction.New(compaction.TestDeps())
	out, err := svc.Compact(context.Background(), compaction.Input{
		SessionID:    "s1",
		ArtifactRefs: []string{"artifact://report-1"},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(out.SourceArtifacts) == 0 {
		t.Fatal("expected artifact references in checkpoint")
	}
}

func TestArtifactStorePersistsPayload(t *testing.T) {
	store, err := artifacts.NewFilesystemStore(t.TempDir())
	if err != nil {
		t.Fatalf("new filesystem store: %v", err)
	}
	ref, err := store.Save("run", "run-1", "report.txt", []byte("payload"))
	if err != nil {
		t.Fatalf("unexpected error saving artifact: %v", err)
	}
	if ref == "" {
		t.Fatal("expected artifact reference")
	}
	item, ok, err := store.Get(ref)
	if err != nil {
		t.Fatalf("get artifact: %v", err)
	}
	if !ok || string(item.Payload) != "payload" {
		t.Fatalf("unexpected artifact payload: ok=%v item=%+v", ok, item)
	}
}

func TestArtifactStorePersistsAcrossFilesystemStoreReload(t *testing.T) {
	root := t.TempDir()
	store, err := artifacts.NewFilesystemStore(root)
	if err != nil {
		t.Fatalf("new filesystem store: %v", err)
	}
	ref, err := store.Save("run", "run-1", "report.txt", []byte("payload"))
	if err != nil {
		t.Fatalf("unexpected error saving artifact: %v", err)
	}
	if ref == "" {
		t.Fatal("expected artifact reference")
	}
	reloaded, err := artifacts.NewFilesystemStore(root)
	if err != nil {
		t.Fatalf("reload filesystem store: %v", err)
	}
	item, ok, err := reloaded.Get(ref)
	if err != nil {
		t.Fatalf("get reloaded artifact: %v", err)
	}
	if !ok || string(item.Payload) != "payload" {
		t.Fatalf("unexpected reloaded artifact: ok=%v item=%+v", ok, item)
	}
}

func TestControlStateSurfacesWorkerApprovals(t *testing.T) {
	approvalSvc := approvals.New(approvals.TestDeps())
	record, err := approvalSvc.Create(approvals.Request{
		WorkerID:   "shell.exec",
		SessionID:  "-1:worker-1-session",
		Payload:    "{}",
		TargetType: "run",
		TargetID:   "worker-run-1",
	})
	if err != nil {
		t.Fatalf("create approval: %v", err)
	}
	apiStore := &runtimexTestStore{
		run: runtimex.RunRecord{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Query:     "delegate",
			Status:    runtimex.StatusRunning,
			StartedAt: integrationNow(),
		},
		ok: true,
		workers: []runtimex.WorkerRecord{{
			WorkerID:        "worker-1",
			ParentChatID:    1001,
			ParentSessionID: "1001:default",
			WorkerChatID:    -1,
			WorkerSessionID: "worker-1-session",
			Status:          runtimex.WorkerWaitingApproval,
			LastRunID:       "worker-run-1",
			CreatedAt:       integrationNow(),
			UpdatedAt:       integrationNow(),
		}},
	}
	api := runtimex.NewAPI(apiStore, runtimex.NewActiveRegistry(), approvalSvc)
	control, err := api.ControlState("1001:default", 1001, provider.RequestConfig{Model: "glm-5-turbo"}, runtimex.MemoryPolicy{Profile: "conservative"}, runtimex.ActionPolicy{})
	if err != nil {
		t.Fatalf("control state: %v", err)
	}
	if len(control.Workers) != 1 || control.Workers[0].WorkerID != "worker-1" {
		t.Fatalf("unexpected workers: %+v", control.Workers)
	}
	if len(control.Approvals) != 1 || control.Approvals[0].ID != record.ID {
		t.Fatalf("unexpected approvals: %+v", control.Approvals)
	}
}

type runtimexTestStore struct {
	run     runtimex.RunRecord
	ok      bool
	workers []runtimex.WorkerRecord
}

func integrationNow() time.Time {
	return time.Now().UTC()
}

func (s *runtimexTestStore) SaveRun(run runtimex.RunRecord) error { s.run = run; s.ok = true; return nil }
func (s *runtimexTestStore) MarkCancelRequested(runID string) error { return nil }
func (s *runtimexTestStore) Run(runID string) (runtimex.RunRecord, bool, error) {
	if s.ok && s.run.RunID == runID {
		return s.run, true, nil
	}
	return runtimex.RunRecord{}, false, nil
}
func (s *runtimexTestStore) ListRuns(query runtimex.RunQuery) ([]runtimex.RunRecord, error) {
	if !s.ok {
		return nil, nil
	}
	return []runtimex.RunRecord{s.run}, nil
}
func (s *runtimexTestStore) SaveEvent(runtimex.RuntimeEvent) error { return nil }
func (s *runtimexTestStore) ListEvents(query runtimex.EventQuery) ([]runtimex.RuntimeEvent, error) {
	return nil, nil
}
func (s *runtimexTestStore) RecoverInterruptedRuns(reason string) (int, error) { return 0, nil }
func (s *runtimexTestStore) ListSessions(query runtimex.SessionQuery) ([]runtimex.SessionRecord, error) {
	if !s.ok {
		return nil, nil
	}
	return []runtimex.SessionRecord{{
		SessionID:      s.run.SessionID,
		LastActivityAt: s.run.StartedAt,
		HasOverrides:   false,
	}}, nil
}
func (s *runtimexTestStore) SavePlan(plan runtimex.PlanRecord) error { return nil }
func (s *runtimexTestStore) Plan(planID string) (runtimex.PlanRecord, bool, error) {
	return runtimex.PlanRecord{}, false, nil
}
func (s *runtimexTestStore) ListPlans(query runtimex.PlanQuery) ([]runtimex.PlanRecord, error) {
	return nil, nil
}
func (s *runtimexTestStore) SaveWorker(record runtimex.WorkerRecord) error { return nil }
func (s *runtimexTestStore) RecoverInterruptedWorkers(reason string) (int, error) { return 0, nil }
func (s *runtimexTestStore) Worker(workerID string) (runtimex.WorkerRecord, bool, error) {
	for _, item := range s.workers {
		if item.WorkerID == workerID {
			return item, true, nil
		}
	}
	return runtimex.WorkerRecord{}, false, nil
}
func (s *runtimexTestStore) ListWorkers(query runtimex.WorkerQuery) ([]runtimex.WorkerRecord, error) {
	return append([]runtimex.WorkerRecord(nil), s.workers...), nil
}
func (s *runtimexTestStore) SaveWorkerHandoff(h runtimex.WorkerHandoff) error { return nil }
func (s *runtimexTestStore) WorkerHandoff(workerID string) (runtimex.WorkerHandoff, bool, error) {
	return runtimex.WorkerHandoff{}, false, nil
}
func (s *runtimexTestStore) SaveJob(job runtimex.JobRecord) error { return nil }
func (s *runtimexTestStore) Job(jobID string) (runtimex.JobRecord, bool, error) {
	return runtimex.JobRecord{}, false, nil
}
func (s *runtimexTestStore) ListJobs(limit int) ([]runtimex.JobRecord, error) { return nil, nil }
func (s *runtimexTestStore) MarkJobCancelRequested(jobID string) error         { return nil }
func (s *runtimexTestStore) SaveJobLog(chunk runtimex.JobLogChunk) error       { return nil }
func (s *runtimexTestStore) JobLogs(query runtimex.JobLogQuery) ([]runtimex.JobLogChunk, error) {
	return nil, nil
}
func (s *runtimexTestStore) RecoverInterruptedJobs(reason string) (int, error) { return 0, nil }
