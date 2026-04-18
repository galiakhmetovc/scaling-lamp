package tui

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/runtime"
	"teamd/internal/runtime/daemon"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
	"teamd/internal/runtime/workspace"
)

func TestWorkspaceTabScaffold(t *testing.T) {
	m, _ := newWorkspaceTerminalTestModel(t)
	got := m.View()
	for _, tab := range []string{"Sessions", "Chat", "Head", "Prompt", "Workspace", "Plan", "Tools", "Settings"} {
		if !strings.Contains(got, tab) {
			t.Fatalf("view missing tab %q: %q", tab, got)
		}
	}

	order := []string{"Sessions", "Chat", "Head", "Prompt", "Workspace", "Plan", "Tools", "Settings"}
	last := -1
	for _, tab := range order {
		idx := strings.Index(got, tab)
		if idx < 0 {
			t.Fatalf("tab %q missing from view: %q", tab, got)
		}
		if idx <= last {
			t.Fatalf("tab %q out of order in view: %q", tab, got)
		}
		last = idx
	}

	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	got = m.View()
	if !strings.Contains(got, "Terminal") {
		t.Fatalf("workspace tab did not render terminal view: %q", got)
	}
}

func TestWorkspaceTerminalDefaultsToTerminal(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})

	if m.tab != tabWorkspace {
		t.Fatalf("tab = %v, want workspace", m.tab)
	}
	got := m.View()
	if !strings.Contains(got, "Terminal") {
		t.Fatalf("workspace view missing terminal mode: %q", got)
	}
	if len(client.workspaceOpenCalls) != 1 || client.workspaceOpenCalls[0] != "session-1" {
		t.Fatalf("workspace open calls = %#v, want first entry for session-1", client.workspaceOpenCalls)
	}
}

func TestWorkspaceTerminalOpensPTYOnFirstEntry(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})

	if len(client.workspaceOpenCalls) != 1 {
		t.Fatalf("workspace open calls = %d, want 1", len(client.workspaceOpenCalls))
	}
	if client.workspaceOpenCalls[0] != "session-1" {
		t.Fatalf("workspace opened for %q, want session-1", client.workspaceOpenCalls[0])
	}
}

func TestWorkspaceTerminalForwardsInputToPTYClientMethods(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'x'}})

	if len(client.workspaceInputCalls) != 1 {
		t.Fatalf("workspace input calls = %d, want 1", len(client.workspaceInputCalls))
	}
	if got := client.workspaceInputCalls[0]; got.PTYID != "pty-session-1" || got.Data != "x" {
		t.Fatalf("workspace input call = %#v, want pty-session-1 with x", got)
	}
}

func TestWorkspaceTerminalSwitchesPTYContextWhenSessionChanges(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})

	m.tab = tabSessions
	m.sessionCursor = 1
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyEnter})

	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})

	if len(client.workspaceOpenCalls) != 2 {
		t.Fatalf("workspace open calls = %#v, want two opens", client.workspaceOpenCalls)
	}
	if client.workspaceOpenCalls[1] != "session-2" {
		t.Fatalf("second workspace open = %q, want session-2", client.workspaceOpenCalls[1])
	}
}

func TestWorkspaceFilesSwitchesModeWithKey2(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'2'}})

	state := m.currentSessionState()
	if state.Workspace.Mode != workspaceModeFiles {
		t.Fatalf("workspace mode = %v, want files", state.Workspace.Mode)
	}
	if len(client.workspaceFileSnapshotCalls) != 1 || client.workspaceFileSnapshotCalls[0] != "session-1" {
		t.Fatalf("workspace file snapshot calls = %#v, want first entry for session-1", client.workspaceFileSnapshotCalls)
	}
}

