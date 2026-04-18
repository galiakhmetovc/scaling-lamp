package tui

import tea "github.com/charmbracelet/bubbletea"

func (m *model) applyShellActionResult(state *sessionState, result ShellActionResult, status string) tea.Cmd {
	if state == nil {
		return nil
	}
	if result.Session.SessionID != "" {
		state.Snapshot = mergeSessionSnapshot(state.Snapshot, result.Session)
	}
	state.LastError = ""
	m.syncRunStateFromSnapshot(state, true)
	m.renderChatViewport(state)
	m.renderToolsViewport(state)
	m.statusMessage = status
	if state.MainRun.Active {
		return tea.Batch(reloadSessionSnapshotCmd(m.ctx, m.client, state.SessionID), tickClockCmd())
	}
	if cmd := m.dispatchNextQueued(state); cmd != nil {
		return tea.Batch(cmd, tickClockCmd())
	}
	return nil
}
