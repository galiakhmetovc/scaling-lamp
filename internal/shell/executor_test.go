package shell

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"

	"teamd/internal/contracts"
	"teamd/internal/runtime/eventing"
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

func TestExecutorRoutesCommandOutsideAllowlistToApproval(t *testing.T) {
	t.Parallel()

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
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
			},
		},
	}, "shell_exec", map[string]any{
		"command": "ls",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if decodeField(t, out, "status") != "approval_pending" {
		t.Fatalf("status = %s, want approval_pending", out)
	}
}

func TestExecutorApproveRunsShellSnippetCommand(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	executor := NewExecutor()
	out, err := executor.ExecuteWithMeta(context.Background(), contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCommandParams{
				AllowedCommands: []string{"pwd"},
			},
		},
		Approval: contracts.ShellApprovalPolicy{
			Enabled:  true,
			Strategy: "always_require",
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
		"command": `cd "` + dir + `" && pwd`,
	}, ExecutionMeta{})
	if err != nil {
		t.Fatalf("ExecuteWithMeta returned error: %v", err)
	}
	if decodeField(t, out, "status") != "approval_pending" {
		t.Fatalf("status = %s, want approval_pending", out)
	}
	approvalID := decodeField(t, out, "approval_id")
	if approvalID == "" {
		t.Fatalf("approval_id missing from %s", out)
	}

	result, err := executor.Approve(context.Background(), approvalID)
	if err != nil {
		t.Fatalf("Approve returned error: %v", err)
	}
	if decodeField(t, result, "status") != "ok" {
		t.Fatalf("approved result = %s, want ok", result)
	}
	if got := strings.TrimSpace(decodeField(t, result, "stdout")); got != dir {
		t.Fatalf("stdout = %q, want %q", got, dir)
	}
}

