package tui

import (
	"context"
	"strings"

	"github.com/charmbracelet/bubbles/textarea"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
)

func (m *model) updateSessions(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
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
		session, err := m.agent.NewChatSession()
		if err != nil {
			m.errMessage = err.Error()
			return m, nil
		}
		state := newSessionState(m.defaultOverrides())
		state.Session = session
		state.Loaded = true
		m.sessions[session.SessionID] = state
		m.sessionOrder = append([]string{session.SessionID}, m.sessionOrder...)
		m.activeSessionID = session.SessionID
		m.sessionCursor = 0
		m.tab = tabChat
	}
	return m, nil
}

func (m *model) viewSessions() string {
	lines := []string{"Sessions", "", "n = new session, Enter = activate"}
	m.mouseSessionTop = len(lines)
	for i, sessionID := range m.sessionOrder {
		state := m.sessions[sessionID]
		prefix := "  "
		if i == m.sessionCursor {
			prefix = "> "
		}
		title := sessionID
		if state != nil && state.Status != "" {
			title += " [" + state.Status + "]"
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
	entries := m.agent.ListSessions()
	for _, entry := range entries {
		state := newSessionState(m.defaultOverrides())
		resumed, err := m.agent.ResumeChatSession(context.Background(), entry.SessionID)
		if err == nil {
			state.Session = resumed
			state.Loaded = true
			m.renderChatViewport(state)
			m.renderToolsViewport(state)
			m.sessions[entry.SessionID] = state
			m.sessionOrder = append(m.sessionOrder, entry.SessionID)
		}
	}
	if resumeID != "" {
		if state, ok := m.sessions[resumeID]; ok {
			m.activeSessionID = resumeID
			state.Loaded = true
			return nil
		}
	}
	session, err := m.agent.NewChatSession()
	if err != nil {
		return err
	}
	state := newSessionState(m.defaultOverrides())
	state.Session = session
	state.Loaded = true
	m.sessions[session.SessionID] = state
	m.sessionOrder = append([]string{session.SessionID}, m.sessionOrder...)
	m.activeSessionID = session.SessionID
	return nil
}

func newSessionState(overrides sessionOverrides) *sessionState {
	input := textarea.New()
	input.Prompt = ""
	input.SetHeight(6)
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

func (m *model) defaultOverrides() sessionOverrides {
	return sessionOverrides{
		MaxToolRounds:          m.agent.MaxToolRounds,
		RenderMarkdown:         m.agent.Contracts.Chat.Output.Params.RenderMarkdown,
		MarkdownStyle:          coalesce(m.agent.Contracts.Chat.Output.Params.MarkdownStyle, "dark"),
		ShowToolCalls:          m.agent.Contracts.Chat.Status.Params.ShowToolCalls,
		ShowToolResults:        m.agent.Contracts.Chat.Status.Params.ShowToolResults,
		ShowPlanAfterPlanTools: m.agent.Contracts.Chat.Status.Params.ShowPlanAfterPlanTools,
	}
}

func (m *model) currentSessionState() *sessionState {
	if m.activeSessionID == "" {
		return nil
	}
	return m.sessions[m.activeSessionID]
}
