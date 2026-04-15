package plans_test

import (
	"testing"
	"time"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/plans"
	"teamd/internal/runtime/projections"
)

func TestServiceInitPlanArchivesPreviousActivePlan(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 4, 14, 16, 0, 0, 0, time.UTC)
	svc := plans.NewService(func() time.Time { return now }, sequenceIDs(
		"evt-plan-archive-1",
		"evt-plan-create-1",
	))

	active := projections.ActivePlanSnapshot{
		Plan: projections.PlanView{ID: "plan-old", Goal: "Old goal", Status: "active", CreatedAt: now.Add(-time.Hour)},
	}

	events, err := svc.InitPlan(active, plans.InitPlanInput{
		SessionID: "session-1",
		Goal:    "New goal",
		Source:  "agent.chat",
		ActorID: "zai-smoke",
	})
	if err != nil {
		t.Fatalf("InitPlan returned error: %v", err)
	}
	if len(events) != 2 {
		t.Fatalf("event count = %d, want 2", len(events))
	}
	if events[0].Kind != eventing.EventPlanArchived {
		t.Fatalf("first event kind = %q, want %q", events[0].Kind, eventing.EventPlanArchived)
	}
	if events[1].Kind != eventing.EventPlanCreated {
		t.Fatalf("second event kind = %q, want %q", events[1].Kind, eventing.EventPlanCreated)
	}
	if events[1].Payload["goal"] != "New goal" {
		t.Fatalf("created goal = %#v", events[1].Payload["goal"])
	}
	if events[1].Payload["session_id"] != "session-1" {
		t.Fatalf("created session_id = %#v, want session-1", events[1].Payload["session_id"])
	}
}

func TestServiceAddTaskRejectsSelfDependencyAndCycles(t *testing.T) {
	t.Parallel()

	svc := plans.NewService(time.Now, sequenceIDs("evt-task-add-1", "evt-task-add-2"))
	active := projections.ActivePlanSnapshot{
		Plan: projections.PlanView{ID: "plan-1", Goal: "Goal", Status: "active"},
		Tasks: map[string]projections.PlanTaskView{
			"t1": {ID: "t1", PlanID: "plan-1", Description: "root", Status: "todo"},
			"t2": {ID: "t2", PlanID: "plan-1", Description: "child", Status: "todo", DependsOn: []string{"t1"}},
		},
	}

	if _, err := svc.AddTask(active, plans.AddTaskInput{
		SessionID:   "session-1",
		PlanID:      "plan-1",
		TaskID:      "t3",
		Description: "bad self",
		DependsOn:   []string{"t3"},
		Source:      "agent.chat",
		ActorID:     "zai-smoke",
	}); err == nil {
		t.Fatal("AddTask self-dependency error = nil, want error")
	}

	if _, err := svc.EditTask(active, plans.EditTaskInput{
		SessionID:       "session-1",
		TaskID:         "t1",
		NewDescription: "root edited",
		NewDependsOn:   []string{"t2"},
		Source:         "agent.chat",
		ActorID:        "zai-smoke",
	}); err == nil {
		t.Fatal("EditTask cycle error = nil, want error")
	}
}

