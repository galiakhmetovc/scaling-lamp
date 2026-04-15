package projections_test

import (
	"testing"
	"time"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestShellCommandProjectionTracksLifecycle(t *testing.T) {
	t.Parallel()

	projection := projections.NewShellCommandProjection()
	events := []eventing.Event{
		{
			Kind:          eventing.EventShellCommandApprovalRequested,
			OccurredAt:    time.Date(2026, 4, 15, 18, 29, 0, 0, time.UTC),
			AggregateID:   "cmd-1",
			AggregateType: eventing.AggregateShellCommand,
			Payload: map[string]any{
				"session_id": "session-1",
				"run_id":     "run-1",
				"command":    "go",
			},
		},
		{
			Kind:          eventing.EventShellCommandApprovalGranted,
			OccurredAt:    time.Date(2026, 4, 15, 18, 29, 30, 0, time.UTC),
			AggregateID:   "cmd-1",
			AggregateType: eventing.AggregateShellCommand,
			Payload: map[string]any{
				"session_id": "session-1",
				"run_id":     "run-1",
			},
		},
		{
			Kind:          eventing.EventShellCommandStarted,
			OccurredAt:    time.Date(2026, 4, 15, 18, 30, 0, 0, time.UTC),
			AggregateID:   "cmd-1",
			AggregateType: eventing.AggregateShellCommand,
			Payload: map[string]any{
				"session_id": "session-1",
				"run_id":     "run-1",
				"command":    "go",
			},
		},
		{
			Kind:          eventing.EventShellCommandOutputChunk,
			OccurredAt:    time.Date(2026, 4, 15, 18, 30, 1, 0, time.UTC),
			AggregateID:   "cmd-1",
			AggregateType: eventing.AggregateShellCommand,
			Payload: map[string]any{
				"offset": 1,
				"text":   "line 1",
			},
		},
		{
			Kind:          eventing.EventShellCommandKillRequested,
			OccurredAt:    time.Date(2026, 4, 15, 18, 30, 2, 0, time.UTC),
			AggregateID:   "cmd-1",
			AggregateType: eventing.AggregateShellCommand,
			Payload: map[string]any{
				"status": "killing",
			},
		},
		{
			Kind:          eventing.EventShellCommandCompleted,
			OccurredAt:    time.Date(2026, 4, 15, 18, 30, 3, 0, time.UTC),
			AggregateID:   "cmd-1",
			AggregateType: eventing.AggregateShellCommand,
			Payload: map[string]any{
				"status":    "killed",
				"exit_code": 137,
			},
		},
	}
	for _, event := range events {
		if err := projection.Apply(event); err != nil {
			t.Fatalf("Apply(%s) returned error: %v", event.Kind, err)
		}
	}

	view := projection.Snapshot().Commands["cmd-1"]
	if view.CommandID != "cmd-1" {
		t.Fatalf("command id = %q, want cmd-1", view.CommandID)
	}
	if view.Status != "killed" {
		t.Fatalf("status = %q, want killed", view.Status)
	}
	if view.NextOffset != 1 {
		t.Fatalf("next offset = %d, want 1", view.NextOffset)
	}
	if view.LastChunk != "line 1" {
		t.Fatalf("last chunk = %q, want line 1", view.LastChunk)
	}
	if view.ExitCode == nil || *view.ExitCode != 137 {
		t.Fatalf("exit code = %#v, want 137", view.ExitCode)
	}
	if view.KillPending {
		t.Fatalf("kill pending = true, want false after completion")
	}
}
