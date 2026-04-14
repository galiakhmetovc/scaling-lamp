package shell_test

import (
	"encoding/json"
	"path/filepath"
	"testing"

	"teamd/internal/contracts"
	"teamd/internal/shell"
)

func TestExecutorRunsAllowlistedCommand(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	executor := shell.NewExecutor()
	out, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCommandParams{
				AllowedCommands: []string{"pwd"},
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
				Cwd:            dir,
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   false,
			},
		},
	}, "shell_exec", map[string]any{
		"command": "pwd",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if payload["status"] != "ok" {
		t.Fatalf("status = %#v, want ok", payload["status"])
	}
	if payload["exit_code"] != float64(0) {
		t.Fatalf("exit_code = %#v, want 0", payload["exit_code"])
	}
}

func TestExecutorRejectsCommandOutsideAllowlist(t *testing.T) {
	t.Parallel()

	executor := shell.NewExecutor()
	_, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCommandParams{
				AllowedCommands: []string{"pwd"},
			},
		},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
			},
		},
	}, "shell_exec", map[string]any{
		"command": "ls",
	})
	if err == nil {
		t.Fatal("Execute returned nil error, want allowlist failure")
	}
}

func TestExecutorRejectsCwdOutsideRuntimeScope(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	executor := shell.NewExecutor()
	_, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCommandParams{
				AllowedCommands: []string{"pwd"},
			},
		},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            filepath.Join(dir, "workspace"),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
			},
		},
	}, "shell_exec", map[string]any{
		"command": "pwd",
		"cwd":     dir,
	})
	if err == nil {
		t.Fatal("Execute returned nil error, want cwd scope failure")
	}
}
