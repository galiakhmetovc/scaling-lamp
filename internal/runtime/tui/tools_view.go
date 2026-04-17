package tui

import (
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

func (m *model) viewTools() string {
	state := m.currentSessionState()
	if state == nil {
		return "No active session"
	}
	m.renderToolsViewport(state)
	left := state.ToolsView.View()
	right := clampLines(m.renderToolDetails(state), state.ToolsView.Height)
	leftWidth, rightWidth := splitPaneWidths(m.width, max(30, (m.width*2)/3), max(24, m.width/3))
	return lipgloss.JoinHorizontal(
		lipgloss.Top,
		lipgloss.NewStyle().Width(leftWidth).MaxWidth(leftWidth).Render(left),
		lipgloss.NewStyle().Width(rightWidth).MaxWidth(rightWidth).Height(state.ToolsView.Height).MaxHeight(state.ToolsView.Height).Render(right),
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
			line := prefixTimestamp(approval.OccurredAt, toolLineForCommand(approval.CommandID, approval.ToolName, approval.Command, approval.Args))
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
			line := prefixTimestamp(command.OccurredAt, toolLineForCommand(command.CommandID, command.Status, command.Command, command.Args))
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
		line := prefixTimestamp(entry.Activity.OccurredAt, "["+string(entry.Activity.Phase)+"] "+entry.Activity.Name)
		if entry.Activity.ErrorText != "" {
			if strings.Contains(entry.Activity.ErrorText, "requires approval") {
				line += " | " + ansiToolAccent("approval required", "1;38;5;214")
			} else {
				line += " | " + ansiToolAccent("error: "+entry.Activity.ErrorText, "1;38;5;203")
			}
		} else if entry.Activity.ResultText != "" {
			line += " | " + ansiToolAccent("ok", "1;38;5;120")
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

func (m *model) renderToolDetails(state *sessionState) string {
	approvals := m.currentApprovals()
	if len(approvals) > 0 && m.toolsFocus == toolsFocusApprovals && m.approvalCursor >= 0 && m.approvalCursor < len(approvals) {
		approval := approvals[m.approvalCursor]
		return strings.Join([]string{
			"Pending Approval",
			"",
			"Time: " + humanTimestamp(approval.OccurredAt),
			"Tool: " + approval.ToolName,
			"Command: " + approval.Command,
			"Args: " + strings.Join(approval.Args, " "),
			"Cwd: " + approval.Cwd,
			"Message: " + approval.Message,
			"",
			"`a` approve  `x` deny",
			"`A` allow forever  `X` deny forever",
		}, "\n")
	}
	commands := m.currentRunningCommands()
	if len(commands) > 0 && m.toolsFocus == toolsFocusCommands && m.commandCursor >= 0 && m.commandCursor < len(commands) {
		command := commands[m.commandCursor]
		lines := []string{
			"Running Shell Command",
			"",
			"Time: " + humanTimestamp(command.OccurredAt),
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
		"Time: " + humanTimestamp(entry.Activity.OccurredAt),
		"Name: " + entry.Activity.Name,
		"Phase: " + string(entry.Activity.Phase),
	}
	if len(entry.Activity.Arguments) > 0 {
		lines = append(lines, "Args: "+summarizeToolArguments(entry.Activity.Arguments))
	}
	if entry.Activity.ErrorText != "" {
		lines = append(lines, ansiToolAccent("Error: "+entry.Activity.ErrorText, "1;38;5;203"))
	}
	if entry.Activity.ResultText != "" {
		lines = append(lines, ansiToolAccent("Result:", "1;38;5;120"), entry.Activity.ResultText)
	}
	lines = append(lines, "", "PgUp/PgDn scroll list, Up/Down select")
	return strings.Join(lines, "\n")
}

func ansiToolAccent(text, sgr string) string {
	return "\x1b[" + sgr + "m" + text + "\x1b[0m"
}

func clampLines(input string, height int) string {
	if height <= 0 {
		return input
	}
	lines := strings.Split(input, "\n")
	if len(lines) <= height {
		return input
	}
	if height == 1 {
		return lines[0]
	}
	clamped := append([]string{}, lines[:height-1]...)
	clamped = append(clamped, fmt.Sprintf("… (%d more lines)", len(lines)-height+1))
	return strings.Join(clamped, "\n")
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
