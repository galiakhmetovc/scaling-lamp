package tui

import (
	"context"
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/textarea"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"teamd/internal/runtime/workspace"
)

func (m *model) ensureWorkspaceEditor(state *sessionState) tea.Cmd {
	if state == nil || state.SessionID == "" {
		return nil
	}
	m.initWorkspaceEditor(state)
	state.Workspace.Mode = workspaceModeEditor

	path := strings.TrimSpace(state.Workspace.PendingEditorPath)
	if path == "" {
		path = strings.TrimSpace(state.Workspace.Editor.Buffer.Path)
	}
	if path == "" {
		return nil
	}
	if state.Workspace.Editor.Loaded && state.Workspace.Editor.Buffer.SessionID == state.SessionID && state.Workspace.Editor.Buffer.Path == path {
		return nil
	}
	return workspaceEditorOpenCmd(m.ctx, m.client, state.SessionID, path)
}

func (m *model) initWorkspaceEditor(state *sessionState) {
	if state == nil || state.Workspace.Editor.Initialized {
		return
	}
	state.Workspace.Editor.Editor = textarea.New()
	state.Workspace.Editor.Editor.Prompt = ""
	state.Workspace.Editor.Editor.ShowLineNumbers = true
	state.Workspace.Editor.Editor.Focus()
	state.Workspace.Editor.Initialized = true
	m.resizeWorkspaceEditor(state)
}

func (m *model) applyWorkspaceEditorBuffer(state *sessionState, buf workspace.EditorBuffer) {
	if state == nil {
		return
	}
	m.initWorkspaceEditor(state)
	state.Workspace.Mode = workspaceModeEditor
	state.Workspace.PendingEditorPath = ""
	state.Workspace.Editor.Buffer = buf
	state.Workspace.Editor.Loaded = true
	state.Workspace.Editor.LastSync = m.now()
	state.Workspace.Editor.Editor.SetValue(buf.Content)
	state.Workspace.Editor.Editor.CursorEnd()
	state.Workspace.Editor.Editor.Focus()
}

func (m *model) resizeWorkspaceEditor(state *sessionState) {
	if state == nil || !state.Workspace.Editor.Initialized {
		return
	}
	width := max(42, m.width-(m.width/5)-4)
	height := m.workspaceFilesPaneHeight()
	state.Workspace.Editor.Editor.SetWidth(width)
	state.Workspace.Editor.Editor.SetHeight(max(6, height-6))
}

func workspaceEditorOpenCmd(ctx context.Context, client OperatorClient, sessionID, relPath string) tea.Cmd {
	return func() tea.Msg {
		result, err := client.WorkspaceEditorOpen(ctx, sessionID, relPath)
		return workspaceEditorOpenedMsg{SessionID: sessionID, Path: relPath, Result: result, Err: err}
	}
}

func workspaceEditorUpdateCmd(ctx context.Context, client OperatorClient, sessionID, relPath, content string) tea.Cmd {
	return func() tea.Msg {
		result, err := client.WorkspaceEditorUpdate(ctx, sessionID, relPath, content)
		return workspaceEditorUpdatedMsg{SessionID: sessionID, Path: relPath, Result: result, Err: err}
	}
}

func workspaceEditorSaveCmd(ctx context.Context, client OperatorClient, sessionID, relPath string) tea.Cmd {
	return func() tea.Msg {
		result, err := client.WorkspaceEditorSave(ctx, sessionID, relPath)
		return workspaceEditorSavedMsg{SessionID: sessionID, Path: relPath, Result: result, Err: err}
	}
}

