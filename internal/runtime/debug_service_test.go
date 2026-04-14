package runtime

import (
	"encoding/json"
	"testing"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/provider"
)

func TestDebugServiceBuildsSessionAndRunViews(t *testing.T) {
	now := time.Now().UTC()
	store := &runtimeAPITestStore{
		run: RunRecord{
			RunID:         "run-1",
			ChatID:        1001,
			SessionID:     "1001:default",
			Query:         "hello",
			FinalResponse: "done",
			Status:        StatusCompleted,
			StartedAt:     now,
			PromptBudget: PromptBudgetMetrics{
				ContextWindowTokens: 200000,
				PromptBudgetTokens:  150000,
				FinalPromptTokens:   42000,
			},
		},
		ok: true,
		head: SessionHead{
			ChatID:             1001,
			SessionID:          "1001:default",
			LastCompletedRunID: "run-1",
			CurrentGoal:        "debug the runtime",
			LastResultSummary:  "done",
			RecentArtifactRefs: []string{"artifact://run/run-1/result"},
			UpdatedAt:          now,
		},
		headOK: true,
		events: []RuntimeEvent{
			mustRuntimeEvent(t, 1, "run", "run-1", 1001, "1001:default", "run.started", map[string]any{"stage": "starting"}, now),
			mustRuntimeEvent(t, 2, "run", "run-1", 1001, "1001:default", "artifact.offloaded", map[string]any{"artifact_ref": "artifact://run/run-1/result"}, now.Add(time.Second)),
		},
	}
	api := NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	service := NewDebugService(api)

	sessionView, err := service.SessionView("1001:default", 1001, 10)
	if err != nil {
		t.Fatalf("session view: %v", err)
	}
	if sessionView.Session.SessionID != "1001:default" {
		t.Fatalf("unexpected session view: %+v", sessionView)
	}
	if sessionView.Session.Head == nil || sessionView.Session.Head.LastCompletedRunID != "run-1" {
		t.Fatalf("expected SessionHead in session view, got %+v", sessionView)
	}
	if sessionView.Control.Session.SessionID != "1001:default" {
		t.Fatalf("expected control state in session view, got %+v", sessionView)
	}
	if len(sessionView.Events) != 2 || sessionView.Events[1].Kind != "artifact.offloaded" {
		t.Fatalf("expected recent events in session view, got %+v", sessionView.Events)
	}

	runView, ok, err := service.RunView("run-1", 10)
	if err != nil || !ok {
		t.Fatalf("run view: ok=%v err=%v", ok, err)
	}
	if runView.Run.RunID != "run-1" {
		t.Fatalf("unexpected debug run view: %+v", runView)
	}
	if runView.Replay == nil || len(runView.Replay.Steps) == 0 {
		t.Fatalf("expected replay in debug run view, got %+v", runView)
	}
	if len(runView.Events) != 2 {
		t.Fatalf("expected run events in debug run view, got %+v", runView.Events)
	}
}

func mustRuntimeEvent(t *testing.T, id int64, entityType, entityID string, chatID int64, sessionID, kind string, payload map[string]any, createdAt time.Time) RuntimeEvent {
	t.Helper()
	raw, err := json.Marshal(payload)
	if err != nil {
		t.Fatalf("marshal payload: %v", err)
	}
	return RuntimeEvent{
		ID:         id,
		EntityType: entityType,
		EntityID:   entityID,
		ChatID:     chatID,
		SessionID:  sessionID,
		RunID:      entityID,
		Kind:       kind,
		Payload:    raw,
		CreatedAt:  createdAt,
	}
}

func TestDebugServiceBuildsContextProvenance(t *testing.T) {
	now := time.Now().UTC()
	store := &runtimeAPITestStore{
		run: RunRecord{
			RunID:       "run-1",
			ChatID:      1001,
			SessionID:   "1001:default",
			Query:       "продолжай",
			Status:      StatusCompleted,
			StartedAt:   now,
			PromptBudget: PromptBudgetMetrics{FinalPromptTokens: 12345},
		},
		ok: true,
		head: SessionHead{
			ChatID:             1001,
			SessionID:          "1001:default",
			LastCompletedRunID: "run-1",
			CurrentGoal:        "finish prior work",
			LastResultSummary:  "partial summary",
			ResolvedEntities:   []string{"tpl-astra-1.7-clean"},
			RecentArtifactRefs: []string{"artifact://run/run-1/report"},
			OpenLoops:          []string{"formalize result"},
			CurrentProject:     "projects/astra-template-update",
			UpdatedAt:          now,
		},
		headOK: true,
	}
	api := NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	service := NewDebugService(api)

	provenance, err := service.ContextProvenance("run-1")
	if err != nil {
		t.Fatalf("context provenance: %v", err)
	}
	if provenance.RunID != "run-1" {
		t.Fatalf("unexpected provenance view: %+v", provenance)
	}
	if provenance.SessionHead == nil || provenance.SessionHead.CurrentProject != "projects/astra-template-update" {
		t.Fatalf("expected SessionHead provenance, got %+v", provenance)
	}
	if provenance.RecentWork == nil || provenance.RecentWork.LastCompletedRunID != "run-1" {
		t.Fatalf("expected recent-work provenance, got %+v", provenance)
	}
	if provenance.MemoryRecall == nil {
		t.Fatalf("expected memory recall placeholder, got %+v", provenance)
	}
	if provenance.Transcript == nil {
		t.Fatalf("expected transcript provenance placeholder, got %+v", provenance)
	}
}

func TestDebugServiceExposesViewsThroughRuntimeCore(t *testing.T) {
	now := time.Now().UTC()
	store := &runtimeAPITestStore{
		run: RunRecord{
			RunID:       "run-1",
			ChatID:      1001,
			SessionID:   "1001:default",
			Query:       "hello",
			Status:      StatusCompleted,
			StartedAt:   now,
			PromptBudget: PromptBudgetMetrics{FinalPromptTokens: 123},
		},
		ok: true,
		head: SessionHead{
			ChatID:             1001,
			SessionID:          "1001:default",
			LastCompletedRunID: "run-1",
			CurrentGoal:        "hello",
			LastResultSummary:  "done",
			UpdatedAt:          now,
		},
		headOK: true,
	}
	api := NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	core := NewRuntimeCore(api, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5"}, MemoryPolicy{Profile: "conservative"}, ActionPolicy{})

	sessionView, err := core.DebugSession("1001:default", 1001, 5)
	if err != nil {
		t.Fatalf("debug session: %v", err)
	}
	if sessionView.Session.SessionID != "1001:default" {
		t.Fatalf("unexpected core session debug view: %+v", sessionView)
	}
	runView, ok, err := core.DebugRun("run-1", 5)
	if err != nil || !ok {
		t.Fatalf("debug run: ok=%v err=%v", ok, err)
	}
	if runView.Run.RunID != "run-1" {
		t.Fatalf("unexpected core run debug view: %+v", runView)
	}
}
