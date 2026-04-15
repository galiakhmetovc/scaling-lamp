package runtime

import (
	"encoding/json"
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/provider"
	"teamd/internal/shell"
)

func TestExecuteToolCommandMaintainsShellLifecycleAcrossCalls(t *testing.T) {
	t.Parallel()

	root := t.TempDir()
	agent := &Agent{
		Contracts:    shellContractsForLifecycleTest(root),
		ShellRuntime: shell.NewExecutor(),
	}

	_, startText, err := agent.executeToolCommand("session-1", nil, nil, nil, agent.ShellRuntime, "test", provider.ToolCall{
		Name: "shell_start",
		Arguments: map[string]any{
			"command": "go",
			"args":    []any{"env", "GOROOT"},
		},
	})
	if err != nil {
		t.Fatalf("shell_start returned error: %v", err)
	}
	commandID := decodeJSONStringField(t, startText, "command_id")
	if commandID == "" {
		t.Fatalf("command_id missing from %s", startText)
	}

	_, pollText, err := agent.executeToolCommand("session-1", nil, nil, nil, agent.ShellRuntime, "test", provider.ToolCall{
		Name: "shell_poll",
		Arguments: map[string]any{
			"command_id":   commandID,
			"after_offset": 0,
		},
	})
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

	_, killText, err := agent.executeToolCommand("session-1", nil, nil, nil, agent.ShellRuntime, "test", provider.ToolCall{
		Name: "shell_kill",
		Arguments: map[string]any{
			"command_id": commandID,
		},
	})
	if err != nil {
		t.Fatalf("shell_kill returned error: %v", err)
	}
	if status := decodeJSONStringField(t, killText, "status"); status == "" {
		t.Fatalf("kill status missing from %s", killText)
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
