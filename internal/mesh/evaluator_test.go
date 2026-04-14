package mesh

import (
	"context"
	"testing"
	"time"
)

func TestEvaluatorPrefersPassingCandidate(t *testing.T) {
	evaluator := NewEvaluator(nil)
	candidates := []CandidateReply{
		{
			AgentID:            "peer-fail",
			Stage:              "error",
			DeterministicScore: 10,
			PassedChecks:       false,
		},
		{
			AgentID:            "peer-pass",
			Stage:              "final",
			DeterministicScore: 1,
			PassedChecks:       true,
		},
	}

	winner, updates, err := evaluator.Evaluate(context.Background(), "coding", candidates)
	if err != nil {
		t.Fatalf("evaluate: %v", err)
	}
	if winner.AgentID != "peer-pass" {
		t.Fatalf("expected passing candidate to win, got %#v", winner)
	}
	if len(updates) != 2 {
		t.Fatalf("expected score updates for both candidates, got %#v", updates)
	}
}

func TestEvaluatorUsesLatencyAsTieBreaker(t *testing.T) {
	evaluator := NewEvaluator(nil)
	candidates := []CandidateReply{
		{
			AgentID:            "peer-slow",
			Stage:              "final",
			DeterministicScore: 5,
			PassedChecks:       true,
			Latency:            2 * time.Second,
		},
		{
			AgentID:            "peer-fast",
			Stage:              "final",
			DeterministicScore: 5,
			PassedChecks:       true,
			Latency:            500 * time.Millisecond,
		},
	}

	winner, _, err := evaluator.Evaluate(context.Background(), "coding", candidates)
	if err != nil {
		t.Fatalf("evaluate: %v", err)
	}
	if winner.AgentID != "peer-fast" {
		t.Fatalf("expected lower latency winner, got %#v", winner)
	}
}
