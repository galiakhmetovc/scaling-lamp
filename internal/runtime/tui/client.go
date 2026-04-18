package tui

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"path"
	"strconv"
	"strings"
	"sync"
	"time"

	"golang.org/x/net/websocket"
	"teamd/internal/contracts"
	"teamd/internal/runtime"
	"teamd/internal/runtime/daemon"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
	"teamd/internal/runtime/workspace"
)

type SessionSummary struct {
	SessionID    string
	Title        string
	CreatedAt    time.Time
	LastActivity time.Time
	MessageCount int
}

type ChatSendResult struct {
	Session daemon.SessionSnapshot
	Queued  bool
	Draft   *daemon.QueuedDraft
	Result  runtimeResultMeta
}

type BtwResult struct {
	Result runtimeResultMeta
}

type PlanMutation struct {
	Session daemon.SessionSnapshot
}

type SessionHistoryChunk struct {
	SessionID   string
	Timeline    []projections.ChatTimelineItem
	LoadedCount int
	TotalCount  int
	HasMore     bool
	WindowLimit int
}

type ShellActionResult struct {
	Session daemon.SessionSnapshot
}

type WorkspacePTYResult struct {
	PTY workspace.PTYSnapshot
}

type OperatorClient interface {
	Bootstrap(context.Context) (daemon.BootstrapPayload, error)
	ListSessions(context.Context) ([]SessionSummary, error)
	CreateSession(context.Context) (daemon.SessionSnapshot, error)
	GetSession(context.Context, string) (daemon.SessionSnapshot, error)
	RenameSession(context.Context, string, string) (daemon.SessionSnapshot, error)
	DeleteSession(context.Context, string) error
	GetSessionHistory(context.Context, string, int, int) (SessionHistoryChunk, error)
	SetSessionPromptOverride(context.Context, string, string) (daemon.SessionSnapshot, error)
	ClearSessionPromptOverride(context.Context, string) (daemon.SessionSnapshot, error)
	SendChat(context.Context, string, string) (ChatSendResult, error)
	CancelApprovalAndSend(context.Context, string, string, string) (ChatSendResult, error)
	SendBtw(context.Context, string, string) (BtwResult, error)
	CreatePlan(context.Context, string, string) (PlanMutation, error)
	AddPlanTask(context.Context, string, string) (PlanMutation, error)
	EditPlanTask(context.Context, string, string, string, []string) (PlanMutation, error)
	SetPlanTaskStatus(context.Context, string, string, string, string) (PlanMutation, error)
	AddPlanTaskNote(context.Context, string, string, string) (PlanMutation, error)
	ApproveShell(context.Context, string) (ShellActionResult, error)
	ApproveShellAlways(context.Context, string) (ShellActionResult, error)
	DenyShell(context.Context, string) (ShellActionResult, error)
	DenyShellAlways(context.Context, string) (ShellActionResult, error)
	KillShell(context.Context, string) (ShellActionResult, error)
	DebugTrace(context.Context, string, string, map[string]any) error
	WorkspacePTYOpen(context.Context, string, int, int) (WorkspacePTYResult, error)
	WorkspacePTYInput(context.Context, string, string) error
	WorkspacePTYSnapshot(context.Context, string) (WorkspacePTYResult, error)
	WorkspacePTYResize(context.Context, string, int, int) (WorkspacePTYResult, error)
	WorkspaceEditorOpen(context.Context, string, string) (workspace.EditorBuffer, error)
	WorkspaceEditorUpdate(context.Context, string, string, string) (workspace.EditorBuffer, error)
	WorkspaceEditorSave(context.Context, string, string) (workspace.EditorBuffer, error)
	WorkspaceFilesSnapshot(context.Context, string) (workspace.FileTreeSnapshot, error)
	WorkspaceFilesExpand(context.Context, string, string) (workspace.FileTreeSnapshot, error)
	WorkspaceArtifactsSnapshot(context.Context, string) (workspace.ArtifactSnapshot, error)
	WorkspaceArtifactsOpen(context.Context, string, string) (workspace.ArtifactSnapshot, error)
	GetSettings(context.Context) (daemon.SettingsSnapshot, error)
	ApplySettingsForm(context.Context, string, map[string]any) (daemon.SettingsSnapshot, error)
	GetSettingsRaw(context.Context, string) (daemon.SettingsRawFileContent, error)
	ApplySettingsRaw(context.Context, string, string, string) (daemon.SettingsSnapshot, error)
	Subscribe(context.Context) (<-chan daemon.WebsocketEnvelope, func(), error)
	DefaultOverrides() sessionOverrides
	ChatCommandPolicy() contracts.ChatCommandParams
	ProviderLabel() string
	ConfigPath() string
	ConfigID() string
}

type localClient struct {
	agent              *runtime.Agent
	workspacePTY       *workspace.WorkspacePTYManager
	workspaceFiles     *workspace.WorkspaceFilesManager
	workspaceEditor    *workspace.WorkspaceEditorManager
	workspaceArtifacts *workspace.WorkspaceArtifactsManager
}

func newLocalClient(agent *runtime.Agent) OperatorClient {
	return &localClient{
		agent:              agent,
		workspacePTY:       workspace.NewWorkspacePTYManager(),
		workspaceFiles:     newLocalWorkspaceFilesManager(agent),
		workspaceEditor:    newLocalWorkspaceEditorManager(agent),
		workspaceArtifacts: newLocalWorkspaceArtifactsManager(agent),
	}
}

func localWorkspaceRoot(agent *runtime.Agent) string {
	root := "."
	if agent != nil {
		root = strings.TrimSpace(agent.Contracts.FilesystemExecution.Scope.Params.RootPath)
		if root == "" {
			root = "."
		}
	}
	return root
}

func newLocalWorkspaceFilesManager(agent *runtime.Agent) *workspace.WorkspaceFilesManager {
	mgr, err := workspace.NewWorkspaceFilesManager(localWorkspaceRoot(agent))
	if err != nil {
		return nil
	}
	return mgr
}

func newLocalWorkspaceEditorManager(agent *runtime.Agent) *workspace.WorkspaceEditorManager {
	mgr, err := workspace.NewWorkspaceEditorManager(localWorkspaceRoot(agent))
	if err != nil {
		return nil
	}
	return mgr
}

func newLocalWorkspaceArtifactsManager(agent *runtime.Agent) *workspace.WorkspaceArtifactsManager {
	if agent == nil {
		return nil
	}
	root, err := agent.ArtifactStorePath()
	if err != nil || strings.TrimSpace(root) == "" {
		return nil
	}
	mgr, err := workspace.NewWorkspaceArtifactsManager(root)
	if err != nil {
		return nil
	}
	return mgr
}

