package tui

import tea "github.com/charmbracelet/bubbletea"

func (m *model) applyShellActionResult(state *sessionState, result ShellActionResult, status string) tea.Cmd {
	if state == nil {
		return nil
	}
	state.Snapshot = mergeSessionSnapshot(state.Snapshot, result.Session)
	state.PendingPrompt = ""
	state.Busy = false
	state.Status = "idle"
	state.LastError = ""
	state.MainRun.Active = false
	state.MainRun.CompletedAt = m.now()
	state.RunCancel = nil
	m.renderChatViewport(state)
	m.renderToolsViewport(state)
	m.statusMessage = status
	if cmd := m.dispatchNextQueued(state); cmd != nil {
		return tea.Batch(cmd, tickClockCmd())
	}
	return nil
}
