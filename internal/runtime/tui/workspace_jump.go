package tui

import (
	"path/filepath"
	"strings"

	tea "github.com/charmbracelet/bubbletea"

	"teamd/internal/runtime/workspace"
)

type workspaceJumpKind int

const (
	workspaceJumpNone workspaceJumpKind = iota
	workspaceJumpPath
	workspaceJumpArtifact
	workspaceJumpCommand
)

type workspaceJumpTarget struct {
	Kind        workspaceJumpKind
	Path        string
	ArtifactRef string
	CommandID   string
}

func (t workspaceJumpTarget) isValid() bool {
	switch t.Kind {
	case workspaceJumpPath:
		return strings.TrimSpace(t.Path) != ""
	case workspaceJumpArtifact:
		return strings.TrimSpace(t.ArtifactRef) != ""
	case workspaceJumpCommand:
		return strings.TrimSpace(t.CommandID) != ""
	default:
		return false
	}
}

func (m *model) jumpToWorkspace(state *sessionState, target workspaceJumpTarget) tea.Cmd {
	if state == nil || !target.isValid() {
		return nil
	}
	m.tab = tabWorkspace
	switch target.Kind {
	case workspaceJumpArtifact:
		state.Workspace.Mode = workspaceModeArtifacts
		state.Workspace.PendingFilesPath = ""
		return workspaceArtifactsOpenCmd(m.ctx, m.client, state.SessionID, target.ArtifactRef)
	case workspaceJumpCommand:
		state.Workspace.Mode = workspaceModeTerminal
		state.Workspace.PendingFilesPath = ""
		if cmd := m.ensureWorkspacePTY(state); cmd != nil {
			return cmd
		}
		return nil
	case workspaceJumpPath:
		state.Workspace.Mode = workspaceModeFiles
		state.Workspace.PendingFilesPath = target.Path
		if state.Workspace.Files.Loaded && state.Workspace.Files.Snapshot.SessionID == state.SessionID {
			m.applyWorkspaceFilesJumpTarget(state)
			return nil
		}
		return workspaceFilesSnapshotCmd(m.ctx, m.client, state.SessionID)
	default:
		return nil
	}
}

func (m *model) applyWorkspaceFilesJumpTarget(state *sessionState) {
	if state == nil {
		return
	}
	target := strings.TrimSpace(state.Workspace.PendingFilesPath)
	if target == "" || len(state.Workspace.Files.Snapshot.Items) == 0 {
		state.Workspace.PendingFilesPath = ""
		return
	}
	state.Workspace.Files.Cursor = selectWorkspaceFilesCursor(state.Workspace.Files.Snapshot.Items, target)
	state.Workspace.PendingFilesPath = ""
}

func selectWorkspaceFilesCursor(items []workspace.FileNode, target string) int {
	if len(items) == 0 {
		return 0
	}
	normalizedTarget := normalizeWorkspacePath(target)
	for i, item := range items {
		if normalizeWorkspacePath(item.Path) == normalizedTarget {
			return i
		}
	}
	base := filepath.Base(normalizedTarget)
	for i, item := range items {
		if item.Name == base {
			return i
		}
	}
	for i, item := range items {
		path := normalizeWorkspacePath(item.Path)
		if strings.HasPrefix(path, normalizedTarget) || strings.HasPrefix(normalizedTarget, path) {
			return i
		}
	}
	return 0
}

func normalizeWorkspacePath(input string) string {
	path := strings.TrimSpace(strings.ReplaceAll(input, "\\", "/"))
	if path == "" || path == "." {
		return "."
	}
	path = filepath.ToSlash(filepath.Clean(path))
	path = strings.TrimPrefix(path, "./")
	if path == "" {
		return "."
	}
	return path
}

func (m *model) jumpTargetFromChat(state *sessionState) (workspaceJumpTarget, bool) {
	if state == nil || len(state.ToolLog) == 0 {
		return workspaceJumpTarget{}, false
	}
	entries := reverseToolEntries(state.ToolLog)
	if m.toolCursor < 0 || m.toolCursor >= len(entries) {
		return workspaceJumpTarget{}, false
	}
	return toolActivityJumpTarget(entries[m.toolCursor].Activity)
}

func (m *model) jumpTargetFromTools(state *sessionState) (workspaceJumpTarget, bool) {
	if state == nil {
		return workspaceJumpTarget{}, false
	}
	switch m.toolsFocus {
	case toolsFocusCommands:
		commands := m.currentRunningCommands()
		if m.commandCursor < 0 || m.commandCursor >= len(commands) {
			return workspaceJumpTarget{}, false
		}
		command := commands[m.commandCursor]
		if strings.TrimSpace(command.CommandID) == "" {
			return workspaceJumpTarget{}, false
		}
		return workspaceJumpTarget{Kind: workspaceJumpCommand, CommandID: command.CommandID}, true
	case toolsFocusLog:
		entries := reverseToolEntries(state.ToolLog)
		if m.toolCursor < 0 || m.toolCursor >= len(entries) {
			return workspaceJumpTarget{}, false
		}
		return toolActivityJumpTarget(entries[m.toolCursor].Activity)
	default:
		return workspaceJumpTarget{}, false
	}
}

func (m *model) jumpToWorkspaceFromChat() tea.Cmd {
	state := m.currentSessionState()
	if state == nil {
		return nil
	}
	target, ok := m.jumpTargetFromChat(state)
	if !ok {
		return nil
	}
	return m.jumpToWorkspace(state, target)
}

func (m *model) jumpToWorkspaceFromTools() tea.Cmd {
	state := m.currentSessionState()
	if state == nil {
		return nil
	}
	target, ok := m.jumpTargetFromTools(state)
	if !ok {
		return nil
	}
	return m.jumpToWorkspace(state, target)
}