func (c *localClient) Bootstrap(ctx context.Context) (daemon.BootstrapPayload, error) {
	_ = ctx
	settings := daemon.SettingsSnapshot{}
	return daemon.BootstrapPayload{
		AgentID:        c.agent.Config.ID,
		ConfigPath:     c.agent.ConfigPath,
		ToolGovernance: daemon.BuildToolGovernanceSnapshot(c.agent),
		Settings:       settings,
	}, nil
}

func (c *localClient) ListSessions(ctx context.Context) ([]SessionSummary, error) {
	_ = ctx
	entries := c.agent.ListSessions()
	out := make([]SessionSummary, 0, len(entries))
	for _, entry := range entries {
		out = append(out, SessionSummary(entry))
	}
	return out, nil
}

func (c *localClient) CreateSession(ctx context.Context) (daemon.SessionSnapshot, error) {
	session, err := c.agent.CreateChatSession(ctx)
	if err != nil {
		return daemon.SessionSnapshot{}, err
	}
	return c.GetSession(ctx, session.SessionID)
}

func (c *localClient) GetSession(ctx context.Context, sessionID string) (daemon.SessionSnapshot, error) {
	_ = ctx
	return buildLocalSessionSnapshot(c.agent, sessionID)
}

func (c *localClient) RenameSession(ctx context.Context, sessionID, title string) (daemon.SessionSnapshot, error) {
	if err := c.agent.RenameSession(ctx, sessionID, title); err != nil {
		return daemon.SessionSnapshot{}, err
	}
	return c.GetSession(ctx, sessionID)
}

func (c *localClient) DeleteSession(ctx context.Context, sessionID string) error {
	return c.agent.DeleteSession(ctx, sessionID)
}

func (c *localClient) GetSessionHistory(_ context.Context, sessionID string, loadedCount, historyLimit int) (SessionHistoryChunk, error) {
	if loadedCount < 0 {
		return SessionHistoryChunk{}, fmt.Errorf("loaded_count must be >= 0")
	}
	found := false
	for _, entry := range c.agent.ListSessions() {
		if entry.SessionID == sessionID {
			found = true
			break
		}
	}
	if !found {
		return SessionHistoryChunk{}, fmt.Errorf("session %q not found", sessionID)
	}
	timeline := c.agent.CurrentChatTimeline(sessionID)
	totalCount := len(timeline)
	if loadedCount > totalCount {
		loadedCount = totalCount
	}
	if historyLimit <= 0 {
		historyLimit = 40
	}
	remaining := totalCount - loadedCount
	chunkSize := min(historyLimit, remaining)
	start := max(0, totalCount-loadedCount-chunkSize)
	end := max(start, totalCount-loadedCount)
	chunk := append([]projections.ChatTimelineItem{}, timeline[start:end]...)
	return SessionHistoryChunk{
		SessionID:   sessionID,
		Timeline:    chunk,
		LoadedCount: loadedCount + len(chunk),
		TotalCount:  totalCount,
		HasMore:     start > 0,
		WindowLimit: historyLimit,
	}, nil
}

func (c *localClient) SetSessionPromptOverride(ctx context.Context, sessionID, content string) (daemon.SessionSnapshot, error) {
	if err := c.agent.SetSessionPromptOverride(ctx, sessionID, content); err != nil {
		return daemon.SessionSnapshot{}, err
	}
	return c.GetSession(ctx, sessionID)
}

func (c *localClient) ClearSessionPromptOverride(ctx context.Context, sessionID string) (daemon.SessionSnapshot, error) {
	if err := c.agent.ClearSessionPromptOverride(ctx, sessionID); err != nil {
		return daemon.SessionSnapshot{}, err
	}
	return c.GetSession(ctx, sessionID)
}

func (c *localClient) SendChat(ctx context.Context, sessionID, prompt string) (ChatSendResult, error) {
	session, err := c.agent.ResumeChatSession(ctx, sessionID)
	if err != nil {
		return ChatSendResult{}, err
	}
	result, err := c.agent.ChatTurn(ctx, session, runtime.ChatTurnInput{Prompt: prompt})
	if err != nil {
		return ChatSendResult{}, err
	}
	snapshot, err := c.GetSession(ctx, sessionID)
	if err != nil {
		return ChatSendResult{}, err
	}
	return ChatSendResult{
		Session: snapshot,
		Result: runtimeResultMeta{
			Provider:     c.ProviderLabel(),
			Model:        result.Provider.Model,
			InputTokens:  result.Provider.Usage.InputTokens,
			OutputTokens: result.Provider.Usage.OutputTokens,
			TotalTokens:  result.Provider.Usage.TotalTokens,
			Content:      result.Provider.Message.Content,
		},
	}, nil
}

func (c *localClient) CancelApprovalAndSend(ctx context.Context, sessionID, approvalID, prompt string) (ChatSendResult, error) {
	if err := c.agent.CancelShellApproval(ctx, approvalID); err != nil {
		return ChatSendResult{}, err
	}
	return c.SendChat(ctx, sessionID, prompt)
}

func (c *localClient) SendBtw(ctx context.Context, sessionID, prompt string) (BtwResult, error) {
	session, err := c.agent.ResumeChatSession(ctx, sessionID)
	if err != nil {
		return BtwResult{}, err
	}
	result, err := c.agent.BtwTurn(ctx, session, runtime.BtwTurnInput{Prompt: prompt})
	if err != nil {
		return BtwResult{}, err
	}
	return BtwResult{Result: runtimeResultMeta{
		Provider:     c.ProviderLabel(),
		Model:        result.Provider.Model,
		InputTokens:  result.Provider.Usage.InputTokens,
		OutputTokens: result.Provider.Usage.OutputTokens,
		TotalTokens:  result.Provider.Usage.TotalTokens,
		Content:      result.Provider.Message.Content,
	}}, nil
}

func (c *localClient) CreatePlan(ctx context.Context, sessionID, goal string) (PlanMutation, error) {
	if err := c.agent.CreatePlan(ctx, sessionID, goal); err != nil {
		return PlanMutation{}, err
	}
	session, err := c.GetSession(ctx, sessionID)
	return PlanMutation{Session: session}, err
}

func (c *localClient) AddPlanTask(ctx context.Context, sessionID, description string) (PlanMutation, error) {
	if err := c.agent.AddPlanTask(ctx, sessionID, description, "", nil); err != nil {
		return PlanMutation{}, err
	}
	session, err := c.GetSession(ctx, sessionID)
	return PlanMutation{Session: session}, err
}

func (c *localClient) EditPlanTask(ctx context.Context, sessionID, taskID, description string, dependsOn []string) (PlanMutation, error) {
	if err := c.agent.EditPlanTask(ctx, sessionID, taskID, description, dependsOn); err != nil {
		return PlanMutation{}, err
	}
	session, err := c.GetSession(ctx, sessionID)
	return PlanMutation{Session: session}, err
}

