package runtime

import (
	"context"
	"encoding/json"
	"fmt"
	"path/filepath"
	"slices"
	"strconv"
	"strings"

	"teamd/internal/artifacts"
	"teamd/internal/contracts"
	"teamd/internal/tools"
)

func (a *Agent) artifactToolDefinitions(contractSet contracts.ResolvedContracts) ([]tools.Definition, error) {
	return artifacts.NewDefinitionExecutor().Build(contractSet.Memory)
}

func (a *Agent) artifactStorePath(contractSet contracts.ResolvedContracts) (string, error) {
	if a == nil {
		return "", fmt.Errorf("agent is nil")
	}
	if !contractSet.Memory.Offload.Enabled || contractSet.Memory.Offload.Strategy != "artifact_store" {
		return "", nil
	}
	root := contractSet.Memory.Offload.Params.StoragePath
	if root != "" {
		return root, nil
	}
	if a.ConfigPath == "" {
		return "", fmt.Errorf("artifact storage path is not configured")
	}
	return filepath.Join(filepath.Dir(a.ConfigPath), "var", "artifacts"), nil
}

func (a *Agent) ArtifactStorePath() (string, error) {
	return a.artifactStorePath(a.Contracts)
}

func (a *Agent) ensureArtifactStore(contractSet contracts.ResolvedContracts) (*artifacts.Store, error) {
	if a == nil {
		return nil, fmt.Errorf("agent is nil")
	}
	if a.ArtifactStore != nil {
		return a.ArtifactStore, nil
	}
	if !contractSet.Memory.Offload.Enabled || contractSet.Memory.Offload.Strategy != "artifact_store" {
		return nil, nil
	}
	root, err := a.artifactStorePath(contractSet)
	if err != nil {
		return nil, err
	}
	store, err := artifacts.NewStore(root)
	if err != nil {
		return nil, err
	}
	a.ArtifactStore = store
	return store, nil
}

func (a *Agent) maybeOffloadToolResult(ctx context.Context, contractSet contracts.ResolvedContracts, toolName, resultText string) (string, []string, error) {
	policy := contractSet.Memory.Offload
	if !policy.Enabled || policy.Strategy != "artifact_store" {
		return resultText, nil, nil
	}
	if toolName == "artifact_read" || toolName == "artifact_search" {
		return resultText, nil, nil
	}
	maxChars := policy.Params.MaxChars
	if maxChars <= 0 || len(resultText) <= maxChars {
		return resultText, nil, nil
	}
	store, err := a.ensureArtifactStore(contractSet)
	if err != nil {
		return "", nil, err
	}
	if store == nil {
		return resultText, nil, nil
	}
	record, err := store.Write(ctx, toolName, resultText, policy.Params.PreviewChars)
	if err != nil {
		return "", nil, err
	}
	summary := summarizeOffloadedToolResult(toolName, resultText)
	return jsonString(map[string]any{
		"status":         "ok",
		"tool":           toolName,
		"offloaded":      true,
		"artifact_ref":   record.Ref,
		"summary":        summary,
		"size_chars":     record.SizeChars,
		"size_bytes":     record.SizeBytes,
		"line_count":     countLines(resultText),
		"token_estimate": approximateTextTokens(resultText, 4),
		"truncated":      true,
		"preview":        record.Preview,
		"retrieval_hint": fmt.Sprintf("Use artifact_read with artifact_ref %q to inspect the full content.", record.Ref),
	}), []string{record.Ref}, nil
}

