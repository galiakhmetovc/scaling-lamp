package tui

import (
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"teamd/internal/runtime/projections"
)

func (m *model) updateChat(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	state := m.currentSessionState()
	if state == nil {
		return m, nil
	}
	switch msg.String() {
	case "pgup":
		state.ChatView.LineUp(max(1, state.ChatView.Height/2))
		return m, nil
	case "pgdown":
		state.ChatView.LineDown(max(1, state.ChatView.Height/2))
		return m, nil
	case "ctrl+s":
		if state.Busy {
			return m, nil
		}
		prompt := strings.TrimSpace(state.Input.Value())
		if prompt == "" {
			return m, nil
		}
		state.Input.Reset()
		state.Streaming.Reset()
		state.LastError = ""
		state.Status = "sending"
		state.Busy = true
		return m, runChatTurnCmd(m.agent, state.Session, prompt, state.Overrides)
	}
	var cmd tea.Cmd
	state.Input, cmd = state.Input.Update(msg)
	return m, cmd
}

func (m *model) viewChat() string {
	state := m.currentSessionState()
	if state == nil {
		return "No active session"
	}
	m.renderChatViewport(state)
	header := fmt.Sprintf("session: %s\nstatus: %s", state.Session.SessionID, coalesce(state.Status, "idle"))
	return lipgloss.JoinVertical(lipgloss.Left, header, state.ChatView.View(), "\nInput (Ctrl+S send, PgUp/PgDn scroll):", state.Input.View())
}

func (m *model) renderChatViewport(state *sessionState) {
	if state == nil || state.Session == nil {
		return
	}
	lines := []string{}
	for _, item := range m.agent.CurrentChatTimeline(state.Session.SessionID) {
		switch item.Kind {
		case projections.ChatTimelineItemMessage:
			lines = append(lines, strings.ToUpper(item.Role)+":")
			content := item.Content
			if item.Role == "assistant" && state.Overrides.RenderMarkdown {
				rendered, err := renderMarkdown(content, state.Overrides.MarkdownStyle)
				if err == nil {
					content = strings.TrimRight(rendered, "\n")
				}
			}
			lines = append(lines, content, "")
		default:
			rendered, err := renderMarkdown(item.Content, state.Overrides.MarkdownStyle)
			if err == nil {
				lines = append(lines, strings.TrimRight(rendered, "\n"), "")
			} else {
				lines = append(lines, item.Content, "")
			}
		}
	}
	if state.Streaming.Len() > 0 {
		lines = append(lines, "ASSISTANT:", state.Streaming.String(), "")
	}
	state.ChatView.SetContent(strings.Join(lines, "\n"))
	state.ChatView.GotoBottom()
}

func (m *model) handleMouseChat(msg tea.MouseMsg) bool {
	state := m.currentSessionState()
	if state == nil {
		return false
	}
	switch msg.Button {
	case tea.MouseButtonWheelUp:
		state.ChatView.LineUp(3)
		return true
	case tea.MouseButtonWheelDown:
		state.ChatView.LineDown(3)
		return true
	}
	if isWheelUp(msg) {
		state.ChatView.LineUp(3)
		return true
	}
	if isWheelDown(msg) {
		state.ChatView.LineDown(3)
		return true
	}
	return false
}