func TestWorkspaceFilesRendersRootTreeForActiveSession(t *testing.T) {
	m, _ := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'2'}})

	got := m.View()
	if !strings.Contains(got, "Files") {
		t.Fatalf("workspace view missing files mode: %q", got)
	}
	if !strings.Contains(got, "dir/") {
		t.Fatalf("workspace view missing directory tree entry: %q", got)
	}
	if !strings.Contains(got, "go.mod") {
		t.Fatalf("workspace view missing file tree entry: %q", got)
	}
}

func TestWorkspaceFilesExpandsSelectedDirOnEnter(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'2'}})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyEnter})

	if len(client.workspaceFileExpandCalls) != 1 {
		t.Fatalf("workspace file expand calls = %#v, want 1", client.workspaceFileExpandCalls)
	}
	if got := client.workspaceFileExpandCalls[0]; got.SessionID != "session-1" || got.RelPath != "dir" {
		t.Fatalf("workspace file expand call = %#v, want session-1 dir", got)
	}
	got := m.View()
	if !strings.Contains(got, "child.txt") {
		t.Fatalf("workspace view missing expanded child item: %q", got)
	}
}

func TestWorkspaceArtifactsSwitchesModeWithKey4(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'4'}})

	state := m.currentSessionState()
	if state.Workspace.Mode != workspaceModeArtifacts {
		t.Fatalf("workspace mode = %v, want artifacts", state.Workspace.Mode)
	}
	if len(client.workspaceArtifactSnapshotCalls) != 1 || client.workspaceArtifactSnapshotCalls[0] != "session-1" {
		t.Fatalf("workspace artifact snapshot calls = %#v, want first entry for session-1", client.workspaceArtifactSnapshotCalls)
	}
}

func TestWorkspaceArtifactsRendersListAndViewer(t *testing.T) {
	m, _ := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'4'}})

	got := m.View()
	if !strings.Contains(got, "Artifacts") {
		t.Fatalf("workspace view missing artifacts mode: %q", got)
	}
	if !strings.Contains(got, "artifact://2") {
		t.Fatalf("workspace view missing artifact list entry: %q", got)
	}
	if !strings.Contains(got, "line 1") {
		t.Fatalf("workspace view missing raw content: %q", got)
	}
}

func TestWorkspaceArtifactsOpensSelectedArtifactOnEnter(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'4'}})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyDown})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyEnter})

	if len(client.workspaceArtifactOpenCalls) != 1 {
		t.Fatalf("workspace artifact open calls = %#v, want 1", client.workspaceArtifactOpenCalls)
	}
	if got := client.workspaceArtifactOpenCalls[0]; got.SessionID != "session-1" || got.Ref != "artifact://1" {
		t.Fatalf("workspace artifact open call = %#v, want session-1 artifact://1", got)
	}
	got := m.View()
	if !strings.Contains(got, "older artifact output") {
		t.Fatalf("workspace view missing opened artifact content: %q", got)
	}
}

func TestWorkspaceArtifactsViewerClampsToPaneHeight(t *testing.T) {
	m, _ := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.WindowSizeMsg{Width: 120, Height: 14})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'4'}})

	got := m.View()
	if strings.Contains(got, "line 20") {
		t.Fatalf("workspace artifacts viewer did not clamp to pane height: %q", got)
	}
}

func TestWorkspaceJumpFromToolLogOpensArtifacts(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	state := m.currentSessionState()
	state.ToolLog = []toolLogEntry{{
		Activity: runtime.ToolActivity{
			Phase:      runtime.ToolActivityPhaseCompleted,
			OccurredAt: time.Date(2026, 4, 15, 11, 0, 0, 0, time.UTC),
			Name:       "fs_read_text",
			Arguments: map[string]any{
				"path":         "go.mod",
				"artifact_ref": "artifact://1",
				"command_id":   "cmd-123",
				"irrelevant":   "value",
			},
			ResultText: `{"artifact_ref":"artifact://1"}`,
		},
	}}
	m.tab = tabTools
	m.toolsFocus = toolsFocusLog
	m.toolCursor = 0

	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'o'}})

	if m.tab != tabWorkspace {
		t.Fatalf("tab = %v, want workspace", m.tab)
	}
	if state.Workspace.Mode != workspaceModeArtifacts {
		t.Fatalf("workspace mode = %v, want artifacts", state.Workspace.Mode)
	}
	if len(client.workspaceArtifactOpenCalls) != 1 {
		t.Fatalf("workspace artifact open calls = %#v, want 1", client.workspaceArtifactOpenCalls)
	}
	if got := client.workspaceArtifactOpenCalls[0]; got.SessionID != "session-1" || got.Ref != "artifact://1" {
		t.Fatalf("workspace artifact open call = %#v, want session-1 artifact://1", got)
	}
}