func TestExecutorTreatsCommandPlusShellOperatorArgsAsShellSnippet(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	executor := &Executor{
		goos: "linux",
		run: func(_ context.Context, cwd, executable string, args []string) (runResult, error) {
			if executable != "sh" {
				t.Fatalf("executable = %q, want sh", executable)
			}
			if len(args) != 2 || args[0] != "-lc" {
				t.Fatalf("args = %#v, want sh -lc invocation", args)
			}
			if want := "cd " + dir + " && pwd"; args[1] != want {
				t.Fatalf("snippet = %q, want %q", args[1], want)
			}
			return runResult{stdout: dir + "\n", exitCode: 0}, nil
		},
		lookupPath: func(file string) (string, error) { return file, nil },
		commands:   map[string]*activeCommand{},
		completed:  map[string]*activeCommand{},
		approvals:  map[string]*pendingApproval{},
	}
	out, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCommandParams{
				AllowedCommands: []string{"cd"},
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
		"command": "cd",
		"args":    []any{dir, "&&", "pwd"},
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if got := strings.TrimSpace(decodeField(t, out, "stdout")); got != dir {
		t.Fatalf("stdout = %q, want %q", got, dir)
	}
}

func TestRecoverApprovalUsesPersistedInvocation(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	executor := &Executor{
		goos: "linux",
		run: func(_ context.Context, cwd, executable string, args []string) (runResult, error) {
			if executable != "sh" {
				t.Fatalf("executable = %q, want sh", executable)
			}
			if len(args) != 2 || args[0] != "-lc" {
				t.Fatalf("args = %#v, want sh -lc invocation", args)
			}
			return runResult{stdout: dir + "\n", exitCode: 0}, nil
		},
		lookupPath: func(file string) (string, error) { return file, nil },
		commands:   map[string]*activeCommand{},
		completed:  map[string]*activeCommand{},
		approvals:  map[string]*pendingApproval{},
	}
	view := PendingApprovalView{
		ApprovalID:           "approval-1",
		CommandID:            "cmd-1",
		SessionID:            "session-1",
		RunID:                "run-1",
		OccurredAt:           time.Now().UTC(),
		ToolName:             "shell_exec",
		Command:              "cd",
		Args:                 []string{dir, "&&", "pwd"},
		Cwd:                  dir,
		Message:              "approval needed",
		InvocationExecutable: "sh",
		InvocationArgs:       []string{"-lc", "cd " + dir + " && pwd"},
	}
	contract := contracts.ShellExecutionContract{
		Command:  contracts.ShellCommandPolicy{Enabled: true, Strategy: "static_allowlist", Params: contracts.ShellCommandParams{AllowedCommands: []string{"cd"}}},
		Approval: contracts.ShellApprovalPolicy{Enabled: true, Strategy: "always_require"},
		Runtime:  contracts.ShellRuntimePolicy{Enabled: true, Strategy: "workspace_write", Params: contracts.ShellRuntimeParams{Cwd: dir, Timeout: "5s", MaxOutputBytes: 4096, AllowNetwork: true}},
	}
	if err := executor.RecoverApproval(contract, view, ExecutionMeta{}); err != nil {
		t.Fatalf("RecoverApproval returned error: %v", err)
	}
	out, err := executor.Approve(context.Background(), "approval-1")
	if err != nil {
		t.Fatalf("Approve returned error: %v", err)
	}
	if got := strings.TrimSpace(decodeField(t, out, "stdout")); got != dir {
		t.Fatalf("stdout = %q, want %q", got, dir)
	}
}

func TestExecutorUsesMetaNewIDForShellCommandAndApprovalIDs(t *testing.T) {
	t.Parallel()

	nextID := uint64(0)
	newID := func(prefix string) string {
		nextID++
		return prefix + "-" + strconv.FormatUint(nextID, 10)
	}
	contract := contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCommandParams{
				AllowedCommands: []string{"pwd"},
			},
		},
		Approval: contracts.ShellApprovalPolicy{
			Enabled:  true,
			Strategy: "always_require",
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
	}

	first := NewExecutor()
	firstOut, err := first.ExecuteWithMeta(context.Background(), contract, "shell_exec", map[string]any{
		"command": "pwd",
	}, ExecutionMeta{NewID: newID})
	if err != nil {
		t.Fatalf("first ExecuteWithMeta returned error: %v", err)
	}

	second := NewExecutor()
	secondOut, err := second.ExecuteWithMeta(context.Background(), contract, "shell_exec", map[string]any{
		"command": "pwd",
	}, ExecutionMeta{NewID: newID})
	if err != nil {
		t.Fatalf("second ExecuteWithMeta returned error: %v", err)
	}

	if got, want := decodeField(t, firstOut, "command_id"), "cmd-1"; got != want {
		t.Fatalf("first command_id = %q, want %q", got, want)
	}
	if got, want := decodeField(t, firstOut, "approval_id"), "approval-2"; got != want {
		t.Fatalf("first approval_id = %q, want %q", got, want)
	}
	if got, want := decodeField(t, secondOut, "command_id"), "cmd-3"; got != want {
		t.Fatalf("second command_id = %q, want %q", got, want)
	}
	if got, want := decodeField(t, secondOut, "approval_id"), "approval-4"; got != want {
		t.Fatalf("second approval_id = %q, want %q", got, want)
	}
}

func TestExecutorRejectsCommandByArgumentRule(t *testing.T) {
	t.Parallel()

	executor := NewExecutor()
	_, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCommandParams{
				AllowedCommands: []string{"go"},
				CommandRules: []contracts.ShellCommandRule{
					{
						Command:            "go",
						DeniedArgPatterns:  []string{"env -w"},
						AllowedArgPrefixes: []string{"test ", "env ", "version"},
					},
				},
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
		"command": "go",
		"args":    []any{"env", "-w", "GOMODCACHE=/tmp/cache"},
	})
	if err == nil {
		t.Fatal("Execute returned nil error, want argument rule denial")
	}
}

