package tui

import tea "github.com/charmbracelet/bubbletea"

func (m *model) updateWorkspace(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	state := m.currentSessionState()
	if state == nil {
		return m, nil
	}
	switch msg.String() {
	case "2":
		state.Workspace.Mode = workspaceModeFiles
		if cmd := m.ensureWorkspaceFiles(state); cmd != nil {
			return m, cmd
		}
	case "4":
		state.Workspace.Mode = workspaceModeArtifacts
		if cmd := m.ensureWorkspaceArtifacts(state); cmd != nil {
			return m, cmd
		}
	}
	switch state.Workspace.Mode {
	case workspaceModeFiles:
		return m.updateWorkspaceFiles(state, msg)
	case workspaceModeArtifacts:
		return m.updateWorkspaceArtifacts(state, msg)
	default:
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
	}
	return m, nil
}

func (m *model) viewWorkspace() string {
	state := m.currentSessionState()
	if state == nil {
		return "No active session"
	}
	switch state.Workspace.Mode {
	case workspaceModeFiles:
		return m.workspaceFilesView(state)
	case workspaceModeArtifacts:
		return m.viewWorkspaceArtifacts(state)
	default:
		return m.workspaceTerminalView(state)
	}
}
