package projections_test

import (
	"testing"
	"time"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestSessionProjectionAppliesSessionCreatedEvent(t *testing.T) {
	t.Parallel()

	p := projections.NewSessionProjection()
	now := time.Date(2026, 4, 14, 8, 20, 0, 0, time.UTC)

	err := p.Apply(eventing.Event{
		ID:            "evt-1",
		Kind:          eventing.EventSessionCreated,
		OccurredAt:    now,
		AggregateID:   "session-1",
		AggregateType: eventing.AggregateSession,
	})
	if err != nil {
		t.Fatalf("Apply returned error: %v", err)
	}

	got := p.Snapshot()
	if got.SessionID != "session-1" {
		t.Fatalf("SessionID = %q, want %q", got.SessionID, "session-1")
	}
	if !got.CreatedAt.Equal(now) {
		t.Fatalf("CreatedAt = %v, want %v", got.CreatedAt, now)
	}
}
