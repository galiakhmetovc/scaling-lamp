package compaction

import (
	"context"
	"errors"
	"testing"
	"time"

	"teamd/internal/provider"
)

func TestCompactProducesStructuredCheckpointFromTranscript(t *testing.T) {
	svc := New(TestDeps())
	out, err := svc.Compact(context.Background(), Input{
		SessionID:   "telegram:1/default",
		ArchiveRefs: []string{"archive://chat/1/session/default#messages-1-4"},
		Transcript: []string{
			"user: deploy service api to cluster blue",
			"assistant: confirmed blue cluster target",
			"tool: kubectl output truncated",
			"user: next rollback must stay ready",
		},
		ArtifactRefs: []string{"artifact://tool-output/1"},
	})
	if err != nil {
		t.Fatalf("compact: %v", err)
	}
	if out.WhatHappened == "" || out.WhatMattersNow == "" {
		t.Fatalf("expected structured checkpoint fields, got %#v", out)
	}
	if out.CompactionMethod != "heuristic-v1" {
		t.Fatalf("unexpected compaction method: %#v", out)
	}
	if len(out.SourceArtifacts) != 1 {
		t.Fatalf("expected source artifacts to survive, got %#v", out)
	}
	if len(out.ArchiveRefs) != 1 {
		t.Fatalf("expected archive refs to survive, got %#v", out)
	}
}

func TestCompactDoesNotTreatTodoistAsUnresolvedKeyword(t *testing.T) {
	svc := New(TestDeps())
	out, err := svc.Compact(context.Background(), Input{
		SessionID: "telegram:1/default",
		Transcript: []string{
			"user: todoist webhook is failing",
			"assistant: investigate todoist token refresh",
		},
	})
	if err != nil {
		t.Fatalf("compact: %v", err)
	}
	for _, item := range out.UnresolvedItems {
		if item == "user: todoist webhook is failing" {
			t.Fatalf("unexpected unresolved item from substring match: %#v", out)
		}
	}
}

func TestCompactUsesLLMSynthesisWhenEnabled(t *testing.T) {
	svc := New(Deps{
		Provider: stubCompactionProvider{
			resp: provider.PromptResponse{
				Text: `{"what_happened":"Reviewed SearXNG settings and local constraints.","what_matters_now":"Recommend only private-network-safe tuning.","unresolved_items":["Decide which engines to keep enabled."],"next_actions":["Tune limiter and selected engines."]}`,
			},
		},
		ProviderTimeout: 2 * time.Second,
		Enabled:         true,
	})

	out, err := svc.Compact(context.Background(), Input{
		SessionID:   "telegram:1/default",
		ArchiveRefs: []string{"archive://chat/1/session/default#messages-1-2"},
		Transcript: []string{
			"user: searxng is local only, what do you recommend?",
			"assistant: checking current engines and limiter config",
		},
		ArtifactRefs: []string{"artifact://tool-output/1"},
	})
	if err != nil {
		t.Fatalf("compact: %v", err)
	}
	if out.CompactionMethod != "llm-v1" {
		t.Fatalf("expected llm-v1 compaction, got %#v", out)
	}
	if out.WhatMattersNow != "Recommend only private-network-safe tuning." {
		t.Fatalf("unexpected llm synthesis: %#v", out)
	}
}

func TestCompactFallsBackToHeuristicOnInvalidLLMResponse(t *testing.T) {
	svc := New(Deps{
		Provider: stubCompactionProvider{
			resp: provider.PromptResponse{Text: "not json"},
		},
		ProviderTimeout: 2 * time.Second,
		Enabled:         true,
	})

	out, err := svc.Compact(context.Background(), Input{
		SessionID: "telegram:1/default",
		Transcript: []string{
			"user: deploy service api to cluster blue",
			"assistant: confirmed blue cluster target",
			"tool: kubectl output truncated",
			"user: next rollback must stay ready",
		},
	})
	if err != nil {
		t.Fatalf("compact: %v", err)
	}
	if out.CompactionMethod != "heuristic-v1" {
		t.Fatalf("expected heuristic fallback, got %#v", out)
	}
}

type stubCompactionProvider struct {
	resp provider.PromptResponse
	err  error
}

func (s stubCompactionProvider) Generate(_ context.Context, _ provider.PromptRequest) (provider.PromptResponse, error) {
	if s.err != nil {
		return provider.PromptResponse{}, s.err
	}
	return s.resp, nil
}

func TestCompactFallsBackToHeuristicOnLLMError(t *testing.T) {
	svc := New(Deps{
		Provider:        stubCompactionProvider{err: errors.New("boom")},
		ProviderTimeout: 2 * time.Second,
		Enabled:         true,
	})

	out, err := svc.Compact(context.Background(), Input{
		SessionID: "telegram:1/default",
		Transcript: []string{
			"user: deploy service api to cluster blue",
		},
	})
	if err != nil {
		t.Fatalf("compact: %v", err)
	}
	if out.CompactionMethod != "heuristic-v1" {
		t.Fatalf("expected heuristic fallback, got %#v", out)
	}
}
