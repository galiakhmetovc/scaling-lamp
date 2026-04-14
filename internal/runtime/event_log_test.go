package runtime_test

import (
	"context"
	"path/filepath"
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
		CorrelationID: "corr-1",
		CausationID:   "cause-1",
		Source:        "test",
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
	if got[0].Sequence != 1 {
		t.Fatalf("event sequence = %d, want 1", got[0].Sequence)
	}
	if got[0].CorrelationID != "corr-1" {
		t.Fatalf("event correlation = %q, want %q", got[0].CorrelationID, "corr-1")
	}
	if got[0].CausationID != "cause-1" {
		t.Fatalf("event causation = %q, want %q", got[0].CausationID, "cause-1")
	}
	if got[0].Source != "test" {
		t.Fatalf("event source = %q, want %q", got[0].Source, "test")
	}
}

func TestFileEventLogPersistsEventsAcrossReopen(t *testing.T) {
	t.Parallel()

	path := filepath.Join(t.TempDir(), "events.jsonl")
	now := time.Date(2026, 4, 14, 11, 50, 0, 0, time.UTC)

	log, err := runtime.NewFileEventLog(path)
	if err != nil {
		t.Fatalf("NewFileEventLog returned error: %v", err)
	}

	event := eventing.Event{
		ID:            "evt-1",
		Kind:          eventing.EventSessionCreated,
		OccurredAt:    now,
		AggregateID:   "session-1",
		AggregateType: eventing.AggregateSession,
		CorrelationID: "corr-1",
		CausationID:   "cause-1",
		Source:        "test",
		Payload: map[string]any{
			"session_id": "session-1",
		},
	}

	if err := log.Append(context.Background(), event); err != nil {
		t.Fatalf("Append returned error: %v", err)
	}

	reopened, err := runtime.NewFileEventLog(path)
	if err != nil {
		t.Fatalf("NewFileEventLog reopen returned error: %v", err)
	}

	got, err := reopened.ListByAggregate(context.Background(), eventing.AggregateSession, "session-1")
	if err != nil {
		t.Fatalf("ListByAggregate returned error: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("ListByAggregate len = %d, want 1", len(got))
	}
	if got[0].Sequence != 1 {
		t.Fatalf("event sequence = %d, want 1", got[0].Sequence)
	}
	if got[0].ID != "evt-1" {
		t.Fatalf("event ID = %q, want %q", got[0].ID, "evt-1")
	}
}
