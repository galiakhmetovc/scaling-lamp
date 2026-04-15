package runtime_test

import (
	"context"
	"testing"
	"time"

	"teamd/internal/runtime"
	"teamd/internal/runtime/projections"
)

func TestAgentPlanOperatorMethodsWriteSessionScopedPlanEvents(t *testing.T) {
	t.Parallel()

	agent := &runtime.Agent{
		Config:     chatRuntimeConfigForTest(),
		EventLog:   runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewActivePlanProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewPlanArchiveProjection(),
			projections.NewChatTimelineProjection(),
		},
		Now:   func() time.Time { return time.Date(2026, 4, 15, 0, 30, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}

	if err := agent.CreatePlan(context.Background(), "session-1", "Refactor auth"); err != nil {
		t.Fatalf("CreatePlan returned error: %v", err)
	}
	if err := agent.AddPlanTask(context.Background(), "session-1", "Audit middleware", "", nil); err != nil {
		t.Fatalf("AddPlanTask returned error: %v", err)
	}

	active, ok := agent.ActivePlan("session-1")
	if !ok {
		t.Fatal("ActivePlan returned ok=false")
	}
	if active.Plan.Goal != "Refactor auth" {
		t.Fatalf("active goal = %q, want Refactor auth", active.Plan.Goal)
	}
	if len(active.Tasks) != 1 {
		t.Fatalf("active task count = %d, want 1", len(active.Tasks))
	}
	timeline := agent.CurrentChatTimeline("session-1")
	if len(timeline) != 2 {
		t.Fatalf("timeline item count = %d, want 2", len(timeline))
	}
	if timeline[0].Kind != projections.ChatTimelineItemPlan {
		t.Fatalf("first timeline item = %#v", timeline[0])
	}
}
