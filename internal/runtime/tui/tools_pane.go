package tui

import (
	"context"
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"teamd/internal/shell"
)

func (m *model) updateTools(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	state := m.currentSessionState()
	if state == nil {
		return m, nil
	}
	approvals := m.currentApprovals()
	commands := m.currentRunningCommands()
	m.normalizeToolsFocus(len(approvals), len(commands), len(state.ToolLog))
	switch msg.String() {
	case "pgup":
		state.ToolsView.LineUp(max(1, state.ToolsView.Height/2))
	case "pgdown":
		state.ToolsView.LineDown(max(1, state.ToolsView.Height/2))
	case "a":
		if len(approvals) > 0 && m.toolsFocus == toolsFocusApprovals && m.agent.ShellRuntime != nil {
			if _, err := m.agent.ShellRuntime.Approve(context.Background(), approvals[m.approvalCursor].ApprovalID); err != nil {
				m.errMessage = err.Error()
			} else {
				m.statusMessage = "shell approval granted"
			}
		}
	case "x":
		if len(approvals) > 0 && m.toolsFocus == toolsFocusApprovals && m.agent.ShellRuntime != nil {
			if err := m.agent.ShellRuntime.Deny(context.Background(), approvals[m.approvalCursor].ApprovalID); err != nil {
				m.errMessage = err.Error()
			} else {
				m.statusMessage = "shell approval denied"
			}
		}
	case "k":
		if len(commands) > 0 && m.toolsFocus == toolsFocusCommands && m.agent != nil && m.agent.ShellRuntime != nil {
			if _, err := m.agent.ShellRuntime.ExecuteWithMeta(context.Background(), m.agent.Contracts.ShellExecution, "shell_kill", map[string]any{
				"command_id": commands[m.commandCursor].CommandID,
			}, shell.ExecutionMeta{}); err != nil {
				m.errMessage = err.Error()
			} else {
				m.statusMessage = "shell command kill requested"
			}
		}
	case "up":
		m.moveToolsSelection(-1, len(approvals), len(commands), len(state.ToolLog))
	case "down", "j":
		m.moveToolsSelection(1, len(approvals), len(commands), len(state.ToolLog))
	}
	return m, nil
}

func (m *model) moveToolsSelection(delta, approvals, commands, toolLog int) {
	switch m.toolsFocus {
	case toolsFocusApprovals:
		if approvals == 0 {
			m.toolsFocus = m.nextAvailableToolsFocus(0, commands, toolLog)
			return
		}
		next := m.approvalCursor + delta
		if next >= 0 && next < approvals {
			m.approvalCursor = next
			return
		}
		if delta > 0 {
			if commands > 0 {
				m.toolsFocus = toolsFocusCommands
			} else if toolLog > 0 {
				m.toolsFocus = toolsFocusLog
			}
		}
	case toolsFocusCommands:
		if commands == 0 {
			m.toolsFocus = m.nextAvailableToolsFocus(approvals, 0, toolLog)
			return
		}
		next := m.commandCursor + delta
		if next >= 0 && next < commands {
			m.commandCursor = next
			return
		}
		if delta < 0 && approvals > 0 {
			m.toolsFocus = toolsFocusApprovals
			return
		}
		if delta > 0 && toolLog > 0 {
			m.toolsFocus = toolsFocusLog
		}
	case toolsFocusLog:
		if toolLog == 0 {
			m.toolsFocus = m.nextAvailableToolsFocus(approvals, commands, 0)
			return
		}
		next := m.toolCursor - delta
		if next >= 0 && next < toolLog {
			m.toolCursor = next
			return
		}
		if delta < 0 && commands > 0 {
			m.toolsFocus = toolsFocusCommands
			return
		}
		if delta < 0 && approvals > 0 && commands == 0 {
			m.toolsFocus = toolsFocusApprovals
		}
	default:
		m.toolsFocus = m.nextAvailableToolsFocus(approvals, commands, toolLog)
	}
}

func (m *model) nextAvailableToolsFocus(approvals, commands, toolLog int) toolsFocusMode {
	if approvals > 0 {
		return toolsFocusApprovals
	}
	if commands > 0 {
		return toolsFocusCommands
	}
	if toolLog > 0 {
		return toolsFocusLog
	}
	return toolsFocusLog
}

func (m *model) viewTools() string {
	state := m.currentSessionState()
	if state == nil {
		return "No active session"
	}
	m.renderToolsViewport(state)
	left := state.ToolsView.View()
	right := m.renderToolDetails(state)
	leftWidth, rightWidth := splitPaneWidths(m.width, max(30, (m.width*2)/3), max(24, m.width/3))
	return lipgloss.JoinHorizontal(
		lipgloss.Top,
		lipgloss.NewStyle().Width(leftWidth).MaxWidth(leftWidth).Render(left),
		lipgloss.NewStyle().Width(rightWidth).MaxWidth(rightWidth).Render(right),
	)
}

