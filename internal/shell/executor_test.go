package shell

import (
	"context"
	"encoding/json"
	"errors"
	"path/filepath"
	"testing"

	"teamd/internal/contracts"
)

func TestExecutorRunsAllowlistedCommand(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	executor := NewExecutor()
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
				AllowNetwork:   true,
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

	executor := NewExecutor()
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
	executor := NewExecutor()
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

func TestExecutorRequiresIsolationLauncherWhenNetworkDisabled(t *testing.T) {
	t.Parallel()

	executor := &Executor{
		lookupPath: func(string) (string, error) { return "", errors.New("not found") },
		run:        defaultRunCommand,
	}
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
				AllowNetwork:   false,
			},
		},
	}, "shell_exec", map[string]any{"command": "pwd"})
	if err == nil {
		t.Fatal("Execute returned nil error, want isolation launcher failure")
	}
}

func TestExecutorUsesIsolationLauncherWhenNetworkDisabled(t *testing.T) {
	t.Parallel()

	var gotExecutable string
	var gotArgs []string
	executor := &Executor{
		lookupPath: func(name string) (string, error) {
			if name != "unshare" {
				t.Fatalf("lookupPath called with %q, want unshare", name)
			}
			return "/usr/bin/unshare", nil
		},
		run: func(_ context.Context, _ string, executable string, args []string) (runResult, error) {
			gotExecutable = executable
			gotArgs = append([]string{}, args...)
			return runResult{stdout: "ok", exitCode: 0}, nil
		},
	}
	out, err := executor.Execute(contracts.ShellExecutionContract{
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
				AllowNetwork:   false,
			},
		},
	}, "shell_exec", map[string]any{"command": "pwd"})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if gotExecutable != "/usr/bin/unshare" {
		t.Fatalf("executable = %q, want /usr/bin/unshare", gotExecutable)
	}
	if len(gotArgs) < 4 || gotArgs[0] != "--fork" || gotArgs[1] != "--kill-child" || gotArgs[2] != "--net" || gotArgs[3] != "--" {
		t.Fatalf("args prefix = %#v, want unshare network launcher prefix", gotArgs)
	}
	if gotArgs[4] != "pwd" {
		t.Fatalf("isolated command = %#v, want pwd payload", gotArgs)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if payload["status"] != "ok" {
		t.Fatalf("status = %#v, want ok", payload["status"])
	}
}
