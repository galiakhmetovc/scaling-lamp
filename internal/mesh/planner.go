package mesh

import (
	"context"
	"encoding/json"
	"fmt"

	"teamd/internal/provider"
)

type PlannerRuntime interface {
	Plan(context.Context, string) (TaskPlan, error)
}

type Planner struct {
	provider provider.Provider
}

func NewPlanner(provider provider.Provider) Planner {
	return Planner{provider: provider}
}

func (p Planner) Plan(ctx context.Context, prompt string) (TaskPlan, error) {
	resp, err := p.provider.Generate(ctx, provider.PromptRequest{
		WorkerID: "mesh:planner",
		Messages: []provider.Message{{Role: "user", Content: prompt}},
	})
	if err != nil {
		return TaskPlan{}, err
	}
	var plan TaskPlan
	if err := json.Unmarshal([]byte(resp.Text), &plan); err != nil {
		return TaskPlan{}, fmt.Errorf("parse task plan: %w", err)
	}
	if plan.TaskShape == "" {
		plan.TaskShape = string(TaskShapeSingle)
	}
	return plan, nil
}