func TestWorkspaceJumpFromToolLogOpensFiles(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	state := m.currentSessionState()
	state.ToolLog = []toolLogEntry{{
		Activity: runtime.ToolActivity{
			Phase:      runtime.ToolActivityPhaseCompleted,
			OccurredAt: time.Date(2026, 4, 15, 11, 5, 0, 0, time.UTC),
			Name:       "fs_read_text",
			Arguments:  map[string]any{"path": "go.mod"},
		},
	}}
	m.tab = tabTools
	m.toolsFocus = toolsFocusLog
	m.toolCursor = 0

	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'o'}})

	if m.tab != tabWorkspace {
		t.Fatalf("tab = %v, want workspace", m.tab)
	}
	if state.Workspace.Mode != workspaceModeFiles {
		t.Fatalf("workspace mode = %v, want files", state.Workspace.Mode)
	}
	if len(client.workspaceFileSnapshotCalls) != 1 || client.workspaceFileSnapshotCalls[0] != "session-1" {
		t.Fatalf("workspace file snapshot calls = %#v, want session-1", client.workspaceFileSnapshotCalls)
	}
	if state.Workspace.Files.Cursor != 1 {
		t.Fatalf("workspace files cursor = %d, want go.mod", state.Workspace.Files.Cursor)
	}
}

func TestWorkspaceJumpFromToolLogOpensTerminal(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	state := m.currentSessionState()
	state.ToolLog = []toolLogEntry{{
		Activity: runtime.ToolActivity{
			Phase:      runtime.ToolActivityPhaseCompleted,
			OccurredAt: time.Date(2026, 4, 15, 11, 10, 0, 0, time.UTC),
			Name:       "shell_poll",
			Arguments:  map[string]any{"command_id": "cmd-123"},
		},
	}}
	m.tab = tabTools
	m.toolsFocus = toolsFocusLog
	m.toolCursor = 0

	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'o'}})

	if m.tab != tabWorkspace {
		t.Fatalf("tab = %v, want workspace", m.tab)
	}
	if state.Workspace.Mode != workspaceModeTerminal {
		t.Fatalf("workspace mode = %v, want terminal", state.Workspace.Mode)
	}
	if len(client.workspaceOpenCalls) != 1 || client.workspaceOpenCalls[0] != "session-1" {
		t.Fatalf("workspace open calls = %#v, want session-1", client.workspaceOpenCalls)
	}
}

func TestWorkspaceJumpFromChatOpensArtifacts(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	state := m.currentSessionState()
	state.ToolLog = []toolLogEntry{{
		Activity: runtime.ToolActivity{
			Phase:      runtime.ToolActivityPhaseCompleted,
			OccurredAt: time.Date(2026, 4, 15, 11, 15, 0, 0, time.UTC),
			Name:       "fs_read_text",
			Arguments: map[string]any{
				"path":         "go.mod",
				"artifact_ref": "artifact://2",
			},
			ResultText: `{"artifact_ref":"artifact://2"}`,
		},
	}}
	m.tab = tabChat
	m.toolCursor = 0

	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'o'}})

	if m.tab != tabWorkspace {
		t.Fatalf("tab = %v, want workspace", m.tab)
	}
	if state.Workspace.Mode != workspaceModeArtifacts {
		t.Fatalf("workspace mode = %v, want artifacts", state.Workspace.Mode)
	}
	if len(client.workspaceArtifactOpenCalls) != 1 {
		t.Fatalf("workspace artifact open calls = %#v, want 1", client.workspaceArtifactOpenCalls)
	}
	if got := client.workspaceArtifactOpenCalls[0]; got.SessionID != "session-1" || got.Ref != "artifact://2" {
		t.Fatalf("workspace artifact open call = %#v, want session-1 artifact://2", got)
	}
}

