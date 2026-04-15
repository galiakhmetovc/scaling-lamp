package tui

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/runtime"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestNewModelCreatesSessionAndRendersTopTabs(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		Now:         func() time.Time { return time.Date(2026, 4, 14, 20, 35, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}

	m, err := newModel(context.Background(), agent, "")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	if m.activeSessionID == "" {
		t.Fatal("active session id is empty")
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	got := modelAfter.View()
	for _, tab := range []string{"Sessions", "Chat", "Plan", "Tools", "Settings"} {
		if !strings.Contains(got, tab) {
			t.Fatalf("view missing tab %q: %q", tab, got)
		}
	}
	state := m.sessions[m.activeSessionID]
	if state == nil {
		t.Fatal("active session state is nil")
	}
	if !state.Input.Focused() {
		t.Fatal("chat input is not focused")
	}
}

func TestChatViewRendersTimelineEntries(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewActivePlanProjection(),
		},
		UIBus: runtime.NewUIEventBus(),
		Now:   func() time.Time { return time.Date(2026, 4, 14, 20, 40, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	if err := agent.RecordEvent(context.Background(), eventSessionCreated("session-1")); err != nil {
		t.Fatalf("RecordEvent session created: %v", err)
	}
	if err := agent.RecordEvent(context.Background(), eventMessage("session-1", "user", "Ping")); err != nil {
		t.Fatalf("RecordEvent user message: %v", err)
	}
	if err := agent.RecordEvent(context.Background(), eventToolStarted("session-1", "fs_list")); err != nil {
		t.Fatalf("RecordEvent tool started: %v", err)
	}
	if err := agent.RecordEvent(context.Background(), eventTaskAdded("session-1", "Audit middleware")); err != nil {
		t.Fatalf("RecordEvent task added: %v", err)
	}

	m, err := newModel(context.Background(), agent, "session-1")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	got := modelAfter.View()
	if !strings.Contains(got, "Ping") {
		t.Fatalf("view missing user message: %q", got)
	}
	if !strings.Contains(got, "Tool:") {
		t.Fatalf("view missing tool timeline line: %q", got)
	}
	if !strings.Contains(got, "Task: added") {
		t.Fatalf("view missing plan timeline line: %q", got)
	}
}

func TestToolsViewRendersSelectedEntryDetails(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewActivePlanProjection(),
		},
		UIBus: runtime.NewUIEventBus(),
		Now:   func() time.Time { return time.Date(2026, 4, 15, 5, 0, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	if err := agent.RecordEvent(context.Background(), eventSessionCreated("session-1")); err != nil {
		t.Fatalf("RecordEvent session created: %v", err)
	}

	m, err := newModel(context.Background(), agent, "session-1")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	m.tab = tabTools
	state := m.sessions[m.activeSessionID]
	if state == nil {
		t.Fatal("active session state is nil")
	}
	state.ToolLog = []toolLogEntry{
		{Activity: runtime.ToolActivity{Phase: runtime.ToolActivityPhaseStarted, Name: "fs_list", Arguments: map[string]any{"path": "."}}},
		{Activity: runtime.ToolActivity{Phase: runtime.ToolActivityPhaseCompleted, Name: "shell_exec", Arguments: map[string]any{"command": "rg"}, ResultText: "ok"}},
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	got := modelAfter.View()
	if !strings.Contains(got, "Tool Details") {
		t.Fatalf("view missing details pane: %q", got)
	}
	if !strings.Contains(got, "shell_exec") {
		t.Fatalf("view missing selected tool details: %q", got)
	}
}

func TestPlanViewShowsNotesAndComputedStateForSelectedTask(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewActivePlanProjection(),
		},
		UIBus: runtime.NewUIEventBus(),
		Now:   func() time.Time { return time.Date(2026, 4, 15, 5, 5, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	m, err := newModel(context.Background(), agent, "")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	sessionID := m.activeSessionID
	events := []eventing.Event{
		{
			Kind:          eventing.EventPlanCreated,
			AggregateID:   "plan-1",
			AggregateType: eventing.AggregatePlan,
			Payload:       map[string]any{"session_id": sessionID, "plan_id": "plan-1", "goal": "Refactor auth"},
		},
		{
			Kind:          eventing.EventTaskAdded,
			AggregateID:   "task-1",
			AggregateType: eventing.AggregatePlanTask,
			Payload: map[string]any{
				"session_id":  sessionID,
				"plan_id":     "plan-1",
				"task_id":     "task-1",
				"description": "Audit middleware",
				"status":      "todo",
				"order":       1,
				"depends_on":  []any{},
			},
		},
		{
			Kind:          eventing.EventTaskNoteAdded,
			AggregateID:   "task-1",
			AggregateType: eventing.AggregatePlanTask,
			Payload:       map[string]any{"session_id": sessionID, "plan_id": "plan-1", "task_id": "task-1", "note_text": "Roles are cached."},
		},
	}
	for _, event := range events {
		if err := agent.RecordEvent(context.Background(), event); err != nil {
			t.Fatalf("RecordEvent %s: %v", event.Kind, err)
		}
	}
	m.tab = tabPlan
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	got := modelAfter.View()
	if !strings.Contains(got, "Computed:") || !strings.Contains(got, "ready") {
		t.Fatalf("view missing computed state: %q", got)
	}
	if !strings.Contains(got, "Latest note:") || !strings.Contains(got, "Roles are") || !strings.Contains(got, "cached.") {
		t.Fatalf("view missing latest note: %q", got)
	}
}

func TestSettingsFormShowsDirtyStateAndResets(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		Now:         func() time.Time { return time.Date(2026, 4, 15, 5, 10, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}

	m, err := newModel(context.Background(), agent, "")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	m.tab = tabSettings
	m.settingsMode = settingsForm
	m.formDraft.RenderMarkdown = false

	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	got := modelAfter.View()
	if !strings.Contains(got, "Draft: modified") {
		t.Fatalf("view missing dirty marker: %q", got)
	}

	modelAfter, _ = modelAfter.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'r'}})
	got = modelAfter.View()
	if strings.Contains(got, "Draft: modified") {
		t.Fatalf("view still dirty after reset: %q", got)
	}
}

func TestMouseWheelScrollsChatViewportFromLegacyMouseType(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		Now:         func() time.Time { return time.Date(2026, 4, 15, 8, 30, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}
	m, err := newModel(context.Background(), agent, "")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 80, Height: 20})
	mm := modelAfter.(*model)
	mm.tab = tabChat
	state := mm.sessions[mm.activeSessionID]
	state.ChatView.SetContent(strings.Repeat("line\n", 100))
	state.ChatView.GotoBottom()
	before := state.ChatView.YOffset
	if before == 0 {
		t.Fatal("chat viewport did not move to bottom in test setup")
	}
	modelAfter, _ = mm.Update(tea.MouseMsg{Type: tea.MouseWheelUp})
	mm = modelAfter.(*model)
	after := mm.sessions[mm.activeSessionID].ChatView.YOffset
	if after >= before {
		t.Fatalf("wheel up did not scroll chat viewport: before=%d after=%d", before, after)
	}
}

func TestPlanViewKeepsTopTabsOnNarrowWidth(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		Now:         func() time.Time { return time.Date(2026, 4, 15, 8, 31, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}
	m, err := newModel(context.Background(), agent, "")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	sessionID := m.activeSessionID
	if err := agent.RecordEvent(context.Background(), eventing.Event{
		Kind:          eventing.EventPlanCreated,
		AggregateID:   "plan-1",
		AggregateType: eventing.AggregatePlan,
		Payload:       map[string]any{"session_id": sessionID, "plan_id": "plan-1", "goal": "Refactor auth"},
	}); err != nil {
		t.Fatalf("RecordEvent plan: %v", err)
	}
	m.tab = tabPlan
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 50, Height: 20})
	got := modelAfter.View()
	firstLine := strings.SplitN(got, "\n", 2)[0]
	for _, tab := range []string{"Sessions", "Chat", "Plan", "Tools", "Settings"} {
		if !strings.Contains(firstLine, tab) {
			t.Fatalf("top tab line missing %q on narrow width: %q", tab, firstLine)
		}
	}
}

