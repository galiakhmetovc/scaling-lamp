package projections_test

import (
	"testing"
	"time"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestSessionCatalogProjectionTracksSessionsAndActivity(t *testing.T) {
	p := projections.NewSessionCatalogProjection()
	createdAt := time.Date(2026, 4, 14, 20, 0, 0, 0, time.UTC)
	messageAt := createdAt.Add(2 * time.Minute)

	if err := p.Apply(eventing.Event{
		Kind:          eventing.EventSessionCreated,
		AggregateID:   "session-1",
		AggregateType: eventing.AggregateSession,
		OccurredAt:    createdAt,
		Payload:       map[string]any{"session_id": "session-1"},
	}); err != nil {
		t.Fatalf("apply session.created: %v", err)
	}
	if err := p.Apply(eventing.Event{
		Kind:          eventing.EventMessageRecorded,
		AggregateID:   "session-1",
		AggregateType: eventing.AggregateSession,
		OccurredAt:    messageAt,
		Payload:       map[string]any{"session_id": "session-1", "role": "user", "content": "ping"},
	}); err != nil {
		t.Fatalf("apply message.recorded: %v", err)
	}

	snapshot := p.Snapshot()
	entry := snapshot.Sessions["session-1"]
	if entry.SessionID != "session-1" {
		t.Fatalf("session id = %q, want session-1", entry.SessionID)
	}
	if !entry.CreatedAt.Equal(createdAt) {
		t.Fatalf("created_at = %v, want %v", entry.CreatedAt, createdAt)
	}
	if !entry.LastActivity.Equal(messageAt) {
		t.Fatalf("last_activity = %v, want %v", entry.LastActivity, messageAt)
	}
	if entry.MessageCount != 1 {
		t.Fatalf("message_count = %d, want 1", entry.MessageCount)
	}
}
