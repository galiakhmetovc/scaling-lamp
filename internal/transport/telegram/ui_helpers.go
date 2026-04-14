package telegram

import (
	"fmt"
	"strconv"
	"strings"

	"teamd/internal/mesh"
	"teamd/internal/provider"
)

func (a *Adapter) sessionSummary(chatID int64, prefix string) string {
	active, _ := a.store.ActiveSession(chatID)
	messages, _ := a.store.Messages(chatID)
	return prefix + "\nactive: " + active + "\nmessages: " + strconv.Itoa(len(messages))
}

func sessionKeyboard() map[string]any {
	return map[string]any{
		"inline_keyboard": [][]map[string]string{
			{
				{"text": "List", "callback_data": "session:list"},
				{"text": "Stats", "callback_data": "session:stats"},
			},
			{
				{"text": "Reset", "callback_data": "session:reset"},
			},
		},
	}
}

func approvalKeyboard(id string) map[string]any {
	return map[string]any{
		"inline_keyboard": [][]map[string]string{
			{
				{"text": "Approve", "callback_data": "approval:approve:" + id},
				{"text": "Reject", "callback_data": "approval:reject:" + id},
			},
		},
	}
}

func summarizeToolCall(call provider.ToolCall) string {
	args := make([]string, 0, 4)
	if command, _ := call.Arguments["command"].(string); strings.TrimSpace(command) != "" {
		args = append(args, "command="+truncateToolValue(command, 48))
	}
	if path, _ := call.Arguments["path"].(string); strings.TrimSpace(path) != "" {
		args = append(args, "path="+truncateToolValue(path, 48))
	}
	if cwd, _ := call.Arguments["cwd"].(string); strings.TrimSpace(cwd) != "" {
		args = append(args, "cwd="+truncateToolValue(cwd, 32))
	}
	if content, _ := call.Arguments["content"].(string); strings.TrimSpace(content) != "" {
		args = append(args, fmt.Sprintf("content=%d bytes", len(content)))
	}
	if len(args) == 0 {
		return "параметры скрыты"
	}
	return strings.Join(args, ", ")
}

func truncateToolValue(value string, limit int) string {
	value = strings.TrimSpace(value)
	if len(value) <= limit {
		return value
	}
	return value[:limit] + "..."
}

func toolIcon(name string) string {
	switch name {
	case "shell.exec":
		return "🖥️"
	case "filesystem.read_file":
		return "📄"
	case "filesystem.write_file":
		return "✍️"
	case "filesystem.list_dir":
		return "📁"
	default:
		return "🛠️"
	}
}

func mustRun(run *RunState, ok bool) *RunState {
	if !ok {
		return nil
	}
	return run
}

func convertMeshTrace(events []mesh.TraceEvent) []TraceEntry {
	out := make([]TraceEntry, 0, len(events))
	for _, event := range events {
		out = append(out, TraceEntry{
			Section: event.Section,
			Summary: event.Summary,
			Payload: event.Payload,
		})
	}
	return out
}
