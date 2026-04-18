package tui

import (
	"context"
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

func (m *model) ensureWorkspacePTY(state *sessionState) tea.Cmd {
	if state == nil || state.SessionID == "" {
		return nil
	}
	state.Workspace.Mode = workspaceModeTerminal
	if state.Workspace.Loaded && state.Workspace.PTY.PTYID != "" && state.Workspace.PTY.SessionID == state.SessionID {
		return workspacePTYSnapshotCmd(m.ctx, m.client, state.SessionID)
	}
	cols, rows := m.workspacePTYSize()
	return workspacePTYOpenCmd(m.ctx, m.client, state.SessionID, cols, rows)
}

func (m *model) workspacePTYSize() (int, int) {
	cols := max(40, m.width-24)
	rows := max(12, m.height-8)
	return cols, rows
}

func workspacePTYOpenCmd(ctx context.Context, client OperatorClient, sessionID string, cols, rows int) tea.Cmd {
	return func() tea.Msg {
		result, err := client.WorkspacePTYOpen(ctx, sessionID, cols, rows)
		return workspacePTYOpenedMsg{SessionID: sessionID, Result: result, Err: err}
	}
}

func workspacePTYInputCmd(ctx context.Context, client OperatorClient, sessionID, ptyID, data string) tea.Cmd {
	return func() tea.Msg {
		if err := client.WorkspacePTYInput(ctx, ptyID, data); err != nil {
			return workspacePTYInputMsg{SessionID: sessionID, Err: err}
		}
		result, err := client.WorkspacePTYSnapshot(ctx, sessionID)
		return workspacePTYRefreshedMsg{SessionID: sessionID, Result: result, Err: err}
	}
}

func workspacePTYSnapshotCmd(ctx context.Context, client OperatorClient, sessionID string) tea.Cmd {
	return func() tea.Msg {
		result, err := client.WorkspacePTYSnapshot(ctx, sessionID)
		return workspacePTYRefreshedMsg{SessionID: sessionID, Result: result, Err: err}
	}
}

func (m *model) renderWorkspaceTerminalPane(state *sessionState) string {
	if state == nil {
		return "Terminal\n\nNo active session"
	}
	lines := []string{"Terminal", ""}
	if !state.Workspace.Loaded || state.Workspace.PTY.PTYID == "" {
		lines = append(lines, "Opening PTY for "+state.SessionID+"...")
		return strings.Join(lines, "\n")
	}
	lines = append(lines,
		"Session: "+state.Workspace.PTY.SessionID,
		"PTY: "+state.Workspace.PTY.PTYID,
		fmt.Sprintf("Size: %dx%d", state.Workspace.PTY.Cols, state.Workspace.PTY.Rows),
	)
	if state.Workspace.PTY.CWD != "" {
		lines = append(lines, "CWD: "+state.Workspace.PTY.CWD)
	}
	if state.Workspace.PTY.PID > 0 {
		lines = append(lines, fmt.Sprintf("PID: %d", state.Workspace.PTY.PID))
	}
	lines = append(lines, "")
	if len(state.Workspace.PTY.Scrollback) == 0 {
		lines = append(lines, "No terminal output yet.")
	} else {
		lines = append(lines, state.Workspace.PTY.Scrollback...)
	}
	return strings.Join(lines, "\n")
}

func (m *model) renderWorkspaceNavigator(state *sessionState) string {
	mode := workspaceModeTerminal
	if state != nil {
		mode = state.Workspace.Mode
	}
	lines := []string{"Workspace pane", ""}
	lines = append(lines, workspaceNavigatorLine("Terminal", mode == workspaceModeTerminal))
	lines = append(lines, workspaceNavigatorLine("Files", mode == workspaceModeFiles))
	lines = append(lines, workspaceNavigatorLine("Editor", mode == workspaceModeEditor))
	lines = append(lines, workspaceNavigatorLine("Artifacts", mode == workspaceModeArtifacts))
	lines = append(lines, "", "1 = Terminal", "2 = Files", "3 = Editor", "4 = Artifacts", "F5 toggle workspace")
	if mode == workspaceModeFiles {
		lines = append(lines, "Enter expand selected dir")
	} else if mode == workspaceModeEditor {
		lines = append(lines, "Type to edit, Ctrl+S save")
	} else if mode == workspaceModeArtifacts {
		lines = append(lines, "Enter open selected artifact")
	} else {
		lines = append(lines, "Type to send input")
	}
	return strings.Join(lines, "\n")
}

func workspaceNavigatorLine(label string, active bool) string {
	if active {
		return "> " + label
	}
	return "  " + label
}

func workspaceTerminalInput(msg tea.KeyMsg) (string, bool) {
	switch msg.Type {
	case tea.KeyRunes:
		return string(msg.Runes), true
	case tea.KeySpace:
		return " ", true
	case tea.KeyEnter:
		return "\r", true
	case tea.KeyTab:
		return "\t", true
	case tea.KeyBackspace:
		return "\x7f", true
	case tea.KeyUp:
		return "\x1b[A", true
	case tea.KeyDown:
		return "\x1b[B", true
	case tea.KeyRight:
		return "\x1b[C", true
	case tea.KeyLeft:
		return "\x1b[D", true
	}
	switch msg.String() {
	case "ctrl+l":
		return "", false
	}
	if text := msg.String(); len(text) == 1 {
		return text, true
	}
	return "", false
}

func (m *model) workspaceTerminalPaneWidth() (int, int) {
	return splitPaneWidths(m.width, max(18, m.width/5), max(42, m.width-(m.width/5)-4))
}

func (m *model) workspaceTerminalPaneHeight() int {
	return max(10, m.height-4)
}

func (m *model) workspaceTerminalShellRefresh(state *sessionState) tea.Cmd {
	if state == nil || state.SessionID == "" {
		return nil
	}
	return workspacePTYSnapshotCmd(m.ctx, m.client, state.SessionID)
}

func (m *model) workspaceTerminalView(state *sessionState) string {
	navigator := lipgloss.NewStyle().
		Width(max(18, m.width/5)).
		MaxWidth(max(18, m.width/5)).
		Height(m.workspaceTerminalPaneHeight()).
		MaxHeight(m.workspaceTerminalPaneHeight()).
		Render(m.renderWorkspaceNavigator(state))
	terminal := lipgloss.NewStyle().
		Width(max(42, m.width-(m.width/5)-4)).
		MaxWidth(max(42, m.width-(m.width/5)-4)).
		Height(m.workspaceTerminalPaneHeight()).
		MaxHeight(m.workspaceTerminalPaneHeight()).
		Render(clampLines(m.renderWorkspaceTerminalPane(state), m.workspaceTerminalPaneHeight()))
	return lipgloss.JoinHorizontal(lipgloss.Top, navigator, terminal)
}

func (m *model) activeWorkspaceTerminalState() *sessionState {
	if m == nil || m.tab != tabWorkspace {
		return nil
	}
	state := m.currentSessionState()
	if state == nil || state.Workspace.Mode != workspaceModeTerminal {
		return nil
	}
	if !state.Workspace.Loaded || state.Workspace.PTY.PTYID == "" || state.Workspace.PTY.SessionID != state.SessionID {
		return nil
	}
	return state
}

func (m *model) shouldTickClock() bool {
	if m.hasActiveRuns() {
		return true
	}
	return m.activeWorkspaceTerminalState() != nil
}
