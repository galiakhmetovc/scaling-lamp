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
				ToolIDs: []string{"init_plan", "add_task"},
			},
		},
	})
	if err != nil {
		t.Fatalf("Build returned error: %v", err)
	}
	if len(got) != 2 {
		t.Fatalf("tool count = %d, want 2", len(got))
	}
	if got[0].ID != "init_plan" || got[1].ID != "add_task" {
		t.Fatalf("plan tools = %#v", got)
	}
	props, ok := got[1].Parameters["properties"].(map[string]any)
	if !ok {
		t.Fatalf("add_task properties = %#v", got[1].Parameters["properties"])
	}
	if _, ok := props["depends_on"]; !ok {
		t.Fatalf("add_task schema missing depends_on: %#v", props)
	}
}