func TestChatViewWrapsLongLinesToViewportWidth(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewActivePlanProjection(),
		},
		UIBus: runtime.NewUIEventBus(),
		Now:   func() time.Time { return time.Date(2026, 4, 15, 9, 18, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	if err := agent.RecordEvent(context.Background(), eventSessionCreated("session-1")); err != nil {
		t.Fatalf("RecordEvent session created: %v", err)
	}
	if err := agent.RecordEvent(context.Background(), eventMessage("session-1", "assistant", strings.Repeat("verylongword", 10))); err != nil {
		t.Fatalf("RecordEvent assistant message: %v", err)
	}
	m, err := newModel(context.Background(), agent, "session-1")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 40, Height: 20})
	mm := modelAfter.(*model)
	state := mm.sessions[mm.activeSessionID]
	if state == nil {
		t.Fatal("active session state is nil")
	}
	mm.renderChatViewport(state)
	for _, line := range strings.Split(state.ChatView.View(), "\n") {
		if lipgloss.Width(line) > state.ChatView.Width {
			t.Fatalf("chat line width %d exceeds viewport width %d: %q", lipgloss.Width(line), state.ChatView.Width, line)
		}
	}
}

func TestF6TogglesMouseCaptureAndFooterIndicator(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		Now:         func() time.Time { return time.Date(2026, 4, 15, 10, 0, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}
	m, err := newModel(context.Background(), agent, "")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 80, Height: 20})
	mm := modelAfter.(*model)
	if !mm.mouseCaptureEnabled {
		t.Fatal("mouse capture should start enabled")
	}
	if !strings.Contains(mm.viewFooter(), "Mouse: on") {
		t.Fatalf("footer missing mouse enabled indicator: %q", mm.viewFooter())
	}

	modelAfter, cmd := mm.Update(tea.KeyMsg{Type: tea.KeyF6})
	mm = modelAfter.(*model)
	if mm.mouseCaptureEnabled {
		t.Fatal("mouse capture should be disabled after F6")
	}
	if cmd == nil {
		t.Fatal("toggle off returned nil command")
	}
	msg := cmd()
	batch, ok := msg.(tea.BatchMsg)
	if !ok || len(batch) != 2 {
		t.Fatalf("toggle off returned %#v, want BatchMsg with 2 commands", msg)
	}
	gotTypes := []string{fmt.Sprintf("%T", batch[0]()), fmt.Sprintf("%T", batch[1]())}
	wantOff := map[string]bool{
		fmt.Sprintf("%T", tea.DisableMouse()): true,
		fmt.Sprintf("%T", tea.ExitAltScreen()): true,
	}
	for _, got := range gotTypes {
		if !wantOff[got] {
			t.Fatalf("toggle off returned unexpected command types: %v", gotTypes)
		}
	}
	if !strings.Contains(mm.viewFooter(), "Mouse: off") {
		t.Fatalf("footer missing mouse disabled indicator: %q", mm.viewFooter())
	}

	modelAfter, cmd = mm.Update(tea.KeyMsg{Type: tea.KeyF6})
	mm = modelAfter.(*model)
	if !mm.mouseCaptureEnabled {
		t.Fatal("mouse capture should be re-enabled after second F6")
	}
	if cmd == nil {
		t.Fatal("toggle on returned nil command")
	}
	msg = cmd()
	batch, ok = msg.(tea.BatchMsg)
	if !ok || len(batch) != 2 {
		t.Fatalf("toggle on returned %#v, want BatchMsg with 2 commands", msg)
	}
	gotTypes = []string{fmt.Sprintf("%T", batch[0]()), fmt.Sprintf("%T", batch[1]())}
	wantOn := map[string]bool{
		fmt.Sprintf("%T", tea.EnableMouseCellMotion()): true,
		fmt.Sprintf("%T", tea.EnterAltScreen()): true,
	}
	for _, got := range gotTypes {
		if !wantOn[got] {
			t.Fatalf("toggle on returned unexpected command types: %v", gotTypes)
		}
	}
	if !strings.Contains(mm.viewFooter(), "Mouse: on") {
		t.Fatalf("footer missing mouse re-enabled indicator: %q", mm.viewFooter())
	}
}

