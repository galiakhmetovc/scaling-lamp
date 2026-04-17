package tui

import (
	"context"
	"fmt"
	"strings"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"teamd/internal/contracts"
	"teamd/internal/runtime"
	"teamd/internal/runtime/daemon"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
)

type stubOperatorClient struct {
	bootstrap daemon.BootstrapPayload
	sessions  []SessionSummary
	snapshot  daemon.SessionSnapshot
	settings  daemon.SettingsSnapshot
	ws        <-chan daemon.WebsocketEnvelope
	history   SessionHistoryChunk
	historyCalls int
	renamedTo string
	deletedSessionID string
	savedPrompt string
	resetPromptSessionID string
	sentChatSessionID string
	sentChatPrompt string
	approvedShellID string
	approvedAlwaysShellID string
	deniedShellID string
	deniedAlwaysShellID string
}

func (c *stubOperatorClient) Bootstrap(context.Context) (daemon.BootstrapPayload, error) {
	return c.bootstrap, nil
}

func (c *stubOperatorClient) ListSessions(context.Context) ([]SessionSummary, error) {
	return append([]SessionSummary(nil), c.sessions...), nil
}

func (c *stubOperatorClient) CreateSession(context.Context) (daemon.SessionSnapshot, error) {
	return c.snapshot, nil
}

func (c *stubOperatorClient) GetSession(context.Context, string) (daemon.SessionSnapshot, error) {
	return c.snapshot, nil
}

func (c *stubOperatorClient) RenameSession(_ context.Context, sessionID, title string) (daemon.SessionSnapshot, error) {
	c.deletedSessionID = ""
	c.renamedTo = title
	c.snapshot.Title = title
	return c.snapshot, nil
}

func (c *stubOperatorClient) DeleteSession(_ context.Context, sessionID string) error {
	c.deletedSessionID = sessionID
	return nil
}

func (c *stubOperatorClient) GetSessionHistory(context.Context, string, int, int) (SessionHistoryChunk, error) {
	c.historyCalls++
	return c.history, nil
}

func (c *stubOperatorClient) SetSessionPromptOverride(_ context.Context, sessionID, content string) (daemon.SessionSnapshot, error) {
	c.savedPrompt = content
	c.snapshot.Prompt.Override = content
	c.snapshot.Prompt.Effective = content
	c.snapshot.Prompt.HasOverride = true
	return c.snapshot, nil
}

func (c *stubOperatorClient) ClearSessionPromptOverride(_ context.Context, sessionID string) (daemon.SessionSnapshot, error) {
	c.resetPromptSessionID = sessionID
	c.snapshot.Prompt.Override = ""
	c.snapshot.Prompt.Effective = c.snapshot.Prompt.Default
	c.snapshot.Prompt.HasOverride = false
	return c.snapshot, nil
}

func (c *stubOperatorClient) SendChat(_ context.Context, sessionID, prompt string) (ChatSendResult, error) {
	c.sentChatSessionID = sessionID
	c.sentChatPrompt = prompt
	return ChatSendResult{}, nil
}

func (c *stubOperatorClient) SendBtw(context.Context, string, string) (BtwResult, error) {
	return BtwResult{}, nil
}

func (c *stubOperatorClient) CreatePlan(context.Context, string, string) (PlanMutation, error) {
	return PlanMutation{}, nil
}

func (c *stubOperatorClient) AddPlanTask(context.Context, string, string) (PlanMutation, error) {
	return PlanMutation{}, nil
}

func (c *stubOperatorClient) EditPlanTask(context.Context, string, string, string, []string) (PlanMutation, error) {
	return PlanMutation{}, nil
}

func (c *stubOperatorClient) SetPlanTaskStatus(context.Context, string, string, string, string) (PlanMutation, error) {
	return PlanMutation{}, nil
}

func (c *stubOperatorClient) AddPlanTaskNote(context.Context, string, string, string) (PlanMutation, error) {
	return PlanMutation{}, nil
}

func (c *stubOperatorClient) ApproveShell(_ context.Context, approvalID string) (ShellActionResult, error) {
	c.approvedShellID = approvalID
	return ShellActionResult{Session: c.snapshot}, nil
}

func (c *stubOperatorClient) ApproveShellAlways(_ context.Context, approvalID string) (ShellActionResult, error) {
	c.approvedAlwaysShellID = approvalID
	return ShellActionResult{Session: c.snapshot}, nil
}

