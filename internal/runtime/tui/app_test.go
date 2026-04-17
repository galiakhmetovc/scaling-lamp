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
	"teamd/internal/runtime/daemon"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
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

	sessionID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, sessionID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	if m.activeSessionID == "" {
		t.Fatal("active session id is empty")
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	got := modelAfter.View()
	for _, tab := range []string{"Sessions", "Chat", "Head", "Prompt", "Plan", "Tools", "Settings"} {
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
	if !strings.Contains(got, "Tool") {
		t.Fatalf("view missing tool timeline line: %q", got)
	}
	if !strings.Contains(got, "Task added") {
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
	sessionID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, sessionID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
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

	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
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
	sessionID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, sessionID)
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
	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
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

func TestChatViewShowsSessionStatusBarAndQueuedDrafts(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
	}

	agent := &runtime.Agent{
		ConfigPath: configPath,
		Config:     config.AgentConfig{ID: "tui-test", Spec: config.AgentConfigSpec{Runtime: config.AgentRuntimeConfig{MaxToolRounds: 7}}},
		Contracts: contracts.ResolvedContracts{
			ProviderRequest: contracts.ProviderRequestContract{
				Transport: contracts.TransportContract{
					ID: "transport-zai",
					Endpoint: contracts.EndpointPolicy{
						Params: contracts.EndpointParams{BaseURL: "https://api.z.ai/api/paas/v4"},
					},
				},
				RequestShape: contracts.RequestShapeContract{
					Model: contracts.ModelPolicy{Params: contracts.ModelParams{Model: "glm-5-turbo"}},
				},
			},
			Chat: contracts.ChatContract{
				Output: contracts.ChatOutputPolicy{Params: contracts.ChatOutputParams{RenderMarkdown: true, MarkdownStyle: "dark"}},
				Status: contracts.ChatStatusPolicy{Params: contracts.ChatStatusParams{ShowToolCalls: true, ShowToolResults: true, ShowPlanAfterPlanTools: true}},
			},
		},
		EventLog:    runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{projections.NewSessionCatalogProjection(), projections.NewTranscriptProjection(), projections.NewChatTimelineProjection(), projections.NewPlanHeadProjection(), projections.NewActivePlanProjection()},
		UIBus:       runtime.NewUIEventBus(),
		Now:         func() time.Time { return time.Date(2026, 4, 15, 18, 0, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}

	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	m.now = func() time.Time { return time.Date(2026, 4, 15, 18, 1, 0, 0, time.UTC) }
	m.clockNow = m.now()
	state := m.sessions[m.activeSessionID]
	state.MainRun = runMeta{
		Active:      true,
		StartedAt:   time.Date(2026, 4, 15, 18, 0, 30, 0, time.UTC),
		Provider:    "api.z.ai",
		Model:       "glm-5-turbo",
		TotalTokens: 42,
	}
	state.Snapshot.ContextBudget = daemon.ContextBudgetSnapshot{
		LastTotalTokens:          42,
		CurrentContextTokens:     120,
		EstimatedNextInputTokens: 133,
		Source:                   "mixed",
		BudgetState:              "healthy",
	}
	state.Queue = []queuedDraft{{Text: "Second question"}, {Text: "Third question"}}

	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	got := modelAfter.View()
	for _, want := range []string{"provider: api.z.ai", "model: glm-5-turbo", "run: running 00:30", "ctx=120", "next≈133", "queue: 2", "Queued interjections:"} {
		if !strings.Contains(got, want) {
			t.Fatalf("view missing %q: %q", want, got)
		}
	}
}

func TestChatEnterQueuesDraftWhileMainRunActive(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 15, 18, 5, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}

	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	state := m.sessions[m.activeSessionID]
	state.MainRun.Active = true
	state.Input.SetValue("Queued while running")

	modelAfter, _ := (&m).Update(tea.KeyMsg{Type: tea.KeyEnter})
	mm := modelAfter.(*model)
	state = mm.sessions[mm.activeSessionID]
	if len(state.Queue) != 1 {
		t.Fatalf("queue len = %d, want 1", len(state.Queue))
	}
	if state.Queue[0].Text != "Queued while running" {
		t.Fatalf("queued draft = %q", state.Queue[0].Text)
	}
	if strings.TrimSpace(state.Input.Value()) != "" {
		t.Fatalf("input value = %q, want cleared", state.Input.Value())
	}
}

func TestChatTabQueuesDraftWithoutSwitchingPanels(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 14, 0, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}

	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	m.tab = tabChat
	state := m.sessions[m.activeSessionID]
	state.Input.SetValue("Queue with tab")

	modelAfter, _ := (&m).Update(tea.KeyMsg{Type: tea.KeyTab})
	mm := modelAfter.(*model)
	if mm.tab != tabChat {
		t.Fatalf("tab switched panel to %v, want chat", mm.tab)
	}
	if len(mm.sessions[mm.activeSessionID].Queue) != 1 {
		t.Fatalf("queue len = %d, want 1", len(mm.sessions[mm.activeSessionID].Queue))
	}
}

func TestChatRunCompletionDispatchesNextQueuedDraft(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 14, 5, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}

	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	state := m.sessions[m.activeSessionID]
	state.MainRun.Active = true
	state.Queue = []queuedDraft{{Text: "follow-up"}}

	modelAfter, cmd := (&m).Update(chatTurnFinishedMsg{
		SessionID: m.activeSessionID,
		Result:    runtimeResultMeta{Provider: "api.z.ai", Model: "glm-5-turbo", TotalTokens: 12},
		Session:   state.Snapshot,
	})
	mm := modelAfter.(*model)
	next := mm.sessions[mm.activeSessionID]
	if !next.MainRun.Active {
		t.Fatal("next queued draft was not dispatched")
	}
	if len(next.Queue) != 0 {
		t.Fatalf("queue len = %d, want 0 after dispatch", len(next.Queue))
	}
	if cmd == nil {
		t.Fatal("dispatch returned nil command")
	}
}