func TestExecutorAllowsCommandWhenArgumentsMatchRule(t *testing.T) {
	t.Parallel()

	executor := NewExecutor()
	out, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCommandParams{
				AllowedCommands: []string{"git"},
				CommandRules: []contracts.ShellCommandRule{
					{
						Command:            "git",
						AllowedArgPrefixes: []string{"status", "diff", "log"},
						DeniedArgPatterns:  []string{"push", "reset --hard"},
					},
				},
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
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}, "shell_exec", map[string]any{
		"command": "git",
		"args":    []any{"status", "--short"},
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if payload["status"] != "ok" && payload["status"] != "error" {
		t.Fatalf("status = %#v, want shell_exec payload", payload["status"])
	}
}

func TestExecutorRoutesCommandOutsideAllowedRuleSetToApproval(t *testing.T) {
	t.Parallel()

	executor := NewExecutor()
	out, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCommandParams{
				AllowedCommands: []string{"git"},
				CommandRules: []contracts.ShellCommandRule{
					{
						Command:            "git",
						AllowedArgPrefixes: []string{"status", "diff", "log"},
						DeniedArgPatterns:  []string{"push", "reset --hard"},
					},
				},
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
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}, "shell_exec", map[string]any{
		"command": "git",
		"args":    []any{"commit", "-m", "msg"},
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if decodeField(t, out, "status") != "approval_pending" {
		t.Fatalf("status = %s, want approval_pending", out)
	}
}

func TestExecutorAllowsWindowsShellLaunchersWhenAllowlisted(t *testing.T) {
	t.Parallel()

	executor := &Executor{
		run: func(_ context.Context, _ string, executable string, args []string) (runResult, error) {
			return runResult{
				stdout:   executable + " " + strings.Join(args, " "),
				exitCode: 0,
			}, nil
		},
		lookupPath: func(file string) (string, error) { return file, nil },
		start:      defaultStartCommand,
		goos:       "windows",
		commands:   map[string]*activeCommand{},
		completed:  map[string]*activeCommand{},
		approvals:  map[string]*pendingApproval{},
	}
	out, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCommandParams{
				AllowedCommands: []string{"powershell", "pwsh", "cmd"},
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
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}, "shell_exec", map[string]any{
		"command": "pwsh",
		"args":    []any{"-NoProfile", "-Command", "Invoke-WebRequest", "https://example.com"},
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
	if payload["command"] != "pwsh" {
		t.Fatalf("command = %#v, want pwsh", payload["command"])
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

func TestExecutorRunsWindowsBuiltinViaCmdLauncher(t *testing.T) {
	t.Parallel()

	var gotExecutable string
	var gotArgs []string
	executor := &Executor{
		goos: "windows",
		run: func(_ context.Context, _ string, executable string, args []string) (runResult, error) {
			gotExecutable = executable
			gotArgs = append([]string{}, args...)
			return runResult{stdout: "Hello", exitCode: 0}, nil
		},
	}

	out, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCommandParams{
				AllowedCommands: []string{"echo"},
			},
		},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}, "shell_exec", map[string]any{
		"command": "echo",
		"args":    []any{"Hello"},
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if gotExecutable != "cmd" {
		t.Fatalf("executable = %q, want cmd", gotExecutable)
	}
	if len(gotArgs) != 3 || gotArgs[0] != "/C" || gotArgs[1] != "echo" || gotArgs[2] != "Hello" {
		t.Fatalf("args = %#v, want cmd /C echo Hello shape", gotArgs)
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(out), &payload); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if payload["command"] != "echo" {
		t.Fatalf("command = %#v, want echo", payload["command"])
	}
}

func TestExecutorStartPollAndKillCommandLifecycle(t *testing.T) {
	t.Parallel()

	stdoutReader, stdoutWriter := io.Pipe()
	stderrReader, stderrWriter := io.Pipe()
	waitCh := make(chan error, 1)
	process := &fakeProcess{
		stdout: stdoutReader,
		stderr: stderrReader,
		wait: func() error {
			return <-waitCh
		},
		kill: func() error {
			waitCh <- errors.New("killed")
			return nil
		},
	}
	executor := &Executor{
		goos: "linux",
		start: func(_ context.Context, _ string, executable string, args []string) (processHandle, error) {
			if executable != "printf" {
				t.Fatalf("executable = %q, want printf", executable)
			}
			if len(args) != 1 || args[0] != "hello" {
				t.Fatalf("args = %#v, want [hello]", args)
			}
			return process, nil
		},
		commands: map[string]*activeCommand{},
	}
	contract := contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"printf"}},
		},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}

	startOut, err := executor.Execute(contract, "shell_start", map[string]any{
		"command": "printf",
		"args":    []any{"hello"},
	})
	if err != nil {
		t.Fatalf("shell_start returned error: %v", err)
	}
	commandID := decodeField(t, startOut, "command_id")
	if commandID == "" {
		t.Fatalf("command_id missing from %s", startOut)
	}

	_, _ = io.Copy(stdoutWriter, bytes.NewBufferString("first line\n"))
	_ = stdoutWriter.Close()
	_, _ = io.Copy(stderrWriter, bytes.NewBufferString("warn line\n"))
	_ = stderrWriter.Close()
	time.Sleep(20 * time.Millisecond)

	pollOut, err := executor.Execute(contract, "shell_poll", map[string]any{
		"command_id":   commandID,
		"after_offset": 0,
	})
	if err != nil {
		t.Fatalf("shell_poll returned error: %v", err)
	}
	var pollPayload map[string]any
	if err := json.Unmarshal([]byte(pollOut), &pollPayload); err != nil {
		t.Fatalf("unmarshal poll result: %v", err)
	}
	if pollPayload["status"] != "running" {
		t.Fatalf("poll status = %#v, want running", pollPayload["status"])
	}
	chunks, ok := pollPayload["chunks"].([]any)
	if !ok || len(chunks) != 2 {
		t.Fatalf("chunks = %#v, want 2 output chunks", pollPayload["chunks"])
	}

	killOut, err := executor.Execute(contract, "shell_kill", map[string]any{"command_id": commandID})
	if err != nil {
		t.Fatalf("shell_kill returned error: %v", err)
	}
	if decodeField(t, killOut, "status") != "killing" {
		t.Fatalf("kill status = %s, want killing", killOut)
	}
	time.Sleep(20 * time.Millisecond)

	finalOut, err := executor.Execute(contract, "shell_poll", map[string]any{
		"command_id":   commandID,
		"after_offset": 2,
	})
	if err != nil {
		t.Fatalf("final shell_poll returned error: %v", err)
	}
	if decodeField(t, finalOut, "status") != "killed" {
		t.Fatalf("final poll status = %s, want killed", finalOut)
	}
}

