package mesh

import (
	"context"
	"testing"

	"teamd/internal/provider"
)

type scriptedClarifierProvider struct {
	responses []provider.PromptResponse
	requests  []provider.PromptRequest
}

func (s *scriptedClarifierProvider) Generate(_ context.Context, req provider.PromptRequest) (provider.PromptResponse, error) {
	s.requests = append(s.requests, req)
	resp := s.responses[0]
	s.responses = s.responses[1:]
	return resp, nil
}

func TestClarifierSingleModeBuildsClarifiedTask(t *testing.T) {
	client := &scriptedClarifierProvider{
		responses: []provider.PromptResponse{{
			Text: `{"goal":"Проверить память сервера","deliverables":["состояние памяти"],"constraints":["использовать shell"],"assumptions":["доступ к shell есть"],"missing_info":[],"task_class":"shell","task_shape":"single"}`,
		}},
	}
	clarifier := NewClarifier(client)

	task, err := clarifier.Clarify(context.Background(), ClarificationInput{
		Mode:   "single",
		Prompt: "проверь память на сервере",
	})
	if err != nil {
		t.Fatalf("clarify: %v", err)
	}
	if task.Goal != "Проверить память сервера" {
		t.Fatalf("unexpected clarified task: %#v", task)
	}
	if task.TaskClass != "shell" || task.TaskShape != "single" {
		t.Fatalf("unexpected clarified shape/class: %#v", task)
	}
}

func TestClarifierRequestsFollowUpOnCriticalMissingInfo(t *testing.T) {
	client := &scriptedClarifierProvider{
		responses: []provider.PromptResponse{{
			Text: `{"goal":"Написать скрипт","deliverables":["скрипт"],"constraints":[],"assumptions":[],"missing_info":["какой язык нужен?"],"task_class":"coding","task_shape":"single"}`,
		}},
	}
	clarifier := NewClarifier(client)

	task, err := clarifier.Clarify(context.Background(), ClarificationInput{
		Mode:                     "single",
		Prompt:                   "напиши скрипт",
		CriticalMissingInfo:      true,
		MaxClarificationRounds:   2,
		CurrentClarificationRound: 1,
	})
	if err != nil {
		t.Fatalf("clarify: %v", err)
	}
	if !task.RequiresFollowUp {
		t.Fatalf("expected follow-up requirement, got %#v", task)
	}
	if task.FollowUpQuestion == "" {
		t.Fatalf("expected follow-up question, got %#v", task)
	}
}

func TestClarifierFallsBackToAssumptionsAfterMaxRounds(t *testing.T) {
	client := &scriptedClarifierProvider{
		responses: []provider.PromptResponse{{
			Text: `{"goal":"Написать скрипт","deliverables":["скрипт"],"constraints":[],"assumptions":["использовать bash"],"missing_info":["какой язык нужен?"],"task_class":"coding","task_shape":"single"}`,
		}},
	}
	clarifier := NewClarifier(client)

	task, err := clarifier.Clarify(context.Background(), ClarificationInput{
		Mode:                     "single",
		Prompt:                   "напиши скрипт",
		CriticalMissingInfo:      true,
		MaxClarificationRounds:   1,
		CurrentClarificationRound: 1,
	})
	if err != nil {
		t.Fatalf("clarify: %v", err)
	}
	if task.RequiresFollowUp {
		t.Fatalf("expected fallback to assumptions after max rounds, got %#v", task)
	}
	if !task.LowConfidence {
		t.Fatalf("expected low confidence fallback, got %#v", task)
	}
}

func TestClarifierFallsBackGracefullyWhenModelReturnsNonJSON(t *testing.T) {
	client := &scriptedClarifierProvider{
		responses: []provider.PromptResponse{{
			Text: "Похоже, нужно уточнить язык и окружение, но начну с общих допущений.",
		}},
	}
	clarifier := NewClarifier(client)

	task, err := clarifier.Clarify(context.Background(), ClarificationInput{
		Mode:                    "single",
		Prompt:                  "напиши скрипт",
		CriticalMissingInfo:     true,
		MaxClarificationRounds:  1,
		CurrentClarificationRound: 1,
	})
	if err != nil {
		t.Fatalf("clarify: %v", err)
	}
	if task.Goal != "напиши скрипт" {
		t.Fatalf("expected original prompt fallback, got %#v", task)
	}
	if !task.LowConfidence {
		t.Fatalf("expected low confidence fallback, got %#v", task)
	}
}
