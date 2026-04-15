package runtime_test

import (
	"context"
	"encoding/json"
	"os"
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
		ID:               "evt-1",
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       now,
		AggregateID:      "session-1",
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		CorrelationID:    "corr-1",
		CausationID:      "cause-1",
		Source:           "test",
		ActorID:          "agent-1",
		ActorType:        "agent",
		TraceSummary:     "session bootstrap",
		TraceRefs:        []string{"trace/provider-request-1.json"},
		ArtifactRefs:     []string{"artifacts/session-created.txt"},
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
	if got[0].AggregateVersion != 1 {
		t.Fatalf("event aggregate version = %d, want 1", got[0].AggregateVersion)
	}
	if got[0].ActorID != "agent-1" {
		t.Fatalf("event actor id = %q, want %q", got[0].ActorID, "agent-1")
	}
	if got[0].ActorType != "agent" {
		t.Fatalf("event actor type = %q, want %q", got[0].ActorType, "agent")
	}
	if got[0].TraceSummary != "session bootstrap" {
		t.Fatalf("event trace summary = %q, want %q", got[0].TraceSummary, "session bootstrap")
	}
	if len(got[0].TraceRefs) != 1 || got[0].TraceRefs[0] != "trace/provider-request-1.json" {
		t.Fatalf("event trace refs = %#v, want single provider trace ref", got[0].TraceRefs)
	}
	if len(got[0].ArtifactRefs) != 1 || got[0].ArtifactRefs[0] != "artifacts/session-created.txt" {
		t.Fatalf("event artifact refs = %#v, want single artifact ref", got[0].ArtifactRefs)
	}
}

func TestInMemoryEventLogListsAllEventsInSequenceOrder(t *testing.T) {
	t.Parallel()

	log := runtime.NewInMemoryEventLog()
	now := time.Date(2026, 4, 15, 11, 0, 0, 0, time.UTC)
	events := []eventing.Event{
		{
			ID:            "evt-1",
			Kind:          eventing.EventSessionCreated,
			OccurredAt:    now,
			AggregateID:   "session-1",
			AggregateType: eventing.AggregateSession,
		},
		{
			ID:            "evt-2",
			Kind:          eventing.EventRunStarted,
			OccurredAt:    now.Add(time.Second),
			AggregateID:   "run-1",
			AggregateType: eventing.AggregateRun,
		},
	}

	for _, event := range events {
		if err := log.Append(context.Background(), event); err != nil {
			t.Fatalf("Append returned error: %v", err)
		}
	}

	got, err := log.ListAll(context.Background())
	if err != nil {
		t.Fatalf("ListAll returned error: %v", err)
	}
	if len(got) != 2 {
		t.Fatalf("ListAll len = %d, want 2", len(got))
	}
	if got[0].Sequence != 1 || got[1].Sequence != 2 {
		t.Fatalf("ListAll sequences = [%d %d], want [1 2]", got[0].Sequence, got[1].Sequence)
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
		ID:               "evt-1",
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       now,
		AggregateID:      "session-1",
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		CorrelationID:    "corr-1",
		CausationID:      "cause-1",
		Source:           "test",
		ActorID:          "agent-1",
		ActorType:        "agent",
		TraceSummary:     "session bootstrap",
		TraceRefs:        []string{"trace/provider-request-1.json"},
		ArtifactRefs:     []string{"artifacts/session-created.txt"},
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
	if got[0].AggregateVersion != 1 {
		t.Fatalf("event aggregate version = %d, want 1", got[0].AggregateVersion)
	}
	if got[0].ActorID != "agent-1" {
		t.Fatalf("event actor id = %q, want %q", got[0].ActorID, "agent-1")
	}
	if got[0].ActorType != "agent" {
		t.Fatalf("event actor type = %q, want %q", got[0].ActorType, "agent")
	}
	if got[0].TraceSummary != "session bootstrap" {
		t.Fatalf("event trace summary = %q, want %q", got[0].TraceSummary, "session bootstrap")
	}
	if len(got[0].TraceRefs) != 1 || got[0].TraceRefs[0] != "trace/provider-request-1.json" {
		t.Fatalf("event trace refs = %#v, want single provider trace ref", got[0].TraceRefs)
	}
	if len(got[0].ArtifactRefs) != 1 || got[0].ArtifactRefs[0] != "artifacts/session-created.txt" {
		t.Fatalf("event artifact refs = %#v, want single artifact ref", got[0].ArtifactRefs)
	}
}

func TestFileEventLogWritesTimestampAlias(t *testing.T) {
	t.Parallel()

	path := filepath.Join(t.TempDir(), "events.jsonl")
	now := time.Date(2026, 4, 14, 15, 0, 0, 123456000, time.UTC)

	log, err := runtime.NewFileEventLog(path)
	if err != nil {
		t.Fatalf("NewFileEventLog returned error: %v", err)
	}

	if err := log.Append(context.Background(), eventing.Event{
		ID:               "evt-1",
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       now,
		AggregateID:      "session-1",
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
	}); err != nil {
		t.Fatalf("Append returned error: %v", err)
	}

	body, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("ReadFile returned error: %v", err)
	}

	var raw map[string]any
	if err := json.Unmarshal(body[:len(body)-1], &raw); err != nil {
		t.Fatalf("Unmarshal returned error: %v", err)
	}

	if raw["timestamp"] != now.Format(time.RFC3339Nano) {
		t.Fatalf("timestamp = %#v, want %q", raw["timestamp"], now.Format(time.RFC3339Nano))
	}
	if raw["OccurredAt"] != now.Format(time.RFC3339Nano) {
		t.Fatalf("OccurredAt = %#v, want %q", raw["OccurredAt"], now.Format(time.RFC3339Nano))
	}
}

func TestFileEventLogListsAllEventsInSequenceOrder(t *testing.T) {
	t.Parallel()

	path := filepath.Join(t.TempDir(), "events.jsonl")
	log, err := runtime.NewFileEventLog(path)
	if err != nil {
		t.Fatalf("NewFileEventLog returned error: %v", err)
	}
	now := time.Date(2026, 4, 15, 12, 0, 0, 0, time.UTC)
	events := []eventing.Event{
		{
			ID:            "evt-1",
			Kind:          eventing.EventSessionCreated,
			OccurredAt:    now,
			AggregateID:   "session-1",
			AggregateType: eventing.AggregateSession,
		},
		{
			ID:            "evt-2",
			Kind:          eventing.EventRunStarted,
			OccurredAt:    now.Add(time.Second),
			AggregateID:   "run-1",
			AggregateType: eventing.AggregateRun,
		},
	}
	for _, event := range events {
		if err := log.Append(context.Background(), event); err != nil {
			t.Fatalf("Append returned error: %v", err)
		}
	}

	got, err := log.ListAll(context.Background())
	if err != nil {
		t.Fatalf("ListAll returned error: %v", err)
	}
	if len(got) != 2 {
		t.Fatalf("ListAll len = %d, want 2", len(got))
	}
	if got[0].Sequence != 1 || got[1].Sequence != 2 {
		t.Fatalf("ListAll sequences = [%d %d], want [1 2]", got[0].Sequence, got[1].Sequence)
	}
}
