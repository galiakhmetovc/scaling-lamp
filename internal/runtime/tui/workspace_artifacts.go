package tui

import (
	"context"
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"teamd/internal/runtime/workspace"
)

func (m *model) ensureWorkspaceArtifacts(state *sessionState) tea.Cmd {
	if state == nil || state.SessionID == "" {
		return nil
	}
	state.Workspace.Mode = workspaceModeArtifacts
	if state.Workspace.Artifacts.Loaded && state.Workspace.Artifacts.Snapshot.SessionID == state.SessionID {
		return nil
	}
	return workspaceArtifactsSnapshotCmd(m.ctx, m.client, state.SessionID)
}

func workspaceArtifactsSnapshotCmd(ctx context.Context, client OperatorClient, sessionID string) tea.Cmd {
	return func() tea.Msg {
		result, err := client.WorkspaceArtifactsSnapshot(ctx, sessionID)
		return workspaceArtifactsSnapshotMsg{SessionID: sessionID, Result: result, Err: err}
	}
}

func workspaceArtifactsOpenCmd(ctx context.Context, client OperatorClient, sessionID, artifactRef string) tea.Cmd {
	return func() tea.Msg {
		result, err := client.WorkspaceArtifactsOpen(ctx, sessionID, artifactRef)
		return workspaceArtifactsOpenedMsg{SessionID: sessionID, Result: result, Err: err}
	}
}

func (m *model) updateWorkspaceArtifacts(state *sessionState, msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	if state == nil {
		return m, nil
	}
	if cmd := m.ensureWorkspaceArtifacts(state); cmd != nil && !state.Workspace.Artifacts.Loaded {
		return m, cmd
	}
	switch msg.String() {
	case "up", "k":
		if state.Workspace.Artifacts.Cursor > 0 {
			state.Workspace.Artifacts.Cursor--
		}
	case "down", "j":
		if maxIndex := len(state.Workspace.Artifacts.Snapshot.Items) - 1; state.Workspace.Artifacts.Cursor < maxIndex {
			state.Workspace.Artifacts.Cursor++
		}
	case "enter":
		item, ok := state.workspaceArtifactSelectedItem()
		if !ok {
			return m, nil
		}
		return m, workspaceArtifactsOpenCmd(m.ctx, m.client, state.SessionID, item.Ref)
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
	}
	return m, nil
}

func (state *sessionState) workspaceArtifactSelectedItem() (workspace.ArtifactListItem, bool) {
	if state == nil || len(state.Workspace.Artifacts.Snapshot.Items) == 0 {
		return workspace.ArtifactListItem{}, false
	}
	cursor := state.Workspace.Artifacts.Cursor
	if cursor < 0 {
		cursor = 0
	}
	if cursor >= len(state.Workspace.Artifacts.Snapshot.Items) {
		cursor = len(state.Workspace.Artifacts.Snapshot.Items) - 1
	}
	return state.Workspace.Artifacts.Snapshot.Items[cursor], true
}

func (m *model) viewWorkspaceArtifacts(state *sessionState) string {
	leftWidth, rightWidth := splitPaneWidths(max(42, m.width-(m.width/5)-4), 32, max(24, m.width-(m.width/5)-38))
	navigator := lipgloss.NewStyle().
		Width(max(18, m.width/5)).
		MaxWidth(max(18, m.width/5)).
		Height(m.workspaceFilesPaneHeight()).
		MaxHeight(m.workspaceFilesPaneHeight()).
		Render(m.renderWorkspaceNavigator(state))
	list := lipgloss.NewStyle().
		Width(leftWidth).
		MaxWidth(leftWidth).
		Height(m.workspaceFilesPaneHeight()).
		MaxHeight(m.workspaceFilesPaneHeight()).
		Render(clampLines(m.renderWorkspaceArtifactList(state), m.workspaceFilesPaneHeight()))
	viewer := lipgloss.NewStyle().
		Width(rightWidth).
		MaxWidth(rightWidth).
		Height(m.workspaceFilesPaneHeight()).
		MaxHeight(m.workspaceFilesPaneHeight()).
		Render(clampLines(m.renderWorkspaceArtifactViewer(state), m.workspaceFilesPaneHeight()))
	return lipgloss.JoinHorizontal(lipgloss.Top, navigator, list, viewer)
}

func (m *model) renderWorkspaceArtifactList(state *sessionState) string {
	if state == nil {
		return "Artifacts\n\nNo active session"
	}
	lines := []string{"Artifacts", ""}
	snap := state.Workspace.Artifacts.Snapshot
	if !state.Workspace.Artifacts.Loaded || snap.SessionID != state.SessionID {
		lines = append(lines, "Loading artifacts for "+state.SessionID+"...")
		return strings.Join(lines, "\n")
	}
	lines = append(lines, "Session: "+snap.SessionID, "Root: "+snap.RootPath, "")
	if len(snap.Items) == 0 {
		lines = append(lines, "No artifacts found.")
		return strings.Join(lines, "\n")
	}
	cursor := state.Workspace.Artifacts.Cursor
	if cursor < 0 {
		cursor = 0
	}
	if cursor >= len(snap.Items) {
		cursor = len(snap.Items) - 1
	}
	for i, item := range snap.Items {
		prefix := "  "
		if i == cursor {
			prefix = "> "
		}
		lines = append(lines, prefix+fmt.Sprintf("%s %s (%d chars)", item.Ref, item.ToolName, item.SizeChars))
	}
	return strings.Join(lines, "\n")
}

func (m *model) renderWorkspaceArtifactViewer(state *sessionState) string {
	if state == nil {
		return "Raw Viewer\n\nNo active session"
	}
	snap := state.Workspace.Artifacts.Snapshot
	lines := []string{"Raw Viewer", ""}
	if snap.SelectedRef != "" {
		lines = append(lines, "Ref: "+snap.SelectedRef, "")
	}
	if strings.TrimSpace(snap.Content) == "" {
		lines = append(lines, "No artifact selected.")
		return strings.Join(lines, "\n")
	}
	lines = append(lines, snap.Content)
	return strings.Join(lines, "\n")
}
