package projections_test

import (
	"testing"
	"time"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestContextBudgetProjectionTracksLastUsageAndSummaryCount(t *testing.T) {
	t.Parallel()

	projection := projections.NewContextBudgetProjection()
	now := time.Date(2026, 4, 16, 12, 40, 0, 0, time.UTC)

	err := projection.Apply(eventing.Event{
		ID:            "evt-run-completed",
		Kind:          eventing.EventRunCompleted,
		OccurredAt:    now,
		AggregateID:   "run-1",
		AggregateType: eventing.AggregateRun,
		Payload: map[string]any{
			"session_id":    "session-1",
			"input_tokens":  120,
			"output_tokens": 48,
			"total_tokens":  168,
		},
	})
	if err != nil {
		t.Fatalf("Apply run completed returned error: %v", err)
	}

	err = projection.Apply(eventing.Event{
		ID:            "evt-summary-count",
		Kind:          eventing.EventMessageRecorded,
		OccurredAt:    now.Add(time.Second),
		AggregateID:   "session-1",
		AggregateType: eventing.AggregateSession,
		Payload: map[string]any{
			"session_id":          "session-1",
			"role":                "assistant",
			"content":             "summary updated",
			"summarization_count": 2,
			"summary_tokens":      90,
		},
	})
	if err != nil {
		t.Fatalf("Apply summary metadata returned error: %v", err)
	}

	got := projection.SnapshotForSession("session-1")
	if got.LastInputTokens != 120 || got.LastOutputTokens != 48 || got.LastTotalTokens != 168 {
		t.Fatalf("last usage = %+v", got)
	}
	if got.SummarizationCount != 2 {
		t.Fatalf("summarization_count = %d, want 2", got.SummarizationCount)
	}
	if got.SummaryTokens != 90 {
		t.Fatalf("summary_tokens = %d, want 90", got.SummaryTokens)
	}
}
