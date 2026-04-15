package runtime

import (
	"context"
	"encoding/json"
	"testing"
	"time"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
)

func TestAgentPendingShellApprovalsRecoverFromProjection(t *testing.T) {
	t.Parallel()

	agent := &Agent{
		Config:       config.AgentConfig{ID: "agent-test"},
		Contracts:    shellContractsForRecoveryTest(t.TempDir()),
		ShellRuntime: shell.NewExecutor(),
		EventLog:     NewInMemoryEventLog(),
		Projections:  []projections.Projection{projections.NewShellCommandProjection()},
		Now:          func() time.Time { return time.Date(2026, 4, 15, 20, 0, 0, 0, time.UTC) },
		NewID:        func(prefix string) string { return prefix + "-1" },
	}
	if err := agent.RecordEvent(context.Background(), eventing.Event{
		Kind:          eventing.EventShellCommandApprovalRequested,
		AggregateID:   "cmd-1",
		AggregateType: eventing.AggregateShellCommand,
		Payload: map[string]any{
			"session_id":       "session-1",
			"run_id":           "run-1",
			"approval_id":      "approval-1",
			"tool_name":        "shell_start",
			"command":          "go",
			"args":             []string{"env", "GOROOT"},
			"cwd":              t.TempDir(),
			"approval_message": "approve go env GOROOT",
		},
	}); err != nil {
		t.Fatalf("RecordEvent returned error: %v", err)
	}

	approvals := agent.PendingShellApprovals("session-1")
	if len(approvals) != 1 {
		t.Fatalf("PendingShellApprovals = %d, want 1", len(approvals))
	}
	if approvals[0].ApprovalID != "approval-1" {
		t.Fatalf("approval id = %q, want approval-1", approvals[0].ApprovalID)
	}
	if approvals[0].Message != "approve go env GOROOT" {
		t.Fatalf("approval message = %q, want approve go env GOROOT", approvals[0].Message)
	}
}

func TestAgentApproveShellCommandRecoversPendingApproval(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	agent := &Agent{
		Config:       config.AgentConfig{ID: "agent-test"},
		Contracts:    shellContractsForRecoveryTest(root),
		ShellRuntime: shell.NewExecutor(),
		EventLog:     NewInMemoryEventLog(),
		Projections:  []projections.Projection{projections.NewShellCommandProjection()},
		Now:          func() time.Time { return time.Date(2026, 4, 15, 20, 5, 0, 0, time.UTC) },
		NewID:        func(prefix string) string { return prefix + "-1" },
	}
	if err := agent.RecordEvent(context.Background(), eventing.Event{
		Kind:          eventing.EventShellCommandApprovalRequested,
		AggregateID:   "cmd-1",
		AggregateType: eventing.AggregateShellCommand,
		Payload: map[string]any{
			"session_id":       "session-1",
			"run_id":           "run-1",
			"approval_id":      "approval-1",
			"tool_name":        "shell_start",
			"command":          "go",
			"args":             []string{"env", "GOROOT"},
			"cwd":              root,
			"approval_message": "approve go env GOROOT",
		},
	}); err != nil {
		t.Fatalf("RecordEvent returned error: %v", err)
	}

	out, err := agent.ApproveShellCommand(context.Background(), "approval-1")
	if err != nil {
		t.Fatalf("ApproveShellCommand returned error: %v", err)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal approval result: %v", err)
	}
	if payload["status"] != "running" {
		t.Fatalf("status = %#v, want running", payload["status"])
	}
	commandID, _ := payload["command_id"].(string)
	if commandID != "cmd-1" {
		t.Fatalf("command_id = %q, want cmd-1", commandID)
	}

	events, err := agent.EventLog.ListByAggregate(context.Background(), eventing.AggregateShellCommand, "cmd-1")
	if err != nil {
		t.Fatalf("ListByAggregate returned error: %v", err)
	}
	if len(events) < 2 {
		t.Fatalf("shell events = %d, want approval + started", len(events))
	}
	if events[1].Kind != eventing.EventShellCommandApprovalGranted {
		t.Fatalf("event[1] = %q, want approval granted", events[1].Kind)
	}
	if events[2].Kind != eventing.EventShellCommandStarted {
		t.Fatalf("event[2] = %q, want started", events[2].Kind)
	}
}

func shellContractsForRecoveryTest(root string) contracts.ResolvedContracts {
	return contracts.ResolvedContracts{
		ShellExecution: contracts.ShellExecutionContract{
			Command: contracts.ShellCommandPolicy{
				Enabled:  true,
				Strategy: "static_allowlist",
				Params:   contracts.ShellCommandParams{AllowedCommands: []string{"go"}},
			},
			Approval: contracts.ShellApprovalPolicy{
				Enabled:  true,
				Strategy: "always_require",
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
