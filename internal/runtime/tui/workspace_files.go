package tui

import (
	"context"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"teamd/internal/runtime/workspace"
)

func (m *model) ensureWorkspaceFiles(state *sessionState) tea.Cmd {
	if state == nil || state.SessionID == "" {
		return nil
	}
	state.Workspace.Mode = workspaceModeFiles
	if state.Workspace.Files.Loaded && state.Workspace.Files.Snapshot.SessionID == state.SessionID {
		return nil
	}
	return workspaceFilesSnapshotCmd(m.ctx, m.client, state.SessionID)
}

func workspaceFilesSnapshotCmd(ctx context.Context, client OperatorClient, sessionID string) tea.Cmd {
	return func() tea.Msg {
		result, err := client.WorkspaceFilesSnapshot(ctx, sessionID)
		return workspaceFilesSnapshotMsg{SessionID: sessionID, Result: result, Err: err}
	}
}

func workspaceFilesExpandCmd(ctx context.Context, client OperatorClient, sessionID, relPath string) tea.Cmd {
	return func() tea.Msg {
		result, err := client.WorkspaceFilesExpand(ctx, sessionID, relPath)
		return workspaceFilesExpandedMsg{SessionID: sessionID, Result: result, Err: err}
	}
}

func (m *model) updateWorkspaceFiles(state *sessionState, msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	if state == nil {
		return m, nil
	}
	if cmd := m.ensureWorkspaceFiles(state); cmd != nil && !state.Workspace.Files.Loaded {
		return m, cmd
	}
	switch msg.String() {
	case "up", "k":
		if state.Workspace.Files.Cursor > 0 {
			state.Workspace.Files.Cursor--
		}
	case "down", "j":
		if maxIndex := len(state.Workspace.Files.Snapshot.Items) - 1; state.Workspace.Files.Cursor < maxIndex {
			state.Workspace.Files.Cursor++
		}
	case "enter":
		node, ok := state.workspaceFilesSelectedNode()
		if !ok {
			return m, nil
		}
		if node.IsDir {
			return m, workspaceFilesExpandCmd(m.ctx, m.client, state.SessionID, node.Path)
		}
		state.Workspace.Mode = workspaceModeEditor
		state.Workspace.PendingFilesPath = ""
		state.Workspace.PendingEditorPath = node.Path
		return m, workspaceEditorOpenCmd(m.ctx, m.client, state.SessionID, node.Path)
	case "1":
		state.Workspace.Mode = workspaceModeTerminal
		if cmd := m.ensureWorkspacePTY(state); cmd != nil {
			return m, cmd
		}
	}
	return m, nil
}

func (state *sessionState) workspaceFilesSelectedNode() (workspace.FileNode, bool) {
	if state == nil || len(state.Workspace.Files.Snapshot.Items) == 0 {
		return workspace.FileNode{}, false
	}
	cursor := state.Workspace.Files.Cursor
	if cursor < 0 {
		cursor = 0
	}
	if cursor >= len(state.Workspace.Files.Snapshot.Items) {
		cursor = len(state.Workspace.Files.Snapshot.Items) - 1
	}
	return state.Workspace.Files.Snapshot.Items[cursor], true
}

func (m *model) viewWorkspaceFiles(state *sessionState) string {
	if state == nil {
		return "Files\n\nNo active session"
	}
	lines := []string{"Files", ""}
	if !state.Workspace.Files.Loaded || state.Workspace.Files.Snapshot.SessionID != state.SessionID {
		lines = append(lines, "Loading workspace files for "+state.SessionID+"...")
		return strings.Join(lines, "\n")
	}
	lines = append(lines,
		"Session: "+state.Workspace.Files.Snapshot.SessionID,
		"Root: "+state.Workspace.Files.Snapshot.RootPath,
		"",
	)
	if len(state.Workspace.Files.Snapshot.Items) == 0 {
		lines = append(lines, "No files found.")
		return strings.Join(lines, "\n")
	}
	cursor := state.Workspace.Files.Cursor
	if cursor < 0 {
		cursor = 0
	}
	if cursor >= len(state.Workspace.Files.Snapshot.Items) {
		cursor = len(state.Workspace.Files.Snapshot.Items) - 1
	}
	for i, item := range state.Workspace.Files.Snapshot.Items {
		prefix := "  "
		if i == cursor {
			prefix = "> "
		}
		lines = append(lines, prefix+workspaceFileTreeLine(item))
	}
	return strings.Join(lines, "\n")
}

func workspaceFileTreeLine(node workspace.FileNode) string {
	depth := 0
	if node.Path != "" && node.Path != "." {
		depth = strings.Count(node.Path, "/")
	}
	indent := strings.Repeat("  ", depth)
	if node.IsDir {
		marker := "+"
		if node.Expanded && node.ChildrenLoaded {
			marker = "-"
		}
		return indent + marker + " " + node.Name + "/"
	}
	return indent + "  " + node.Name
}

func (m *model) workspaceFilesPaneWidth() (int, int) {
	return splitPaneWidths(m.width, max(18, m.width/5), max(42, m.width-(m.width/5)-4))
}

func (m *model) workspaceFilesPaneHeight() int {
	return max(10, m.height-4)
}

func (m *model) workspaceFilesView(state *sessionState) string {
	navigator := lipgloss.NewStyle().
		Width(max(18, m.width/5)).
		MaxWidth(max(18, m.width/5)).
		Height(m.workspaceFilesPaneHeight()).
		MaxHeight(m.workspaceFilesPaneHeight()).
		Render(m.renderWorkspaceNavigator(state))
	files := lipgloss.NewStyle().
		Width(max(42, m.width-(m.width/5)-4)).
		MaxWidth(max(42, m.width-(m.width/5)-4)).
		Height(m.workspaceFilesPaneHeight()).
		MaxHeight(m.workspaceFilesPaneHeight()).
		Render(clampLines(m.viewWorkspaceFiles(state), m.workspaceFilesPaneHeight()))
	return lipgloss.JoinHorizontal(lipgloss.Top, navigator, files)
}