func TestExecutorQueuesApprovalAndStartsAfterApprove(t *testing.T) {
	t.Parallel()

	stdoutReader, stdoutWriter := io.Pipe()
	stderrReader, stderrWriter := io.Pipe()
	waitCh := make(chan error, 1)
	process := &fakeProcess{
		stdout: stdoutReader,
		stderr: stderrReader,
		wait: func() error {
			return <-waitCh
		},
		kill: func() error { return nil },
	}
	executor := &Executor{
		goos: "linux",
		start: func(_ context.Context, _ string, executable string, args []string) (processHandle, error) {
			return process, nil
		},
		commands:  map[string]*activeCommand{},
		approvals: map[string]*pendingApproval{},
	}
	contract := contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"go"}},
		},
		Approval: contracts.ShellApprovalPolicy{
			Enabled:  true,
			Strategy: "always_require",
			Params:   contracts.ShellApprovalParams{ApprovalMessageTemplate: "approve {{command}}"},
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
	}

	startOut, err := executor.ExecuteWithMeta(context.Background(), contract, "shell_start", map[string]any{
		"command": "go",
		"args":    []any{"test"},
	}, ExecutionMeta{SessionID: "session-1", RunID: "run-1"})
	if err != nil {
		t.Fatalf("ExecuteWithMeta returned error: %v", err)
	}
	if decodeField(t, startOut, "status") != "approval_pending" {
		t.Fatalf("status = %s, want approval_pending", startOut)
	}
	approvalID := decodeField(t, startOut, "approval_id")
	if approvalID == "" {
		t.Fatalf("approval_id missing: %s", startOut)
	}
	if len(executor.PendingApprovals("session-1")) != 1 {
		t.Fatalf("pending approvals = %d, want 1", len(executor.PendingApprovals("session-1")))
	}

	approveOut, err := executor.Approve(context.Background(), approvalID)
	if err != nil {
		t.Fatalf("Approve returned error: %v", err)
	}
	if decodeField(t, approveOut, "status") != "running" {
		t.Fatalf("approve status = %s, want running", approveOut)
	}

	_, _ = io.Copy(stdoutWriter, bytes.NewBufferString("ok\n"))
	_ = stdoutWriter.Close()
	_ = stderrWriter.Close()
	waitCh <- nil
}

