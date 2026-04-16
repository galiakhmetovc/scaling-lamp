package tools

import (
	"fmt"

	"teamd/internal/contracts"
)

type PlanToolExecutor struct{}

func NewPlanToolExecutor() *PlanToolExecutor {
	return &PlanToolExecutor{}
}

func (e *PlanToolExecutor) Build(contract contracts.PlanToolContract) ([]Definition, error) {
	if e == nil {
		return nil, fmt.Errorf("plan tool executor is nil")
	}
	if !contract.PlanTool.Enabled {
		return nil, nil
	}
	if contract.PlanTool.Strategy != "default_plan_tools" {
		return nil, fmt.Errorf("unsupported plan tool strategy %q", contract.PlanTool.Strategy)
	}
	all := defaultPlanToolDefinitions()
	byID := make(map[string]Definition, len(all))
	for _, definition := range all {
		byID[definition.ID] = definition
	}
	out := make([]Definition, 0, len(contract.PlanTool.Params.ToolIDs))
	for _, id := range contract.PlanTool.Params.ToolIDs {
		definition, ok := byID[id]
		if !ok {
			return nil, fmt.Errorf("plan tool %q is not defined", id)
		}
		out = append(out, definition)
	}
	return out, nil
}

func defaultPlanToolDefinitions() []Definition {
	return []Definition{
		{
			ID:          "init_plan",
			Name:        "init_plan",
			Description: "Create a new active internal plan only when no active plan exists yet. Do not use this to replace or archive an existing plan.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"goal": map[string]any{"type": "string"},
				},
				"required": []string{"goal"},
			},
		},
		{
			ID:          "add_task",
			Name:        "add_task",
			Description: "Add a task to the active plan, optionally under a parent task.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"plan_id":        map[string]any{"type": "string"},
					"description":    map[string]any{"type": "string"},
					"parent_task_id": map[string]any{"type": "string"},
					"depends_on": map[string]any{
						"type":  "array",
						"items": map[string]any{"type": "string"},
					},
				},
				"required": []string{"plan_id", "description"},
			},
		},
		{
			ID:          "set_task_status",
			Name:        "set_task_status",
			Description: "Change task status in the active plan.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"task_id":        map[string]any{"type": "string"},
					"new_status":     map[string]any{"type": "string", "enum": []string{"todo", "in_progress", "done", "blocked", "cancelled"}},
					"blocked_reason": map[string]any{"type": "string"},
				},
				"required": []string{"task_id", "new_status"},
			},
		},
		{
			ID:          "add_task_note",
			Name:        "add_task_note",
			Description: "Attach a note to a task in the active plan.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"task_id":   map[string]any{"type": "string"},
					"note_text": map[string]any{"type": "string"},
				},
				"required": []string{"task_id", "note_text"},
			},
		},
		{
			ID:          "edit_task",
			Name:        "edit_task",
			Description: "Edit task description in the active plan.",
			Parameters: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"task_id":         map[string]any{"type": "string"},
					"new_description": map[string]any{"type": "string"},
					"new_depends_on": map[string]any{
						"type":  "array",
						"items": map[string]any{"type": "string"},
					},
				},
				"required": []string{"task_id", "new_description"},
			},
		},
	}
}