func (c *localClient) SetPlanTaskStatus(ctx context.Context, sessionID, taskID, status, blockedReason string) (PlanMutation, error) {
	if err := c.agent.SetPlanTaskStatus(ctx, sessionID, taskID, status, blockedReason); err != nil {
		return PlanMutation{}, err
	}
	session, err := c.GetSession(ctx, sessionID)
	return PlanMutation{Session: session}, err
}

func (c *localClient) AddPlanTaskNote(ctx context.Context, sessionID, taskID, note string) (PlanMutation, error) {
	if err := c.agent.AddPlanTaskNote(ctx, sessionID, taskID, note); err != nil {
		return PlanMutation{}, err
	}
	session, err := c.GetSession(ctx, sessionID)
	return PlanMutation{Session: session}, err
}

func (c *localClient) ApproveShell(ctx context.Context, approvalID string) (ShellActionResult, error) {
	view, ok := c.agent.PendingShellApproval(approvalID)
	if !ok {
		if existing, ok := c.agent.ShellCommandByApprovalID(approvalID); ok {
			session, err := c.GetSession(ctx, existing.SessionID)
			return ShellActionResult{Session: session}, err
		}
		return ShellActionResult{}, fmt.Errorf("shell approval %q not found", approvalID)
	}
	if _, err := c.agent.ApproveShellCommand(ctx, approvalID); err != nil {
		return ShellActionResult{}, err
	}
	session, err := c.GetSession(ctx, view.SessionID)
	return ShellActionResult{Session: session}, err
}

func (c *localClient) ApproveShellAlways(ctx context.Context, approvalID string) (ShellActionResult, error) {
	view, ok := c.agent.PendingShellApproval(approvalID)
	if !ok {
		if existing, ok := c.agent.ShellCommandByApprovalID(approvalID); ok {
			session, err := c.GetSession(ctx, existing.SessionID)
			return ShellActionResult{Session: session}, err
		}
		return ShellActionResult{}, fmt.Errorf("shell approval %q not found", approvalID)
	}
	reloaded, err := daemon.PersistShellApprovalRuleAndReload(c.agent.ConfigPath, "allow", daemonShellApprovalPrefix(view.Command, view.Args))
	if err != nil {
		return ShellActionResult{}, err
	}
	reloaded.UIBus = c.agent.UIBus
	c.agent.CopySuspendedToolLoopTo(approvalID, reloaded)
	c.agent = reloaded
	if _, err := c.agent.ApproveShellCommand(ctx, approvalID); err != nil {
		return ShellActionResult{}, err
	}
	session, err := c.GetSession(ctx, view.SessionID)
	return ShellActionResult{Session: session}, err
}

func (c *localClient) DenyShell(ctx context.Context, approvalID string) (ShellActionResult, error) {
	view, ok := c.agent.PendingShellApproval(approvalID)
	if !ok {
		if existing, ok := c.agent.ShellCommandByApprovalID(approvalID); ok {
			session, err := c.GetSession(ctx, existing.SessionID)
			return ShellActionResult{Session: session}, err
		}
		return ShellActionResult{}, fmt.Errorf("shell approval %q not found", approvalID)
	}
	if err := c.agent.DenyShellCommand(ctx, approvalID); err != nil {
		return ShellActionResult{}, err
	}
	session, err := c.GetSession(ctx, view.SessionID)
	return ShellActionResult{Session: session}, err
}

func (c *localClient) DenyShellAlways(ctx context.Context, approvalID string) (ShellActionResult, error) {
	view, ok := c.agent.PendingShellApproval(approvalID)
	if !ok {
		if existing, ok := c.agent.ShellCommandByApprovalID(approvalID); ok {
			session, err := c.GetSession(ctx, existing.SessionID)
			return ShellActionResult{Session: session}, err
		}
		return ShellActionResult{}, fmt.Errorf("shell approval %q not found", approvalID)
	}
	reloaded, err := daemon.PersistShellApprovalRuleAndReload(c.agent.ConfigPath, "deny", daemonShellApprovalPrefix(view.Command, view.Args))
	if err != nil {
		return ShellActionResult{}, err
	}
	reloaded.UIBus = c.agent.UIBus
	c.agent.CopySuspendedToolLoopTo(approvalID, reloaded)
	c.agent = reloaded
	if err := c.agent.DenyShellCommand(ctx, approvalID); err != nil {
		return ShellActionResult{}, err
	}
	session, err := c.GetSession(ctx, view.SessionID)
	return ShellActionResult{Session: session}, err
}

func (c *localClient) KillShell(ctx context.Context, commandID string) (ShellActionResult, error) {
	view, ok := c.agent.CurrentShellCommand(commandID)
	if !ok {
		return ShellActionResult{}, fmt.Errorf("shell command %q not found", commandID)
	}
	if _, err := c.agent.KillShellCommand(ctx, commandID); err != nil {
		return ShellActionResult{}, err
	}
	session, err := c.GetSession(ctx, view.SessionID)
	return ShellActionResult{Session: session}, err
}

func (c *localClient) DebugTrace(ctx context.Context, sessionID, trace string, fields map[string]any) error {
	if c.agent == nil || strings.TrimSpace(sessionID) == "" || strings.TrimSpace(trace) == "" {
		return nil
	}
	eventID := fmt.Sprintf("evt-trace-%d", time.Now().UTC().UnixNano())
	if c.agent.NewID != nil {
		eventID = c.agent.NewID("evt-trace")
	}
	now := time.Now().UTC()
	if c.agent.Now != nil {
		now = c.agent.Now().UTC()
	}
	return c.agent.RecordEvent(ctx, eventing.Event{
		ID:               eventID,
		Kind:             eventing.EventTraceRecorded,
		OccurredAt:       now,
		AggregateID:      sessionID,
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		Source:           "operator.tui",
		ActorID:          c.agent.Config.ID,
		ActorType:        "operator",
		TraceSummary:     trace,
		Payload: map[string]any{
			"session_id": sessionID,
			"trace":      trace,
			"fields":     cloneTraceFields(fields),
		},
	})
}

func (c *localClient) WorkspacePTYOpen(_ context.Context, sessionID string, cols, rows int) (WorkspacePTYResult, error) {
	pty, err := c.workspacePTY.Open(sessionID, cols, rows)
	return WorkspacePTYResult{PTY: pty}, err
}

func (c *localClient) WorkspacePTYInput(_ context.Context, ptyID, data string) error {
	return c.workspacePTY.Input(ptyID, []byte(data))
}

func (c *localClient) WorkspacePTYSnapshot(_ context.Context, sessionID string) (WorkspacePTYResult, error) {
	pty, ok := c.workspacePTY.Snapshot(sessionID)
	if !ok {
		return WorkspacePTYResult{}, fmt.Errorf("workspace pty for session %q not found", sessionID)
	}
	return WorkspacePTYResult{PTY: pty}, nil
}

