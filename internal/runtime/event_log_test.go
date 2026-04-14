package runtime_test

import (
	"context"
	"testing"
	"time"

	"teamd/internal/runtime"
	"teamd/internal/runtime/eventing"
)

func TestInMemoryEventLogAppendsAndListsEvents(t *testing.T) {
	t.Parallel()

	log := runtime.NewInMemoryEventLog()
	now := time.Date(2026, 4, 14, 8, 15, 0, 0, time.UTC)

	event := eventing.Event{
		ID:            "evt-1",
		Kind:          eventing.EventSessionCreated,
		OccurredAt:    now,
		AggregateID:   "session-1",
		AggregateType: eventing.AggregateSession,
		Payload: map[string]any{
			"session_id": "session-1",
		},
	}

	if err := log.Append(context.Background(), event); err != nil {
		t.Fatalf("Append returned error: %v", err)
	}

	got, err := log.ListByAggregate(context.Background(), eventing.AggregateSession, "session-1")
	if err != nil {
		t.Fatalf("ListByAggregate returned error: %v", err)
	}

	if len(got) != 1 {
		t.Fatalf("ListByAggregate len = %d, want 1", len(got))
	}
	if got[0].ID != "evt-1" {
		t.Fatalf("event ID = %q, want %q", got[0].ID, "evt-1")
	}
	if got[0].Kind != eventing.EventSessionCreated {
		t.Fatalf("event kind = %q, want %q", got[0].Kind, eventing.EventSessionCreated)
	}
}
