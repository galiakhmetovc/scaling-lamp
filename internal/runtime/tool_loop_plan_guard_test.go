package runtime

import (
	"encoding/json"
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

func TestExecutePlanCommandReturnsPlanSnapshot(t *testing.T) {
	t.Parallel()

	agent := &Agent{
		MaxToolRounds: 2,
		Now:           func() time.Time { return time.Date(2026, 4, 17, 12, 0, 0, 0, time.UTC) },
		NewID:         func(prefix string) string { return prefix + "-1" },
	}
	service := plans.NewService(agent.now, agent.newID)
	active := projections.ActivePlanSnapshot{
		Plan: projections.PlanView{
			ID:        "plan-1",
			Goal:      "Refactor auth",
			Status:    "active",
			CreatedAt: time.Date(2026, 4, 17, 11, 0, 0, 0, time.UTC),
		},
		Tasks: map[string]projections.PlanTaskView{
			"t1": {ID: "t1", PlanID: "plan-1", Description: "Audit middleware", Status: "in_progress", Order: 1},
			"t2": {ID: "t2", PlanID: "plan-1", Description: "Write tests", Status: "todo", Order: 2, DependsOn: []string{"t1"}},
		},
	}

	events, result, err := agent.executePlanCommand(
		"session-1",
		active,
		service,
		"runtime.test",
		provider.ToolCall{ID: "call-1", Name: "plan_snapshot", Arguments: map[string]any{}},
	)
	if err != nil {
		t.Fatalf("executePlanCommand returned error: %v", err)
	}
	if len(events) != 0 {
		t.Fatalf("events = %#v, want none", events)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(result), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if payload["tool"] != "plan_snapshot" {
		t.Fatalf("tool = %#v, want plan_snapshot", payload["tool"])
	}
	plan := payload["plan"].(map[string]any)
	if plan["goal"] != "Refactor auth" {
		t.Fatalf("goal = %#v, want Refactor auth", plan["goal"])
	}
}

func TestExecutePlanCommandReturnsPlanLintDiagnostics(t *testing.T) {
	t.Parallel()

	agent := &Agent{
		MaxToolRounds: 2,
		Now:           func() time.Time { return time.Date(2026, 4, 17, 12, 0, 0, 0, time.UTC) },
		NewID:         func(prefix string) string { return prefix + "-1" },
	}
	service := plans.NewService(agent.now, agent.newID)
	active := projections.ActivePlanSnapshot{
		Plan: projections.PlanView{
			ID:        "plan-1",
			Goal:      "Refactor auth",
			Status:    "active",
			CreatedAt: time.Date(2026, 4, 17, 11, 0, 0, 0, time.UTC),
		},
		Tasks: map[string]projections.PlanTaskView{
			"t1": {ID: "t1", PlanID: "plan-1", Description: "Task one", Status: "in_progress", Order: 1},
			"t2": {ID: "t2", PlanID: "plan-1", Description: "Task two", Status: "in_progress", Order: 2},
			"t3": {ID: "t3", PlanID: "plan-1", Description: "Blocked task", Status: "blocked", Order: 3},
			"t4": {ID: "t4", PlanID: "plan-1", Description: "Missing dep", Status: "todo", Order: 4, DependsOn: []string{"missing"}},
		},
	}

	events, result, err := agent.executePlanCommand(
		"session-1",
		active,
		service,
		"runtime.test",
		provider.ToolCall{ID: "call-1", Name: "plan_lint", Arguments: map[string]any{}},
	)
	if err != nil {
		t.Fatalf("executePlanCommand returned error: %v", err)
	}
	if len(events) != 0 {
		t.Fatalf("events = %#v, want none", events)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(result), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	issues := payload["issues"].([]any)
	if len(issues) < 3 {
		t.Fatalf("issues = %#v, want diagnostics", issues)
	}
}
