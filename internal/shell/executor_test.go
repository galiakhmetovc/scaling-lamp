package shell

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"path/filepath"
	"testing"
	"time"

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