func (m *model) updateWorkspaceEditor(state *sessionState, msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	if state == nil {
		return m, nil
	}
	if cmd := m.ensureWorkspaceEditor(state); cmd != nil && !state.Workspace.Editor.Loaded {
		return m, cmd
	}
	switch msg.String() {
	case "ctrl+s":
		if strings.TrimSpace(state.Workspace.Editor.Buffer.Path) == "" {
			return m, nil
		}
		return m, workspaceEditorSaveCmd(m.ctx, m.client, state.SessionID, state.Workspace.Editor.Buffer.Path)
	case "1":
		state.Workspace.Mode = workspaceModeTerminal
		if cmd := m.ensureWorkspacePTY(state); cmd != nil {
			return m, tea.Batch(cmd, tickClockCmd())
		}
	case "2":
		state.Workspace.Mode = workspaceModeFiles
		if cmd := m.ensureWorkspaceFiles(state); cmd != nil {
			return m, cmd
		}
	case "3":
		return m, nil
	case "4":
		state.Workspace.Mode = workspaceModeArtifacts
		if cmd := m.ensureWorkspaceArtifacts(state); cmd != nil {
			return m, cmd
		}
	}
	if !state.Workspace.Editor.Initialized {
		m.initWorkspaceEditor(state)
	}
	if handled := m.applyWorkspaceEditorKey(state, msg); handled {
		content := state.Workspace.Editor.Editor.Value()
		state.Workspace.Editor.Buffer.Content = content
		state.Workspace.Editor.Buffer.Dirty = true
		state.Workspace.Editor.Loaded = true
		return m, workspaceEditorUpdateCmd(m.ctx, m.client, state.SessionID, state.Workspace.Editor.Buffer.Path, content)
	}
	var textareaCmd tea.Cmd
	state.Workspace.Editor.Editor, textareaCmd = state.Workspace.Editor.Editor.Update(msg)
	return m, textareaCmd
}

func (m *model) applyWorkspaceEditorKey(state *sessionState, msg tea.KeyMsg) bool {
	if state == nil {
		return false
	}
	switch msg.Type {
	case tea.KeyRunes:
		for _, r := range msg.Runes {
			state.Workspace.Editor.Editor.InsertRune(r)
		}
		return len(msg.Runes) > 0
	case tea.KeySpace:
		state.Workspace.Editor.Editor.InsertRune(' ')
		return true
	case tea.KeyEnter:
		state.Workspace.Editor.Editor.InsertRune('\n')
		return true
	case tea.KeyBackspace, tea.KeyDelete:
		content := []rune(state.Workspace.Editor.Editor.Value())
		if len(content) == 0 {
			return false
		}
		state.Workspace.Editor.Editor.SetValue(string(content[:len(content)-1]))
		state.Workspace.Editor.Editor.CursorEnd()
		return true
	default:
		return false
	}
}

func (m *model) viewWorkspaceEditor(state *sessionState) string {
	navigator := lipgloss.NewStyle().
		Width(max(18, m.width/5)).
		MaxWidth(max(18, m.width/5)).
		Height(m.workspaceFilesPaneHeight()).
		MaxHeight(m.workspaceFilesPaneHeight()).
		Render(m.renderWorkspaceNavigator(state))
	editor := lipgloss.NewStyle().
		Width(max(42, m.width-(m.width/5)-4)).
		MaxWidth(max(42, m.width-(m.width/5)-4)).
		Height(m.workspaceFilesPaneHeight()).
		MaxHeight(m.workspaceFilesPaneHeight()).
		Render(clampLines(m.renderWorkspaceEditorPane(state), m.workspaceFilesPaneHeight()))
	return lipgloss.JoinHorizontal(lipgloss.Top, navigator, editor)
}

func (m *model) renderWorkspaceEditorPane(state *sessionState) string {
	if state == nil {
		return "Editor\n\nNo active session"
	}
	m.initWorkspaceEditor(state)
	buf := state.Workspace.Editor.Buffer
	lines := []string{"Editor", ""}
	if strings.TrimSpace(buf.Path) == "" {
		lines = append(lines, "Open a file from Files mode.")
		return strings.Join(lines, "\n")
	}
	lines = append(lines,
		"Session: "+buf.SessionID,
		"Path: "+buf.Path,
		fmt.Sprintf("Dirty: %t", buf.Dirty),
		"",
		"Ctrl+S save, 1 terminal, 2 files, 4 artifacts",
		"",
	)
	lines = append(lines, state.Workspace.Editor.Editor.View())
	return strings.Join(lines, "\n")
}

func batchWorkspaceCmds(cmds ...tea.Cmd) tea.Cmd {
	filtered := make([]tea.Cmd, 0, len(cmds))
	for _, cmd := range cmds {
		if cmd != nil {
			filtered = append(filtered, cmd)
		}
	}
	if len(filtered) == 0 {
		return nil
	}
	return tea.Batch(filtered...)
}
