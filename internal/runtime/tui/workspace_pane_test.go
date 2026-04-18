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
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestWorkspaceTabScaffold(t *testing.T) {
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

	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	got := modelAfter.View()
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

	m.tab = tabWorkspace
	modelAfter, _ = (&m).Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	got = modelAfter.View()
	if !strings.Contains(got, "Workspace pane") {
		t.Fatalf("workspace tab did not render placeholder view: %q", got)
	}
	if strings.Contains(got, "No active plan") || strings.Contains(got, "Goal:") {
		t.Fatalf("workspace tab fell through to the plan pane: %q", got)
	}
}
