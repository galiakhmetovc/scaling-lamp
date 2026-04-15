package runtime

import (
	"context"
	"strings"
	"testing"
	"time"

	"teamd/internal/runtime/eventing"
)

func TestInspectSessionReportsStuckRunsAndShellRecoveryHints(t *testing.T) {
	t.Parallel()

	agent := &Agent{
		EventLog: NewInMemoryEventLog(),
		Now:      func() time.Time { return time.Date(2026, 4, 15, 13, 0, 0, 0, time.UTC) },
		NewID:    func(prefix string) string { return prefix + "-1" },
	}
	events := []eventing.Event{
		{
			ID:            "evt-1",
			Sequence:      1,
			Kind:          eventing.EventSessionCreated,
			OccurredAt:    time.Date(2026, 4, 15, 13, 0, 0, 0, time.UTC),
			AggregateID:   "session-1",
			AggregateType: eventing.AggregateSession,
			Payload:       map[string]any{"session_id": "session-1"},
		},
		{
			ID:            "evt-2",
			Sequence:      2,
			Kind:          eventing.EventRunStarted,
			OccurredAt:    time.Date(2026, 4, 15, 13, 0, 1, 0, time.UTC),
			AggregateID:   "run-1",
			AggregateType: eventing.AggregateRun,
			Payload:       map[string]any{"session_id": "session-1", "prompt": "long task"},
		},
		{
			ID:            "evt-3",
			Sequence:      3,
			Kind:          eventing.EventShellCommandStarted,
			OccurredAt:    time.Date(2026, 4, 15, 13, 0, 2, 0, time.UTC),
			AggregateID:   "cmd-1",
			AggregateType: eventing.AggregateShellCommand,
			Payload: map[string]any{
				"session_id": "session-1",
				"run_id":     "run-1",
				"command_id": "cmd-1",
				"command":    "sleep",
				"status":     "running",
			},
		},
		{
			ID:            "evt-4",
			Sequence:      4,
			Kind:          eventing.EventShellCommandOutputChunk,
			OccurredAt:    time.Date(2026, 4, 15, 13, 0, 3, 0, time.UTC),
			AggregateID:   "cmd-1",
			AggregateType: eventing.AggregateShellCommand,
			Payload: map[string]any{
				"session_id": "session-1",
				"run_id":     "run-1",
				"command_id": "cmd-1",
				"offset":     1,
				"stream":     "stdout",
				"text":       "still running",
			},
		},
	}
	for _, event := range events {
		if err := agent.RecordEvent(context.Background(), event); err != nil {
			t.Fatalf("RecordEvent(%s) returned error: %v", event.Kind, err)
		}
	}

	report, err := agent.InspectSession(context.Background(), "session-1", InspectOptions{})
	if err != nil {
		t.Fatalf("InspectSession returned error: %v", err)
	}
	if report.Diagnostics == nil {
		t.Fatal("Diagnostics = nil, want populated diagnostics summary")
	}
	if len(report.Diagnostics.StuckRuns) != 1 || report.Diagnostics.StuckRuns[0] != "run-1" {
		t.Fatalf("StuckRuns = %#v, want [run-1]", report.Diagnostics.StuckRuns)
	}
	if len(report.Diagnostics.ShellCommands) != 1 {
		t.Fatalf("ShellCommands len = %d, want 1", len(report.Diagnostics.ShellCommands))
	}
	if report.Diagnostics.ShellCommands[0].CommandID != "cmd-1" || report.Diagnostics.ShellCommands[0].Status != "running" {
		t.Fatalf("ShellCommands[0] = %#v, want running cmd-1", report.Diagnostics.ShellCommands[0])
	}
	if joined := strings.Join(report.Diagnostics.RecoveryHints, "\n"); !strings.Contains(joined, "press k to kill") {
		t.Fatalf("RecoveryHints = %#v, want kill hint", report.Diagnostics.RecoveryHints)
	}
}