func TestChatTimelineToolLinesDoNotIntroduceDoubleBlankSpacing(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewActivePlanProjection(),
		},
		UIBus: runtime.NewUIEventBus(),
		Now:   func() time.Time { return time.Date(2026, 4, 15, 10, 20, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	if err := agent.RecordEvent(context.Background(), eventSessionCreated("session-1")); err != nil {
		t.Fatalf("RecordEvent session created: %v", err)
	}
	if err := agent.RecordEvent(context.Background(), eventToolStarted("session-1", "fs_list")); err != nil {
		t.Fatalf("RecordEvent tool started: %v", err)
	}
	if err := agent.RecordEvent(context.Background(), eventing.Event{
		Kind:          eventing.EventToolCallCompleted,
		AggregateID:   "run-1",
		AggregateType: eventing.AggregateRun,
		Payload:       map[string]any{"session_id": "session-1", "tool_name": "fs_list", "result_text": "ok"},
	}); err != nil {
		t.Fatalf("RecordEvent tool completed: %v", err)
	}

	m, err := newModel(context.Background(), agent, "session-1")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 100, Height: 30})
	mm := modelAfter.(*model)
	state := mm.currentSessionState()
	if state == nil {
		t.Fatal("current session state is nil")
	}
	mm.renderChatViewport(state)
	content := state.ChatView.View()
	if strings.Contains(content, "Tool: `fs_list`\n\n\n") || strings.Contains(content, "Tool result: `ok`\n\n\n") {
		t.Fatalf("chat timeline still contains excessive blank spacing: %q", content)
	}
}