func TestExecutorPollWaitsBrieflyForSilentRunningCommand(t *testing.T) {
	t.Parallel()

	stdoutReader, stdoutWriter := io.Pipe()
	stderrReader, stderrWriter := io.Pipe()
	waitCh := make(chan error)
	process := &fakeProcess{
		stdout: stdoutReader,
		stderr: stderrReader,
		wait: func() error {
			return <-waitCh
		},
		kill: func() error {
			close(waitCh)
			return nil
		},
	}
	executor := &Executor{
		goos:     "linux",
		pollWait: 75 * time.Millisecond,
		start: func(_ context.Context, _ string, executable string, args []string) (processHandle, error) {
			return process, nil
		},
		commands:  map[string]*activeCommand{},
		approvals: map[string]*pendingApproval{},
	}
	contract := contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"go"}},
		},
		Approval: contracts.ShellApprovalPolicy{
			Enabled:  true,
			Strategy: "always_allow",
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
	}

	startOut, err := executor.Execute(contract, "shell_start", map[string]any{
		"command": "go",
		"args":    []any{"test"},
	})
	if err != nil {
		t.Fatalf("shell_start returned error: %v", err)
	}
	commandID := decodeField(t, startOut, "command_id")
	startedAt := time.Now()
	pollOut, err := executor.Execute(contract, "shell_poll", map[string]any{
		"command_id":   commandID,
		"after_offset": 0,
	})
	elapsed := time.Since(startedAt)
	if err != nil {
		t.Fatalf("shell_poll returned error: %v", err)
	}
	if elapsed < 60*time.Millisecond {
		t.Fatalf("shell_poll elapsed = %v, want >= 60ms for silent running command", elapsed)
	}
	if decodeField(t, pollOut, "status") != "running" {
		t.Fatalf("poll status = %s, want running", pollOut)
	}

	_ = stdoutWriter.Close()
	_ = stderrWriter.Close()
	_ = process.Kill()
}

func TestExecutorApproveRunsShellExecSynchronously(t *testing.T) {
	t.Parallel()

	executor := &Executor{
		goos: "linux",
		run: func(_ context.Context, cwd, executable string, args []string) (runResult, error) {
			if executable == "" {
				t.Fatal("executable is empty")
			}
			return runResult{stdout: "approved output\n", stderr: "", exitCode: 0}, nil
		},
		lookupPath: func(file string) (string, error) { return file, nil },
		commands:   map[string]*activeCommand{},
		completed:  map[string]*activeCommand{},
		approvals:  map[string]*pendingApproval{},
	}
	contract := contracts.ShellExecutionContract{
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
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}

	startOut, err := executor.ExecuteWithMeta(context.Background(), contract, "shell_exec", map[string]any{
		"command": "go",
		"args":    []any{"version"},
	}, ExecutionMeta{SessionID: "session-1", RunID: "run-1"})
	if err != nil {
		t.Fatalf("ExecuteWithMeta returned error: %v", err)
	}
	if decodeField(t, startOut, "status") != "approval_pending" {
		t.Fatalf("status = %s, want approval_pending", startOut)
	}
	approvalID := decodeField(t, startOut, "approval_id")
	if approvalID == "" {
		t.Fatalf("approval_id missing from %s", startOut)
	}

	approveOut, err := executor.Approve(context.Background(), approvalID)
	if err != nil {
		t.Fatalf("Approve returned error: %v", err)
	}
	if decodeField(t, approveOut, "status") != "ok" {
		t.Fatalf("approve status = %s, want ok", approveOut)
	}
	if decodeField(t, approveOut, "stdout") != "approved output\n" {
		t.Fatalf("stdout = %q, want approved output", decodeField(t, approveOut, "stdout"))
	}
}

func TestExecutorDeniesPendingApproval(t *testing.T) {
	t.Parallel()

	executor := &Executor{
		goos:      "linux",
		commands:  map[string]*activeCommand{},
		approvals: map[string]*pendingApproval{},
	}
	contract := contracts.ShellExecutionContract{
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
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
			},
		},
	}
	startOut, err := executor.ExecuteWithMeta(context.Background(), contract, "shell_start", map[string]any{
		"command": "go",
	}, ExecutionMeta{SessionID: "session-1"})
	if err != nil {
		t.Fatalf("ExecuteWithMeta returned error: %v", err)
	}
	approvalID := decodeField(t, startOut, "approval_id")
	if err := executor.Deny(context.Background(), approvalID); err != nil {
		t.Fatalf("Deny returned error: %v", err)
	}
	if len(executor.PendingApprovals("session-1")) != 0 {
		t.Fatalf("pending approvals still present after deny")
	}
}

