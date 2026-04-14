package mesh

import (
	"context"
	"testing"

	"teamd/internal/provider"
)

type scriptedPlannerProvider struct {
	responses []provider.PromptResponse
}

func (s *scriptedPlannerProvider) Generate(_ context.Context, _ provider.PromptRequest) (provider.PromptResponse, error) {
	resp := s.responses[0]
	s.responses = s.responses[1:]
	return resp, nil
}

func TestPlannerParsesSingleAndCompositePlans(t *testing.T) {
	client := &scriptedPlannerProvider{
		responses: []provider.PromptResponse{{
			Text: `{"task_shape":"composite","steps":[{"step_id":"step-1","title":"Написать скрипт","task_class":"coding","description":"Подготовить скрипт резервного копирования","requires_tools":true},{"step_id":"step-2","title":"Задокументировать","task_class":"writing","description":"Кратко описать использование скрипта","requires_tools":false}]}`,
		}},
	}
	planner := NewPlanner(client)

	plan, err := planner.Plan(context.Background(), "Напиши скрипт и задокументируй его")
	if err != nil {
		t.Fatalf("plan: %v", err)
	}
	if plan.TaskShape != string(TaskShapeComposite) {
		t.Fatalf("expected composite task shape, got %#v", plan)
	}
	if len(plan.Steps) != 2 {
		t.Fatalf("expected 2 planned steps, got %#v", plan)
	}
}
