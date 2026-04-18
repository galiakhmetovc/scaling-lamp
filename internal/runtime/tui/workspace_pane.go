package tui

import tea "github.com/charmbracelet/bubbletea"

func (m *model) updateWorkspace(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	return m, nil
}

func (m *model) viewWorkspace() string {
	return "Workspace pane\n\nWorkspace scaffold placeholder."
}
