package tui

import (
	"fmt"
	"strings"
)

func reverseToolEntries(entries []toolLogEntry) []toolLogEntry {
	out := make([]toolLogEntry, 0, len(entries))
	for i := len(entries) - 1; i >= 0; i-- {
		out = append(out, entries[i])
	}
	return out
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