func TestPlanViewRendersMarkdownFormattingInBrowseMode(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewActivePlanProjection(),
		},
		UIBus: runtime.NewUIEventBus(),
		Now:   func() time.Time { return time.Date(2026, 4, 15, 10, 21, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	m, err := newModel(context.Background(), agent, "")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	sessionID := m.activeSessionID
	events := []eventing.Event{
		{
			Kind:          eventing.EventPlanCreated,
			AggregateID:   "plan-1",
			AggregateType: eventing.AggregatePlan,
			Payload:       map[string]any{"session_id": sessionID, "plan_id": "plan-1", "goal": "Refactor **auth**"},
		},
		{
			Kind:          eventing.EventTaskAdded,
			AggregateID:   "task-1",
			AggregateType: eventing.AggregatePlanTask,
			Payload: map[string]any{
				"session_id":  sessionID,
				"plan_id":     "plan-1",
				"task_id":     "task-1",
				"description": "Audit `middleware`",
				"status":      "todo",
				"order":       1,
				"depends_on":  []any{},
			},
		},
	}
	for _, event := range events {
		if err := agent.RecordEvent(context.Background(), event); err != nil {
			t.Fatalf("RecordEvent %s: %v", event.Kind, err)
		}
	}
	m.tab = tabPlan
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	got := modelAfter.View()
	if strings.Contains(got, "**auth**") {
		t.Fatalf("plan goal still shows raw markdown syntax: %q", got)
	}
	if strings.Contains(got, "`middleware`") {
		t.Fatalf("plan task still shows raw markdown syntax: %q", got)
	}
	if !strings.Contains(got, "auth") || !strings.Contains(got, "middleware") {
		t.Fatalf("plan view missing rendered content: %q", got)
	}
}

func TestChatViewDoesNotResetManualScrollToBottomOnRender(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewActivePlanProjection(),
		},
		UIBus: runtime.NewUIEventBus(),
		Now:   func() time.Time { return time.Date(2026, 4, 15, 10, 40, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	if err := agent.RecordEvent(context.Background(), eventSessionCreated("session-1")); err != nil {
		t.Fatalf("RecordEvent session created: %v", err)
	}
	for i := range 60 {
		if err := agent.RecordEvent(context.Background(), eventMessage("session-1", "assistant", fmt.Sprintf("line %02d", i))); err != nil {
			t.Fatalf("RecordEvent message %d: %v", i, err)
		}
	}
	m, err := newModel(context.Background(), agent, "session-1")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 80, Height: 20})
	mm := modelAfter.(*model)
	state := mm.currentSessionState()
	if state == nil {
		t.Fatal("current session state is nil")
	}
	mm.renderChatViewport(state)
	state.ChatView.GotoBottom()
	state.ChatView.LineUp(10)
	before := state.ChatView.YOffset
	if before <= 0 {
		t.Fatalf("expected non-zero offset after manual scroll, got %d", before)
	}
	_ = mm.viewChat()
	after := state.ChatView.YOffset
	if after != before {
		t.Fatalf("chat render reset manual scroll: before=%d after=%d", before, after)
	}
}

