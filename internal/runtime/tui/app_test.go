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
