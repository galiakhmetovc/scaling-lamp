package tools_test

import (
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/tools"
)

func TestPlanToolExecutorBuildsDefaultPlanTools(t *testing.T) {
	t.Parallel()

	executor := tools.NewPlanToolExecutor()
	got, err := executor.Build(contracts.PlanToolContract{
		PlanTool: contracts.PlanToolPolicy{
			Enabled:  true,
			Strategy: "default_plan_tools",
			Params: contracts.PlanToolParams{
					ToolIDs: []string{"init_plan", "add_task", "plan_snapshot", "plan_lint"},
				},
			},
		})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if len(got) != 4 {
		t.Fatalf("tool count = %d, want 4", len(got))
	}
	if got[0].ID != "init_plan" || got[1].ID != "add_task" || got[2].ID != "plan_snapshot" || got[3].ID != "plan_lint" {
		t.Fatalf("plan tools = %#v", got)
	}
	props, ok := got[1].Parameters["properties"].(map[string]any)
	if !ok {
		t.Fatalf("add_task properties = %#v", got[1].Parameters["properties"])
	}
	if _, ok := props["depends_on"]; !ok {
		t.Fatalf("add_task schema missing depends_on: %#v", props)
	}
	if got[2].Parameters["type"] != "object" {
		t.Fatalf("plan_snapshot parameters = %#v, want object schema", got[2].Parameters)
	}
	if got[3].Parameters["type"] != "object" {
		t.Fatalf("plan_lint parameters = %#v, want object schema", got[3].Parameters)
	}
}
