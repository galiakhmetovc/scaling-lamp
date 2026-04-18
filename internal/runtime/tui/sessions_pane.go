package tui

import (
	"context"
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/textarea"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"

	"teamd/internal/runtime/daemon"
)

func (m *model) updateSessions(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	if m.sessionMode == sessionsModeRename {
		switch msg.String() {
		case "esc":
			m.sessionMode = sessionsModeBrowse
			return m, nil
		case "enter":
			if len(m.sessionOrder) == 0 {
				m.sessionMode = sessionsModeBrowse
				return m, nil
			}
			title := strings.TrimSpace(m.sessionTitleInput.Value())
			if title == "" {
				m.errMessage = "session label is empty"
				return m, nil
			}
			return m, renameSessionCmd(m.ctx, m.client, m.sessionOrder[m.sessionCursor], title)
		}
		var cmd tea.Cmd
		m.sessionTitleInput, cmd = m.sessionTitleInput.Update(msg)
		return m, cmd
	}
	if m.sessionMode == sessionsModeDeleteConfirm {
		switch msg.String() {
		case "y":
			if len(m.sessionOrder) == 0 {
				m.sessionMode = sessionsModeBrowse
				return m, nil
			}
			return m, deleteSessionCmd(m.ctx, m.client, m.sessionOrder[m.sessionCursor])
		case "n", "esc":
			m.sessionMode = sessionsModeBrowse
		}
		return m, nil
	}
	switch msg.String() {
	case "up", "k":
		if m.sessionCursor > 0 {
			m.sessionCursor--
		}
	case "down", "j":
		if m.sessionCursor < len(m.sessionOrder)-1 {
			m.sessionCursor++
		}
	case "enter":
		if len(m.sessionOrder) == 0 {
			return m, nil
		}
		m.activeSessionID = m.sessionOrder[m.sessionCursor]
		if state := m.currentSessionState(); state != nil {
			state.Input.Focus()
		}
		m.tab = tabChat
	case "n":
		session, err := m.client.CreateSession(m.ctx)
		if err != nil {
			m.errMessage = err.Error()
			return m, nil
		}
		state := newSessionState(m.defaultOverrides())
		state.SessionID = session.SessionID
		state.Snapshot = session
		state.Loaded = true
		m.sessions[session.SessionID] = state
		m.sessionOrder = append([]string{session.SessionID}, m.sessionOrder...)
		m.activeSessionID = session.SessionID
		m.sessionCursor = 0
		m.tab = tabChat
	case "r":
		if len(m.sessionOrder) == 0 {
			return m, nil
		}
		sessionID := m.sessionOrder[m.sessionCursor]
		title := sessionID
		if state := m.sessions[sessionID]; state != nil && strings.TrimSpace(state.Snapshot.Title) != "" {
			title = state.Snapshot.Title
		}
		m.sessionTitleInput.SetValue(title)
		m.sessionTitleInput.CursorEnd()
		m.sessionTitleInput.Focus()
		m.sessionMode = sessionsModeRename
	case "d":
		if len(m.sessionOrder) == 0 {
			return m, nil
		}
		m.sessionMode = sessionsModeDeleteConfirm
	}
	return m, nil
}

func (m *model) viewSessions() string {
	lines := []string{"Sessions", "", "n = new session, Enter = activate, r = rename label, d = delete"}
	if m.sessionMode == sessionsModeRename {
		lines = append(lines, "", "Rename session label:", m.sessionTitleInput.View(), "Enter save, Esc cancel")
		return strings.Join(lines, "\n")
	}
	if m.sessionMode == sessionsModeDeleteConfirm {
		target := ""
		if len(m.sessionOrder) > 0 {
			target = m.sessionOrder[m.sessionCursor]
			if state := m.sessions[target]; state != nil && strings.TrimSpace(state.Snapshot.Title) != "" {
				target = state.Snapshot.Title
			}
		}
		lines = append(lines, "", fmt.Sprintf("Delete %q permanently? y/n", target))
		return strings.Join(lines, "\n")
	}
	m.mouseSessionTop = len(lines)
	for i, sessionID := range m.sessionOrder {
		state := m.sessions[sessionID]
		prefix := "  "
		if i == m.sessionCursor {
			prefix = "> "
		}
		title := sessionID
		if state != nil && strings.TrimSpace(state.Snapshot.Title) != "" {
			title = state.Snapshot.Title
		}
		if state != nil && state.Status != "" {
			title += " [" + state.Status + "]"
		}
		if state != nil {
			title += fmt.Sprintf(" | created %s | active %s", humanTimestamp(state.Snapshot.CreatedAt), humanTimestamp(state.Snapshot.LastActivity))
		}
		lines = append(lines, prefix+title)
	}
	return strings.Join(lines, "\n")
}