func TestChatDeleteRemovesSelectedQueuedDraft(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 14, 10, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}

	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	state := m.sessions[m.activeSessionID]
	state.Queue = []queuedDraft{{Text: "one"}, {Text: "two"}}
	state.QueueCursor = 1

	modelAfter, _ := (&m).Update(tea.KeyMsg{Type: tea.KeyCtrlD})
	mm := modelAfter.(*model)
	queue := mm.sessions[mm.activeSessionID].Queue
	if len(queue) != 1 || queue[0].Text != "one" {
		t.Fatalf("queue after delete = %#v, want only first draft", queue)
	}
}

func TestChatViewKeepsLayoutWithinWindowHeight(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 14, 15, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}

	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	state := m.sessions[m.activeSessionID]
	state.Queue = []queuedDraft{
		{Text: "one"}, {Text: "two"}, {Text: "three"}, {Text: "four"}, {Text: "five"}, {Text: "six"},
	}

	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 80, Height: 20})
	got := modelAfter.View()
	if lines := len(strings.Split(got, "\n")); lines > 20 {
		t.Fatalf("view line count = %d, want <= 20\n%s", lines, got)
	}
}

func TestGlobalCtrlArrowSwitchesTabs(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 14, 20, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}
	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	m.tab = tabChat
	modelAfter, _ := (&m).Update(tea.KeyMsg{Type: tea.KeyCtrlRight})
	mm := modelAfter.(*model)
	if mm.tab != tabHead {
		t.Fatalf("tab after ctrl+right = %v, want head", mm.tab)
	}
	modelAfter, _ = mm.Update(tea.KeyMsg{Type: tea.KeyCtrlLeft})
	mm = modelAfter.(*model)
	if mm.tab != tabChat {
		t.Fatalf("tab after ctrl+left = %v, want chat", mm.tab)
	}
}

func TestChatCtrlXCancelsActiveRun(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 14, 21, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}
	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	cancelled := false
	state := m.sessions[m.activeSessionID]
	state.MainRun.Active = true
	state.RunCancel = func() { cancelled = true }

	modelAfter, _ := (&m).Update(tea.KeyMsg{Type: tea.KeyCtrlX})
	mm := modelAfter.(*model)
	next := mm.sessions[mm.activeSessionID]
	if !cancelled {
		t.Fatal("run cancel func was not called")
	}
	if next.MainRun.Active {
		t.Fatal("main run still active after ctrl+x")
	}
	if next.Status != "cancelled" {
		t.Fatalf("status = %q, want cancelled", next.Status)
	}
}

func TestChatViewShowsInterjectionHintWhileRunActive(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 14, 22, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}
	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	state := m.sessions[m.activeSessionID]
	state.MainRun.Active = true

	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 100, Height: 24})
	got := modelAfter.View()
	if !strings.Contains(got, "Enter queue interjection") {
		t.Fatalf("view missing interjection hint: %q", got)
	}
}

