package projections_test

import (
	"testing"
	"time"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestActivePlanProjectionTracksPlansPerSession(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 4, 14, 18, 0, 0, 0, time.UTC)
	projection := projections.NewActivePlanProjection()
	events := []eventing.Event{
		{
			Kind:       eventing.EventPlanCreated,
			OccurredAt: now,
			Payload: map[string]any{
				"session_id": "session-a",
				"plan_id":    "plan-a",
				"goal":       "Plan A",
			},
		},
		{
			Kind:       eventing.EventPlanCreated,
			OccurredAt: now.Add(time.Second),
			Payload: map[string]any{
				"session_id": "session-b",
				"plan_id":    "plan-b",
				"goal":       "Plan B",
			},
		},
		{
			Kind:       eventing.EventTaskAdded,
			OccurredAt: now.Add(2 * time.Second),
			Payload: map[string]any{
				"session_id":   "session-a",
				"plan_id":      "plan-a",
				"task_id":      "task-a1",
				"description":  "Task A1",
				"status":       "todo",
				"order":        1,
				"depends_on":   []any{},
				"parent_task_id": "",
			},
		},
	}
	for _, event := range events {
		if err := projection.Apply(event); err != nil {
			t.Fatalf("Apply(%s) error: %v", event.Kind, err)
		}
	}

	sessionA := projection.SnapshotForSession("session-a")
	sessionB := projection.SnapshotForSession("session-b")
	if sessionA.Plan.ID != "plan-a" {
		t.Fatalf("session-a plan id = %q, want plan-a", sessionA.Plan.ID)
	}
	if sessionB.Plan.ID != "plan-b" {
		t.Fatalf("session-b plan id = %q, want plan-b", sessionB.Plan.ID)
	}
	if len(sessionA.Tasks) != 1 {
		t.Fatalf("session-a tasks len = %d, want 1", len(sessionA.Tasks))
	}
	if len(sessionB.Tasks) != 0 {
		t.Fatalf("session-b tasks len = %d, want 0", len(sessionB.Tasks))
	}
}