func (m *model) renderToolsViewport(state *sessionState) {
	if state == nil {
		return
	}
	lines := []string{"Tools", ""}
	approvals := m.currentApprovals()
	commands := m.currentRunningCommands()
	m.normalizeToolsFocus(len(approvals), len(commands), len(state.ToolLog))
	if len(approvals) > 0 {
		lines = append(lines, "Pending Approvals")
		for i, approval := range approvals {
			line := fmt.Sprintf("[%s] %s", approval.ToolName, approval.Command)
			if m.toolsFocus == toolsFocusApprovals && i == m.approvalCursor {
				line = "> " + line
			} else {
				line = "  " + line
			}
			lines = append(lines, line)
		}
		lines = append(lines, "")
	}
	if len(commands) > 0 {
		lines = append(lines, "Running Shell Commands")
		for i, command := range commands {
			line := fmt.Sprintf("[%s] %s", command.Status, command.Command)
			if len(command.Args) > 0 {
				line += " " + strings.Join(command.Args, " ")
			}
			if m.toolsFocus == toolsFocusCommands && i == m.commandCursor {
				line = "> " + line
			} else {
				line = "  " + line
			}
			lines = append(lines, line)
		}
		lines = append(lines, "")
	}
	m.mouseToolTop = len(lines)
	entries := reverseToolEntries(state.ToolLog)
	if m.toolCursor >= len(entries) && len(entries) > 0 {
		m.toolCursor = len(entries) - 1
	}
	for i, entry := range entries {
		line := fmt.Sprintf("[%s] %s", entry.Activity.Phase, entry.Activity.Name)
		if entry.Activity.ErrorText != "" {
			line += " | error: " + entry.Activity.ErrorText
		} else if entry.Activity.ResultText != "" {
			line += " | ok"
		}
		if m.toolsFocus == toolsFocusLog && i == m.toolCursor {
			line = "> " + line
		} else {
			line = "  " + line
		}
		lines = append(lines, line)
	}
	state.ToolsView.SetContent(strings.Join(lines, "\n"))
}

func (m *model) normalizeToolsFocus(approvals, commands, toolLog int) {
	switch m.toolsFocus {
	case toolsFocusApprovals:
		if approvals > 0 {
			return
		}
	case toolsFocusCommands:
		if commands > 0 {
			return
		}
	case toolsFocusLog:
		if toolLog > 0 {
			return
		}
	}
	if approvals > 0 {
		m.toolsFocus = toolsFocusApprovals
		return
	}
	if commands > 0 {
		m.toolsFocus = toolsFocusCommands
		return
	}
	m.toolsFocus = toolsFocusLog
}

func (m *model) renderToolDetails(state *sessionState) string {
	approvals := m.currentApprovals()
	if len(approvals) > 0 && m.toolsFocus == toolsFocusApprovals && m.approvalCursor >= 0 && m.approvalCursor < len(approvals) {
		approval := approvals[m.approvalCursor]
		return strings.Join([]string{
			"Pending Approval",
			"",
			"Tool: " + approval.ToolName,
			"Command: " + approval.Command,
			"Args: " + strings.Join(approval.Args, " "),
			"Cwd: " + approval.Cwd,
			"Message: " + approval.Message,
			"",
			"`a` approve  `x` deny",
		}, "\n")
	}
	commands := m.currentRunningCommands()
	if len(commands) > 0 && m.toolsFocus == toolsFocusCommands && m.commandCursor >= 0 && m.commandCursor < len(commands) {
		command := commands[m.commandCursor]
		lines := []string{
			"Running Shell Command",
			"",
			"Command ID: " + command.CommandID,
			"Command: " + command.Command,
			"Args: " + strings.Join(command.Args, " "),
			"Cwd: " + command.Cwd,
			"Status: " + command.Status,
		}
		if command.LastChunk != "" {
			lines = append(lines, "Last output: "+command.LastChunk)
		}
		if command.Error != "" {
			lines = append(lines, "Error: "+command.Error)
		}
		lines = append(lines, "", "`k` kill running command")
		return strings.Join(lines, "\n")
	}
	entries := reverseToolEntries(state.ToolLog)
	if len(entries) == 0 || m.toolCursor < 0 || m.toolCursor >= len(entries) {
		return "Tool Details\n\nNo tool activity yet."
	}
	entry := entries[m.toolCursor]
	lines := []string{
		"Tool Details",
		"",
		"Name: " + entry.Activity.Name,
		"Phase: " + string(entry.Activity.Phase),
	}
	if len(entry.Activity.Arguments) > 0 {
		lines = append(lines, "Args: "+summarizeToolArguments(entry.Activity.Arguments))
	}
	if entry.Activity.ErrorText != "" {
		lines = append(lines, "Error: "+entry.Activity.ErrorText)
	}
	if entry.Activity.ResultText != "" {
		lines = append(lines, "Result: "+summarizeToolText(entry.Activity.ResultText))
	}
	lines = append(lines, "", "PgUp/PgDn scroll list, Up/Down select")
	return strings.Join(lines, "\n")
}

func (m *model) currentApprovals() []shell.PendingApprovalView {
	if m.agent == nil || m.agent.ShellRuntime == nil {
		return nil
	}
	state := m.currentSessionState()
	if state == nil || state.Session == nil {
		return nil
	}
	approvals := m.agent.ShellRuntime.PendingApprovals(state.Session.SessionID)
	if m.approvalCursor >= len(approvals) && len(approvals) > 0 {
		m.approvalCursor = len(approvals) - 1
	}
	return approvals
}

func (m *model) currentRunningCommands() []shell.ActiveCommandView {
	if m.agent == nil || m.agent.ShellRuntime == nil {
		return nil
	}
	state := m.currentSessionState()
	if state == nil || state.Session == nil {
		return nil
	}
	commands := m.agent.ShellRuntime.ActiveCommands(state.Session.SessionID)
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

func (m *model) handleMouseTools(msg tea.MouseMsg) bool {
	state := m.currentSessionState()
	if state == nil {
		return false
	}
	switch msg.Button {
	case tea.MouseButtonWheelUp:
		state.ToolsView.LineUp(3)
		return true
	case tea.MouseButtonWheelDown:
		state.ToolsView.LineDown(3)
		return true
	}
	if isWheelUp(msg) {
		state.ToolsView.LineUp(3)
		return true
	}
	if isWheelDown(msg) {
		state.ToolsView.LineDown(3)
		return true
	}
	return false
}