func TestChatEnterWhileRunActiveReportsQueuedInterjection(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 14, 23, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}
	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	state := m.sessions[m.activeSessionID]
	state.MainRun.Active = true
	state.Input.SetValue("Please stop after this tool")

	modelAfter, _ := (&m).Update(tea.KeyMsg{Type: tea.KeyEnter})
	mm := modelAfter.(*model)
	if mm.statusMessage != "interjection queued for next turn" {
		t.Fatalf("status message = %q", mm.statusMessage)
	}
}

func TestChatQueueViewShowsInterjectionAffordances(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 14, 24, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}
	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	state := m.sessions[m.activeSessionID]
	state.MainRun.Active = true
	state.Queue = []queuedDraft{{Text: "first"}, {Text: "second"}}

	got := m.viewQueue(state)
	for _, want := range []string{"Queued interjections:", "Ctrl+E edit", "Ctrl+D drop", "[next]"} {
		if !strings.Contains(got, want) {
			t.Fatalf("queue view missing %q: %q", want, got)
		}
	}
}

func TestChatHeaderShowsRunAndQueueBadges(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 14, 30, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}
	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	state := m.sessions[m.activeSessionID]
	state.MainRun.Active = true
	state.Queue = []queuedDraft{{Text: "one"}, {Text: "two"}}
	got := m.chatHeader(state)
	for _, want := range []string{"[RUNNING]", "[QUEUED:2]"} {
		if !strings.Contains(got, want) {
			t.Fatalf("header missing %q: %q", want, got)
		}
	}
}

func TestChatViewShowsInterjectionHistory(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 14, 31, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}
	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	state := m.sessions[m.activeSessionID]
	state.MainRun.Active = true
	state.Interjections = []interjectionEntry{
		{Text: "stop after current tool", Status: "queued"},
		{Text: "also check auth", Status: "sent"},
	}

	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 100, Height: 24})
	got := modelAfter.View()
	for _, want := range []string{"OPERATOR:", "[QUEUED] stop after current tool", "[SENT] also check auth"} {
		if !strings.Contains(got, want) {
			t.Fatalf("view missing %q: %q", want, got)
		}
	}
}

func TestChatViewShowsLiveToolActivity(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 8, 0, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}

	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 100, Height: 30})
	mm := modelAfter.(*model)
	state := mm.sessions[mm.activeSessionID]
	state.ToolLog = []toolLogEntry{
		{Activity: runtime.ToolActivity{Phase: runtime.ToolActivityPhaseStarted, Name: "shell_start", Arguments: map[string]any{"command": "curl"}}},
		{Activity: runtime.ToolActivity{Phase: runtime.ToolActivityPhaseCompleted, Name: "shell_start", Arguments: map[string]any{"command": "curl"}, ErrorText: "tool call \"shell_start\" requires approval"}},
	}

	mm.renderChatViewport(state)
	got := state.ChatView.View()
	if !strings.Contains(got, "tool started: shell_start") {
		t.Fatalf("chat view missing live tool start: %q", got)
	}
	if !strings.Contains(got, "approval required: shell_start") {
		t.Fatalf("chat view missing live tool approval state: %q", got)
	}
}

