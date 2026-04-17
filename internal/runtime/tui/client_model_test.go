package tui

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"net/http"
	"path/filepath"
	"strings"
	"testing"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"teamd/internal/contracts"
	"teamd/internal/delegation"
	"teamd/internal/filesystem"
	"teamd/internal/provider"
	"teamd/internal/runtime"
	"teamd/internal/runtime/daemon"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
	"teamd/internal/tools"
)

type stubOperatorClient struct {
	bootstrap             daemon.BootstrapPayload
	sessions              []SessionSummary
	snapshot              daemon.SessionSnapshot
	approveShellResult    *daemon.SessionSnapshot
	approveAlwaysResult   *daemon.SessionSnapshot
	denyShellResult       *daemon.SessionSnapshot
	denyAlwaysResult      *daemon.SessionSnapshot
	settings              daemon.SettingsSnapshot
	ws                    <-chan daemon.WebsocketEnvelope
	history               SessionHistoryChunk
	historyCalls          int
	createCalls           int
	renamedTo             string
	deletedSessionID      string
	savedPrompt           string
	resetPromptSessionID  string
	sentChatSessionID     string
	sentChatPrompt        string
	approvedShellID       string
	approvedAlwaysShellID string
	deniedShellID         string
	deniedAlwaysShellID   string
}

func (c *stubOperatorClient) Bootstrap(context.Context) (daemon.BootstrapPayload, error) {
	return c.bootstrap, nil
}

func (c *stubOperatorClient) ListSessions(context.Context) ([]SessionSummary, error) {
	return append([]SessionSummary(nil), c.sessions...), nil
}

func (c *stubOperatorClient) CreateSession(context.Context) (daemon.SessionSnapshot, error) {
	c.createCalls++
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
	if c.approveShellResult != nil {
		return ShellActionResult{Session: *c.approveShellResult}, nil
	}
	return ShellActionResult{Session: c.snapshot}, nil
}

func (c *stubOperatorClient) ApproveShellAlways(_ context.Context, approvalID string) (ShellActionResult, error) {
	c.approvedAlwaysShellID = approvalID
	if c.approveAlwaysResult != nil {
		return ShellActionResult{Session: *c.approveAlwaysResult}, nil
	}
	return ShellActionResult{Session: c.snapshot}, nil
}

func (c *stubOperatorClient) DenyShell(_ context.Context, approvalID string) (ShellActionResult, error) {
	c.deniedShellID = approvalID
	if c.denyShellResult != nil {
		return ShellActionResult{Session: *c.denyShellResult}, nil
	}
	return ShellActionResult{Session: c.snapshot}, nil
}

