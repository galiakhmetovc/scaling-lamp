package mesh

import (
	"context"
	"testing"

	"teamd/internal/provider"
)

type stubClassifierProvider struct {
	response provider.PromptResponse
	err      error
}

func (s stubClassifierProvider) Generate(_ context.Context, _ provider.PromptRequest) (provider.PromptResponse, error) {
	return s.response, s.err
}

func TestClassifierParsesTaskClassFromProviderReply(t *testing.T) {
	valid := NewLLMTaskClassifier(stubClassifierProvider{
		response: provider.PromptResponse{
			Text: `{"task_class":"coding","confidence":0.82,"reasoning":"needs file edits"}`,
		},
	})
	taskClass, confidence, err := valid.Classify(context.Background(), "write a deploy script")
	if err != nil {
		t.Fatalf("classify valid: %v", err)
	}
	if taskClass != "coding" {
		t.Fatalf("unexpected task class: %q", taskClass)
	}
	if confidence != 0.82 {
		t.Fatalf("unexpected confidence: %v", confidence)
	}

	invalid := NewLLMTaskClassifier(stubClassifierProvider{
		response: provider.PromptResponse{Text: `not-json`},
	})
	taskClass, confidence, err = invalid.Classify(context.Background(), "unknown")
	if err != nil {
		t.Fatalf("classify invalid: %v", err)
	}
	if taskClass != "analysis" {
		t.Fatalf("expected fallback class analysis, got %q", taskClass)
	}
	if confidence != 0 {
		t.Fatalf("expected zero confidence on fallback, got %v", confidence)
	}
}