func TestPlanArrowNavigationChangesSelectedTaskAndViewportFitsPane(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
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
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewActivePlanProjection(),
		},
		UIBus: runtime.NewUIEventBus(),
		Now:   func() time.Time { return time.Date(2026, 4, 15, 10, 41, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	m, err := newModel(context.Background(), agent, "")
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	sessionID := m.activeSessionID
	events := []eventing.Event{
		{Kind: eventing.EventPlanCreated, AggregateID: "plan-1", AggregateType: eventing.AggregatePlan, Payload: map[string]any{"session_id": sessionID, "plan_id": "plan-1", "goal": "Refactor auth"}},
		{Kind: eventing.EventTaskAdded, AggregateID: "task-1", AggregateType: eventing.AggregatePlanTask, Payload: map[string]any{"session_id": sessionID, "plan_id": "plan-1", "task_id": "task-1", "description": "First task", "status": "todo", "order": 1, "depends_on": []any{}}},
		{Kind: eventing.EventTaskAdded, AggregateID: "task-2", AggregateType: eventing.AggregatePlanTask, Payload: map[string]any{"session_id": sessionID, "plan_id": "plan-1", "task_id": "task-2", "description": "Second task", "status": "todo", "order": 2, "depends_on": []any{}}},
	}
	for _, event := range events {
		if err := agent.RecordEvent(context.Background(), event); err != nil {
			t.Fatalf("RecordEvent %s: %v", event.Kind, err)
		}
	}
	m.tab = tabPlan
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 70, Height: 20})
	mm := modelAfter.(*model)
	before := mm.View()
	if !strings.Contains(before, "First task") {
		t.Fatalf("initial plan view missing first task: %q", before)
	}
	if mm.planCursor != 0 {
		t.Fatalf("expected initial plan cursor 0, got %d", mm.planCursor)
	}
	modelAfter, _ = mm.Update(tea.KeyMsg{Type: tea.KeyDown})
	mm = modelAfter.(*model)
	after := mm.View()
	if mm.planCursor != 1 {
		t.Fatalf("expected plan cursor 1 after down key, got %d", mm.planCursor)
	}
	if !strings.Contains(after, "Second task") {
		t.Fatalf("plan view missing second task after navigation: %q", after)
	}
	firstLine := strings.SplitN(after, "\n", 2)[0]
	for _, tab := range []string{"Sessions", "Chat", "Plan", "Tools", "Settings"} {
		if !strings.Contains(firstLine, tab) {
			t.Fatalf("top tabs missing %q after plan render: %q", tab, firstLine)
		}
	}
}

func eventSessionCreated(sessionID string) eventing.Event {
	return eventing.Event{
		Kind:          eventing.EventSessionCreated,
		AggregateID:   sessionID,
		AggregateType: eventing.AggregateSession,
		Payload:       map[string]any{"session_id": sessionID},
	}
}

func eventMessage(sessionID, role, content string) eventing.Event {
	return eventing.Event{
		Kind:          eventing.EventMessageRecorded,
		AggregateID:   sessionID,
		AggregateType: eventing.AggregateSession,
		Payload: map[string]any{
			"session_id": sessionID,
			"role":       role,
			"content":    content,
		},
	}
}

func eventToolStarted(sessionID, toolName string) eventing.Event {
	return eventing.Event{
		Kind:          eventing.EventToolCallStarted,
		AggregateID:   "run-1",
		AggregateType: eventing.AggregateRun,
		Payload: map[string]any{
			"session_id": sessionID,
			"tool_name":  toolName,
		},
	}
}

func eventTaskAdded(sessionID, description string) eventing.Event {
	return eventing.Event{
		Kind:          eventing.EventTaskAdded,
		AggregateID:   "task-1",
		AggregateType: eventing.AggregatePlanTask,
		Payload: map[string]any{
			"session_id":  sessionID,
			"plan_id":     "plan-1",
			"task_id":     "task-1",
			"description": description,
			"status":      "todo",
			"order":       1,
			"depends_on":  []any{},
		},
	}
}
