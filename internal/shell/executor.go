package shell

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"teamd/internal/contracts"
)

type Executor struct{}

func NewExecutor() *Executor {
	return &Executor{}
}

func (e *Executor) Execute(contract contracts.ShellExecutionContract, toolName string, argsMap map[string]any) (string, error) {
	if e == nil {
		return "", fmt.Errorf("shell executor is nil")
	}
	if toolName != "shell_exec" {
		return "", fmt.Errorf("shell tool %q is not implemented", toolName)
	}
	command, err := stringArg(argsMap, "command")
	if err != nil {
		return "", err
	}
	args, err := optionalStringSlice(argsMap, "args")
	if err != nil {
		return "", err
	}
	if err := validateCommand(contract.Command, command, args); err != nil {
		return "", err
	}
	cwd, err := resolveCwd(contract.Runtime, argsMap)
	if err != nil {
		return "", err
	}
	timeout := 30 * time.Second
	if contract.Runtime.Params.Timeout != "" {
		parsed, err := time.ParseDuration(contract.Runtime.Params.Timeout)
		if err != nil {
			return "", fmt.Errorf("parse shell timeout: %w", err)
		}
		timeout = parsed
	}
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()

	cmd := exec.CommandContext(ctx, command, args...)
	cmd.Dir = cwd
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	start := time.Now()
	err = cmd.Run()
	duration := time.Since(start)
	if ctx.Err() == context.DeadlineExceeded {
		return "", fmt.Errorf("shell command timed out")
	}
	maxOutput := contract.Runtime.Params.MaxOutputBytes
	if maxOutput > 0 && stdout.Len()+stderr.Len() > maxOutput {
		return "", fmt.Errorf("shell output exceeds max_output_bytes")
	}
	exitCode := 0
	status := "ok"
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			exitCode = exitErr.ExitCode()
			status = "error"
		} else {
			return "", fmt.Errorf("run shell command: %w", err)
		}
	}
	return jsonText(map[string]any{
		"status":      status,
		"tool":        toolName,
		"command":     command,
		"args":        args,
		"cwd":         cwd,
		"exit_code":   exitCode,
		"stdout":      stdout.String(),
		"stderr":      stderr.String(),
		"duration_ms": duration.Milliseconds(),
		"timed_out":   false,
	}), nil
}

func validateCommand(policy contracts.ShellCommandPolicy, command string, args []string) error {
	if policy.Enabled {
		switch policy.Strategy {
		case "deny_all":
			return fmt.Errorf("shell commands are denied by policy")
		case "static_allowlist":
			allowed := len(policy.Params.AllowedCommands) == 0
			for _, candidate := range policy.Params.AllowedCommands {
				if candidate == command {
					allowed = true
					break
				}
			}
			if !allowed {
				return fmt.Errorf("shell command %q is not in allowlist", command)
			}
			full := strings.TrimSpace(strings.Join(append([]string{command}, args...), " "))
			for _, pattern := range policy.Params.DenyPatterns {
				if pattern != "" && strings.Contains(full, pattern) {
					return fmt.Errorf("shell command matches denied pattern")
				}
			}
			if len(policy.Params.AllowedPrefixes) > 0 {
				prefixAllowed := false
				for _, prefix := range policy.Params.AllowedPrefixes {
					if strings.HasPrefix(full, prefix) {
						prefixAllowed = true
						break
					}
				}
				if !prefixAllowed {
					return fmt.Errorf("shell command %q does not match allowed prefixes", full)
				}
			}
		default:
			return fmt.Errorf("unsupported shell command strategy %q", policy.Strategy)
		}
	}
	return nil
}

func resolveCwd(policy contracts.ShellRuntimePolicy, args map[string]any) (string, error) {
	base := policy.Params.Cwd
	if base == "" {
		base = "."
	}
	baseAbs, err := filepath.Abs(base)
	if err != nil {
		return "", fmt.Errorf("resolve shell base cwd: %w", err)
	}
	requested := baseAbs
	if raw, ok := args["cwd"]; ok {
		text, ok := raw.(string)
		if !ok || text == "" {
			return "", fmt.Errorf("argument %q must be a non-empty string", "cwd")
		}
		if filepath.IsAbs(text) {
			requested = filepath.Clean(text)
		} else {
			requested = filepath.Clean(filepath.Join(baseAbs, text))
		}
	}
	rel, err := filepath.Rel(baseAbs, requested)
	if err != nil {
		return "", fmt.Errorf("resolve shell cwd: %w", err)
	}
	if rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("shell cwd escapes runtime scope")
	}
	return requested, nil
}

func stringArg(args map[string]any, key string) (string, error) {
	value, ok := args[key]
	if !ok {
		return "", fmt.Errorf("missing required argument %q", key)
	}
	text, ok := value.(string)
	if !ok || text == "" {
		return "", fmt.Errorf("argument %q must be a non-empty string", key)
	}
	return text, nil
}

func optionalStringSlice(args map[string]any, key string) ([]string, error) {
	value, ok := args[key]
	if !ok || value == nil {
		return nil, nil
	}
	items, ok := value.([]any)
	if !ok {
		if typed, ok := value.([]string); ok {
			return typed, nil
		}
		return nil, fmt.Errorf("argument %q must be an array of strings", key)
	}
	out := make([]string, 0, len(items))
	for _, item := range items {
		text, ok := item.(string)
		if !ok {
			return nil, fmt.Errorf("argument %q must be an array of strings", key)
		}
		out = append(out, text)
	}
	return out, nil
}

func jsonText(value any) string {
	data, _ := json.Marshal(value)
	return string(data)
}