func TestPlanProjectionsBuildActiveArchiveAndHeadViews(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 4, 14, 16, 0, 0, 0, time.UTC)
	active := projections.NewActivePlanProjection()
	archive := projections.NewPlanArchiveProjection()
	head := projections.NewPlanHeadProjection()

	events := []eventing.Event{
		{
			Kind:          eventing.EventPlanCreated,
			OccurredAt:    now,
			AggregateID:   "plan-1",
			AggregateType: eventing.AggregatePlan,
			Payload: map[string]any{
				"plan_id": "plan-1",
				"session_id": "session-1",
				"goal":    "Refactor auth",
			},
		},
		{
			Kind:          eventing.EventTaskAdded,
			OccurredAt:    now,
			AggregateID:   "t1",
			AggregateType: eventing.AggregatePlanTask,
			Payload: map[string]any{
				"plan_id":     "plan-1",
				"session_id":  "session-1",
				"task_id":     "t1",
				"description": "Design schema",
				"status":      "done",
				"order":       1,
				"depends_on":  []any{},
			},
		},
		{
			Kind:          eventing.EventTaskAdded,
			OccurredAt:    now,
			AggregateID:   "t2",
			AggregateType: eventing.AggregatePlanTask,
			Payload: map[string]any{
				"plan_id":     "plan-1",
				"session_id":  "session-1",
				"task_id":     "t2",
				"description": "Write middleware",
				"status":      "todo",
				"order":       2,
				"depends_on":  []any{"t1"},
			},
		},
		{
			Kind:          eventing.EventTaskAdded,
			OccurredAt:    now,
			AggregateID:   "t3",
			AggregateType: eventing.AggregatePlanTask,
			Payload: map[string]any{
				"plan_id":     "plan-1",
				"session_id":  "session-1",
				"task_id":     "t3",
				"description": "Integrate routes",
				"status":      "todo",
				"order":       3,
				"depends_on":  []any{"t4"},
			},
		},
		{
			Kind:          eventing.EventTaskAdded,
			OccurredAt:    now,
			AggregateID:   "t4",
			AggregateType: eventing.AggregatePlanTask,
			Payload: map[string]any{
				"plan_id":        "plan-1",
				"session_id":     "session-1",
				"task_id":        "t4",
				"description":    "Write tests",
				"status":         "blocked",
				"order":          4,
				"blocked_reason": "waiting for Vasya",
				"depends_on":     []any{},
			},
		},
		{
			Kind:          eventing.EventTaskNoteAdded,
			OccurredAt:    now,
			AggregateID:   "t2",
			AggregateType: eventing.AggregatePlanTask,
			Payload: map[string]any{
				"plan_id":   "plan-1",
				"session_id": "session-1",
				"task_id":   "t2",
				"note_text": "Roles are still cached.",
			},
		},
	}

	for _, event := range events {
		for _, projection := range []projections.Projection{active, archive, head} {
			if err := projection.Apply(event); err != nil {
				t.Fatalf("Apply(%s, %s) error: %v", projection.ID(), event.Kind, err)
			}
		}
	}

	headSnapshot := head.SnapshotForSession("session-1")
	if !headSnapshot.Ready["t2"] {
		t.Fatalf("task t2 ready = false, want true")
	}
	if !headSnapshot.WaitingOnDependencies["t3"] {
		t.Fatalf("task t3 waiting_on_dependencies = false, want true")
	}
	if headSnapshot.Blocked["t4"] != "waiting for Vasya" {
		t.Fatalf("task t4 blocked reason = %q", headSnapshot.Blocked["t4"])
	}
	if got := headSnapshot.Notes["t2"]; len(got) != 1 || got[0] != "Roles are still cached." {
		t.Fatalf("task t2 notes = %#v", got)
	}

	archiveEvent := eventing.Event{
		Kind:          eventing.EventPlanArchived,
		OccurredAt:    now.Add(time.Minute),
		AggregateID:   "plan-1",
		AggregateType: eventing.AggregatePlan,
		Payload: map[string]any{
			"plan_id": "plan-1",
			"session_id": "session-1",
		},
	}
	for _, projection := range []projections.Projection{active, archive, head} {
		if err := projection.Apply(archiveEvent); err != nil {
			t.Fatalf("Apply(%s, %s) error: %v", projection.ID(), archiveEvent.Kind, err)
		}
	}

	if active.SnapshotForSession("session-1").Plan.ID != "" {
		t.Fatalf("active plan after archive = %#v, want empty", active.SnapshotForSession("session-1").Plan)
	}
	if len(archive.SnapshotForSession("session-1")) != 1 {
		t.Fatalf("archive plans len = %d, want 1", len(archive.SnapshotForSession("session-1")))
	}
	if head.SnapshotForSession("session-1").Plan.ID != "" {
		t.Fatalf("plan head after archive = %#v, want empty", head.SnapshotForSession("session-1").Plan)
	}
	if len(head.SnapshotForSession("session-1").Ready) != 0 || len(head.SnapshotForSession("session-1").WaitingOnDependencies) != 0 {
		t.Fatalf("plan head dependency views after archive = %#v / %#v, want empty", head.SnapshotForSession("session-1").Ready, head.SnapshotForSession("session-1").WaitingOnDependencies)
	}
}

func sequenceIDs(ids ...string) func(string) string {
	return func(prefix string) string {
		if len(ids) == 0 {
			return prefix + "-overflow"
		}
		id := ids[0]
		ids = ids[1:]
		return id
	}
}
