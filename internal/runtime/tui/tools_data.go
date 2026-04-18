package tui

import (
	"encoding/json"
	"fmt"
	"path/filepath"
	"strconv"
	"strings"

	"teamd/internal/runtime"
)

func toolActivityJumpTarget(activity runtime.ToolActivity) (workspaceJumpTarget, bool) {
	arguments := activity.Arguments
	if target := workspaceJumpTargetFromToolFields(arguments, activity.ResultText); target.isValid() {
		return target, true
	}
	return workspaceJumpTarget{}, false
}

func reverseToolEntries(entries []toolLogEntry) []toolLogEntry {
	out := make([]toolLogEntry, 0, len(entries))
	for i := len(entries) - 1; i >= 0; i-- {
		out = append(out, entries[i])
	}
	return out
}

func compactToolActivityLine(activity runtime.ToolActivity, width int) string {
	base := compactToolInvocation(activity.Name, activity.Arguments)
	var line string
	switch activity.Phase {
	case runtime.ToolActivityPhaseStarted:
		line = base + " started"
	case runtime.ToolActivityPhaseCompleted:
		switch {
		case strings.Contains(activity.ErrorText, "requires approval"):
			line = base + " approval required"
		case strings.TrimSpace(activity.ErrorText) != "":
			line = base + " error: " + sanitizeToolError(activity.Name, activity.ErrorText)
		default:
			if summary := compactToolResult(activity.Name, activity.ResultText); summary != "" {
				line = base + " " + summary
			} else {
				line = base + " ok"
			}
		}
	default:
		line = base
	}
	line = prefixTimestamp(activity.OccurredAt, line)
	return ellipsizeForWidth(line, width)
}

func compactToolInvocation(toolName string, arguments map[string]any) string {
	switch toolName {
	case "plan_snapshot":
		return "plan_snapshot current plan"
	case "fs_read_lines":
		path := compactPath(arguments["path"])
		if path == "" {
			return "fs_read_lines"
		}
		return fmt.Sprintf("fs_read_lines %s:%s-%s", path, compactInt(arguments["start_line"]), compactInt(arguments["end_line"]))
	case "fs_read_text", "fs_write_text", "fs_patch_text", "fs_replace_lines", "fs_replace_in_line", "fs_insert_text", "fs_mkdir", "fs_trash":
		if path := compactPath(arguments["path"]); path != "" {
			return toolName + " " + path
		}
		return toolName
	case "fs_list":
		if path := compactPath(arguments["path"]); path != "" {
			return "fs_list " + path
		}
		return "fs_list"
	case "fs_search_text":
		path := compactPath(arguments["path"])
		query := compactString(arguments["query"])
		switch {
		case path != "" && query != "":
			return fmt.Sprintf("fs_search_text %s %q", path, query)
		case path != "":
			return "fs_search_text " + path
		case query != "":
			return fmt.Sprintf("fs_search_text %q", query)
		default:
			return "fs_search_text"
		}
	case "fs_find_in_files", "fs_replace_in_files":
		if query := compactString(arguments["query"]); query != "" {
			return fmt.Sprintf("%s %q", toolName, query)
		}
		return toolName
	case "fs_move":
		src := compactPath(arguments["src"])
		dest := compactPath(arguments["dest"])
		if src != "" && dest != "" {
			return fmt.Sprintf("fs_move %s -> %s", src, dest)
		}
		return "fs_move"
	case "shell_exec", "shell_start":
		return compactShellInvocation(toolName, arguments)
	case "shell_poll", "shell_kill":
		return toolName + " " + compactString(arguments["command_id"])
	default:
		if summary := summarizeToolArguments(arguments); summary != "" {
			return toolName + " " + summary
		}
		return toolName
	}
}

func compactToolResult(toolName, resultText string) string {
	if strings.TrimSpace(resultText) == "" {
		return ""
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(resultText), &payload); err != nil {
		return summarizeToolText(resultText)
	}
	switch toolName {
	case "fs_read_lines":
		if lines, ok := payload["lines"].([]any); ok {
			return fmt.Sprintf("ok %d lines", len(lines))
		}
	case "fs_list":
		if count, ok := intFromAny(payload["entry_count"]); ok {
			return fmt.Sprintf("ok %d entries", count)
		}
		if entries, ok := payload["entries"].([]any); ok {
			return fmt.Sprintf("ok %d entries", len(entries))
		}
	case "shell_exec":
		status := compactString(payload["status"])
		exitCode, _ := intFromAny(payload["exit_code"])
		if status == "" {
			status = "ok"
		}
		return fmt.Sprintf("%s exit=%d", status, exitCode)
	case "shell_start":
		if id := compactString(payload["command_id"]); id != "" {
			return "running " + id
		}
	}
	return "ok"
}