func (c *stubOperatorClient) DenyShell(_ context.Context, approvalID string) (ShellActionResult, error) {
	c.deniedShellID = approvalID
	return ShellActionResult{Session: c.snapshot}, nil
}

func (c *stubOperatorClient) DenyShellAlways(_ context.Context, approvalID string) (ShellActionResult, error) {
	c.deniedAlwaysShellID = approvalID
	return ShellActionResult{Session: c.snapshot}, nil
}

func (c *stubOperatorClient) KillShell(context.Context, string) (ShellActionResult, error) {
	return ShellActionResult{}, nil
}

func (c *stubOperatorClient) GetSettings(context.Context) (daemon.SettingsSnapshot, error) {
	return c.settings, nil
}

func (c *stubOperatorClient) ApplySettingsForm(context.Context, string, map[string]any) (daemon.SettingsSnapshot, error) {
	return c.settings, nil
}

func (c *stubOperatorClient) GetSettingsRaw(context.Context, string) (daemon.SettingsRawFileContent, error) {
	return daemon.SettingsRawFileContent{}, nil
}

func (c *stubOperatorClient) ApplySettingsRaw(context.Context, string, string, string) (daemon.SettingsSnapshot, error) {
	return c.settings, nil
}

func (c *stubOperatorClient) Subscribe(context.Context) (<-chan daemon.WebsocketEnvelope, func(), error) {
	return c.ws, func() {}, nil
}

func (c *stubOperatorClient) DefaultOverrides() sessionOverrides {
	return sessionOverrides{MaxToolRounds: 7, RenderMarkdown: true, MarkdownStyle: "dark"}
}

func (c *stubOperatorClient) ChatCommandPolicy() contracts.ChatCommandParams {
	return contracts.ChatCommandParams{ExitCommand: "/exit", HelpCommand: "/help", SessionCommand: "/session", BtwCommand: "/btw"}
}

func (c *stubOperatorClient) ProviderLabel() string { return "stub-provider" }
func (c *stubOperatorClient) ConfigPath() string    { return "/tmp/agent.yaml" }
func (c *stubOperatorClient) ConfigID() string      { return "stub-agent" }

func TestNewModelWithClientInitializesWithoutRuntimeAgent(t *testing.T) {
	ws := make(chan daemon.WebsocketEnvelope)
	close(ws)
	now := time.Date(2026, 4, 15, 20, 15, 0, 0, time.UTC)
	client := &stubOperatorClient{
		sessions: []SessionSummary{{
			SessionID:    "session-1",
			CreatedAt:    now,
			LastActivity: now,
			MessageCount: 1,
		}},
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
		settings: daemon.SettingsSnapshot{
			Revision: "rev-1",
			FormFields: []daemon.SettingsFieldState{
				{Key: "max_tool_rounds", Value: 7},
				{Key: "render_markdown", Value: true},
				{Key: "markdown_style", Value: "dark"},
				{Key: "show_tool_calls", Value: true},
				{Key: "show_tool_results", Value: true},
				{Key: "show_plan_after_plan_tools", Value: true},
			},
		},
		ws: ws,
	}

	m, err := newModelWithClient(context.Background(), client, "")
	if err != nil {
		t.Fatalf("newModelWithClient returned error: %v", err)
	}
	if m.activeSessionID != "session-1" {
		t.Fatalf("activeSessionID = %q, want session-1", m.activeSessionID)
	}
	if m.currentSessionState() == nil {
		t.Fatalf("currentSessionState returned nil")
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 100, Height: 30})
	got := modelAfter.View()
	if got == "" {
		t.Fatalf("View returned empty output")
	}
}

func TestNewModelWithClientRendersHeadAndPromptTabs(t *testing.T) {
	ws := make(chan daemon.WebsocketEnvelope)
	close(ws)
	now := time.Date(2026, 4, 17, 10, 15, 0, 0, time.UTC)
	client := &stubOperatorClient{
		sessions: []SessionSummary{{
			SessionID:    "session-1",
			Title:        "Release work",
			CreatedAt:    now,
			LastActivity: now,
			MessageCount: 1,
		}},
		snapshot: daemon.SessionSnapshot{
			SessionID:    "session-1",
			Title:        "Release work",
			CreatedAt:    now,
			LastActivity: now,
			MessageCount: 1,
			Prompt: daemon.SessionPromptSnapshot{
				Default:   "default prompt",
				Effective: "default prompt",
			},
		},
		ws: ws,
	}

	m, err := newModelWithClient(context.Background(), client, "")
	if err != nil {
		t.Fatalf("newModelWithClient returned error: %v", err)
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 100, Height: 30})
	got := modelAfter.View()
	for _, tab := range []string{"Sessions", "Chat", "Head", "Prompt", "Plan", "Tools", "Settings"} {
		if !strings.Contains(got, tab) {
			t.Fatalf("view missing tab %q: %q", tab, got)
		}
	}
}

