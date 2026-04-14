package tools

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"log/slog"
	"os/exec"
	"strings"
	"time"

	"teamd/internal/mcp"
)

const defaultShellTimeout = 15 * time.Second

func RegisterShellTools(runtime registrar) {
	runtime.Register(mcp.Tool{
		Name:        "shell.exec",
		Description: "Execute a shell command locally.",
		Parameters: map[string]any{
			"type": "object",
			"properties": map[string]any{
				"command": map[string]any{"type": "string"},
				"cwd":     map[string]any{"type": "string"},
			},
			"required": []string{"command"},
		},
		Call: func(ctx context.Context, input mcp.CallInput) (mcp.CallResult, error) {
			command := stringArg(input, "command")
			if command == "" {
				return mcp.CallResult{}, fmt.Errorf("command is required")
			}
			if strings.Contains(command, " -i ") || strings.HasPrefix(command, "-i ") || strings.Contains(command, "--interactive") {
				return mcp.CallResult{}, fmt.Errorf("interactive shell flags are not allowed")
			}

			runCtx := ctx
			if _, ok := ctx.Deadline(); !ok {
				var cancel context.CancelFunc
				runCtx, cancel = context.WithTimeout(ctx, defaultShellTimeout)
				defer cancel()
			}

			cwd := stringArg(input, "cwd")
			hash := sha256.Sum256([]byte(command))
			slog.Debug("mcp shell exec", "command_hash", hex.EncodeToString(hash[:]), "cwd", cwd)

			cmd := exec.CommandContext(runCtx, "bash", "-lc", command)
			if cwd != "" {
				cmd.Dir = cwd
			}
			out, err := cmd.CombinedOutput()
			if err != nil {
				return mcp.CallResult{}, fmt.Errorf("shell exec: %w: %s", err, strings.TrimSpace(string(out)))
			}
			return mcp.CallResult{Content: string(out)}, nil
		},
	})
}