func (c *localClient) WorkspacePTYResize(_ context.Context, ptyID string, cols, rows int) (WorkspacePTYResult, error) {
	if err := c.workspacePTY.Resize(ptyID, cols, rows); err != nil {
		return WorkspacePTYResult{}, err
	}
	for _, sessionID := range c.workspacePTY.SessionIDs() {
		pty, ok := c.workspacePTY.Snapshot(sessionID)
		if ok && pty.PTYID == ptyID {
			return WorkspacePTYResult{PTY: pty}, nil
		}
	}
	return WorkspacePTYResult{}, fmt.Errorf("workspace pty %q not found", ptyID)
}

func (c *localClient) WorkspaceEditorOpen(_ context.Context, sessionID, relPath string) (workspace.EditorBuffer, error) {
	if c.workspaceEditor == nil {
		return workspace.EditorBuffer{}, fmt.Errorf("workspace editor manager not available")
	}
	return c.workspaceEditor.Open(sessionID, relPath)
}

func (c *localClient) WorkspaceEditorUpdate(_ context.Context, sessionID, relPath, content string) (workspace.EditorBuffer, error) {
	if c.workspaceEditor == nil {
		return workspace.EditorBuffer{}, fmt.Errorf("workspace editor manager not available")
	}
	return c.workspaceEditor.Update(sessionID, relPath, content)
}

func (c *localClient) WorkspaceEditorSave(_ context.Context, sessionID, relPath string) (workspace.EditorBuffer, error) {
	if c.workspaceEditor == nil {
		return workspace.EditorBuffer{}, fmt.Errorf("workspace editor manager not available")
	}
	return c.workspaceEditor.Save(sessionID, relPath)
}

func (c *localClient) WorkspaceFilesSnapshot(_ context.Context, sessionID string) (workspace.FileTreeSnapshot, error) {
	if c.workspaceFiles == nil {
		return workspace.FileTreeSnapshot{}, fmt.Errorf("workspace files manager not available")
	}
	return c.workspaceFiles.Snapshot(sessionID)
}

func (c *localClient) WorkspaceFilesExpand(_ context.Context, sessionID, relPath string) (workspace.FileTreeSnapshot, error) {
	if c.workspaceFiles == nil {
		return workspace.FileTreeSnapshot{}, fmt.Errorf("workspace files manager not available")
	}
	return c.workspaceFiles.Expand(sessionID, relPath)
}

func (c *localClient) WorkspaceArtifactsSnapshot(_ context.Context, sessionID string) (workspace.ArtifactSnapshot, error) {
	if c.workspaceArtifacts == nil {
		return workspace.ArtifactSnapshot{SessionID: sessionID}, nil
	}
	return c.workspaceArtifacts.Snapshot(sessionID)
}

func (c *localClient) WorkspaceArtifactsOpen(_ context.Context, sessionID, artifactRef string) (workspace.ArtifactSnapshot, error) {
	if c.workspaceArtifacts == nil {
		return workspace.ArtifactSnapshot{SessionID: sessionID}, nil
	}
	return c.workspaceArtifacts.Open(sessionID, artifactRef)
}

func (c *localClient) GetSettings(ctx context.Context) (daemon.SettingsSnapshot, error) {
	_ = ctx
	return c.settingsSnapshot(), nil
}
func (c *localClient) ApplySettingsForm(ctx context.Context, base string, values map[string]any) (daemon.SettingsSnapshot, error) {
	_, _ = ctx, base
	for key, raw := range values {
		switch key {
		case "max_tool_rounds":
			switch typed := raw.(type) {
			case int:
				c.agent.MaxToolRounds = typed
				c.agent.Config.Spec.Runtime.MaxToolRounds = typed
			case float64:
				c.agent.MaxToolRounds = int(typed)
				c.agent.Config.Spec.Runtime.MaxToolRounds = int(typed)
			case string:
				if parsed, err := strconv.Atoi(strings.TrimSpace(typed)); err == nil {
					c.agent.MaxToolRounds = parsed
					c.agent.Config.Spec.Runtime.MaxToolRounds = parsed
				}
			}
		case "render_markdown":
			if typed, ok := raw.(bool); ok {
				c.agent.Contracts.Chat.Output.Params.RenderMarkdown = typed
			}
		case "markdown_style":
			if typed, ok := raw.(string); ok {
				c.agent.Contracts.Chat.Output.Params.MarkdownStyle = typed
			}
		case "show_tool_calls":
			if typed, ok := raw.(bool); ok {
				c.agent.Contracts.Chat.Status.Params.ShowToolCalls = typed
			}
		case "show_tool_results":
			if typed, ok := raw.(bool); ok {
				c.agent.Contracts.Chat.Status.Params.ShowToolResults = typed
			}
		case "show_plan_after_plan_tools":
			if typed, ok := raw.(bool); ok {
				c.agent.Contracts.Chat.Status.Params.ShowPlanAfterPlanTools = typed
			}
		}
	}
	return c.settingsSnapshot(), nil
}
func (c *localClient) GetSettingsRaw(ctx context.Context, path string) (daemon.SettingsRawFileContent, error) {
	_, _ = ctx, path
	return daemon.SettingsRawFileContent{}, fmt.Errorf("local settings client is unsupported")
}
func (c *localClient) ApplySettingsRaw(ctx context.Context, path, base, content string) (daemon.SettingsSnapshot, error) {
	_, _, _, _ = ctx, path, base, content
	return daemon.SettingsSnapshot{}, fmt.Errorf("local settings client is unsupported")
}

func (c *localClient) Subscribe(ctx context.Context) (<-chan daemon.WebsocketEnvelope, func(), error) {
	id, ch := c.agent.UIBus.Subscribe(128)
	out := make(chan daemon.WebsocketEnvelope, 128)
	stop := make(chan struct{})
	go func() {
		defer close(out)
		for {
			select {
			case <-ctx.Done():
				return
			case <-stop:
				return
			case event, ok := <-ch:
				if !ok {
					return
				}
				out <- daemon.WebsocketEnvelope{Type: "ui_event", Event: &event, GeneratedAt: c.agent.Now().UTC()}
			}
		}
	}()
	return out, func() { close(stop); c.agent.UIBus.Unsubscribe(id) }, nil
}