func (m *model) handleMouseSessions(msg tea.MouseMsg) bool {
	if msg.Button != tea.MouseButtonLeft || msg.Action != tea.MouseActionRelease {
		return false
	}
	row := msg.Y - m.mouseSessionTop
	if row < 0 || row >= len(m.sessionOrder) {
		return false
	}
	m.sessionCursor = row
	m.activeSessionID = m.sessionOrder[row]
	return true
}

func (m *model) loadSessions(resumeID string) error {
	entries, err := m.client.ListSessions(context.Background())
	if err != nil {
		return err
	}
	for _, entry := range entries {
		state := newSessionState(m.defaultOverrides())
		snapshot, err := m.client.GetSession(context.Background(), entry.SessionID)
		if err != nil {
			continue
		}
		state.SessionID = entry.SessionID
		state.Snapshot = snapshot
		state.Status = "idle"
		state.Loaded = true
		m.renderChatViewport(state)
		m.renderToolsViewport(state)
		m.sessions[entry.SessionID] = state
		m.sessionOrder = append(m.sessionOrder, entry.SessionID)
	}
	if resumeID != "" {
		if state, ok := m.sessions[resumeID]; ok {
			m.activeSessionID = resumeID
			state.Loaded = true
			return nil
		}
	}
	if len(m.sessionOrder) > 0 {
		m.activeSessionID = m.sessionOrder[0]
	}
	return nil
}

func renameSessionCmd(ctx context.Context, client OperatorClient, sessionID, title string) tea.Cmd {
	return func() tea.Msg {
		session, err := client.RenameSession(ctx, sessionID, title)
		return sessionRenamedMsg{Session: session, Err: err}
	}
}

func deleteSessionCmd(ctx context.Context, client OperatorClient, sessionID string) tea.Cmd {
	return func() tea.Msg {
		err := client.DeleteSession(ctx, sessionID)
		return sessionDeletedMsg{SessionID: sessionID, Err: err}
	}
}

func newSessionState(overrides sessionOverrides) *sessionState {
	input := textarea.New()
	input.Prompt = ""
	input.CharLimit = 0
	input.SetHeight(5)
	input.Focus()
	chatView := viewport.New(80, 20)
	chatView.MouseWheelEnabled = true
	chatView.MouseWheelDelta = 3
	toolsView := viewport.New(80, 20)
	toolsView.MouseWheelEnabled = true
	toolsView.MouseWheelDelta = 3
	return &sessionState{
		Input:     input,
		Overrides: overrides,
		ChatView:  chatView,
		ToolsView: toolsView,
		Status:    "idle",
	}
}

func mergeSessionSnapshot(current, refreshed daemon.SessionSnapshot) daemon.SessionSnapshot {
	merged := refreshed
	if merged.ContextBudget.LastInputTokens <= 0 {
		merged.ContextBudget.LastInputTokens = current.ContextBudget.LastInputTokens
	}
	if merged.ContextBudget.LastOutputTokens <= 0 {
		merged.ContextBudget.LastOutputTokens = current.ContextBudget.LastOutputTokens
	}
	if merged.ContextBudget.LastTotalTokens <= 0 {
		merged.ContextBudget.LastTotalTokens = current.ContextBudget.LastTotalTokens
	}
	if merged.ContextBudget.CurrentContextTokens <= 0 {
		merged.ContextBudget.CurrentContextTokens = current.ContextBudget.CurrentContextTokens
	}
	if merged.ContextBudget.EstimatedNextInputTokens <= 0 {
		merged.ContextBudget.EstimatedNextInputTokens = current.ContextBudget.EstimatedNextInputTokens
	}
	if merged.ContextBudget.DraftTokens <= 0 {
		merged.ContextBudget.DraftTokens = current.ContextBudget.DraftTokens
	}
	if merged.ContextBudget.QueuedDraftTokens <= 0 {
		merged.ContextBudget.QueuedDraftTokens = current.ContextBudget.QueuedDraftTokens
	}
	if merged.ContextBudget.SummaryTokens <= 0 {
		merged.ContextBudget.SummaryTokens = current.ContextBudget.SummaryTokens
	}
	if merged.ContextBudget.SummarizationCount <= 0 {
		merged.ContextBudget.SummarizationCount = current.ContextBudget.SummarizationCount
	}
	if merged.ContextBudget.CompactedMessageCount <= 0 {
		merged.ContextBudget.CompactedMessageCount = current.ContextBudget.CompactedMessageCount
	}
	if merged.ContextBudget.Source == "" {
		merged.ContextBudget.Source = current.ContextBudget.Source
	}
	if merged.ContextBudget.BudgetState == "" {
		merged.ContextBudget.BudgetState = current.ContextBudget.BudgetState
	}
	return merged
}