func TestToolDetailsShowFullResultText(t *testing.T) {
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
		Now:         func() time.Time { return time.Date(2026, 4, 16, 8, 0, 0, 0, time.UTC) },
		NewID:       func(prefix string) string { return prefix + "-1" },
	}

	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	state := m.sessions[m.activeSessionID]
	longResult := strings.Repeat("line with payload\n", 20)
	state.ToolLog = []toolLogEntry{
		{Activity: runtime.ToolActivity{Phase: runtime.ToolActivityPhaseCompleted, Name: "shell_exec", ResultText: longResult}},
	}
	details := m.renderToolDetails(state)
	if !strings.Contains(details, longResult) {
		t.Fatalf("tool details truncated result: %q", details)
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
	resumeID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, resumeID)
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

	modelAfter, cmd := mm.Update(tea.KeyMsg{Type: tea.KeyF8})
	mm = modelAfter.(*model)
	if mm.mouseCaptureEnabled {
		t.Fatal("mouse capture should be disabled after F8")
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
		fmt.Sprintf("%T", tea.DisableMouse()):  true,
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

	modelAfter, cmd = mm.Update(tea.KeyMsg{Type: tea.KeyF8})
	mm = modelAfter.(*model)
	if !mm.mouseCaptureEnabled {
		t.Fatal("mouse capture should be re-enabled after second F8")
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
		fmt.Sprintf("%T", tea.EnterAltScreen()):        true,
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
	sessionID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, sessionID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
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
	sessionID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, sessionID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
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

func TestToolsViewShowsPendingShellApproval(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
	}

	agent := &runtime.Agent{
		ConfigPath:   configPath,
		Config:       config.AgentConfig{ID: "tui-test", Spec: config.AgentConfigSpec{Runtime: config.AgentRuntimeConfig{MaxToolRounds: 7}}},
		EventLog:     runtime.NewInMemoryEventLog(),
		Projections:  []projections.Projection{projections.NewSessionCatalogProjection(), projections.NewTranscriptProjection(), projections.NewChatTimelineProjection(), projections.NewPlanHeadProjection(), projections.NewActivePlanProjection(), projections.NewShellCommandProjection()},
		UIBus:        runtime.NewUIEventBus(),
		ShellRuntime: shell.NewExecutor(),
		Contracts: contracts.ResolvedContracts{
			Chat: contracts.ChatContract{
				Output: contracts.ChatOutputPolicy{Params: contracts.ChatOutputParams{RenderMarkdown: true, MarkdownStyle: "dark"}},
				Status: contracts.ChatStatusPolicy{Params: contracts.ChatStatusParams{ShowToolCalls: true, ShowToolResults: true, ShowPlanAfterPlanTools: true}},
			},
			ShellExecution: contracts.ShellExecutionContract{
				Command: contracts.ShellCommandPolicy{
					Enabled:  true,
					Strategy: "static_allowlist",
					Params:   contracts.ShellCommandParams{AllowedCommands: []string{"go"}},
				},
				Approval: contracts.ShellApprovalPolicy{
					Enabled:  true,
					Strategy: "always_require",
				},
				Runtime: contracts.ShellRuntimePolicy{
					Enabled:  true,
					Strategy: "workspace_write",
					Params: contracts.ShellRuntimeParams{
						Cwd:            dir,
						Timeout:        "5s",
						MaxOutputBytes: 4096,
						AllowNetwork:   true,
					},
				},
			},
		},
		Now:   func() time.Time { return time.Date(2026, 4, 15, 12, 0, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	sessionID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, sessionID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}

	if _, err := agent.ShellRuntime.ExecuteWithMeta(context.Background(), agent.Contracts.ShellExecution, "shell_start", map[string]any{
		"command": "go",
		"args":    []any{"test"},
	}, shell.ExecutionMeta{SessionID: m.activeSessionID, RunID: "run-1", RecordEvent: agent.RecordEvent, Now: agent.Now, NewID: agent.NewID}); err != nil {
		t.Fatalf("ExecuteWithMeta returned error: %v", err)
	}

	m.tab = tabTools
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	got := modelAfter.View()
	if !strings.Contains(got, "Pending Approvals") {
		t.Fatalf("tools view missing approvals section: %q", got)
	}
	if !strings.Contains(got, "shell_start") {
		t.Fatalf("tools view missing approval tool name: %q", got)
	}
	if !strings.Contains(got, "approve") {
		t.Fatalf("tools details missing approval actions: %q", got)
	}
	if !strings.Contains(got, "allow forever") || !strings.Contains(got, "deny forever") {
		t.Fatalf("tools details missing persistent approval actions: %q", got)
	}
}

func TestToolsViewCanKillRunningShellCommand(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
	}

	agent := &runtime.Agent{
		ConfigPath:   configPath,
		Config:       config.AgentConfig{ID: "tui-test", Spec: config.AgentConfigSpec{Runtime: config.AgentRuntimeConfig{MaxToolRounds: 7}}},
		EventLog:     runtime.NewInMemoryEventLog(),
		Projections:  []projections.Projection{projections.NewSessionCatalogProjection(), projections.NewTranscriptProjection(), projections.NewChatTimelineProjection(), projections.NewPlanHeadProjection(), projections.NewActivePlanProjection(), projections.NewShellCommandProjection()},
		UIBus:        runtime.NewUIEventBus(),
		ShellRuntime: shell.NewExecutor(),
		Contracts: contracts.ResolvedContracts{
			Chat: contracts.ChatContract{
				Output: contracts.ChatOutputPolicy{Params: contracts.ChatOutputParams{RenderMarkdown: true, MarkdownStyle: "dark"}},
				Status: contracts.ChatStatusPolicy{Params: contracts.ChatStatusParams{ShowToolCalls: true, ShowToolResults: true, ShowPlanAfterPlanTools: true}},
			},
			ShellExecution: contracts.ShellExecutionContract{
				Command: contracts.ShellCommandPolicy{
					Enabled:  true,
					Strategy: "static_allowlist",
					Params:   contracts.ShellCommandParams{AllowedCommands: []string{"sleep"}},
				},
				Approval: contracts.ShellApprovalPolicy{
					Enabled:  true,
					Strategy: "always_allow",
				},
				Runtime: contracts.ShellRuntimePolicy{
					Enabled:  true,
					Strategy: "workspace_write",
					Params: contracts.ShellRuntimeParams{
						Cwd:            dir,
						Timeout:        "5s",
						MaxOutputBytes: 4096,
						AllowNetwork:   true,
					},
				},
			},
		},
		Now:   func() time.Time { return time.Date(2026, 4, 15, 12, 5, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	sessionID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, sessionID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}

	if _, err := agent.ShellRuntime.ExecuteWithMeta(context.Background(), agent.Contracts.ShellExecution, "shell_start", map[string]any{
		"command": "sleep",
		"args":    []any{"2"},
	}, shell.ExecutionMeta{SessionID: m.activeSessionID, RunID: "run-1", RecordEvent: agent.RecordEvent, Now: agent.Now, NewID: agent.NewID}); err != nil {
		t.Fatalf("ExecuteWithMeta returned error: %v", err)
	}

	m.tab = tabTools
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	mm := modelAfter.(*model)
	if got := mm.View(); !strings.Contains(got, "Running Shell Commands") || !strings.Contains(got, "sleep") {
		t.Fatalf("tools view missing running command: %q", got)
	}
	modelAfter, _ = mm.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("k")})
	mm = modelAfter.(*model)
	if !strings.Contains(mm.statusMessage, "kill requested") {
		t.Fatalf("statusMessage = %q, want kill requested", mm.statusMessage)
	}
}

func TestToolsViewReadsRunningCommandsFromProjection(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "agent.yaml")
	if err := os.WriteFile(configPath, []byte("kind: AgentConfig\nversion: v1\nid: tui-test\nspec:\n  runtime:\n    max_tool_rounds: 7\n"), 0o644); err != nil {
		t.Fatalf("WriteFile config: %v", err)
	}

	agent := &runtime.Agent{
		ConfigPath: configPath,
		Config:     config.AgentConfig{ID: "tui-test", Spec: config.AgentConfigSpec{Runtime: config.AgentRuntimeConfig{MaxToolRounds: 7}}},
		EventLog:   runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewActivePlanProjection(),
			projections.NewShellCommandProjection(),
		},
		UIBus: runtime.NewUIEventBus(),
		Contracts: contracts.ResolvedContracts{
			Chat: contracts.ChatContract{
				Output: contracts.ChatOutputPolicy{Params: contracts.ChatOutputParams{RenderMarkdown: true, MarkdownStyle: "dark"}},
				Status: contracts.ChatStatusPolicy{Params: contracts.ChatStatusParams{ShowToolCalls: true, ShowToolResults: true, ShowPlanAfterPlanTools: true}},
			},
		},
		Now:   func() time.Time { return time.Date(2026, 4, 15, 12, 10, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	sessionID := seedSessionForTUITest(t, agent)
	m, err := newModel(context.Background(), agent, sessionID)
	if err != nil {
		t.Fatalf("newModel returned error: %v", err)
	}
	if err := agent.RecordEvent(context.Background(), eventing.Event{
		Kind:          eventing.EventShellCommandStarted,
		AggregateID:   "cmd-1",
		AggregateType: eventing.AggregateShellCommand,
		Payload: map[string]any{
			"session_id": sessionID,
			"run_id":     "run-1",
			"command":    "sleep",
			"args":       []string{"2"},
			"cwd":        dir,
		},
	}); err != nil {
		t.Fatalf("RecordEvent shell command started: %v", err)
	}
	m.tab = tabTools
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	got := modelAfter.View()
	if !strings.Contains(got, "Running Shell Commands") || !strings.Contains(got, "sleep 2") {
		t.Fatalf("tools view missing projection-backed command: %q", got)
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

func seedSessionForTUITest(t *testing.T, agent *runtime.Agent) string {
	t.Helper()
	sessionID := "session-1"
	if err := agent.RecordEvent(context.Background(), eventSessionCreated(sessionID)); err != nil {
		t.Fatalf("RecordEvent session created: %v", err)
	}
	return sessionID
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
