package runtime

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/provider"
)

type runtimeAPITestStore struct {
	run     RunRecord
	ok      bool
	events  []RuntimeEvent
	plans   map[string]PlanRecord
	workers []WorkerRecord
	handoff map[string]WorkerHandoff
	jobs    []JobRecord
	head    SessionHead
	headOK  bool
}

func (s *runtimeAPITestStore) SaveRun(run RunRecord) error {
	s.run = run
	s.ok = true
	return nil
}

func (s *runtimeAPITestStore) MarkCancelRequested(runID string) error {
	s.run.CancelRequested = true
	return nil
}

func (s *runtimeAPITestStore) Run(runID string) (RunRecord, bool, error) {
	if s.ok && s.run.RunID == runID {
		return s.run, true, nil
	}
	return RunRecord{}, false, nil
}

func (s *runtimeAPITestStore) ListRuns(query RunQuery) ([]RunRecord, error) {
	if s.ok {
		return []RunRecord{s.run}, nil
	}
	return nil, nil
}

func (s *runtimeAPITestStore) ListSessions(query SessionQuery) ([]SessionRecord, error) {
	if s.ok {
		return []SessionRecord{{
			SessionID:      s.run.SessionID,
			LastActivityAt: s.run.StartedAt,
			HasOverrides:   false,
		}}, nil
	}
	return nil, nil
}

func (s *runtimeAPITestStore) SaveEvent(RuntimeEvent) error {
	return nil
}

func (s *runtimeAPITestStore) ListEvents(query EventQuery) ([]RuntimeEvent, error) {
	return append([]RuntimeEvent(nil), s.events...), nil
}

func (s *runtimeAPITestStore) RecoverInterruptedRuns(reason string) (int, error) {
	return 0, nil
}

func (s *runtimeAPITestStore) SaveCheckpoint(Checkpoint) error { return nil }
func (s *runtimeAPITestStore) Checkpoint(chatID int64, sessionID string) (Checkpoint, bool, error) {
	return Checkpoint{}, false, nil
}
func (s *runtimeAPITestStore) SaveContinuity(Continuity) error { return nil }
func (s *runtimeAPITestStore) Continuity(chatID int64, sessionID string) (Continuity, bool, error) {
	return Continuity{}, false, nil
}
func (s *runtimeAPITestStore) SaveSessionHead(head SessionHead) error {
	s.head = head
	s.headOK = true
	return nil
}
func (s *runtimeAPITestStore) SessionHead(chatID int64, sessionID string) (SessionHead, bool, error) {
	if s.headOK && s.head.ChatID == chatID && s.head.SessionID == sessionID {
		return s.head, true, nil
	}
	return SessionHead{}, false, nil
}

func (s *runtimeAPITestStore) SavePlan(plan PlanRecord) error {
	if s.plans == nil {
		s.plans = map[string]PlanRecord{}
	}
	s.plans[plan.PlanID] = plan
	return nil
}

func (s *runtimeAPITestStore) Plan(planID string) (PlanRecord, bool, error) {
	item, ok := s.plans[planID]
	return item, ok, nil
}