func (c *localClient) DefaultOverrides() sessionOverrides {
	return sessionOverrides{
		MaxToolRounds:          c.agent.MaxToolRounds,
		RenderMarkdown:         c.agent.Contracts.Chat.Output.Params.RenderMarkdown,
		MarkdownStyle:          coalesce(c.agent.Contracts.Chat.Output.Params.MarkdownStyle, "dark"),
		ShowToolCalls:          c.agent.Contracts.Chat.Status.Params.ShowToolCalls,
		ShowToolResults:        c.agent.Contracts.Chat.Status.Params.ShowToolResults,
		ShowPlanAfterPlanTools: c.agent.Contracts.Chat.Status.Params.ShowPlanAfterPlanTools,
	}
}
func (c *localClient) ChatCommandPolicy() contracts.ChatCommandParams {
	return c.agent.Contracts.Chat.Command.Params
}
func (c *localClient) ProviderLabel() string { return providerLabel(c.agent) }
func (c *localClient) ConfigPath() string    { return c.agent.ConfigPath }
func (c *localClient) ConfigID() string      { return c.agent.Config.ID }

func (c *localClient) settingsSnapshot() daemon.SettingsSnapshot {
	params := c.agent.Contracts.OperatorSurface.Settings.Params
	fields := make([]daemon.SettingsFieldState, 0, max(len(params.FormFields), 6))
	addField := func(key, label, kind string, value any) {
		revision := hashSettingsValue(key, value)
		fields = append(fields, daemon.SettingsFieldState{
			Key:      key,
			Label:    label,
			Type:     kind,
			Value:    value,
			Revision: revision,
		})
	}
	if len(params.FormFields) == 0 {
		addField("max_tool_rounds", "Max Tool Rounds", "int", c.agent.Config.Spec.Runtime.MaxToolRounds)
		addField("render_markdown", "Render Markdown", "bool", c.agent.Contracts.Chat.Output.Params.RenderMarkdown)
		addField("markdown_style", "Markdown Style", "string", c.agent.Contracts.Chat.Output.Params.MarkdownStyle)
		addField("show_tool_calls", "Show Tool Calls", "bool", c.agent.Contracts.Chat.Status.Params.ShowToolCalls)
		addField("show_tool_results", "Show Tool Results", "bool", c.agent.Contracts.Chat.Status.Params.ShowToolResults)
		addField("show_plan_after_plan_tools", "Show Plan After Plan Tools", "bool", c.agent.Contracts.Chat.Status.Params.ShowPlanAfterPlanTools)
	} else {
		for _, field := range params.FormFields {
			switch field.Key {
			case "max_tool_rounds":
				addField(field.Key, field.Label, field.Type, c.agent.Config.Spec.Runtime.MaxToolRounds)
			case "render_markdown":
				addField(field.Key, field.Label, field.Type, c.agent.Contracts.Chat.Output.Params.RenderMarkdown)
			case "markdown_style":
				addField(field.Key, field.Label, field.Type, c.agent.Contracts.Chat.Output.Params.MarkdownStyle)
			case "show_tool_calls":
				addField(field.Key, field.Label, field.Type, c.agent.Contracts.Chat.Status.Params.ShowToolCalls)
			case "show_tool_results":
				addField(field.Key, field.Label, field.Type, c.agent.Contracts.Chat.Status.Params.ShowToolResults)
			case "show_plan_after_plan_tools":
				addField(field.Key, field.Label, field.Type, c.agent.Contracts.Chat.Status.Params.ShowPlanAfterPlanTools)
			}
		}
	}
	revisionParts := make([]string, 0, len(fields))
	for _, field := range fields {
		revisionParts = append(revisionParts, field.Revision)
	}
	return daemon.SettingsSnapshot{
		Revision:   strings.Join(revisionParts, ":"),
		FormFields: fields,
		RawFiles:   nil,
	}
}

func hashSettingsValue(key string, value any) string {
	return fmt.Sprintf("%s=%v", key, value)
}

type daemonClient struct {
	baseURL       string
	origin        string
	configPath    string
	configID      string
	providerLabel string
	overrides     sessionOverrides
	commandPolicy contracts.ChatCommandParams
	httpPath      string
	wsPath        string
}

func newDaemonClientFromAgent(agent *runtime.Agent) (OperatorClient, error) {
	host := strings.TrimSpace(agent.Contracts.OperatorSurface.DaemonServer.Params.ListenHost)
	port := agent.Contracts.OperatorSurface.DaemonServer.Params.ListenPort
	if host == "" || port <= 0 {
		return nil, fmt.Errorf("operator surface daemon address is not configured")
	}
	connectHost := host
	if host == "0.0.0.0" || host == "::" {
		connectHost = "127.0.0.1"
	}
	baseURL := "http://" + net.JoinHostPort(connectHost, fmt.Sprintf("%d", port))
	return &daemonClient{
		baseURL:       baseURL,
		origin:        baseURL,
		configPath:    agent.ConfigPath,
		configID:      agent.Config.ID,
		providerLabel: providerLabel(agent),
		overrides: sessionOverrides{
			MaxToolRounds:          agent.MaxToolRounds,
			RenderMarkdown:         agent.Contracts.Chat.Output.Params.RenderMarkdown,
			MarkdownStyle:          coalesce(agent.Contracts.Chat.Output.Params.MarkdownStyle, "dark"),
			ShowToolCalls:          agent.Contracts.Chat.Status.Params.ShowToolCalls,
			ShowToolResults:        agent.Contracts.Chat.Status.Params.ShowToolResults,
			ShowPlanAfterPlanTools: agent.Contracts.Chat.Status.Params.ShowPlanAfterPlanTools,
		},
		commandPolicy: agent.Contracts.Chat.Command.Params,
		httpPath:      agent.Contracts.OperatorSurface.ClientTransport.Params.EndpointPath,
		wsPath:        agent.Contracts.OperatorSurface.ClientTransport.Params.WebSocketPath,
	}, nil
}

func (c *daemonClient) Bootstrap(ctx context.Context) (daemon.BootstrapPayload, error) {
	var payload daemon.BootstrapPayload
	if err := c.commandHTTP(ctx, path.Join(c.httpPath, "bootstrap"), &payload); err != nil {
		return daemon.BootstrapPayload{}, err
	}
	return payload, nil
}

func (c *daemonClient) ListSessions(ctx context.Context) ([]SessionSummary, error) {
	boot, err := c.Bootstrap(ctx)
	if err != nil {
		return nil, err
	}
	out := make([]SessionSummary, 0, len(boot.Sessions))
	for _, s := range boot.Sessions {
		out = append(out, SessionSummary(s))
	}
	return out, nil
}

func (c *daemonClient) CreateSession(ctx context.Context) (daemon.SessionSnapshot, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-create-session", Command: "session.create"}, &result)
	return result.Session, err
}

func (c *daemonClient) GetSession(ctx context.Context, sessionID string) (daemon.SessionSnapshot, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-get-session", Command: "session.get", Payload: map[string]any{"session_id": sessionID}}, &result)
	return result.Session, err
}