func TestChatPgUpLoadsOlderHistoryWhenAtTop(t *testing.T) {
	ws := make(chan daemon.WebsocketEnvelope)
	close(ws)
	now := time.Date(2026, 4, 17, 10, 30, 0, 0, time.UTC)
	client := &stubOperatorClient{
		sessions: []SessionSummary{{SessionID: "session-1", CreatedAt: now, LastActivity: now, MessageCount: 3}},
		snapshot: daemon.SessionSnapshot{
			SessionID: "session-1",
			History: daemon.ChatHistorySnapshot{
				LoadedCount: 1,
				TotalCount:  3,
				HasMore:     true,
				WindowLimit: 1,
			},
			Timeline: []projections.ChatTimelineItem{{Kind: projections.ChatTimelineItemMessage, Role: "assistant", Content: "latest"}},
			Prompt:   daemon.SessionPromptSnapshot{Default: "default prompt", Effective: "default prompt"},
		},
		history: SessionHistoryChunk{
			SessionID:   "session-1",
			LoadedCount: 2,
			TotalCount:  3,
			HasMore:     true,
			WindowLimit: 1,
			Timeline:    []projections.ChatTimelineItem{{Kind: projections.ChatTimelineItemMessage, Role: "user", Content: "older"}},
		},
		ws: ws,
	}

	m, err := newModelWithClient(context.Background(), client, "")
	if err != nil {
		t.Fatalf("newModelWithClient returned error: %v", err)
	}
	state := m.currentSessionState()
	state.ChatView.SetYOffset(0)

	next, cmd := (&m).Update(tea.WindowSizeMsg{Width: 100, Height: 30})
	if cmd != nil {
		_, _ = next, cmd
	}
	updated, cmd := next.(*model).Update(tea.KeyMsg{Type: tea.KeyPgUp})
	if cmd == nil {
		t.Fatalf("pgup did not return history load cmd")
	}
	msg := cmd()
	final, _ := updated.(*model).Update(msg)
	got := final.(*model).sessions["session-1"].Snapshot
	if client.historyCalls != 1 {
		t.Fatalf("historyCalls = %d, want 1", client.historyCalls)
	}
	if len(got.Timeline) != 2 {
		t.Fatalf("timeline len = %d, want 2", len(got.Timeline))
	}
	if got.Timeline[0].Content != "older" {
		t.Fatalf("first timeline item = %q, want older", got.Timeline[0].Content)
	}
}

func TestLocalSessionSnapshotStartsWithHistoryWindow(t *testing.T) {
	now := time.Date(2026, 4, 17, 11, 0, 0, 0, time.UTC)
	agent := &runtime.Agent{
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewShellCommandProjection(),
			projections.NewDelegateProjection(),
		},
		Now:   func() time.Time { return now },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	if err := agent.RecordEvent(context.Background(), eventing.Event{
		ID:               "evt-session-created",
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       now,
		AggregateID:      "session-1",
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		Payload:          map[string]any{"session_id": "session-1"},
	}); err != nil {
		t.Fatalf("record session created: %v", err)
	}
	for i := 0; i < 45; i++ {
		if err := agent.RecordEvent(context.Background(), eventing.Event{
			ID:               fmt.Sprintf("evt-message-%d", i),
			Kind:             eventing.EventMessageRecorded,
			OccurredAt:       now.Add(time.Duration(i) * time.Minute),
			AggregateID:      "session-1",
			AggregateType:    eventing.AggregateSession,
			AggregateVersion: uint64(i + 2),
			Payload: map[string]any{
				"session_id": "session-1",
				"role":       "assistant",
				"content":    fmt.Sprintf("message-%02d", i),
			},
		}); err != nil {
			t.Fatalf("record message %d: %v", i, err)
		}
	}

	snapshot, err := buildLocalSessionSnapshot(agent, "session-1")
	if err != nil {
		t.Fatalf("buildLocalSessionSnapshot returned error: %v", err)
	}
	if snapshot.History.LoadedCount != 40 {
		t.Fatalf("loaded_count = %d, want 40", snapshot.History.LoadedCount)
	}
	if snapshot.History.TotalCount != 45 {
		t.Fatalf("total_count = %d, want 45", snapshot.History.TotalCount)
	}
	if !snapshot.History.HasMore {
		t.Fatalf("has_more = false, want true")
	}
	if len(snapshot.Timeline) != 40 {
		t.Fatalf("timeline len = %d, want 40", len(snapshot.Timeline))
	}
	if snapshot.Timeline[0].Content != "message-05" {
		t.Fatalf("first timeline item = %q, want message-05", snapshot.Timeline[0].Content)
	}
}

