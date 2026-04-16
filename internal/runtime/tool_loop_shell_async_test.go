package runtime

import (
	"context"
	"encoding/json"
	"testing"
	"time"

	"teamd/internal/contracts"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
)

func TestShellLifecyclePersistsEventsAcrossCalls(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	agent := &Agent{
		Contracts:    shellContractsForLifecycleTest(root),
		ShellRuntime: shell.NewExecutor(),
		EventLog:     NewInMemoryEventLog(),
		Projections:  []projections.Projection{projections.NewShellCommandProjection()},
		Now:          func() time.Time { return time.Date(2026, 4, 15, 18, 0, 0, 0, time.UTC) },
		NewID: func(prefix string) string {
			return prefix + "-1"
		},
	}

	startText, err := agent.ShellRuntime.ExecuteWithMeta(context.Background(), agent.Contracts.ShellExecution, "shell_start", map[string]any{
		"command": "go",
		"args":    []any{"env", "GOROOT"},
	}, shell.ExecutionMeta{
		SessionID:   "session-1",
		RunID:       "run-1",
		Source:      "test",
		ActorID:     "agent-test",
		ActorType:   "agent",
		RecordEvent: agent.RecordEvent,
		Now:         agent.now,
		NewID:       agent.newID,
	})
	if err != nil {
		t.Fatalf("shell_start returned error: %v", err)
	}
	commandID := decodeJSONStringField(t, startText, "command_id")
	if commandID == "" {
		t.Fatalf("command_id missing from %s", startText)
	}

	deadline := time.Now().Add(2 * time.Second)
	for {
		pollText, err := agent.ShellRuntime.ExecuteWithMeta(context.Background(), agent.Contracts.ShellExecution, "shell_poll", map[string]any{
			"command_id":   commandID,
			"after_offset": 0,
		}, shell.ExecutionMeta{})
		if err != nil {
			t.Fatalf("shell_poll returned error: %v", err)
		}
		var pollPayload map[string]any
		if err := json.Unmarshal([]byte(pollText), &pollPayload); err != nil {
			t.Fatalf("unmarshal poll result: %v", err)
		}
		if pollPayload["status"] == "" {
			t.Fatalf("poll status missing from %s", pollText)
		}
		if status, _ := pollPayload["status"].(string); status == "completed" {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("shell command did not complete before deadline; last poll = %s", pollText)
		}
		time.Sleep(20 * time.Millisecond)
	}

	events, err := agent.EventLog.ListByAggregate(context.Background(), eventing.AggregateShellCommand, commandID)
	if err != nil {
		t.Fatalf("ListByAggregate returned error: %v", err)
	}
	if len(events) == 0 {
		t.Fatal("expected persisted shell command events")
	}
	if events[0].Kind != eventing.EventShellCommandStarted {
		t.Fatalf("first shell event kind = %q, want %q", events[0].Kind, eventing.EventShellCommandStarted)
	}

	projection := agent.Projections[0].(*projections.ShellCommandProjection)
	view := projection.Snapshot().Commands[commandID]
	if view.CommandID != commandID {
		t.Fatalf("projection command id = %q, want %q", view.CommandID, commandID)
	}
	if view.Status != "completed" {
		t.Fatalf("projection status = %q, want completed", view.Status)
	}
	if view.NextOffset == 0 {
		t.Fatalf("projection next offset = %d, want > 0", view.NextOffset)
	}
	if view.LastChunk == "" {
		t.Fatalf("projection last chunk missing")
	}
	if view.ExitCode == nil || *view.ExitCode != 0 {
		t.Fatalf("projection exit code = %#v, want 0", view.ExitCode)
	}
}

func shellContractsForLifecycleTest(root string) contracts.ResolvedContracts {
	return contracts.ResolvedContracts{
		ShellExecution: contracts.ShellExecutionContract{
			Command: contracts.ShellCommandPolicy{
				Enabled:  true,
				Strategy: "static_allowlist",
				Params: contracts.ShellCommandParams{
					AllowedCommands: []string{"go"},
				},
			},
			Approval: contracts.ShellApprovalPolicy{
				Enabled:  true,
				Strategy: "always_allow",
			},
			Runtime: contracts.ShellRuntimePolicy{
				Enabled:  true,
				Strategy: "workspace_write",
				Params: contracts.ShellRuntimeParams{
					Cwd:            root,
					Timeout:        "5s",
					MaxOutputBytes: 4096,
					AllowNetwork:   true,
				},
			},
		},
	}
}

func decodeJSONStringField(t *testing.T, body, field string) string {
	t.Helper()
	var payload map[string]any
	if err := json.Unmarshal([]byte(body), &payload); err != nil {
		t.Fatalf("unmarshal %s: %v", body, err)
	}
	value, _ := payload[field].(string)
	return value
}
