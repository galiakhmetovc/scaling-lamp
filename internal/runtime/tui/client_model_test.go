package tui

import (
	"context"
	"strings"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"teamd/internal/contracts"
	"teamd/internal/runtime/daemon"
	"teamd/internal/runtime/projections"
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

func (c *stubOperatorClient) SendChat(context.Context, string, string) (ChatSendResult, error) {
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

func (c *stubOperatorClient) ApproveShell(context.Context, string) (ShellActionResult, error) {
	return ShellActionResult{}, nil
}

func (c *stubOperatorClient) ApproveShellAlways(context.Context, string) (ShellActionResult, error) {
	return ShellActionResult{}, nil
}

func (c *stubOperatorClient) DenyShell(context.Context, string) (ShellActionResult, error) {
	return ShellActionResult{}, nil
}

func (c *stubOperatorClient) DenyShellAlways(context.Context, string) (ShellActionResult, error) {
	return ShellActionResult{}, nil
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
