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
	if result.Session.MainRunActive || result.Session.MainRun.Active {
		state.Busy = true
		state.Status = "running"
		state.MainRun.Active = true
		if !result.Session.MainRun.StartedAt.IsZero() {
			state.MainRun.StartedAt = result.Session.MainRun.StartedAt
		}
		if result.Session.MainRun.Provider != "" {
			state.MainRun.Provider = result.Session.MainRun.Provider
		}
		if result.Session.MainRun.Model != "" {
			state.MainRun.Model = result.Session.MainRun.Model
		}
	} else {
		state.PendingPrompt = ""
		state.Busy = false
		state.Status = "idle"
		state.MainRun.Active = false
		state.MainRun.CompletedAt = m.now()
		state.RunCancel = nil
	}
	m.renderChatViewport(state)
	m.renderToolsViewport(state)
	m.statusMessage = status
	if state.MainRun.Active {
		return nil
	}
	if cmd := m.dispatchNextQueued(state); cmd != nil {
		return tea.Batch(cmd, tickClockCmd())
	}
	return nil
}