func (c *daemonClient) RenameSession(ctx context.Context, sessionID, title string) (daemon.SessionSnapshot, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-session-rename", Command: "session.rename", Payload: map[string]any{"session_id": sessionID, "title": title}}, &result)
	return result.Session, err
}

func (c *daemonClient) DeleteSession(ctx context.Context, sessionID string) error {
	return c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-session-delete", Command: "session.delete", Payload: map[string]any{"session_id": sessionID}}, nil)
}

func (c *daemonClient) GetSessionHistory(ctx context.Context, sessionID string, loadedCount, historyLimit int) (SessionHistoryChunk, error) {
	var result SessionHistoryChunk
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-session-history", Command: "session.history", Payload: map[string]any{"session_id": sessionID, "loaded_count": loadedCount, "history_limit": historyLimit}}, &result)
	return result, err
}

func (c *daemonClient) SetSessionPromptOverride(ctx context.Context, sessionID, content string) (daemon.SessionSnapshot, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-session-prompt-set", Command: "session.prompt.set", Payload: map[string]any{"session_id": sessionID, "content": content}}, &result)
	return result.Session, err
}

func (c *daemonClient) ClearSessionPromptOverride(ctx context.Context, sessionID string) (daemon.SessionSnapshot, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-session-prompt-clear", Command: "session.prompt.clear", Payload: map[string]any{"session_id": sessionID}}, &result)
	return result.Session, err
}

func (c *daemonClient) SendChat(ctx context.Context, sessionID, prompt string) (ChatSendResult, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
		Queued  bool                   `json:"queued"`
		Draft   *daemon.QueuedDraft    `json:"draft"`
		Result  runtimeResultMeta      `json:"result"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-chat-send", Command: "chat.send", Payload: map[string]any{"session_id": sessionID, "prompt": prompt}}, &result)
	return ChatSendResult(result), err
}

func (c *daemonClient) CancelApprovalAndSend(ctx context.Context, sessionID, approvalID, prompt string) (ChatSendResult, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
		Queued  bool                   `json:"queued"`
		Draft   *daemon.QueuedDraft    `json:"draft"`
		Result  runtimeResultMeta      `json:"result"`
	}
	err := c.command(ctx, daemon.CommandRequest{
		Type:    "command",
		ID:      "cmd-chat-cancel-approval-send",
		Command: "chat.cancel_approval_and_send",
		Payload: map[string]any{"session_id": sessionID, "approval_id": approvalID, "prompt": prompt},
	}, &result)
	return ChatSendResult(result), err
}

func (c *daemonClient) SendBtw(ctx context.Context, sessionID, prompt string) (BtwResult, error) {
	var result struct {
		Result runtimeResultMeta `json:"result"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-chat-btw", Command: "chat.btw", Payload: map[string]any{"session_id": sessionID, "prompt": prompt}}, &result)
	return BtwResult(result), err
}

