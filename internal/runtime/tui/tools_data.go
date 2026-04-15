package tui

import (
	"fmt"
	"strings"

	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
)

func (m *model) currentApprovals() []shell.PendingApprovalView {
	if m.agent == nil || m.agent.ShellRuntime == nil {
		return nil
	}
	state := m.currentSessionState()
	if state == nil || state.Session == nil {
		return nil
	}
	approvals := m.agent.PendingShellApprovals(state.Session.SessionID)
	if m.approvalCursor >= len(approvals) && len(approvals) > 0 {
		m.approvalCursor = len(approvals) - 1
	}
	return approvals
}

func (m *model) currentRunningCommands() []projections.ShellCommandView {
	if m.agent == nil {
		return nil
	}
	state := m.currentSessionState()
	if state == nil || state.Session == nil {
		return nil
	}
	commands := m.agent.CurrentRunningShellCommands(state.Session.SessionID)
	if m.commandCursor >= len(commands) && len(commands) > 0 {
		m.commandCursor = len(commands) - 1
	}
	return commands
}

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