func TestWorkspaceFilesEnterOnFileOpensEditor(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'2'}})
	state := m.currentSessionState()
	state.Workspace.Files.Cursor = 1

	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyEnter})

	if state.Workspace.Mode != workspaceModeEditor {
		t.Fatalf("workspace mode = %v, want editor", state.Workspace.Mode)
	}
	if len(client.workspaceEditorOpenCalls) != 1 {
		t.Fatalf("workspace editor open calls = %#v, want 1", client.workspaceEditorOpenCalls)
	}
	if got := client.workspaceEditorOpenCalls[0]; got.SessionID != "session-1" || got.RelPath != "go.mod" {
		t.Fatalf("workspace editor open call = %#v, want session-1 go.mod", got)
	}
}

func TestWorkspaceEditorTypingChangesBuffer(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'2'}})
	state := m.currentSessionState()
	state.Workspace.Files.Cursor = 1
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyEnter})

	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'x'}})

	if len(client.workspaceEditorUpdateCalls) != 1 {
		t.Fatalf("workspace editor update calls = %#v, want 1", client.workspaceEditorUpdateCalls)
	}
	if got := state.Workspace.Editor.Buffer.Content; !strings.Contains(got, "x") {
		t.Fatalf("editor buffer content = %q, want typed x", got)
	}
	if !state.Workspace.Editor.Buffer.Dirty {
		t.Fatal("editor buffer dirty = false, want true")
	}
}

func TestWorkspaceEditorCtrlSSaves(t *testing.T) {
	m, client := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'2'}})
	state := m.currentSessionState()
	state.Workspace.Files.Cursor = 1
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyEnter})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'x'}})

	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyCtrlS})

	if len(client.workspaceEditorSaveCalls) != 1 {
		t.Fatalf("workspace editor save calls = %#v, want 1", client.workspaceEditorSaveCalls)
	}
	if state.Workspace.Editor.Buffer.Dirty {
		t.Fatal("editor buffer dirty = true, want false")
	}
}

func TestWorkspaceEditorStatusShowsPathAndDirtyState(t *testing.T) {
	m, _ := newWorkspaceTerminalTestModel(t)
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyF5})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'2'}})
	state := m.currentSessionState()
	state.Workspace.Files.Cursor = 1
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyEnter})
	m = runWorkspaceTerminalStep(t, m, tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'x'}})

	got := m.View()
	if !strings.Contains(got, "Editor") {
		t.Fatalf("workspace view missing editor mode: %q", got)
	}
	if !strings.Contains(got, "go.mod") {
		t.Fatalf("workspace view missing file path: %q", got)
	}
	if !strings.Contains(strings.ToLower(got), "dirty") {
		t.Fatalf("workspace view missing dirty state: %q", got)
	}
}

func runWorkspaceTerminalStep(t *testing.T, m model, msg tea.Msg) model {
	t.Helper()
	next, cmd := (&m).Update(msg)
	updated, ok := next.(*model)
	if !ok {
		t.Fatalf("Update returned %T, want tui.model", next)
	}
	if cmd == nil {
		return *updated
	}
	next, cmd = updated.Update(cmd())
	updated, ok = next.(*model)
	if !ok {
		t.Fatalf("command Update returned %T, want tui.model", next)
	}
	if cmd != nil {
		next, cmd = updated.Update(cmd())
		updated, ok = next.(*model)
		if !ok {
			t.Fatalf("second command Update returned %T, want tui.model", next)
		}
		if cmd != nil {
			t.Fatalf("unexpected third workspace command %T", cmd)
		}
	}
	return *updated
}