func (c *daemonClient) CreatePlan(ctx context.Context, sessionID, goal string) (PlanMutation, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-plan-create", Command: "plan.create", Payload: map[string]any{"session_id": sessionID, "goal": goal}}, &result)
	return PlanMutation{Session: result.Session}, err
}
func (c *daemonClient) AddPlanTask(ctx context.Context, sessionID, description string) (PlanMutation, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-plan-add", Command: "plan.add_task", Payload: map[string]any{"session_id": sessionID, "description": description}}, &result)
	return PlanMutation{Session: result.Session}, err
}
func (c *daemonClient) EditPlanTask(ctx context.Context, sessionID, taskID, description string, dependsOn []string) (PlanMutation, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-plan-edit", Command: "plan.edit_task", Payload: map[string]any{"session_id": sessionID, "task_id": taskID, "description": description, "depends_on": dependsOn}}, &result)
	return PlanMutation{Session: result.Session}, err
}
func (c *daemonClient) SetPlanTaskStatus(ctx context.Context, sessionID, taskID, status, blockedReason string) (PlanMutation, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-plan-status", Command: "plan.set_task_status", Payload: map[string]any{"session_id": sessionID, "task_id": taskID, "status": status, "blocked_reason": blockedReason}}, &result)
	return PlanMutation{Session: result.Session}, err
}
func (c *daemonClient) AddPlanTaskNote(ctx context.Context, sessionID, taskID, note string) (PlanMutation, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-plan-note", Command: "plan.add_task_note", Payload: map[string]any{"session_id": sessionID, "task_id": taskID, "note": note}}, &result)
	return PlanMutation{Session: result.Session}, err
}
func (c *daemonClient) ApproveShell(ctx context.Context, approvalID string) (ShellActionResult, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-shell-approve", Command: "shell.approve", Payload: map[string]any{"approval_id": approvalID}}, &result)
	return ShellActionResult{Session: result.Session}, err
}
func (c *daemonClient) ApproveShellAlways(ctx context.Context, approvalID string) (ShellActionResult, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-shell-approve-always", Command: "shell.approve_always", Payload: map[string]any{"approval_id": approvalID}}, &result)
	return ShellActionResult{Session: result.Session}, err
}
func (c *daemonClient) DenyShell(ctx context.Context, approvalID string) (ShellActionResult, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-shell-deny", Command: "shell.deny", Payload: map[string]any{"approval_id": approvalID}}, &result)
	return ShellActionResult{Session: result.Session}, err
}
func (c *daemonClient) DenyShellAlways(ctx context.Context, approvalID string) (ShellActionResult, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-shell-deny-always", Command: "shell.deny_always", Payload: map[string]any{"approval_id": approvalID}}, &result)
	return ShellActionResult{Session: result.Session}, err
}
func (c *daemonClient) KillShell(ctx context.Context, commandID string) (ShellActionResult, error) {
	var result struct {
		Session daemon.SessionSnapshot `json:"session"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-shell-kill", Command: "shell.kill", Payload: map[string]any{"command_id": commandID}}, &result)
	return ShellActionResult{Session: result.Session}, err
}
func (c *daemonClient) DebugTrace(ctx context.Context, sessionID, trace string, fields map[string]any) error {
	return c.command(ctx, daemon.CommandRequest{
		Type:    "command",
		ID:      "cmd-debug-trace",
		Command: "debug.trace",
		Payload: map[string]any{
			"session_id": sessionID,
			"trace":      trace,
			"fields":     cloneTraceFields(fields),
		},
	}, nil)
}
func (c *daemonClient) WorkspacePTYOpen(ctx context.Context, sessionID string, cols, rows int) (WorkspacePTYResult, error) {
	var result struct {
		PTY workspace.PTYSnapshot `json:"pty"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-workspace-pty-open", Command: "workspace.pty.open", Payload: map[string]any{"session_id": sessionID, "cols": cols, "rows": rows}}, &result)
	return WorkspacePTYResult(result), err
}
func (c *daemonClient) WorkspacePTYInput(ctx context.Context, ptyID, data string) error {
	return c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-workspace-pty-input", Command: "workspace.pty.input", Payload: map[string]any{"pty_id": ptyID, "data": data}}, nil)
}
func (c *daemonClient) WorkspacePTYSnapshot(ctx context.Context, sessionID string) (WorkspacePTYResult, error) {
	var result struct {
		PTY workspace.PTYSnapshot `json:"pty"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-workspace-pty-snapshot", Command: "workspace.pty.snapshot", Payload: map[string]any{"session_id": sessionID}}, &result)
	return WorkspacePTYResult(result), err
}
func (c *daemonClient) WorkspacePTYResize(ctx context.Context, ptyID string, cols, rows int) (WorkspacePTYResult, error) {
	var result struct {
		PTY workspace.PTYSnapshot `json:"pty"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-workspace-pty-resize", Command: "workspace.pty.resize", Payload: map[string]any{"pty_id": ptyID, "cols": cols, "rows": rows}}, &result)
	return WorkspacePTYResult(result), err
}
func (c *daemonClient) WorkspaceEditorOpen(ctx context.Context, sessionID, relPath string) (workspace.EditorBuffer, error) {
	var result struct {
		Buffer workspace.EditorBuffer `json:"buffer"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-workspace-editor-open", Command: "workspace.editor.open", Payload: map[string]any{"session_id": sessionID, "rel_path": relPath}}, &result)
	return result.Buffer, err
}
func (c *daemonClient) WorkspaceEditorUpdate(ctx context.Context, sessionID, relPath, content string) (workspace.EditorBuffer, error) {
	var result struct {
		Buffer workspace.EditorBuffer `json:"buffer"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-workspace-editor-update", Command: "workspace.editor.update", Payload: map[string]any{"session_id": sessionID, "rel_path": relPath, "content": content}}, &result)
	return result.Buffer, err
}
func (c *daemonClient) WorkspaceEditorSave(ctx context.Context, sessionID, relPath string) (workspace.EditorBuffer, error) {
	var result struct {
		Buffer workspace.EditorBuffer `json:"buffer"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-workspace-editor-save", Command: "workspace.editor.save", Payload: map[string]any{"session_id": sessionID, "rel_path": relPath}}, &result)
	return result.Buffer, err
}
func (c *daemonClient) WorkspaceFilesSnapshot(ctx context.Context, sessionID string) (workspace.FileTreeSnapshot, error) {
	var result struct {
		Files workspace.FileTreeSnapshot `json:"files"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-workspace-files-snapshot", Command: "workspace.files.snapshot", Payload: map[string]any{"session_id": sessionID}}, &result)
	return result.Files, err
}
func (c *daemonClient) WorkspaceFilesExpand(ctx context.Context, sessionID, relPath string) (workspace.FileTreeSnapshot, error) {
	var result struct {
		Files workspace.FileTreeSnapshot `json:"files"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-workspace-files-expand", Command: "workspace.files.expand", Payload: map[string]any{"session_id": sessionID, "rel_path": relPath}}, &result)
	return result.Files, err
}
func (c *daemonClient) WorkspaceArtifactsSnapshot(ctx context.Context, sessionID string) (workspace.ArtifactSnapshot, error) {
	var result struct {
		Artifacts workspace.ArtifactSnapshot `json:"artifacts"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-workspace-artifacts-snapshot", Command: "workspace.artifacts.snapshot", Payload: map[string]any{"session_id": sessionID}}, &result)
	return result.Artifacts, err
}
func (c *daemonClient) WorkspaceArtifactsOpen(ctx context.Context, sessionID, artifactRef string) (workspace.ArtifactSnapshot, error) {
	var result struct {
		Artifacts workspace.ArtifactSnapshot `json:"artifacts"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-workspace-artifacts-open", Command: "workspace.artifacts.open", Payload: map[string]any{"session_id": sessionID, "artifact_ref": artifactRef}}, &result)
	return result.Artifacts, err
}
func (c *daemonClient) GetSettings(ctx context.Context) (daemon.SettingsSnapshot, error) {
	var result struct {
		Settings daemon.SettingsSnapshot `json:"settings"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-settings-get", Command: "settings.get"}, &result)
	return result.Settings, err
}
func (c *daemonClient) ApplySettingsForm(ctx context.Context, base string, values map[string]any) (daemon.SettingsSnapshot, error) {
	var result struct {
		Settings daemon.SettingsSnapshot `json:"settings"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-settings-form-apply", Command: "settings.form.apply", Payload: map[string]any{"base_revision": base, "values": values}}, &result)
	return result.Settings, err
}
func (c *daemonClient) GetSettingsRaw(ctx context.Context, file string) (daemon.SettingsRawFileContent, error) {
	var result struct {
		File daemon.SettingsRawFileContent `json:"file"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-settings-raw-get", Command: "settings.raw.get", Payload: map[string]any{"path": file}}, &result)
	return result.File, err
}
func (c *daemonClient) ApplySettingsRaw(ctx context.Context, file, base, content string) (daemon.SettingsSnapshot, error) {
	var result struct {
		Settings daemon.SettingsSnapshot `json:"settings"`
	}
	err := c.command(ctx, daemon.CommandRequest{Type: "command", ID: "cmd-settings-raw-apply", Command: "settings.raw.apply", Payload: map[string]any{"path": file, "base_revision": base, "content": content}}, &result)
	return result.Settings, err
}

func (c *daemonClient) Subscribe(ctx context.Context) (<-chan daemon.WebsocketEnvelope, func(), error) {
	wsURL := c.websocketURL()
	conn, err := websocket.Dial(wsURL, "", c.origin)
	if err != nil {
		return nil, nil, err
	}
	var mu sync.Mutex
	out := make(chan daemon.WebsocketEnvelope, 128)
	stop := make(chan struct{})
	go func() {
		defer close(out)
		defer conn.Close()
		decoder := json.NewDecoder(conn)
		for {
			var envelope daemon.WebsocketEnvelope
			if err := decoder.Decode(&envelope); err != nil {
				return
			}
			if envelope.Type == "hello" {
				continue
			}
			select {
			case <-ctx.Done():
				return
			case <-stop:
				return
			case out <- envelope:
			}
		}
	}()
	return out, func() {
		close(stop)
		mu.Lock()
		_ = conn.Close()
		mu.Unlock()
	}, nil
}

func (c *daemonClient) DefaultOverrides() sessionOverrides             { return c.overrides }
func (c *daemonClient) ChatCommandPolicy() contracts.ChatCommandParams { return c.commandPolicy }
func (c *daemonClient) ProviderLabel() string                          { return c.providerLabel }
func (c *daemonClient) ConfigPath() string                             { return c.configPath }
func (c *daemonClient) ConfigID() string                               { return c.configID }

func (c *daemonClient) websocketURL() string {
	parsed, _ := url.Parse(c.baseURL)
	switch parsed.Scheme {
	case "https":
		parsed.Scheme = "wss"
	default:
		parsed.Scheme = "ws"
	}
	parsed.Path = c.wsPath
	return parsed.String()
}

func (c *daemonClient) commandHTTP(ctx context.Context, route string, out any) error {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, strings.TrimRight(c.baseURL, "/")+route, nil)
	if err != nil {
		return err
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 300 {
		return fmt.Errorf("daemon http %s returned %s", route, resp.Status)
	}
	return json.NewDecoder(resp.Body).Decode(out)
}

func (c *daemonClient) command(ctx context.Context, req daemon.CommandRequest, out any) error {
	wsURL := c.websocketURL()
	conn, err := websocket.Dial(wsURL, "", c.origin)
	if err != nil {
		return err
	}
	defer conn.Close()
	done := make(chan struct{})
	go func() {
		select {
		case <-ctx.Done():
			_ = conn.Close()
		case <-done:
		}
	}()
	defer close(done)
	decoder := json.NewDecoder(conn)
	encoder := json.NewEncoder(conn)
	for {
		var envelope daemon.WebsocketEnvelope
		if err := decoder.Decode(&envelope); err != nil {
			return err
		}
		if envelope.Type == "hello" {
			break
		}
	}
	if err := encoder.Encode(req); err != nil {
		return err
	}
	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}
		var envelope daemon.WebsocketEnvelope
		if err := decoder.Decode(&envelope); err != nil {
			return err
		}
		if envelope.ID != req.ID {
			continue
		}
		switch envelope.Type {
		case "command_completed":
			body, err := json.Marshal(envelope.Payload)
			if err != nil {
				return err
			}
			if out == nil {
				return nil
			}
			return json.Unmarshal(body, out)
		case "command_failed":
			return fmt.Errorf("%s", envelope.Error)
		}
	}
}

func cloneTraceFields(fields map[string]any) map[string]any {
	if len(fields) == 0 {
		return map[string]any{}
	}
	out := make(map[string]any, len(fields))
	for k, v := range fields {
		out[k] = v
	}
	return out
}

func buildLocalSessionSnapshot(agent *runtime.Agent, sessionID string) (daemon.SessionSnapshot, error) {
	var entry projections.SessionCatalogEntry
	found := false
	for _, candidate := range agent.ListSessions() {
		if candidate.SessionID == sessionID {
			entry = candidate
			found = true
			break
		}
	}
	if !found {
		return daemon.SessionSnapshot{}, fmt.Errorf("session %q not found", sessionID)
	}
	plan, _ := agent.CurrentPlanHead(sessionID)
	fullTranscript := agent.CurrentTranscript(sessionID)
	compactedTranscript := agent.CompactedMessagesForSession(sessionID, fullTranscript)
	fullTimeline := agent.CurrentChatTimeline(sessionID)
	windowLimit := 40
	transcriptWindow := tailMessagesLocal(fullTranscript, windowLimit)
	timelineWindow := tailTimelineLocal(fullTimeline, windowLimit)
	return daemon.SessionSnapshot{
		SessionID:    entry.SessionID,
		Title:        entry.Title,
		CreatedAt:    entry.CreatedAt,
		LastActivity: entry.LastActivity,
		MessageCount: entry.MessageCount,
		ContextBudget: daemon.ContextBudgetSnapshot{
			LastInputTokens:          agent.CurrentContextBudget(sessionID).LastInputTokens,
			LastOutputTokens:         agent.CurrentContextBudget(sessionID).LastOutputTokens,
			LastTotalTokens:          agent.CurrentContextBudget(sessionID).LastTotalTokens,
			CurrentContextTokens:     approximateTextTokensFromMessages(compactedTranscript),
			EstimatedNextInputTokens: approximateTextTokensFromMessages(compactedTranscript),
			SummaryTokens:            agent.CurrentContextBudget(sessionID).SummaryTokens,
			SummarizationCount:       agent.CurrentContextBudget(sessionID).SummarizationCount,
			CompactedMessageCount:    agent.CurrentContextBudget(sessionID).CompactedMessageCount,
			Source:                   coalesce(agent.CurrentContextBudget(sessionID).Source, "mixed"),
			BudgetState:              "healthy",
		},
		Prompt: daemon.SessionPromptSnapshot{
			Default:     defaultPromptForLocalClient(agent),
			Override:    agent.CurrentSessionPromptOverride(sessionID),
			Effective:   effectivePromptForLocalClient(agent, sessionID),
			HasOverride: strings.TrimSpace(agent.CurrentSessionPromptOverride(sessionID)) != "",
		},
		History: daemon.ChatHistorySnapshot{
			LoadedCount: len(timelineWindow),
			TotalCount:  len(fullTimeline),
			HasMore:     len(timelineWindow) < len(fullTimeline),
			WindowLimit: windowLimit,
		},
		Transcript:       transcriptWindow,
		Timeline:         timelineWindow,
		Plan:             plan,
		ToolGovernance:   daemon.BuildToolGovernanceSnapshot(agent),
		PendingApprovals: agent.PendingShellApprovals(sessionID),
		RunningCommands:  agent.CurrentRunningShellCommands(sessionID),
		Delegates:        agent.CurrentDelegates(sessionID),
	}, nil
}

func daemonShellApprovalPrefix(command string, args []string) string {
	command = strings.TrimSpace(command)
	if command == "" {
		return strings.TrimSpace(strings.Join(args, " "))
	}
	base := path.Base(command)
	if base == "." || base == "/" {
		return command
	}
	return strings.TrimSpace(base)
}

func defaultPromptForLocalClient(agent *runtime.Agent) string {
	content, _ := agent.DefaultSystemPrompt()
	return content
}

func effectivePromptForLocalClient(agent *runtime.Agent, sessionID string) string {
	content, _ := agent.EffectiveSystemPrompt(sessionID)
	return content
}

func approximateTextTokensFromMessages(messages []contracts.Message) int {
	chars := 0
	for _, message := range messages {
		chars += len(message.Content)
	}
	if chars <= 0 {
		return 0
	}
	return (chars + 3) / 4
}

func tailMessagesLocal(messages []contracts.Message, limit int) []contracts.Message {
	if limit <= 0 || len(messages) <= limit {
		return append([]contracts.Message{}, messages...)
	}
	return append([]contracts.Message{}, messages[len(messages)-limit:]...)
}

func tailTimelineLocal(items []projections.ChatTimelineItem, limit int) []projections.ChatTimelineItem {
	if limit <= 0 || len(items) <= limit {
		return append([]projections.ChatTimelineItem{}, items...)
	}
	return append([]projections.ChatTimelineItem{}, items[len(items)-limit:]...)
}
