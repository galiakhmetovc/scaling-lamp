package telegram

import (
	"context"
	"strconv"
	"strings"
	"time"

	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
)

func formatToolActivity(call provider.ToolCall, output string) string {
	lines := []string{"tool: " + runtimeToolName(call.Name)}

	if command, _ := call.Arguments["command"].(string); strings.TrimSpace(command) != "" {
		lines = append(lines, "command: "+command)
	}
	if path, _ := call.Arguments["path"].(string); strings.TrimSpace(path) != "" {
		lines = append(lines, "path: "+path)
	}
	if cwd, _ := call.Arguments["cwd"].(string); strings.TrimSpace(cwd) != "" {
		lines = append(lines, "cwd: "+cwd)
	}

	lines = append(lines, "", output)
	return strings.Join(lines, "\n")
}

func detachedContext(ctx context.Context) (context.Context, context.CancelFunc) {
	return context.WithTimeout(context.WithoutCancel(ctx), 10*time.Second)
}

func boolPtrString(v *bool) string {
	if v == nil {
		return "default"
	}
	if *v {
		return "true"
	}
	return "false"
}

func floatPtrString(v *float64) string {
	if v == nil {
		return "default"
	}
	return strconv.FormatFloat(*v, 'f', -1, 64)
}

func intPtrString(v *int) string {
	if v == nil {
		return "default"
	}
	return strconv.Itoa(*v)
}

func valueOrUnknown(v string) string {
	v = strings.TrimSpace(v)
	if v == "" {
		return "unknown"
	}
	return v
}

func intOrUnknown(v int) string {
	if v <= 0 {
		return "unknown"
	}
	return strconv.Itoa(v)
}

func cloneToolArguments(src map[string]any) map[string]any {
	if len(src) == 0 {
		return map[string]any{}
	}
	dst := make(map[string]any, len(src))
	for k, v := range src {
		dst[k] = v
	}
	return dst
}

func (a *Adapter) artifactOffloadPolicy() runtimex.ArtifactOffloadPolicy {
	maxInlineChars := a.budget.MaxToolContextChars
	if maxInlineChars <= 0 {
		maxInlineChars = 4096
	}
	return runtimex.ArtifactOffloadPolicy{
		MaxInlineChars: maxInlineChars,
		MaxInlineLines: 32,
		PreviewLines:   8,
	}
}

func (a *Adapter) shapeToolResult(chatID int64, call provider.ToolCall, content string) (runtimex.OffloadedToolResult, error) {
	if a.artifacts == nil {
		return runtimex.OffloadedToolResult{Content: content}, nil
	}
	ownerID := activeRunID(a.runs, chatID)
	if strings.TrimSpace(ownerID) == "" {
		ownerID = a.meshSessionID(chatID)
	}
	return runtimex.MaybeOffloadToolResult(
		a.artifacts,
		a.artifactOffloadPolicy(),
		runtimex.ArtifactOwnerRef{OwnerType: "run", OwnerID: ownerID},
		runtimeToolName(call.Name),
		content,
	)
}