func (m *model) defaultOverrides() sessionOverrides {
	return m.client.DefaultOverrides()
}

func (m *model) syncRunStateFromSnapshot(state *sessionState, preserveActive bool) {
	if state == nil {
		return
	}
	active := state.Snapshot.MainRunActive || state.Snapshot.MainRun.Active
	phase := strings.TrimSpace(state.Snapshot.MainRun.Phase)
	if phase == "running" || phase == "waiting_approval" || phase == "waiting_shell" || phase == "resuming" {
		active = true
	}
	if (preserveActive || state.AwaitingRunCompletion) && state.MainRun.Active && !active {
		active = true
	}
	if !state.Snapshot.MainRun.StartedAt.IsZero() {
		state.MainRun.StartedAt = state.Snapshot.MainRun.StartedAt
	}
	if state.Snapshot.MainRun.Provider != "" {
		state.MainRun.Provider = state.Snapshot.MainRun.Provider
	}
	if state.Snapshot.MainRun.Model != "" {
		state.MainRun.Model = state.Snapshot.MainRun.Model
	}
	if state.Snapshot.MainRun.InputTokens > 0 {
		state.MainRun.InputTokens = state.Snapshot.MainRun.InputTokens
	}
	if state.Snapshot.MainRun.OutputTokens > 0 {
		state.MainRun.OutputTokens = state.Snapshot.MainRun.OutputTokens
	}
	if state.Snapshot.MainRun.TotalTokens > 0 {
		state.MainRun.TotalTokens = state.Snapshot.MainRun.TotalTokens
	}
	if active {
		state.MainRun.Active = true
		state.Busy = true
		switch phase {
		case "waiting_approval":
			state.Status = "approval_pending"
		case "waiting_shell":
			state.Status = "waiting_shell"
		default:
			if state.Status == "" || state.Status == "idle" || state.Status == "done" || state.Status == "waiting_shell" {
				state.Status = "running"
			}
		}
		return
	}
	state.MainRun.Active = false
	state.Busy = false
	state.PendingPrompt = ""
	state.RunCancel = nil
	state.MainRun.CompletedAt = m.now()
	state.AwaitingRunCompletion = false
	state.LastTurnEndedAt = state.MainRun.CompletedAt
	if state.Status == "" || state.Status == "running" || state.Status == "approval_pending" || state.Status == "waiting_shell" {
		state.Status = "idle"
	}
}

func (m *model) currentSessionState() *sessionState {
	if m.activeSessionID == "" {
		return nil
	}
	state := m.sessions[m.activeSessionID]
	if state == nil {
		return nil
	}
	if _, ok := m.client.(*localClient); ok {
		if snapshot, err := m.client.GetSession(context.Background(), m.activeSessionID); err == nil {
			state.Snapshot = mergeSessionSnapshot(state.Snapshot, snapshot)
		}
	}
	return state
}

func (m *model) reloadSessionSnapshot(sessionID string) error {
	snapshot, err := m.client.GetSession(context.Background(), sessionID)
	if err != nil {
		return err
	}
	state := m.sessions[sessionID]
	if state == nil {
		state = newSessionState(m.defaultOverrides())
		state.SessionID = sessionID
		m.sessions[sessionID] = state
		m.sessionOrder = append(m.sessionOrder, sessionID)
	}
	state.SessionID = sessionID
	state.Snapshot = snapshot
	m.syncRunStateFromSnapshot(state, false)
	state.Loaded = true
	m.renderChatViewport(state)
	m.renderToolsViewport(state)
	return nil
}