func TestExecutorAllowsPersistentApprovalPrefix(t *testing.T) {
	t.Parallel()

	executor := NewExecutor()
	out, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"go"}},
		},
		Approval: contracts.ShellApprovalPolicy{
			Enabled:  true,
			Strategy: "always_require",
			Params:   contracts.ShellApprovalParams{AllowPrefixes: []string{"go test"}},
		},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}, "shell_exec", map[string]any{
		"command": "go",
		"args":    []any{"test", "./..."},
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if decodeField(t, out, "status") == "approval_pending" {
		t.Fatalf("allow prefix still triggered approval: %s", out)
	}
}

func TestExecutorAllowsPersistentApprovalByExecutablePrefix(t *testing.T) {
	t.Parallel()

	executor := NewExecutor()
	out, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"/bin/pwd"}},
		},
		Approval: contracts.ShellApprovalPolicy{
			Enabled:  true,
			Strategy: "always_require",
			Params:   contracts.ShellApprovalParams{AllowPrefixes: []string{"pwd"}},
		},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}, "shell_exec", map[string]any{
		"command": "/bin/pwd",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if decodeField(t, out, "status") == "approval_pending" {
		t.Fatalf("executable allow prefix still triggered approval: %s", out)
	}
}

func TestExecutorAllowsPersistentApprovalByExecutablePrefixForShellSnippetArgs(t *testing.T) {
	t.Parallel()

	executor := &Executor{
		goos: "linux",
		run: func(_ context.Context, _ string, executable string, args []string) (runResult, error) {
			return runResult{stdout: executable + " " + strings.Join(args, " "), exitCode: 0}, nil
		},
		lookupPath: func(file string) (string, error) { return file, nil },
		commands:   map[string]*activeCommand{},
		completed:  map[string]*activeCommand{},
		approvals:  map[string]*pendingApproval{},
	}
	dir := t.TempDir()
	out, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"cd"}},
		},
		Approval: contracts.ShellApprovalPolicy{
			Enabled:  true,
			Strategy: "always_require",
			Params:   contracts.ShellApprovalParams{AllowPrefixes: []string{"pwd"}},
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
		"command": "cd",
		"args":    []any{dir, "&&", "pwd"},
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if decodeField(t, out, "status") == "approval_pending" {
		t.Fatalf("snippet allow prefix still triggered approval: %s", out)
	}
}

func TestExecutorDeniesPersistentApprovalPrefix(t *testing.T) {
	t.Parallel()

	executor := NewExecutor()
	_, err := executor.Execute(contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"go"}},
		},
		Approval: contracts.ShellApprovalPolicy{
			Enabled:  true,
			Strategy: "always_allow",
			Params:   contracts.ShellApprovalParams{DenyPrefixes: []string{"go env"}},
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
		"command": "go",
		"args":    []any{"env", "GOROOT"},
	})
	if err == nil {
		t.Fatal("Execute returned nil error, want persistent deny")
	}
	if !strings.Contains(err.Error(), "denied by persistent policy") {
		t.Fatalf("Execute error = %q", err)
	}
}

