package tui

import tea "github.com/charmbracelet/bubbletea"

func (m *model) updateWorkspace(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	state := m.currentSessionState()
	if state == nil {
		return m, nil
	}
	if state.Workspace.Mode != workspaceModeTerminal {
		state.Workspace.Mode = workspaceModeTerminal
	}
	if !state.Workspace.Loaded || state.Workspace.PTY.PTYID == "" || state.Workspace.PTY.SessionID != state.SessionID {
		if cmd := m.ensureWorkspacePTY(state); cmd != nil {
			return m, cmd
		}
	}
	if data, ok := workspaceTerminalInput(msg); ok {
		return m, workspacePTYInputCmd(m.ctx, m.client, state.SessionID, state.Workspace.PTY.PTYID, data)
	}
	if msg.String() == "ctrl+l" {
		return m, m.workspaceTerminalShellRefresh(state)
	}
	return m, nil
}

func (m *model) viewWorkspace() string {
	state := m.currentSessionState()
	if state == nil {
		return "No active session"
	}
	if state.Workspace.Mode != workspaceModeTerminal {
		state.Workspace.Mode = workspaceModeTerminal
	}
	return m.workspaceTerminalView(state)
}
