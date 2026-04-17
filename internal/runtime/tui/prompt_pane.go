package tui

import (
	"context"
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
)

func (m *model) ensurePromptEditorLoaded() {
	state := m.currentSessionState()
	if state == nil {
		return
	}
	if m.promptLoadedSession == state.SessionID && m.promptDirty {
		return
	}
	if m.promptLoadedSession == state.SessionID && m.promptEditor.Value() != "" {
		return
	}
	content := state.Snapshot.Prompt.Override
	if strings.TrimSpace(content) == "" {
		content = state.Snapshot.Prompt.Effective
	}
	m.promptEditor.SetValue(content)
	m.promptLoadedSession = state.SessionID
	m.promptDirty = false
}

func (m *model) updatePrompt(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	state := m.currentSessionState()
	if state == nil {
		return m, nil
	}
	m.ensurePromptEditorLoaded()
	switch msg.String() {
	case "ctrl+s":
		content := strings.TrimSpace(m.promptEditor.Value())
		if content == "" {
			m.errMessage = "prompt override is empty"
			return m, nil
		}
		return m, savePromptCmd(m.ctx, m.client, state.SessionID, content)
	case "ctrl+r":
		return m, resetPromptCmd(m.ctx, m.client, state.SessionID)
	}
	var cmd tea.Cmd
	before := m.promptEditor.Value()
	m.promptEditor, cmd = m.promptEditor.Update(msg)
	m.promptDirty = m.promptDirty || before != m.promptEditor.Value()
	return m, cmd
}

func (m *model) viewPrompt() string {
	state := m.currentSessionState()
	if state == nil {
		return "No active session"
	}
	m.ensurePromptEditorLoaded()
	status := "default"
	if state.Snapshot.Prompt.HasOverride {
		status = "overridden"
	}
	lines := []string{
		"System Prompt",
		"",
		"Ctrl+S save override, Ctrl+R reset to default",
		"status: " + status,
	}
	if strings.TrimSpace(state.Snapshot.Prompt.Default) != "" {
		lines = append(lines, "default bytes: "+itoa(len(state.Snapshot.Prompt.Default)))
	}
	lines = append(lines, "", m.promptEditor.View())
	return strings.Join(lines, "\n")
}

func savePromptCmd(ctx context.Context, client OperatorClient, sessionID, content string) tea.Cmd {
	return func() tea.Msg {
		session, err := client.SetSessionPromptOverride(ctx, sessionID, content)
		return promptSavedMsg{Session: session, Err: err}
	}
}

func resetPromptCmd(ctx context.Context, client OperatorClient, sessionID string) tea.Cmd {
	return func() tea.Msg {
		session, err := client.ClearSessionPromptOverride(ctx, sessionID)
		return promptResetMsg{Session: session, Err: err}
	}
}

func itoa(v int) string {
	return fmt.Sprintf("%d", v)
}