func (c *stubOperatorClient) DenyShellAlways(_ context.Context, approvalID string) (ShellActionResult, error) {
	c.deniedAlwaysShellID = approvalID
	if c.denyAlwaysResult != nil {
		return ShellActionResult{Session: *c.denyAlwaysResult}, nil
	}
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

func TestNewModelWithClientDoesNotAutoCreateSessionWhenCatalogEmpty(t *testing.T) {
	ws := make(chan daemon.WebsocketEnvelope)
	close(ws)
	client := &stubOperatorClient{
		sessions: nil,
		snapshot: daemon.SessionSnapshot{
			SessionID: "session-created",
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
	if client.createCalls != 0 {
		t.Fatalf("createCalls = %d, want 0", client.createCalls)
	}
	if m.activeSessionID != "" {
		t.Fatalf("activeSessionID = %q, want empty", m.activeSessionID)
	}
	if len(m.sessionOrder) != 0 {
		t.Fatalf("sessionOrder len = %d, want 0", len(m.sessionOrder))
	}
	modelAfter, _ := (&m).Update(tea.WindowSizeMsg{Width: 100, Height: 30})
	got := modelAfter.View()
	if !strings.Contains(got, "No active session") {
		t.Fatalf("view missing empty-state text: %q", got)
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

func TestLocalClientApproveShellContinuesChatRun(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	dir := t.TempDir()
	now := time.Date(2026, 4, 17, 18, 0, 0, 0, time.UTC)
	call := 0
	agent := &runtime.Agent{
		ConfigPath:    filepath.Join(dir, "agent.yaml"),
		Contracts:     localClientChatContractsForTest(dir),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(localClientFakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				call++
				if call == 1 {
					return &http.Response{
						StatusCode: http.StatusOK,
						Header:     http.Header{"Content-Type": []string{"application/json"}},
						Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-local-approval-1","model":"glm-5-turbo","choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":"","tool_calls":[{"id":"call-local-approval-1","function":{"name":"shell_exec","arguments":{"command":"pwd"}}}]}}]}`)),
					}, nil
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-local-approval-2","model":"glm-5-turbo","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"done after approval"}}]}`)),
				}, nil
			},
		}),
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewShellCommandProjection(),
			projections.NewDelegateProjection(),
			projections.NewRunProjection(),
		},
		Now:   func() time.Time { return now },
		NewID: func(prefix string) string { return fmt.Sprintf("%s-%d", prefix, time.Now().UTC().UnixNano()) },
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)
	agent.ShellRuntime = shell.NewExecutor()
	agent.Contracts.ShellExecution.Approval = contracts.ShellApprovalPolicy{Enabled: true, Strategy: "always_require"}
	agent.Contracts.ShellExecution.Runtime.Params.AllowNetwork = true

	client := newLocalClient(agent)
	session, err := agent.NewChatSession()
	if err != nil {
		t.Fatalf("NewChatSession returned error: %v", err)
	}
	if err := agent.RecordEvent(context.Background(), eventing.Event{
		ID:               "evt-session-created-local-client",
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       now,
		AggregateID:      session.SessionID,
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		Payload:          map[string]any{"session_id": session.SessionID},
	}); err != nil {
		t.Fatalf("record session created: %v", err)
	}

	sendResult, err := client.SendChat(context.Background(), session.SessionID, "run pwd")
	if err != nil {
		t.Fatalf("SendChat returned error: %v", err)
	}
	if len(sendResult.Session.PendingApprovals) != 1 {
		t.Fatalf("pending approvals = %d, want 1", len(sendResult.Session.PendingApprovals))
	}
	if call != 1 {
		t.Fatalf("provider call count after send = %d, want 1", call)
	}

	approvalID := sendResult.Session.PendingApprovals[0].ApprovalID
	approveResult, err := client.ApproveShell(context.Background(), approvalID)
	if err != nil {
		t.Fatalf("ApproveShell returned error: %v", err)
	}
	if call != 2 {
		t.Fatalf("provider call count after approve = %d, want 2", call)
	}
	if len(approveResult.Session.PendingApprovals) != 0 {
		t.Fatalf("pending approvals after approve = %d, want 0", len(approveResult.Session.PendingApprovals))
	}
	if got := approveResult.Session.Timeline[len(approveResult.Session.Timeline)-1].Content; !strings.Contains(got, "done after approval") {
		t.Fatalf("final timeline item = %q, want done after approval", got)
	}
}

type localClientFakeDoer struct {
	do func(*http.Request) (*http.Response, error)
}

func (d localClientFakeDoer) Do(req *http.Request) (*http.Response, error) {
	return d.do(req)
}

func localClientChatContractsForTest(root string) contracts.ResolvedContracts {
	out := contracts.ResolvedContracts{
		ProviderRequest: contracts.ProviderRequestContract{
			Transport: contracts.TransportContract{
				ID: "transport-local-client-test",
				Endpoint: contracts.EndpointPolicy{
					Enabled:  true,
					Strategy: "static",
					Params: contracts.EndpointParams{
						BaseURL: "https://api.z.ai/api/coding/paas/v4",
						Path:    "/chat/completions",
						Method:  http.MethodPost,
					},
				},
				Auth: contracts.AuthPolicy{
					Enabled:  true,
					Strategy: "bearer_token",
					Params: contracts.AuthParams{
						Header:      "Authorization",
						Prefix:      "Bearer",
						ValueEnvVar: "TEAMD_ZAI_API_KEY",
					},
				},
			},
			RequestShape: contracts.RequestShapeContract{
				ID:        "request-shape-local-client-test",
				Model:     contracts.ModelPolicy{Enabled: true, Strategy: "static_model", Params: contracts.ModelParams{Model: "glm-5-turbo"}},
				Messages:  contracts.MessagePolicy{Enabled: true, Strategy: "raw_messages"},
				Tools:     contracts.ToolPolicy{Enabled: true, Strategy: "tools_inline"},
				Streaming: contracts.StreamingPolicy{Enabled: true, Strategy: "static_stream", Params: contracts.StreamingParams{Stream: false}},
			},
		},
	}
	out.Tools = contracts.ToolContract{
		Catalog: contracts.ToolCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ToolCatalogParams{ToolIDs: []string{"shell_exec"}},
		},
		Serialization: contracts.ToolSerializationPolicy{
			Enabled:  true,
			Strategy: "openai_function_tools",
			Params:   contracts.ToolSerializationParams{IncludeDescriptions: true},
		},
	}
	out.ToolExecution = contracts.ToolExecutionContract{
		Access: contracts.ToolAccessPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ToolAccessParams{ToolIDs: []string{"shell_exec"}},
		},
		Approval: contracts.ToolApprovalPolicy{Enabled: true, Strategy: "always_allow"},
		Sandbox:  contracts.ToolSandboxPolicy{Enabled: true, Strategy: "workspace_write"},
	}
	out.ShellTools = contracts.ShellToolContract{
		Catalog: contracts.ShellCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCatalogParams{ToolIDs: []string{"shell_exec"}},
		},
		Description: contracts.ShellDescriptionPolicy{
			Enabled:  true,
			Strategy: "static_builtin_descriptions",
		},
	}
	out.ShellExecution = contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCommandParams{AllowedCommands: []string{"pwd"}},
		},
		Approval: contracts.ShellApprovalPolicy{Enabled: true, Strategy: "always_allow"},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            root,
				Timeout:        "5s",
				MaxOutputBytes: 4096,
			},
		},
	}
	return out
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

func TestChatInputApproveShortcutCompletesPendingRunState(t *testing.T) {
	ws := make(chan daemon.WebsocketEnvelope)
	close(ws)
	now := time.Date(2026, 4, 17, 12, 6, 0, 0, time.UTC)
	finalSnapshot := daemon.SessionSnapshot{
		SessionID:        "session-1",
		PendingApprovals: nil,
		Timeline: []projections.ChatTimelineItem{
			{
				Kind:       projections.ChatTimelineItemMessage,
				Role:       "assistant",
				Content:    "done after approval",
				OccurredAt: now,
			},
		},
		Prompt: daemon.SessionPromptSnapshot{
			Default:   "default prompt",
			Effective: "default prompt",
		},
	}
	client := &stubOperatorClient{
		sessions: []SessionSummary{{SessionID: "session-1", CreatedAt: now, LastActivity: now, MessageCount: 0}},
		snapshot: daemon.SessionSnapshot{
			SessionID: "session-1",
			PendingApprovals: []shell.PendingApprovalView{{
				ApprovalID: "approval-1",
				Command:    "rm",
				Args:       []string{"-rf", "tmp"},
			}},
			Prompt: daemon.SessionPromptSnapshot{
				Default:   "default prompt",
				Effective: "default prompt",
			},
		},
		approveAlwaysResult: &finalSnapshot,
		ws:                  ws,
	}

	m, err := newModelWithClient(context.Background(), client, "")
	if err != nil {
		t.Fatalf("newModelWithClient returned error: %v", err)
	}
	state := m.currentSessionState()
	state.PendingPrompt = "run rm"
	state.Busy = true
	state.MainRun.Active = true
	state.RunCancel = func() {}
	state.Input.SetValue("a")

	modelAfter, cmd := (&m).Update(tea.KeyMsg{Type: tea.KeyEnter})
	if cmd != nil {
		t.Fatalf("approval shortcut returned unexpected cmd")
	}
	mm := modelAfter.(*model)
	state = mm.currentSessionState()

	if client.approvedAlwaysShellID != "approval-1" {
		t.Fatalf("approvedAlways shell id = %q, want approval-1", client.approvedAlwaysShellID)
	}
	if state.PendingPrompt != "" {
		t.Fatalf("pending prompt = %q, want cleared", state.PendingPrompt)
	}
	if state.Busy {
		t.Fatal("state.Busy = true, want false")
	}
	if state.MainRun.Active {
		t.Fatal("state.MainRun.Active = true, want false")
	}
	if state.RunCancel != nil {
		t.Fatal("state.RunCancel != nil, want nil")
	}
	if got := state.Snapshot.Timeline[len(state.Snapshot.Timeline)-1].Content; got != "done after approval" {
		t.Fatalf("last timeline content = %q, want done after approval", got)
	}
	if strings.Contains(state.ChatView.View(), "USER [pending]:") {
		t.Fatalf("chat view still shows pending prompt after approval: %q", state.ChatView.View())
	}
}

func TestSessionsViewShowsHumanReadableTimestamps(t *testing.T) {
	ws := make(chan daemon.WebsocketEnvelope)
	close(ws)
	now := time.Date(2026, 4, 17, 12, 5, 0, 0, time.UTC)
	client := &stubOperatorClient{
		sessions: []SessionSummary{{
			SessionID:    "session-1",
			Title:        "Main session",
			CreatedAt:    now.Add(-2 * time.Hour),
			LastActivity: now,
			MessageCount: 3,
		}},
		snapshot: daemon.SessionSnapshot{
			SessionID:    "session-1",
			Title:        "Main session",
			CreatedAt:    now.Add(-2 * time.Hour),
			LastActivity: now,
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
	view := m.viewSessions()
	if !strings.Contains(view, "created 2026-04-17 10:05") {
		t.Fatalf("sessions view missing created timestamp: %q", view)
	}
	if !strings.Contains(view, "active 2026-04-17 12:05") {
		t.Fatalf("sessions view missing activity timestamp: %q", view)
	}
}