func TestChatSubmitShowsPendingPromptBeforeTurnCompletes(t *testing.T) {
	ws := make(chan daemon.WebsocketEnvelope)
	close(ws)
	now := time.Date(2026, 4, 17, 12, 0, 0, 0, time.UTC)
	client := &stubOperatorClient{
		sessions: []SessionSummary{{SessionID: "session-1", CreatedAt: now, LastActivity: now, MessageCount: 0}},
		snapshot: daemon.SessionSnapshot{
			SessionID: "session-1",
			Prompt: daemon.SessionPromptSnapshot{
				Default:   "default prompt",
				Effective: "default prompt",
			},
		},
		ws: ws,
	}

	m, err := newModelWithClient(context.Background(), client, "")
	if err != nil {
		t.Fatalf("newModelWithClient returned error: %v", err)
	}
	m.now = func() time.Time { return now }
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 100, Height: 30})
	mm := modelAfter.(*model)
	state := mm.currentSessionState()
	state.Input.SetValue("ship it")

	modelAfter, cmd := mm.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd == nil {
		t.Fatal("submit did not return chat command")
	}
	mm = modelAfter.(*model)
	state = mm.currentSessionState()
	if state.PendingPrompt != "ship it" {
		t.Fatalf("pending prompt = %q, want ship it", state.PendingPrompt)
	}
	if strings.TrimSpace(state.Input.Value()) != "" {
		t.Fatalf("input value = %q, want cleared", state.Input.Value())
	}
	if !strings.Contains(state.ChatView.View(), "USER [pending]:") || !strings.Contains(state.ChatView.View(), "ship it") {
		t.Fatalf("chat view missing pending prompt: %q", state.ChatView.View())
	}
}

func TestChatInputApproveShortcutHandlesPendingApproval(t *testing.T) {
	ws := make(chan daemon.WebsocketEnvelope)
	close(ws)
	now := time.Date(2026, 4, 17, 12, 5, 0, 0, time.UTC)
	client := &stubOperatorClient{
		sessions: []SessionSummary{{SessionID: "session-1", CreatedAt: now, LastActivity: now, MessageCount: 0}},
		snapshot: daemon.SessionSnapshot{
			SessionID: "session-1",
			PendingApprovals: []shell.PendingApprovalView{{
				ApprovalID: "approval-1",
				Command:    "go",
				Args:       []string{"test"},
			}},
			Prompt: daemon.SessionPromptSnapshot{
				Default:   "default prompt",
				Effective: "default prompt",
			},
		},
		ws: ws,
	}

	m, err := newModelWithClient(context.Background(), client, "")
	if err != nil {
		t.Fatalf("newModelWithClient returned error: %v", err)
	}
	state := m.currentSessionState()
	state.Input.SetValue("y")

	modelAfter, cmd := (&m).Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd != nil {
		t.Fatalf("approval shortcut returned unexpected cmd")
	}
	mm := modelAfter.(*model)
	if client.approvedShellID != "approval-1" {
		t.Fatalf("approved shell id = %q, want approval-1", client.approvedShellID)
	}
	if got := mm.currentSessionState().Input.Value(); strings.TrimSpace(got) != "" {
		t.Fatalf("input value = %q, want cleared", got)
	}
	if !strings.Contains(mm.statusMessage, "approval granted") {
		t.Fatalf("statusMessage = %q, want approval granted", mm.statusMessage)
	}
}