func (s *runtimeAPITestStore) ListPlans(query PlanQuery) ([]PlanRecord, error) {
	out := []PlanRecord{}
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

func (s *runtimeAPITestStore) SaveWorker(record WorkerRecord) error { return nil }
func (s *runtimeAPITestStore) RecoverInterruptedWorkers(reason string) (int, error) { return 0, nil }
func (s *runtimeAPITestStore) Worker(workerID string) (WorkerRecord, bool, error) {
	for _, item := range s.workers {
		if item.WorkerID == workerID {
			return item, true, nil
		}
	}
	return WorkerRecord{}, false, nil
}
func (s *runtimeAPITestStore) ListWorkers(query WorkerQuery) ([]WorkerRecord, error) {
	out := make([]WorkerRecord, 0, len(s.workers))
	for _, item := range s.workers {
		if query.HasParentChatID && item.ParentChatID != query.ParentChatID {
			continue
		}
		out = append(out, item)
	}
	return out, nil
}
func (s *runtimeAPITestStore) SaveWorkerHandoff(h WorkerHandoff) error { return nil }
func (s *runtimeAPITestStore) WorkerHandoff(workerID string) (WorkerHandoff, bool, error) {
	if s.handoff == nil {
		return WorkerHandoff{}, false, nil
	}
	item, ok := s.handoff[workerID]
	return item, ok, nil
}
func (s *runtimeAPITestStore) SaveJob(JobRecord) error { return nil }
func (s *runtimeAPITestStore) Job(jobID string) (JobRecord, bool, error) {
	for _, item := range s.jobs {
		if item.JobID == jobID {
			return item, true, nil
		}
	}
	return JobRecord{}, false, nil
}
func (s *runtimeAPITestStore) ListJobs(limit int) ([]JobRecord, error) {
	return append([]JobRecord(nil), s.jobs...), nil
}
func (s *runtimeAPITestStore) MarkJobCancelRequested(jobID string) error         { return nil }
func (s *runtimeAPITestStore) SaveJobLog(JobLogChunk) error                      { return nil }
func (s *runtimeAPITestStore) JobLogs(query JobLogQuery) ([]JobLogChunk, error)  { return nil, nil }
func (s *runtimeAPITestStore) RecoverInterruptedJobs(reason string) (int, error) { return 0, nil }

func TestRuntimeAPIExposesRunAndApprovalViews(t *testing.T) {
	store := &runtimeAPITestStore{}
	approvalSvc := approvals.New(approvals.TestDeps())
	api := NewAPI(store, NewActiveRegistry(), approvalSvc)

	prepared, ok, err := api.PrepareRun(context.Background(), "run-1", 1001, "1001:default", "hello", PolicySnapshot{})
	if err != nil || !ok {
		t.Fatalf("prepare run: ok=%v err=%v", ok, err)
	}
	view, ok := api.ActiveRunView(1001)
	if !ok {
		t.Fatal("expected active run view")
	}
	if view.RunID != "run-1" || !view.Active || view.Status != StatusRunning {
		t.Fatalf("unexpected active run view: %+v", view)
	}

	api.LaunchRun(prepared, func(ctx context.Context, runID string) error { return nil })

	record, err := approvalSvc.Create(approvals.Request{
		WorkerID:  "shell.exec",
		SessionID: "1001:default",
		Payload:   "{}",
	})
	if err != nil {
		t.Fatalf("create approval: %v", err)
	}
	approvalsView := api.PendingApprovals("1001:default")
	if len(approvalsView) != 1 || approvalsView[0].ID != record.ID {
		t.Fatalf("unexpected approvals: %+v", approvalsView)
	}
}

func TestRuntimeAPIBuildsControlState(t *testing.T) {
	now := time.Now().UTC()
	store := &runtimeAPITestStore{
		run: RunRecord{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Query:     "hello",
			Status:    StatusRunning,
			StartedAt: now,
			PromptBudget: PromptBudgetMetrics{
				ContextWindowTokens: 200000,
				PromptBudgetTokens:  150000,
				FinalPromptTokens:   90000,
				SystemOverheadTokens: 12000,
				PromptBudgetPercent: 60,
				ContextWindowPercent: 45,
			},
		},
		ok: true,
		workers: []WorkerRecord{{
			WorkerID:        "worker-1",
			ParentChatID:    1001,
			ParentSessionID: "1001:default",
			WorkerChatID:    -1,
			WorkerSessionID: "worker-1-session",
			Status:          WorkerWaitingApproval,
			LastRunID:       "worker-run-1",
			CreatedAt:       now,
			UpdatedAt:       now,
		}},
		jobs: []JobRecord{{
			JobID:     "job-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Command:   "echo",
			Status:    JobRunning,
			StartedAt: now,
		}},
		head: SessionHead{
			ChatID:             1001,
			SessionID:          "1001:default",
			LastCompletedRunID: "run-prev",
			CurrentGoal:        "оформить недавнюю работу",
			LastResultSummary:  "шаблон астры обновлён и выключен",
			RecentArtifactRefs: []string{"artifact://run/run-prev/report"},
			UpdatedAt:          now,
		},
		headOK: true,
	}
	approvalSvc := approvals.New(approvals.TestDeps())
	record, err := approvalSvc.Create(approvals.Request{
		WorkerID:   "shell.exec",
		SessionID:  "1001:default",
		Payload:    "{}",
		TargetType: "run",
		TargetID:   "worker-run-1",
	})
	if err != nil {
		t.Fatalf("create approval: %v", err)
	}
	workerRecord, err := approvalSvc.Create(approvals.Request{
		WorkerID:   "shell.exec",
		SessionID:  "-1:worker-1-session",
		Payload:    "{}",
		TargetType: "run",
		TargetID:   "worker-run-1",
	})
	if err != nil {
		t.Fatalf("create worker approval: %v", err)
	}
	api := NewAPI(store, NewActiveRegistry(), approvalSvc)
	control, err := api.ControlState("1001:default", 1001, provider.RequestConfig{Model: "glm-5-turbo"}, MemoryPolicy{Profile: "conservative"}, ActionPolicy{})
	if err != nil {
		t.Fatalf("control state: %v", err)
	}
	if control.Session.SessionID != "1001:default" || len(control.Approvals) != 2 {
		t.Fatalf("unexpected control approvals/session: %+v", control)
	}
	if control.Approvals[0].ID != record.ID && control.Approvals[1].ID != record.ID {
		t.Fatalf("missing parent approval in control state: %+v", control.Approvals)
	}
	if control.Approvals[0].ID != workerRecord.ID && control.Approvals[1].ID != workerRecord.ID {
		t.Fatalf("missing worker approval in control state: %+v", control.Approvals)
	}
	if len(control.Workers) != 1 || control.Workers[0].WorkerID != "worker-1" {
		t.Fatalf("unexpected control workers: %+v", control.Workers)
	}
	if len(control.Jobs) != 1 || control.Jobs[0].JobID != "job-1" {
		t.Fatalf("unexpected control jobs: %+v", control.Jobs)
	}
	if control.Session.Head == nil || control.Session.Head.LastCompletedRunID != "run-prev" {
		t.Fatalf("expected session head in control state: %+v", control.Session.Head)
	}
	report := FormatControlReport(control)[0]
	if !strings.Contains(report, "Prompt budget: 60%") || !strings.Contains(report, "System overhead: 12000") {
		t.Fatalf("expected prompt budget breakdown in report: %s", report)
	}
}

func TestRuntimeAPIListsRunsAndSessions(t *testing.T) {
	now := time.Now().UTC()
	store := &runtimeAPITestStore{
		run: RunRecord{
			RunID:         "run-1",
			ChatID:        1001,
			SessionID:     "1001:default",
			Query:         "hello",
			Status:        StatusCompleted,
			FinalResponse: "final hello",
			StartedAt:     now,
			PolicySnapshot: PolicySnapshot{
				Runtime:      provider.RequestConfig{Model: "glm-5.1"},
				MemoryPolicy: MemoryPolicy{Profile: "conservative"},
				ActionPolicy: ActionPolicy{ApprovalRequiredTools: []string{"shell.exec"}},
			},
		},
		ok: true,
		head: SessionHead{
			ChatID:             1001,
			SessionID:          "1001:default",
			LastCompletedRunID: "run-0",
			CurrentGoal:        "продолжить недавнюю работу",
			LastResultSummary:  "предыдущий результат сохранён",
			RecentArtifactRefs: []string{"artifact://run/run-0/result"},
			UpdatedAt:          now,
		},
		headOK: true,
		events: []RuntimeEvent{
			{
				ID:         1,
				EntityType: "run",
				EntityID:   "run-1",
				SessionID:  "1001:default",
				Kind:       "artifact.offloaded",
				Payload:    json.RawMessage(`{"artifact_ref":"artifact://tool-output-1"}`),
			},
		},
	}
	api := NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	runs, err := api.ListRuns(RunQuery{SessionID: "1001:default", Limit: 1})
	if err != nil {
		t.Fatalf("list runs: %v", err)
	}
	if len(runs) != 1 || runs[0].RunID != "run-1" {
		t.Fatalf("unexpected runs: %+v", runs)
	}
	if runs[0].FinalResponse != "final hello" {
		t.Fatalf("missing run final response: %+v", runs[0])
	}
	if len(runs[0].ArtifactRefs) != 1 || runs[0].ArtifactRefs[0] != "artifact://tool-output-1" {
		t.Fatalf("missing run artifact refs: %+v", runs[0].ArtifactRefs)
	}
	if runs[0].PolicySnapshot.Runtime.Model != "glm-5.1" || runs[0].PolicySnapshot.MemoryPolicy.Profile != "conservative" {
		t.Fatalf("missing run policy snapshot: %+v", runs[0].PolicySnapshot)
	}
	sessions, err := api.ListSessions(SessionQuery{ChatID: 1001, HasChatID: true, Limit: 10}, provider.RequestConfig{Model: "glm-5-turbo"}, MemoryPolicy{Profile: "conservative"}, ActionPolicy{})
	if err != nil {
		t.Fatalf("list sessions: %v", err)
	}
	if len(sessions) != 1 || sessions[0].SessionID != "1001:default" {
		t.Fatalf("unexpected sessions: %+v", sessions)
	}
	if sessions[0].Head == nil || sessions[0].Head.LastCompletedRunID != "run-0" {
		t.Fatalf("missing session head in list sessions: %+v", sessions[0].Head)
	}
	session, err := api.SessionState("1001:default", 1001, provider.RequestConfig{Model: "glm-5-turbo"}, MemoryPolicy{Profile: "conservative"}, ActionPolicy{})
	if err != nil {
		t.Fatalf("session state: %v", err)
	}
	if session.Head == nil || session.Head.LastResultSummary != "предыдущий результат сохранён" {
		t.Fatalf("missing session head in session state: %+v", session.Head)
	}
}

func TestRuntimeAPIListsEvents(t *testing.T) {
	store := &runtimeAPITestStore{
		events: []RuntimeEvent{
			{ID: 1, EntityType: "run", EntityID: "run-1", SessionID: "1001:default", Kind: "run.started"},
			{ID: 2, EntityType: "run", EntityID: "run-1", SessionID: "1001:default", Kind: "run.completed"},
		},
	}
	api := NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	events, err := api.ListEvents(EventQuery{EntityType: "run", EntityID: "run-1", Limit: 10})
	if err != nil {
		t.Fatalf("list events: %v", err)
	}
	if len(events) != 2 || events[0].Kind != "run.started" || events[1].Kind != "run.completed" {
		t.Fatalf("unexpected events: %+v", events)
	}
}

func TestRuntimeAPIExecutesRunControlActions(t *testing.T) {
	store := &runtimeAPITestStore{
		run: RunRecord{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Query:     "hello",
			Status:    StatusRunning,
			StartedAt: time.Now().UTC(),
		},
		ok: true,
		head: SessionHead{
			ChatID:             1001,
			SessionID:          "1001:default",
			LastCompletedRunID: "run-prev",
			CurrentGoal:        "довести прошлый кейс",
			LastResultSummary:  "прошлый run завершён успешно",
			RecentArtifactRefs: []string{"artifact://run/run-prev/result"},
			UpdatedAt:          time.Now().UTC(),
		},
		headOK: true,
	}
	registry := NewActiveRegistry()
	registry.TryStart(ActiveRun{RunID: "run-1", ChatID: 1001, SessionID: "1001:default", Query: "hello", StartedAt: time.Now().UTC()})
	api := NewAPI(store, registry, approvals.New(approvals.TestDeps()))

	statusResult, err := api.ExecuteControlAction("1001:default", 1001, provider.RequestConfig{Model: "glm-5-turbo"}, MemoryPolicy{Profile: "conservative"}, ActionPolicy{}, ControlActionRunStatus)
	if err != nil {
		t.Fatalf("run status action: %v", err)
	}
	if len(statusResult.Pages) == 0 || statusResult.Control.Session.LatestRun == nil || statusResult.Control.Session.LatestRun.RunID != "run-1" {
		t.Fatalf("unexpected status result: %+v", statusResult)
	}
	if got := statusResult.Pages[0]; !strings.Contains(got, "Recent context:") || !strings.Contains(got, "run-prev") {
		t.Fatalf("status report missing recent context: %s", got)
	}

	cancelResult, err := api.ExecuteControlAction("1001:default", 1001, provider.RequestConfig{Model: "glm-5-turbo"}, MemoryPolicy{Profile: "conservative"}, ActionPolicy{}, ControlActionRunCancel)
	if err != nil {
		t.Fatalf("run cancel action: %v", err)
	}
	if cancelResult.Message != "Отмена запрошена" {
		t.Fatalf("unexpected cancel message: %+v", cancelResult)
	}
	if !store.run.CancelRequested {
		t.Fatalf("expected cancel request to be persisted: %+v", store.run)
	}
}

func TestRuntimeAPIRecentWorkSnapshotUsesSessionHeadAndReplay(t *testing.T) {
	now := time.Now().UTC()
	store := &runtimeAPITestStore{
		run: RunRecord{
			RunID:         "run-prev",
			ChatID:        1001,
			SessionID:     "1001:default",
			Query:         "обновить шаблон астры",
			Status:        StatusCompleted,
			FinalResponse: "шаблон обновлён и выключен",
			StartedAt:     now,
		},
		ok: true,
		head: SessionHead{
			ChatID:             1001,
			SessionID:          "1001:default",
			LastCompletedRunID: "run-prev",
			CurrentGoal:        "обновить шаблон астры",
			LastResultSummary:  "шаблон обновлён и выключен",
			RecentArtifactRefs: []string{"artifact://run/run-prev/report"},
			UpdatedAt:          now,
		},
		headOK: true,
		events: []RuntimeEvent{
			{ID: 1, EntityType: "run", EntityID: "run-prev", SessionID: "1001:default", Kind: "tool.completed", Payload: json.RawMessage(`{"artifact_ref":"artifact://run/run-prev/report"}`), CreatedAt: now},
		},
	}
	api := NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	snapshot, ok, err := api.RecentWorkSnapshot(1001, "1001:default", "запиши это как проект")
	if err != nil || !ok {
		t.Fatalf("recent work snapshot: ok=%v err=%v", ok, err)
	}
	if snapshot.Intent != RecentWorkIntentProjectSave {
		t.Fatalf("unexpected snapshot intent: %+v", snapshot)
	}
	if snapshot.Head.LastCompletedRunID != "run-prev" {
		t.Fatalf("missing session head in snapshot: %+v", snapshot)
	}
	if snapshot.Replay == nil || snapshot.Replay.Run.RunID != "run-prev" {
		t.Fatalf("expected replay in snapshot: %+v", snapshot)
	}
}

func TestRuntimeAPIRunStatusActionWithoutActiveRun(t *testing.T) {
	api := NewAPI(&runtimeAPITestStore{}, NewActiveRegistry(), approvals.New(approvals.TestDeps()))

	result, err := api.ExecuteControlAction("1001:default", 1001, provider.RequestConfig{Model: "glm-5-turbo"}, MemoryPolicy{Profile: "conservative"}, ActionPolicy{}, ControlActionRunStatus)
	if err != nil {
		t.Fatalf("run status action: %v", err)
	}
	if result.Message != "Нет активного выполнения" {
		t.Fatalf("unexpected result: %+v", result)
	}
}

func TestRuntimeAPIManagesPlans(t *testing.T) {
	store := &runtimeAPITestStore{}
	api := NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	plan, err := api.CreatePlan(context.Background(), "run", "run-1", "Investigate rollout")
	if err != nil {
		t.Fatalf("create plan: %v", err)
	}
	plan, err = api.ReplacePlanItems(plan.PlanID, []PlanItem{{Content: "Inspect runtime"}, {Content: "Verify cli"}})
	if err != nil {
		t.Fatalf("replace items: %v", err)
	}
	plan, err = api.AppendPlanNote(plan.PlanID, "Focus on API parity.")
	if err != nil {
		t.Fatalf("append note: %v", err)
	}
	plan, err = api.StartPlanItem(plan.PlanID, plan.Items[0].ItemID)
	if err != nil {
		t.Fatalf("start item: %v", err)
	}
	plan, err = api.CompletePlanItem(plan.PlanID, plan.Items[0].ItemID)
	if err != nil {
		t.Fatalf("complete item: %v", err)
	}
	if len(plan.Notes) != 1 || plan.Items[0].Status != PlanItemCompleted {
		t.Fatalf("unexpected plan state: %+v", plan)
	}
	items, err := api.ListPlans(PlanQuery{OwnerType: "run", OwnerID: "run-1", Limit: 10})
	if err != nil {
		t.Fatalf("list plans: %v", err)
	}
	if len(items) != 1 || items[0].PlanID != plan.PlanID {
		t.Fatalf("unexpected plan list: %+v", items)
	}
}