func TestExecutorActiveCommandsExcludesCompletedCommands(t *testing.T) {
	t.Parallel()

	stdoutReader, stdoutWriter := io.Pipe()
	stderrReader, stderrWriter := io.Pipe()
	waitCh := make(chan error, 1)
	process := &fakeProcess{
		stdout: stdoutReader,
		stderr: stderrReader,
		wait: func() error {
			return <-waitCh
		},
		kill: func() error { return nil },
	}
	executor := &Executor{
		goos: "linux",
		start: func(_ context.Context, _ string, _ string, _ []string) (processHandle, error) {
			return process, nil
		},
		commands:  map[string]*activeCommand{},
		completed: map[string]*activeCommand{},
	}
	contract := contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"printf"}},
		},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}

	startOut, err := executor.ExecuteWithMeta(context.Background(), contract, "shell_start", map[string]any{
		"command": "printf",
	}, ExecutionMeta{SessionID: "session-1"})
	if err != nil {
		t.Fatalf("ExecuteWithMeta returned error: %v", err)
	}
	commandID := decodeField(t, startOut, "command_id")
	if commandID == "" {
		t.Fatalf("command_id missing from %s", startOut)
	}
	if got := executor.ActiveCommands("session-1"); len(got) != 1 {
		t.Fatalf("ActiveCommands before completion = %d, want 1", len(got))
	}

	_ = stdoutWriter.Close()
	_ = stderrWriter.Close()
	waitCh <- nil
	time.Sleep(20 * time.Millisecond)

	if got := executor.ActiveCommands("session-1"); len(got) != 0 {
		t.Fatalf("ActiveCommands after completion = %d, want 0", len(got))
	}
	pollOut, err := executor.Execute(contract, "shell_poll", map[string]any{
		"command_id": commandID,
	})
	if err != nil {
		t.Fatalf("shell_poll after completion returned error: %v", err)
	}
	if decodeField(t, pollOut, "status") != "completed" {
		t.Fatalf("poll status after completion = %s, want completed", pollOut)
	}
}

func TestExecutorRecordsCompletionWithoutPoll(t *testing.T) {
	t.Parallel()

	stdoutReader, stdoutWriter := io.Pipe()
	stderrReader, stderrWriter := io.Pipe()
	waitCh := make(chan error, 1)
	process := &fakeProcess{
		stdout: stdoutReader,
		stderr: stderrReader,
		wait: func() error {
			return <-waitCh
		},
		kill: func() error { return nil },
	}
	var kinds []eventing.EventKind
	executor := &Executor{
		goos: "linux",
		start: func(_ context.Context, _ string, _ string, _ []string) (processHandle, error) {
			return process, nil
		},
		commands:  map[string]*activeCommand{},
		completed: map[string]*activeCommand{},
	}
	contract := contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"printf"}},
		},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}

	_, err := executor.ExecuteWithMeta(context.Background(), contract, "shell_start", map[string]any{
		"command": "printf",
	}, ExecutionMeta{
		SessionID: "session-1",
		RunID:     "run-1",
		RecordEvent: func(_ context.Context, event eventing.Event) error {
			kinds = append(kinds, event.Kind)
			return nil
		},
	})
	if err != nil {
		t.Fatalf("ExecuteWithMeta returned error: %v", err)
	}
	_, _ = io.Copy(stdoutWriter, bytes.NewBufferString("done\n"))
	_ = stdoutWriter.Close()
	_ = stderrWriter.Close()
	waitCh <- nil
	time.Sleep(20 * time.Millisecond)

	if len(kinds) < 2 {
		t.Fatalf("recorded kinds = %#v, want started and completed", kinds)
	}
	if kinds[0] != eventing.EventShellCommandStarted {
		t.Fatalf("first event kind = %q, want %q", kinds[0], eventing.EventShellCommandStarted)
	}
	foundCompleted := false
	for _, kind := range kinds {
		if kind == eventing.EventShellCommandCompleted {
			foundCompleted = true
			break
		}
	}
	if !foundCompleted {
		t.Fatalf("recorded kinds = %#v, want completed event", kinds)
	}
}

func TestShellStartDoesNotAttachRuntimeDeadlineToProcessContext(t *testing.T) {
	t.Parallel()

	stdoutReader, stdoutWriter := io.Pipe()
	stderrReader, stderrWriter := io.Pipe()
	waitCh := make(chan error, 1)
	process := &fakeProcess{
		stdout: stdoutReader,
		stderr: stderrReader,
		wait: func() error {
			return <-waitCh
		},
		kill: func() error { return nil },
	}

	var startCtx context.Context
	executor := &Executor{
		goos: "linux",
		start: func(ctx context.Context, _ string, _ string, _ []string) (processHandle, error) {
			startCtx = ctx
			return process, nil
		},
		commands: map[string]*activeCommand{},
	}
	contract := contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"printf"}},
		},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            t.TempDir(),
				Timeout:        "5ms",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}

	startOut, err := executor.ExecuteWithMeta(context.Background(), contract, "shell_start", map[string]any{
		"command": "printf",
	}, ExecutionMeta{SessionID: "session-1"})
	if err != nil {
		t.Fatalf("ExecuteWithMeta returned error: %v", err)
	}
	if startCtx == nil {
		t.Fatal("start ctx was not captured")
	}
	if _, hasDeadline := startCtx.Deadline(); hasDeadline {
		t.Fatal("shell_start process ctx unexpectedly has deadline")
	}
	commandID := decodeField(t, startOut, "command_id")
	if commandID == "" {
		t.Fatalf("command_id missing from %s", startOut)
	}

	time.Sleep(20 * time.Millisecond)
	if got := executor.ActiveCommands("session-1"); len(got) != 1 {
		t.Fatalf("ActiveCommands after timeout window = %d, want 1", len(got))
	}

	_ = stdoutWriter.Close()
	_ = stderrWriter.Close()
	waitCh <- nil
	time.Sleep(20 * time.Millisecond)
}

