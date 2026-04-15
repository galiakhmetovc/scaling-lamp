package projections

import (
	"testing"
	"time"

	"teamd/internal/runtime/eventing"
)

func TestDelegateProjectionTracksLifecycleAndHandoff(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 4, 15, 15, 0, 0, 0, time.UTC)
	projection := NewDelegateProjection()

	events := []eventing.Event{
		{
			Kind:          eventing.EventDelegateSpawned,
			OccurredAt:    now,
			AggregateID:   "delegate-1",
			AggregateType: eventing.AggregateDelegate,
			Payload: map[string]any{
				"backend":          "local_worker",
				"owner_session_id": "session-owner-1",
				"delegate_session_id": "session-delegate-1",
				"policy_snapshot": map[string]any{
					"backend": "local_worker",
				},
			},
		},
		{
			Kind:          eventing.EventDelegateRunStarted,
			OccurredAt:    now.Add(time.Second),
			AggregateID:   "delegate-1",
			AggregateType: eventing.AggregateDelegate,
			Payload: map[string]any{
				"delegate_run_id": "delegate-run-1",
			},
		},
		{
			Kind:          eventing.EventDelegateCompleted,
			OccurredAt:    now.Add(2 * time.Second),
			AggregateID:   "delegate-1",
			AggregateType: eventing.AggregateDelegate,
			Payload: map[string]any{
				"delegate_run_id": "delegate-run-1",
			},
		},
		{
			Kind:          eventing.EventDelegateHandoffCreated,
			OccurredAt:    now.Add(3 * time.Second),
			AggregateID:   "delegate-1",
			AggregateType: eventing.AggregateDelegate,
			Payload: map[string]any{
				"backend":              "local_worker",
				"delegate_run_id":      "delegate-run-1",
				"summary":              "done",
				"recommended_next_step": "review results",
				"created_at":           now.Add(3 * time.Second).Format(time.RFC3339Nano),
			},
		},
	}

	for _, event := range events {
		if err := projection.Apply(event); err != nil {
			t.Fatalf("Apply(%q) returned error: %v", event.Kind, err)
		}
	}

	snapshot := projection.Snapshot()
	view, ok := snapshot.Delegates["delegate-1"]
	if !ok {
		t.Fatal("delegate view missing")
	}
	if view.DelegateID != "delegate-1" {
		t.Fatalf("delegate id = %q, want delegate-1", view.DelegateID)
	}
	if view.Backend != "local_worker" {
		t.Fatalf("backend = %q, want local_worker", view.Backend)
	}
	if view.OwnerSessionID != "session-owner-1" {
		t.Fatalf("owner session = %q, want session-owner-1", view.OwnerSessionID)
	}
	if view.DelegateSessionID != "session-delegate-1" {
		t.Fatalf("delegate session = %q, want session-delegate-1", view.DelegateSessionID)
	}
	if view.Status != "idle" {
		t.Fatalf("status = %q, want idle", view.Status)
	}
	if view.LastRunID != "delegate-run-1" {
		t.Fatalf("last run id = %q, want delegate-run-1", view.LastRunID)
	}

	handoff, ok := snapshot.Handoffs["delegate-1"]
	if !ok {
		t.Fatal("handoff missing")
	}
	if handoff.Summary != "done" {
		t.Fatalf("handoff summary = %q, want done", handoff.Summary)
	}
	if handoff.RecommendedNextStep != "review results" {
		t.Fatalf("recommended next step = %q, want review results", handoff.RecommendedNextStep)
	}
}