func newWorkspaceTerminalTestModel(t *testing.T) (model, *stubOperatorClient) {
	t.Helper()
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
	}

	now := time.Date(2026, 4, 15, 10, 0, 0, 0, time.UTC)
	client := &stubOperatorClient{
		sessions: []SessionSummary{
			{SessionID: "session-1", CreatedAt: now, LastActivity: now, MessageCount: 1},
			{SessionID: "session-2", CreatedAt: now, LastActivity: now, MessageCount: 2},
		},
		snapshot: daemon.SessionSnapshot{
			SessionID:    "session-1",
			CreatedAt:    now,
			LastActivity: now,
			MessageCount: 1,
			Prompt: daemon.SessionPromptSnapshot{
				Default:   "default prompt",
				Effective: "default prompt",
			},
		},
		workspaceFileSnapshots: map[string]workspace.FileTreeSnapshot{
			"session-1": {
				SessionID: "session-1",
				RootPath:  filepath.Join(dir, "workspace-root"),
				Items: []workspace.FileNode{
					{Path: "dir", Name: "dir", IsDir: true, Size: 0, ModTime: now},
					{Path: "go.mod", Name: "go.mod", IsDir: false, Size: 13, ModTime: now},
				},
			},
		},
		workspaceArtifactSnapshots: map[string]workspace.ArtifactSnapshot{
			"session-1": {
				SessionID:   "session-1",
				RootPath:    filepath.Join(dir, "artifacts"),
				SelectedRef: "artifact://2",
				Content:     "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\nline 14\nline 15\nline 16\nline 17\nline 18\nline 19\nline 20\n",
				Items: []workspace.ArtifactListItem{
					{Ref: "artifact://2", ToolName: "shell_exec", CreatedAt: now.Add(time.Minute), SizeChars: 120, SizeBytes: 120, Preview: "line 1"},
					{Ref: "artifact://1", ToolName: "fs_read_lines", CreatedAt: now, SizeChars: 32, SizeBytes: 32, Preview: "older artifact output"},
				},
			},
		},
	}
	agent := &runtime.Agent{
		ConfigPath: configPath,
		Config:     config.AgentConfig{ID: "tui-test", Spec: config.AgentConfigSpec{Runtime: config.AgentRuntimeConfig{MaxToolRounds: 7}}},
		Contracts: contracts.ResolvedContracts{
			Chat: contracts.ChatContract{
				Output: contracts.ChatOutputPolicy{Params: contracts.ChatOutputParams{RenderMarkdown: true, MarkdownStyle: "dark"}},
				Status: contracts.ChatStatusPolicy{Params: contracts.ChatStatusParams{ShowToolCalls: true, ShowToolResults: true, ShowPlanAfterPlanTools: true}},
			},
		},
		EventLog:    runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{projections.NewSessionCatalogProjection(), projections.NewTranscriptProjection(), projections.NewChatTimelineProjection(), projections.NewPlanHeadProjection(), projections.NewActivePlanProjection()},
		UIBus:       runtime.NewUIEventBus(),
		Now:         func() time.Time { return now },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}
	if err := agent.RecordEvent(context.Background(), eventing.Event{
		ID:               "evt-session-created",
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       agent.Now(),
		AggregateID:      "session-1",
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		Payload:          map[string]any{"session_id": "session-1"},
	}); err != nil {
		t.Fatalf("RecordEvent session created: %v", err)
	}

	m, err := newModelWithClient(context.Background(), client, "session-1")
	if err != nil {
		t.Fatalf("newModelWithClient returned error: %v", err)
	}
	m = runWorkspaceTerminalStep(t, m, tea.WindowSizeMsg{Width: 120, Height: 40})
	return m, client
}