func (a *Agent) executeArtifactCommand(ctx context.Context, contractSet contracts.ResolvedContracts, callName string, args map[string]any) (string, error) {
	store, err := a.ensureArtifactStore(contractSet)
	if err != nil {
		return "", err
	}
	if store == nil {
		return "", fmt.Errorf("artifact store is not configured")
	}
	switch callName {
	case "artifact_read":
		artifactRef, err := stringArg(args, "artifact_ref")
		if err != nil {
			return "", err
		}
		record, content, err := store.Read(ctx, artifactRef)
		if err != nil {
			return "", err
		}
		body, err := json.Marshal(map[string]any{
			"status":       "ok",
			"tool":         callName,
			"artifact_ref": record.Ref,
			"tool_name":    record.ToolName,
			"size_chars":   record.SizeChars,
			"size_bytes":   record.SizeBytes,
			"content":      content,
		})
		if err != nil {
			return "", fmt.Errorf("encode artifact read result: %w", err)
		}
		return string(body), nil
	case "artifact_search":
		query, err := stringArg(args, "query")
		if err != nil {
			return "", err
		}
		limit, err := optionalIntArg(args, "limit")
		if err != nil {
			return "", err
		}
		if limit <= 0 {
			limit = contractSet.Memory.Offload.Params.SearchLimit
		}
		results, err := store.Search(ctx, query, limit)
		if err != nil {
			return "", err
		}
		body, err := json.Marshal(map[string]any{
			"status":  "ok",
			"tool":    callName,
			"query":   query,
			"results": results,
		})
		if err != nil {
			return "", fmt.Errorf("encode artifact search result: %w", err)
		}
		return string(body), nil
	default:
		return "", fmt.Errorf("artifact tool %q is not implemented", callName)
	}
}

func summarizeOffloadedToolResult(toolName, content string) string {
	if summary := summarizeJSONContent(content); summary != "" {
		return summary
	}
	lineCount := countLines(content)
	switch toolName {
	case "shell_exec", "shell.exec":
		return summarizeShellOutput(content, lineCount)
	default:
		return summarizeTextOutput(content, lineCount)
	}
}

func summarizeJSONContent(content string) string {
	trimmed := strings.TrimSpace(content)
	if trimmed == "" || !strings.HasPrefix(trimmed, "{") {
		return ""
	}
	var obj map[string]any
	if err := json.Unmarshal([]byte(trimmed), &obj); err != nil {
		return ""
	}
	keys := make([]string, 0, len(obj))
	for key := range obj {
		keys = append(keys, key)
	}
	slices.Sort(keys)
	if len(keys) > 4 {
		keys = keys[:4]
	}
	parts := []string{"json object offloaded"}
	if len(keys) > 0 {
		parts = append(parts, "keys="+strings.Join(keys, ","))
	}
	if status, ok := stringValue(obj["status"]); ok && status != "" {
		parts = append(parts, "status="+status)
	}
	if count, ok := numberValue(obj["count"]); ok {
		parts = append(parts, "count="+strconv.Itoa(count))
	}
	return strings.Join(parts, "; ")
}

func summarizeShellOutput(content string, lineCount int) string {
	parts := []string{fmt.Sprintf("shell output offloaded; %d lines", lineCount)}
	if markers := detectSeverityMarkers(content); len(markers) > 0 {
		parts = append(parts, "markers="+strings.Join(markers, ","))
	}
	if line := firstSignificantLine(content); line != "" {
		parts = append(parts, "sample="+quoteSummary(line))
	}
	return strings.Join(parts, "; ")
}

func summarizeTextOutput(content string, lineCount int) string {
	parts := []string{fmt.Sprintf("text output offloaded; %d lines", lineCount)}
	if markers := detectSeverityMarkers(content); len(markers) > 0 {
		parts = append(parts, "markers="+strings.Join(markers, ","))
	}
	if line := firstSignificantLine(content); line != "" {
		parts = append(parts, "sample="+quoteSummary(line))
	}
	return strings.Join(parts, "; ")
}

func detectSeverityMarkers(content string) []string {
	lower := strings.ToLower(content)
	out := make([]string, 0, 3)
	for _, marker := range []string{"error", "warn", "fail"} {
		if strings.Contains(lower, marker) {
			out = append(out, marker)
		}
	}
	return out
}

func firstSignificantLine(content string) string {
	for _, line := range strings.Split(content, "\n") {
		line = strings.TrimSpace(line)
		if line != "" {
			return line
		}
	}
	return ""
}

func quoteSummary(v string) string {
	v = strings.TrimSpace(v)
	if len(v) > 72 {
		v = strings.TrimSpace(v[:72]) + "..."
	}
	return strconv.Quote(v)
}

func countLines(text string) int {
	if text == "" {
		return 0
	}
	return strings.Count(text, "\n") + 1
}

func stringValue(v any) (string, bool) {
	s, ok := v.(string)
	return s, ok
}

func numberValue(v any) (int, bool) {
	switch n := v.(type) {
	case float64:
		return int(n), true
	case int:
		return n, true
	case int64:
		return int(n), true
	default:
		return 0, false
	}
}
