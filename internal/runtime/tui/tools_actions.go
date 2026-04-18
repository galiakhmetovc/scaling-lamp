package tui

import (
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
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
	case "o", "enter":
		return m, m.jumpToWorkspaceFromTools()
	case "a":
		if len(approvals) > 0 && m.toolsFocus == toolsFocusApprovals {
			state.Status = "running"
			return m, tea.Batch(runShellActionCmd(m.ctx, m.client, state.SessionID, approvals[m.approvalCursor].ApprovalID, "approve"), tickClockCmd())
		}
	case "A":
		if len(approvals) > 0 && m.toolsFocus == toolsFocusApprovals {
			state.Status = "running"
			return m, tea.Batch(runShellActionCmd(m.ctx, m.client, state.SessionID, approvals[m.approvalCursor].ApprovalID, "allow_forever"), tickClockCmd())
		}
	case "x":
		if len(approvals) > 0 && m.toolsFocus == toolsFocusApprovals {
			state.Status = "running"
			return m, tea.Batch(runShellActionCmd(m.ctx, m.client, state.SessionID, approvals[m.approvalCursor].ApprovalID, "deny"), tickClockCmd())
		}
	case "X":
		if len(approvals) > 0 && m.toolsFocus == toolsFocusApprovals {
			state.Status = "running"
			return m, tea.Batch(runShellActionCmd(m.ctx, m.client, state.SessionID, approvals[m.approvalCursor].ApprovalID, "deny_forever"), tickClockCmd())
		}
	case "k":
		if len(commands) > 0 && m.toolsFocus == toolsFocusCommands {
			command := commands[m.commandCursor]
			result, err := m.client.KillShell(m.ctx, command.CommandID)
			if err != nil {
				m.errMessage = err.Error()
			} else {
				state.Snapshot = result.Session
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
			m.toolsFocus = nextAvailableToolsFocus(0, commands, toolLog)
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
			m.toolsFocus = nextAvailableToolsFocus(approvals, 0, toolLog)
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
			m.toolsFocus = nextAvailableToolsFocus(approvals, commands, 0)
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
		m.toolsFocus = nextAvailableToolsFocus(approvals, commands, toolLog)
	}
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
	m.toolsFocus = nextAvailableToolsFocus(approvals, commands, toolLog)
}

func nextAvailableToolsFocus(approvals, commands, toolLog int) toolsFocusMode {
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

func toolLineForCommand(commandID, status, command string, args []string) string {
	line := fmt.Sprintf("[%s] %s", status, command)
	if len(args) > 0 {
		line += " " + strings.Join(args, " ")
	}
	if commandID != "" {
		line += " (" + commandID + ")"
	}
	return line
}
