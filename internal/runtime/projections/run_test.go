package projections_test

import (
	"testing"
	"time"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestRunProjectionAppliesRunStartedEvent(t *testing.T) {
	t.Parallel()

	p := projections.NewRunProjection()
	now := time.Date(2026, 4, 14, 8, 25, 0, 0, time.UTC)

	err := p.Apply(eventing.Event{
		ID:            "evt-1",
		Kind:          eventing.EventRunStarted,
		OccurredAt:    now,
		AggregateID:   "run-1",
		AggregateType: eventing.AggregateRun,
		Payload: map[string]any{
			"session_id": "session-1",
		},
	})
	if err != nil {
		t.Fatalf("Apply returned error: %v", err)
	}

	got := p.Snapshot()
	if got.RunID != "run-1" {
		t.Fatalf("RunID = %q, want %q", got.RunID, "run-1")
	}
	if got.SessionID != "session-1" {
		t.Fatalf("SessionID = %q, want %q", got.SessionID, "session-1")
	}
	if got.Status != projections.RunStatusRunning {
		t.Fatalf("Status = %q, want %q", got.Status, projections.RunStatusRunning)
	}
}