func TestExecutorApproveKeepsPendingApprovalWhenStartFails(t *testing.T) {
	t.Parallel()

	var recorded []eventing.EventKind
	executor := &Executor{
		goos:      "linux",
		commands:  map[string]*activeCommand{},
		completed: map[string]*activeCommand{},
		approvals: map[string]*pendingApproval{},
		start: func(_ context.Context, _ string, _ string, _ []string) (processHandle, error) {
			return nil, errors.New("boom")
		},
	}
	contract := contracts.ShellExecutionContract{
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
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}

	startOut, err := executor.ExecuteWithMeta(context.Background(), contract, "shell_start", map[string]any{
		"command": "go",
		"args":    []any{"test"},
	}, ExecutionMeta{
		SessionID: "session-1",
		RunID:     "run-1",
		RecordEvent: func(_ context.Context, event eventing.Event) error {
			recorded = append(recorded, event.Kind)
			return nil
		},
	})
	if err != nil {
		t.Fatalf("ExecuteWithMeta returned error: %v", err)
	}
	approvalID := decodeField(t, startOut, "approval_id")
	if approvalID == "" {
		t.Fatalf("approval_id missing from %s", startOut)
	}

	_, err = executor.Approve(context.Background(), approvalID)
	if err == nil {
		t.Fatal("Approve returned nil error, want start failure")
	}
	if len(executor.PendingApprovals("session-1")) != 1 {
		t.Fatalf("pending approvals after failed approve = %d, want 1", len(executor.PendingApprovals("session-1")))
	}
	for _, kind := range recorded {
		if kind == eventing.EventShellCommandApprovalGranted {
			t.Fatalf("recorded approval_granted despite failed start: %#v", recorded)
		}
	}
}

func TestExecutorSyncHonorsCanceledContext(t *testing.T) {
	t.Parallel()

	executor := &Executor{
		goos: "linux",
		run: func(ctx context.Context, _ string, _ string, _ []string) (runResult, error) {
			<-ctx.Done()
			return runResult{}, ctx.Err()
		},
		commands:  map[string]*activeCommand{},
		completed: map[string]*activeCommand{},
	}
	contract := contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"printf"}},
		},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            t.TempDir(),
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	_, err := executor.ExecuteWithMeta(ctx, contract, "shell_exec", map[string]any{
		"command": "printf",
	}, ExecutionMeta{})
	if err == nil || !strings.Contains(err.Error(), "canceled") {
		t.Fatalf("ExecuteWithMeta error = %v, want canceled", err)
	}
}

type fakeProcess struct {
	stdout io.ReadCloser
	stderr io.ReadCloser
	wait   func() error
	kill   func() error
}

func (p *fakeProcess) StdoutPipe() (io.ReadCloser, error) { return p.stdout, nil }
func (p *fakeProcess) StderrPipe() (io.ReadCloser, error) { return p.stderr, nil }
func (p *fakeProcess) Start() error                       { return nil }
func (p *fakeProcess) Wait() error                        { return p.wait() }
func (p *fakeProcess) Kill() error                        { return p.kill() }

func decodeField(t *testing.T, body string, field string) string {
	t.Helper()
	var payload map[string]any
	if err := json.Unmarshal([]byte(body), &payload); err != nil {
		t.Fatalf("unmarshal %s: %v", body, err)
	}
	value, _ := payload[field].(string)
	return value
}
