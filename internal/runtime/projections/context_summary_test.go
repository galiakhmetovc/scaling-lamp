package projections_test

import (
	"testing"
	"time"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestContextSummaryProjectionTracksLatestRollingSummary(t *testing.T) {
	projection := projections.NewContextSummaryProjection()
	event := eventing.Event{
		ID:            "evt-summary-1",
		Kind:          eventing.EventContextSummaryUpdated,
		OccurredAt:    time.Date(2026, 4, 16, 13, 0, 0, 0, time.UTC),
		AggregateID:   "session-1",
		AggregateType: eventing.AggregateSession,
		Payload: map[string]any{
			"session_id":            "session-1",
			"summary_text":          "Auth middleware was audited and shell approval flow was fixed.",
			"covered_messages":      6,
			"artifact_ref":          "artifact://summary-1",
			"summarization_count":   2,
			"compacted_message_count": 6,
		},
	}
	if err := projection.Apply(event); err != nil {
		t.Fatalf("Apply returned error: %v", err)
	}

	got := projection.SnapshotForSession("session-1")
	if got.SummaryText != "Auth middleware was audited and shell approval flow was fixed." {
		t.Fatalf("summary_text = %q", got.SummaryText)
	}
	if got.CoveredMessages != 6 {
		t.Fatalf("covered_messages = %d, want 6", got.CoveredMessages)
	}
	if got.ArtifactRef != "artifact://summary-1" {
		t.Fatalf("artifact_ref = %q, want artifact://summary-1", got.ArtifactRef)
	}
	if got.SummarizationCount != 2 {
		t.Fatalf("summarization_count = %d, want 2", got.SummarizationCount)
	}
	if got.CompactedMessageCount != 6 {
		t.Fatalf("compacted_message_count = %d, want 6", got.CompactedMessageCount)
	}
	if got.LastGuardPercent != 0 {
		t.Fatalf("last_guard_percent = %d, want 0 after summary refresh", got.LastGuardPercent)
	}
	if err := projection.Apply(eventing.Event{
		ID:            "evt-guard-1",
		Kind:          eventing.EventContextGuardTriggered,
		OccurredAt:    time.Date(2026, 4, 16, 13, 5, 0, 0, time.UTC),
		AggregateID:   "session-1",
		AggregateType: eventing.AggregateSession,
		Payload: map[string]any{
			"session_id":    "session-1",
			"guard_percent": 70,
		},
	}); err != nil {
		t.Fatalf("Apply guard returned error: %v", err)
	}
	got = projection.SnapshotForSession("session-1")
	if got.LastGuardPercent != 70 {
		t.Fatalf("last_guard_percent = %d, want 70", got.LastGuardPercent)
	}
}