func summarizeToolArguments(arguments map[string]any) string {
	if len(arguments) == 0 {
		return ""
	}
	parts := make([]string, 0, len(arguments))
	if command, ok := arguments["command"].(string); ok && strings.TrimSpace(command) != "" {
		parts = append(parts, "command="+command)
	}
	if path, ok := arguments["path"].(string); ok && strings.TrimSpace(path) != "" {
		parts = append(parts, "path="+path)
	}
	if description, ok := arguments["description"].(string); ok && strings.TrimSpace(description) != "" {
		parts = append(parts, "description="+description)
	}
	if goal, ok := arguments["goal"].(string); ok && strings.TrimSpace(goal) != "" {
		parts = append(parts, "goal="+goal)
	}
	if len(parts) == 0 {
		return fmt.Sprintf("%d fields", len(arguments))
	}
	return strings.Join(parts, " | ")
}

func summarizeToolText(input string) string {
	text := strings.TrimSpace(strings.ReplaceAll(input, "\n", " "))
	if len(text) > 120 {
		return text[:117] + "..."
	}
	return text
}

func compactShellInvocation(toolName string, arguments map[string]any) string {
	command := compactString(arguments["command"])
	args := stringSliceArg(arguments, "args")
	if len(args) == 0 {
		return toolName + " " + command
	}
	return toolName + " " + command + " " + strings.Join(args, " ")
}

func sanitizeToolError(toolName, text string) string {
	out := strings.TrimSpace(strings.ReplaceAll(text, "\n", " "))
	out = strings.TrimPrefix(out, fmt.Sprintf("tool call %q: ", toolName))
	out = strings.TrimPrefix(out, fmt.Sprintf("tool call %q ", toolName))
	out = strings.TrimPrefix(out, "tool call ")
	if strings.HasPrefix(out, "\""+toolName+"\"") {
		out = strings.TrimSpace(strings.TrimPrefix(out, "\""+toolName+"\""))
	}
	out = strings.TrimLeft(out, ": ")
	return out
}

func ellipsizeForWidth(input string, width int) string {
	text := strings.TrimSpace(strings.ReplaceAll(input, "\n", " "))
	if width <= 0 {
		return text
	}
	if len(text) <= width {
		return text
	}
	if width <= 1 {
		return text[:width]
	}
	return text[:width-1] + "…"
}

func compactPath(v any) string {
	path := compactString(v)
	if path == "" {
		return ""
	}
	dir, file := filepath.Split(path)
	if dir == "" || dir == "/" {
		return path
	}
	base := filepath.Base(strings.TrimRight(dir, "/"))
	if base == "." || base == "/" || base == "" {
		return path
	}
	return filepath.Join(base, file)
}

func compactString(v any) string {
	s, _ := v.(string)
	return strings.TrimSpace(s)
}

func compactInt(v any) string {
	if n, ok := intFromAny(v); ok {
		return strconv.Itoa(n)
	}
	return "?"
}

func intFromAny(v any) (int, bool) {
	switch value := v.(type) {
	case int:
		return value, true
	case int32:
		return int(value), true
	case int64:
		return int(value), true
	case float64:
		return int(value), true
	case json.Number:
		n, err := value.Int64()
		if err == nil {
			return int(n), true
		}
	case string:
		n, err := strconv.Atoi(strings.TrimSpace(value))
		if err == nil {
			return n, true
		}
	}
	return 0, false
}

func stringSliceArg(args map[string]any, key string) []string {
	raw, ok := args[key]
	if !ok {
		return nil
	}
	list, ok := raw.([]any)
	if !ok {
		if typed, ok := raw.([]string); ok {
			return append([]string(nil), typed...)
		}
		return nil
	}
	out := make([]string, 0, len(list))
	for _, item := range list {
		if value, ok := item.(string); ok {
			out = append(out, value)
		}
	}
	return out
}

func workspaceJumpTargetFromToolFields(arguments map[string]any, resultText string) workspaceJumpTarget {
	target := workspaceJumpTarget{}
	if ref := compactString(arguments["artifact_ref"]); ref != "" {
		target.Kind = workspaceJumpArtifact
		target.ArtifactRef = ref
		return target
	}
	if ref := compactResultString(resultText, "artifact_ref"); ref != "" {
		target.Kind = workspaceJumpArtifact
		target.ArtifactRef = ref
		return target
	}
	if commandID := compactString(arguments["command_id"]); commandID != "" {
		target.Kind = workspaceJumpCommand
		target.CommandID = commandID
		return target
	}
	if commandID := compactResultString(resultText, "command_id"); commandID != "" {
		target.Kind = workspaceJumpCommand
		target.CommandID = commandID
		return target
	}
	if path := compactPath(arguments["path"]); path != "" {
		target.Kind = workspaceJumpPath
		target.Path = path
		return target
	}
	if path := compactResultString(resultText, "path"); path != "" {
		target.Kind = workspaceJumpPath
		target.Path = path
		return target
	}
	return target
}

func compactResultString(resultText, key string) string {
	if strings.TrimSpace(resultText) == "" {
		return ""
	}
	var payload map[string]any
	if err := json.Unmarshal([]byte(resultText), &payload); err != nil {
		return ""
	}
	return compactString(payload[key])
}
