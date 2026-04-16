package runtime

import (
	"strings"
	"testing"
	"time"

	"teamd/internal/provider"
	"teamd/internal/runtime/plans"
	"teamd/internal/runtime/projections"
)

func TestExecutePlanCommandRejectsInitPlanWhenActivePlanExists(t *testing.T) {
	t.Parallel()

	agent := &Agent{
		MaxToolRounds: 2,
		Now:           func() time.Time { return time.Date(2026, 4, 16, 16, 0, 0, 0, time.UTC) },
		NewID:         func(prefix string) string { return prefix + "-1" },
	}
	service := plans.NewService(agent.now, agent.newID)
	active := projections.ActivePlanSnapshot{
		Plan: projections.PlanView{
			ID:        "plan-existing-1",
			Goal:      "Keep existing plan",
			Status:    "active",
			CreatedAt: time.Date(2026, 4, 16, 15, 0, 0, 0, time.UTC),
		},
		Tasks: map[string]projections.PlanTaskView{},
	}

	events, result, err := agent.executePlanCommand(
		"session-1",
		active,
		service,
		"runtime.test",
		provider.ToolCall{
			ID:   "call-1",
			Name: "init_plan",
			Arguments: map[string]any{
				"goal": "Replace active plan",
			},
		},
	)
	if err == nil {
		t.Fatal("executePlanCommand returned nil error, want init_plan rejection")
	}
	if !strings.Contains(err.Error(), "active plan already exists") {
		t.Fatalf("executePlanCommand error = %v, want active plan rejection", err)
	}
	if len(events) != 0 {
		t.Fatalf("events = %#v, want no plan mutation events", events)
	}
	if result != "" {
		t.Fatalf("result = %q, want empty on rejection", result)
	}
}
