package tui

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"teamd/internal/runtime/projections"
)

var approvalMenuActions = []string{
	"Approve once",
	"Allow forever",
	"Deny once",
	"Deny forever",
	"Cancel tool and send message...",
}

func (m *model) updateChat(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	state := m.currentSessionState()
	if state == nil {
		return m, nil
	}
	switch msg.String() {
	case "pgup":
		state.ChatView.LineUp(max(1, state.ChatView.Height/2))
		if state.ChatView.YOffset == 0 && state.Snapshot.History.HasMore {
			return m, loadOlderHistoryCmd(m.ctx, m.client, state)
		}
		return m, nil
	case "pgdown":
		state.ChatView.LineDown(max(1, state.ChatView.Height/2))
		return m, nil
	case "left":
		if m.chatApprovalMenuVisible(state) && !state.ApprovalMenu.ComposeMode && state.ApprovalInFlightID == "" {
			if len(m.currentApprovals()) > 0 && m.approvalCursor > 0 {
				m.approvalCursor--
				m.renderChatViewport(state)
				m.traceApprovalMenuState(state, "left")
			}
			return m, nil
		}
	case "right":
		if m.chatApprovalMenuVisible(state) && !state.ApprovalMenu.ComposeMode && state.ApprovalInFlightID == "" {
			if approvals := m.currentApprovals(); len(approvals) > 0 && m.approvalCursor < len(approvals)-1 {
				m.approvalCursor++
				m.renderChatViewport(state)
				m.traceApprovalMenuState(state, "right")
			}
			return m, nil
		}
	case "up":
		if m.chatApprovalMenuVisible(state) && !state.ApprovalMenu.ComposeMode && state.ApprovalInFlightID == "" {
			if state.ApprovalMenu.ActionIndex > 0 {
				state.ApprovalMenu.ActionIndex--
				m.renderChatViewport(state)
				m.traceApprovalMenuState(state, "up")
			}
			return m, nil
		}
	case "down":
		if m.chatApprovalMenuVisible(state) && !state.ApprovalMenu.ComposeMode && state.ApprovalInFlightID == "" {
			if state.ApprovalMenu.ActionIndex < len(approvalMenuActions)-1 {
				state.ApprovalMenu.ActionIndex++
				m.renderChatViewport(state)
				m.traceApprovalMenuState(state, "down")
			}
			return m, nil
		}
	case "esc":
		if m.chatApprovalMenuVisible(state) && state.ApprovalMenu.ComposeMode {
			state.ApprovalMenu.ComposeMode = false
			state.Input.Reset()
			m.renderChatViewport(state)
			m.traceApprovalMenuState(state, "esc")
			return m, nil
		}
	case "alt+up":
		if len(state.Queue) > 0 && state.QueueCursor > 0 {
			state.QueueCursor--
		}
		return m, nil
	case "alt+down":
		if len(state.Queue) > 0 && state.QueueCursor < len(state.Queue)-1 {
			state.QueueCursor++
		}
		return m, nil
	case "ctrl+e":
		return m, m.recallSelectedDraft(state)
	case "ctrl+d", "delete":
		m.deleteSelectedDraft(state)
		return m, nil
	case "ctrl+x":
		return m, m.cancelMainRun(state)
	case "o":
		return m, m.jumpToWorkspaceFromChat()
	case "tab":
		return m, m.stageOrRecallDraft(state)
	case "enter", "ctrl+s":
		if m.chatApprovalMenuVisible(state) {
			if state.ApprovalMenu.ComposeMode {
				return m, m.submitApprovalCancelMessage(state)
			}
			if state.ApprovalInFlightID != "" {
				return m, nil
			}
			return m, m.submitApprovalMenuSelection(state)
		}
		return m, m.submitChatInput(state)
	}
	if m.chatApprovalMenuVisible(state) && !state.ApprovalMenu.ComposeMode {
		return m, nil
	}
	var cmd tea.Cmd
	state.Input, cmd = state.Input.Update(msg)
	return m, cmd
}

func (m *model) viewChat() string {
	state := m.currentSessionState()
	if state == nil {
		return "No active session"
	}
	m.resizeChatState(state)
	m.renderChatViewport(state)
	header := m.chatHeader(state)
	queue := m.viewQueue(state)
	status := m.viewChatStatusBar(state)
	hint := m.chatComposerHint(state)
	parts := []string{
		header,
		state.ChatView.View(),
	}
	if hint != "" {
		parts = append(parts, hint)
	}
	if !m.chatApprovalMenuVisible(state) || state.ApprovalMenu.ComposeMode {
		parts = append(parts, state.Input.View())
	}
	parts = append(parts, status)
	if queue != "" {
		parts = append(parts, queue)
	}
	return lipgloss.JoinVertical(
		lipgloss.Left,
		parts...,
	)
}

func (m *model) renderChatViewport(state *sessionState) {
	if state == nil {
		return
	}
	wasAtBottom := state.ChatView.AtBottom() || state.ChatView.TotalLineCount() == 0
	contentWidth := max(20, state.ChatView.Width-1)
	lines := []string{}
	for _, item := range state.Snapshot.Timeline {
		switch item.Kind {
		case projections.ChatTimelineItemMessage:
			lines = append(lines, prefixTimestamp(item.OccurredAt, renderChatRoleLabel(item.Role)))
			content := item.Content
			if state.Overrides.RenderMarkdown {
				rendered, err := renderMarkdown(content, state.Overrides.MarkdownStyle, contentWidth)
				if err == nil {
					content = strings.TrimRight(rendered, "\n")
				} else {
					content = wrapText(content, contentWidth)
				}
			} else {
				content = wrapText(content, contentWidth)
			}
			lines = append(lines, content, "")
		case projections.ChatTimelineItemTool:
			continue
		default:
			lines = append(lines, prefixTimestamp(item.OccurredAt, ""), m.renderMarkdownBlock(item.Content, state.Overrides.MarkdownStyle, contentWidth), "")
		}
	}
	if strings.TrimSpace(state.PendingPrompt) != "" {
		lines = append(lines, prefixTimestamp(state.MainRun.StartedAt, ansiChatUser("USER [pending]:")))
		lines = append(lines, wrapText(state.PendingPrompt, contentWidth), "")
	}
	for _, item := range m.renderLiveToolLog(state, contentWidth) {
		lines = append(lines, item)
	}
	if menu := m.renderApprovalMenu(state, contentWidth); len(menu) > 0 {
		lines = append(lines, menu...)
	}
	if len(state.ToolLog) > 0 || m.chatApprovalMenuVisible(state) {
		lines = append(lines, "")
	}
	if state.Streaming.Len() > 0 {
		lines = append(lines, "ASSISTANT:", wrapText(state.Streaming.String(), contentWidth), "")
	}
	for _, item := range m.renderInterjectionHistory(state, contentWidth) {
		lines = append(lines, item, "")
	}
	for _, run := range state.BtwRuns {
		lines = append(lines, m.renderBtwBlock(run, state.Overrides.MarkdownStyle, contentWidth), "")
	}
	if marker := m.renderRunCompletedMarker(state); marker != "" {
		lines = append(lines, marker, "")
	}
	state.ChatView.SetContent(strings.TrimRight(strings.Join(lines, "\n"), "\n"))
	m.traceApprovalMenuState(state, "render")
	if wasAtBottom || state.MainRun.Active || strings.TrimSpace(state.PendingPrompt) != "" {
		state.ChatView.GotoBottom()
	}
}

func (m *model) handleMouseChat(msg tea.MouseMsg) bool {
	state := m.currentSessionState()
	if state == nil {
		return false
	}
	if isWheelUp(msg) || isWheelDown(msg) {
		if isWheelUp(msg) {
			state.ChatView.ScrollUp(state.ChatView.MouseWheelDelta)
			if state.ChatView.YOffset == 0 && state.Snapshot.History.HasMore {
				return false
			}
		} else {
			state.ChatView.ScrollDown(state.ChatView.MouseWheelDelta)
		}
		return true
	}
	return false
}

func loadOlderHistoryCmd(ctx context.Context, client OperatorClient, state *sessionState) tea.Cmd {
	if state == nil || !state.Snapshot.History.HasMore {
		return nil
	}
	return func() tea.Msg {
		chunk, err := client.GetSessionHistory(ctx, state.SessionID, state.Snapshot.History.LoadedCount, state.Snapshot.History.WindowLimit)
		return historyLoadedMsg{SessionID: state.SessionID, Chunk: chunk, Err: err}
	}
}

func (m *model) submitChatInput(state *sessionState) tea.Cmd {
	prompt := strings.TrimSpace(state.Input.Value())
	if prompt == "" {
		return nil
	}
	if handled, cmd := m.handleChatCommand(state, prompt); handled {
		return cmd
	}
	if state.MainRun.Active {
		m.enqueueDraft(state, prompt)
		m.recordInterjection(state, prompt, "queued")
		state.Input.Reset()
		m.statusMessage = "interjection queued for next turn"
		return nil
	}
	return m.startMainRun(state, prompt)
}

func (m *model) performChatApprovalAction(state *sessionState, action string) tea.Cmd {
	approvals := m.currentApprovals()
	if state == nil || len(approvals) == 0 {
		return nil
	}
	approvalIndex := 0
	if m.approvalCursor >= 0 && m.approvalCursor < len(approvals) {
		approvalIndex = m.approvalCursor
	}
	approvalID := approvals[approvalIndex].ApprovalID
	m.traceApprovalMenuAction(state, "tui.approval_menu.submitted", map[string]any{"action": action, "approval_id": approvalID})
	state.ApprovalInFlightID = approvalID
	state.Status = "running"
	return tea.Batch(runShellActionCmd(m.ctx, m.client, state.SessionID, approvalID, action), tickClockCmd())
}

func (m *model) submitApprovalMenuSelection(state *sessionState) tea.Cmd {
	if state == nil || !m.chatApprovalMenuVisible(state) {
		return nil
	}
	switch state.ApprovalMenu.ActionIndex {
	case 0:
		return m.performChatApprovalAction(state, "approve")
	case 1:
		return m.performChatApprovalAction(state, "allow_forever")
	case 2:
		return m.performChatApprovalAction(state, "deny")
	case 3:
		return m.performChatApprovalAction(state, "deny_forever")
	case 4:
		m.traceApprovalMenuAction(state, "tui.approval_menu.compose_started", nil)
		state.ApprovalMenu.ComposeMode = true
		state.Input.Reset()
		state.Input.Focus()
		m.renderChatViewport(state)
		return nil
	default:
		return nil
	}
}

func (m *model) submitApprovalCancelMessage(state *sessionState) tea.Cmd {
	if state == nil || !m.chatApprovalMenuVisible(state) || !state.ApprovalMenu.ComposeMode {
		return nil
	}
	prompt := strings.TrimSpace(state.Input.Value())
	if prompt == "" {
		return nil
	}
	approvals := m.currentApprovals()
	if len(approvals) == 0 {
		return nil
	}
	approval := approvals[min(max(m.approvalCursor, 0), len(approvals)-1)]
	state.ApprovalMenu.ComposeMode = false
	m.traceApprovalMenuAction(state, "tui.approval_menu.cancel_and_send_submitted", map[string]any{"approval_id": approval.ApprovalID})
	return m.startMainRunWithCmd(state, prompt, func(runCtx context.Context) tea.Cmd {
		return runCancelApprovalAndSendCmd(runCtx, m.client, state.SessionID, approval.ApprovalID, prompt)
	})
}

func (m *model) stageOrRecallDraft(state *sessionState) tea.Cmd {
	prompt := strings.TrimSpace(state.Input.Value())
	if prompt != "" {
		m.enqueueDraft(state, prompt)
		state.Input.Reset()
		m.statusMessage = "draft queued"
		return nil
	}
	return m.recallSelectedDraft(state)
}

func (m *model) recallSelectedDraft(state *sessionState) tea.Cmd {
	if len(state.Queue) == 0 {
		return nil
	}
	if state.QueueCursor < 0 {
		state.QueueCursor = 0
	}
	if state.QueueCursor >= len(state.Queue) {
		state.QueueCursor = len(state.Queue) - 1
	}
	item := state.Queue[state.QueueCursor]
	state.Queue = append(state.Queue[:state.QueueCursor], state.Queue[state.QueueCursor+1:]...)
	if state.QueueCursor >= len(state.Queue) && state.QueueCursor > 0 {
		state.QueueCursor--
	}
	m.markInterjectionStatus(state, item.Text, "editing")
	state.Input.SetValue(item.Text)
	state.Input.Focus()
	m.statusMessage = "queued draft recalled for editing"
	return nil
}

func (m *model) handleChatCommand(state *sessionState, prompt string) (bool, tea.Cmd) {
	trimmed := strings.TrimSpace(prompt)
	cmds := m.client.ChatCommandPolicy()
	exitCmd := coalesce(cmds.ExitCommand, "/exit")
	helpCmd := coalesce(cmds.HelpCommand, "/help")
	sessionCmd := coalesce(cmds.SessionCommand, "/session")
	btwCmd := coalesce(cmds.BtwCommand, "/btw")

	switch {
	case trimmed == exitCmd:
		if m.stopWS != nil {
			m.stopWS()
		}
		return true, tea.Quit
	case trimmed == helpCmd:
		state.Input.Reset()
		m.statusMessage = fmt.Sprintf("commands: %s %s %s %s", helpCmd, sessionCmd, btwCmd, exitCmd)
		return true, nil
	case trimmed == sessionCmd:
		state.Input.Reset()
		m.statusMessage = "session: " + state.SessionID
		return true, nil
	case strings.HasPrefix(trimmed, btwCmd+" "):
		promptText := strings.TrimSpace(strings.TrimPrefix(trimmed, btwCmd))
		if promptText == "" {
			m.errMessage = "btw prompt is empty"
			return true, nil
		}
		state.Input.Reset()
		runID := fmt.Sprintf("btw-%d", len(state.BtwRuns)+1)
		state.BtwRuns = append(state.BtwRuns, btwRun{
			ID:        runID,
			Prompt:    promptText,
			StartedAt: m.now(),
			Active:    true,
		})
		m.renderChatViewport(state)
		return true, tea.Batch(runBtwTurnClientCmd(m.client, state.SessionID, promptText, runID), tickClockCmd())
	case trimmed == btwCmd:
		m.errMessage = "usage: " + btwCmd + " <question>"
		return true, nil
	default:
		return false, nil
	}
}

func (m *model) startMainRun(state *sessionState, prompt string) tea.Cmd {
	return m.startMainRunWithCmd(state, prompt, func(runCtx context.Context) tea.Cmd {
		return runChatTurnClientCmd(runCtx, m.client, state.SessionID, prompt, state.Overrides)
	})
}

func (m *model) startMainRunWithCmd(state *sessionState, prompt string, buildCmd func(context.Context) tea.Cmd) tea.Cmd {
	state.PendingPrompt = prompt
	state.Input.Reset()
	state.Streaming.Reset()
	state.LastError = ""
	state.LastTurnEndedAt = time.Time{}
	state.Status = "running"
	state.Busy = true
	if state.RunCancel != nil {
		state.RunCancel()
	}
	runCtx, cancel := context.WithCancel(m.ctx)
	state.RunCancel = cancel
	state.MainRun.Active = true
	state.ApprovalInFlightID = ""
	state.AwaitingRunCompletion = true
	state.MainRun.StartedAt = m.now()
	state.MainRun.CompletedAt = time.Time{}
	state.MainRun.Provider = m.client.ProviderLabel()
	m.renderChatViewport(state)
	return tea.Batch(buildCmd(runCtx), tickClockCmd())
}

func (m *model) dispatchNextQueued(state *sessionState) tea.Cmd {
	if state == nil || state.MainRun.Active || len(state.Queue) == 0 {
		return nil
	}
	next := state.Queue[0]
	state.Queue = state.Queue[1:]
	if state.QueueCursor >= len(state.Queue) && state.QueueCursor > 0 {
		state.QueueCursor--
	}
	m.markNextInterjectionStarted(state, next.Text)
	return m.startMainRun(state, next.Text)
}

func (m *model) enqueueDraft(state *sessionState, prompt string) {
	state.Queue = append(state.Queue, queuedDraft{Text: prompt, QueuedAt: m.now()})
	if len(state.Queue) == 1 {
		state.QueueCursor = 0
	}
}

func (m *model) deleteSelectedDraft(state *sessionState) {
	if state == nil || len(state.Queue) == 0 {
		return
	}
	if state.QueueCursor < 0 {
		state.QueueCursor = 0
	}
	if state.QueueCursor >= len(state.Queue) {
		state.QueueCursor = len(state.Queue) - 1
	}
	removed := state.Queue[state.QueueCursor]
	state.Queue = append(state.Queue[:state.QueueCursor], state.Queue[state.QueueCursor+1:]...)
	if state.QueueCursor >= len(state.Queue) && state.QueueCursor > 0 {
		state.QueueCursor--
	}
	m.markInterjectionStatus(state, removed.Text, "dropped")
	m.statusMessage = "draft deleted"
}

func (m *model) cancelMainRun(state *sessionState) tea.Cmd {
	if state == nil || !state.MainRun.Active {
		return nil
	}
	if state.RunCancel != nil {
		state.RunCancel()
		state.RunCancel = nil
	}
	commands := m.currentRunningCommands()
	state.PendingPrompt = ""
	if len(commands) > 0 {
		cmds := make([]tea.Cmd, 0, len(commands)+2)
		for _, command := range commands {
			cmds = append(cmds, runKillShellCmd(m.ctx, m.client, state.SessionID, command.CommandID))
		}
		cmds = append(cmds, reloadSessionSnapshotAfterDelayCmd(m.ctx, m.client, state.SessionID, 250*time.Millisecond), tickClockCmd())
		m.statusMessage = "stopping run and shell commands"
		m.renderChatViewport(state)
		return tea.Batch(cmds...)
	}
	state.MainRun.Active = false
	state.Busy = false
	state.Status = "cancelled"
	m.statusMessage = "run cancelled"
	return nil
}

func (m *model) hasActiveRuns() bool {
	for _, state := range m.sessions {
		if state.MainRun.Active {
			return true
		}
		for _, run := range state.BtwRuns {
			if run.Active {
				return true
			}
		}
	}
	return false
}

func (m *model) viewChatStatusBar(state *sessionState) string {
	now := m.clockNow
	if now.IsZero() {
		now = m.now()
	}
	runText := "idle"
	switch strings.TrimSpace(state.Snapshot.MainRun.Phase) {
	case "waiting_shell":
		runText = "waiting_shell " + formatElapsed(now.Sub(state.MainRun.StartedAt))
	case "waiting_approval":
		runText = "waiting_approval " + formatElapsed(now.Sub(state.MainRun.StartedAt))
	case "running", "resuming":
		runText = "running " + formatElapsed(now.Sub(state.MainRun.StartedAt))
	case "":
		if state.MainRun.Active {
			runText = "running " + formatElapsed(now.Sub(state.MainRun.StartedAt))
		}
	default:
		if state.MainRun.Active {
			runText = strings.TrimSpace(state.Snapshot.MainRun.Phase) + " " + formatElapsed(now.Sub(state.MainRun.StartedAt))
		}
	}
	if runText == "idle" && state.MainRun.Active {
		runText = "running " + formatElapsed(now.Sub(state.MainRun.StartedAt))
	}
	runValue := ansiRunIdle(runText)
	if state.MainRun.Active {
		runValue = ansiRunRunning(runText)
	}
	provider := coalesce(state.MainRun.Provider, m.client.ProviderLabel())
	model := coalesce(state.MainRun.Model, "model")
	ctxTokens := state.Snapshot.ContextBudget.CurrentContextTokens
	if ctxTokens <= 0 {
		ctxTokens = approximateContextTokens(state)
	}
	nextTokens := state.Snapshot.ContextBudget.EstimatedNextInputTokens
	lastUsage := ""
	if state.Snapshot.ContextBudget.LastTotalTokens > 0 {
		lastUsage = fmt.Sprintf(" | last=%d", state.Snapshot.ContextBudget.LastTotalTokens)
	} else if state.MainRun.TotalTokens > 0 {
		lastUsage = fmt.Sprintf(" | last=%d", state.MainRun.TotalTokens)
	}
	activeBtw := 0
	for _, run := range state.BtwRuns {
		if run.Active {
			activeBtw++
		}
	}
	parts := []string{
		"provider: " + provider,
		"model: " + model,
		"time: " + now.UTC().Format("15:04:05"),
		"run: " + runValue,
		fmt.Sprintf("ctx=%d%s", ctxTokens, lastUsage),
		fmt.Sprintf("next≈%d", nextTokens),
		fmt.Sprintf("queue: %d", len(state.Queue)),
	}
	if activeBtw > 0 {
		parts = append(parts, fmt.Sprintf("btw: %d", activeBtw))
	}
	if state.LastError != "" {
		parts = append(parts, "error: "+state.LastError)
	}
	return strings.Join(parts, " | ")
}

func (m *model) renderLiveToolLog(state *sessionState, width int) []string {
	if state == nil {
		return nil
	}
	items := m.buildLiveToolItems(state, width)
	if len(items) == 0 {
		return nil
	}
	start, end := liveToolItemWindowBounds(items, m.approvalCursor)
	lines := []string{}
	for _, item := range items[start:end] {
		lines = append(lines, item.Line)
	}
	return lines
}

func (m *model) renderApprovalMenu(state *sessionState, width int) []string {
	if !m.chatApprovalMenuVisible(state) {
		return nil
	}
	approvals := m.currentApprovals()
	if len(approvals) == 0 {
		return nil
	}
	approval := approvals[min(max(m.approvalCursor, 0), len(approvals)-1)]
	lines := []string{
		prefixTimestamp(approval.OccurredAt, fmt.Sprintf("APPROVAL %d/%d %s", m.approvalCursor+1, len(approvals), compactApprovalInvocation(approval))),
	}
	if state.ApprovalInFlightID == approval.ApprovalID {
		lines = append(lines, ansiApprovalApplying("Applying approval..."))
		return lines
	}
	for i, label := range approvalMenuActions {
		prefix := "  "
		if i == state.ApprovalMenu.ActionIndex {
			prefix = "> "
		}
		lines = append(lines, prefix+label)
	}
	if state.ApprovalMenu.ComposeMode {
		lines = append(lines, wrapText("Write a normal user message to send after cancelling this tool call.", width))
	}
	return lines
}

func (m *model) viewQueue(state *sessionState) string {
	if len(state.Queue) == 0 {
		return ""
	}
	header := "Queued drafts:"
	if state.MainRun.Active {
		header = "Queued interjections:"
	}
	lines := []string{header, "Alt+Up/Down select | Ctrl+E edit | Ctrl+D drop"}
	start := max(0, min(state.QueueCursor, max(0, len(state.Queue)-4)))
	end := min(len(state.Queue), start+4)
	for i := start; i < end; i++ {
		item := state.Queue[i]
		prefix := "  "
		if i == state.QueueCursor {
			prefix = "> "
		}
		label := summarizeChatText(item.Text)
		if i == 0 {
			label += "  [next]"
		}
		lines = append(lines, prefix+label)
	}
	if len(state.Queue) > end {
		lines = append(lines, fmt.Sprintf("  … %d more", len(state.Queue)-end))
	}
	return strings.Join(lines, "\n")
}

func (m *model) chatHeader(state *sessionState) string {
	if state == nil {
		return "session"
	}
	parts := []string{fmt.Sprintf("session: %s", state.SessionID)}
	if state.MainRun.Active {
		parts = append(parts, "[RUNNING]")
	} else if state.Status == "cancelled" {
		parts = append(parts, "[CANCELLED]")
	}
	if len(state.Queue) > 0 {
		parts = append(parts, fmt.Sprintf("[QUEUED:%d]", len(state.Queue)))
	}
	return strings.Join(parts, " ")
}

func (m *model) renderInterjectionHistory(state *sessionState, width int) []string {
	if state == nil || len(state.Interjections) == 0 {
		return nil
	}
	start := max(0, len(state.Interjections)-3)
	lines := []string{"OPERATOR:"}
	for _, item := range state.Interjections[start:] {
		status := strings.ToUpper(item.Status)
		if status == "" {
			status = "QUEUED"
		}
		lines = append(lines, wrapText(fmt.Sprintf("[%s] %s", status, summarizeChatText(item.Text)), width))
	}
	return lines
}

func (m *model) recordInterjection(state *sessionState, text, status string) {
	if state == nil {
		return
	}
	state.Interjections = append(state.Interjections, interjectionEntry{
		Text:     text,
		QueuedAt: m.now(),
		Status:   status,
	})
	if len(state.Interjections) > 24 {
		state.Interjections = state.Interjections[len(state.Interjections)-24:]
	}
}

func (m *model) markNextInterjectionStarted(state *sessionState, text string) {
	if state == nil {
		return
	}
	for i := range state.Interjections {
		if state.Interjections[i].Text == text && state.Interjections[i].Status == "queued" {
			state.Interjections[i].Status = "sent"
			state.Interjections[i].StartedAt = m.now()
			return
		}
	}
}

func (m *model) markInterjectionStatus(state *sessionState, text, status string) {
	if state == nil {
		return
	}
	for i := len(state.Interjections) - 1; i >= 0; i-- {
		if state.Interjections[i].Text == text {
			state.Interjections[i].Status = status
			return
		}
	}
}

func (m *model) chatComposerHint(state *sessionState) string {
	if state == nil {
		return "Input"
	}
	if m.chatApprovalMenuVisible(state) {
		if state.ApprovalMenu.ComposeMode {
			return "Cancel tool and send message (Enter send, Esc back):"
		}
		return "Approval menu (Left/Right select request, Up/Down choose action, Enter confirm):"
	}
	if state.MainRun.Active {
		return "Input (Enter queue interjection, Tab stage draft, Ctrl+E recall, Ctrl+D delete, Ctrl+X stop run, Shift+Enter newline, Alt+Up/Down queue select):"
	}
	return "Input (Enter send, Tab queue, Ctrl+E recall, Ctrl+D delete, Shift+Enter newline, Alt+Up/Down queue select):"
}

func (m *model) chatApprovalMenuVisible(state *sessionState) bool {
	return state != nil && len(state.Snapshot.PendingApprovals) > 0
}

func (m *model) renderRunCompletedMarker(state *sessionState) string {
	if state == nil || state.MainRun.Active || state.AwaitingRunCompletion || state.LastTurnEndedAt.IsZero() {
		return ""
	}
	return prefixTimestamp(state.LastTurnEndedAt, ansiRunEnded("AGENT END TURN"))
}

func (m *model) renderMarkdownBlock(content, style string, width int) string {
	if rendered, err := renderMarkdown(content, style, width); err == nil {
		return strings.TrimRight(rendered, "\n")
	}
	return wrapText(content, width)
}

func (m *model) renderBtwBlock(run btwRun, style string, width int) string {
	status := "running"
	if !run.Active {
		status = "done"
	}
	body := run.Response
	if run.Error != "" {
		body = "Error: " + run.Error
	} else if run.Active {
		body = "_Waiting for response..._"
	}
	markdown := fmt.Sprintf("#### /btw\n**Q:** %s\n\n**Status:** %s\n\n%s", run.Prompt, status, body)
	if !run.CompletedAt.IsZero() && run.TotalTokens > 0 {
		markdown += fmt.Sprintf("\n\n`%s | %s | %d tok`", coalesce(run.Provider, "provider"), coalesce(run.Model, "model"), run.TotalTokens)
	}
	return m.renderMarkdownBlock(markdown, style, width)
}

type liveToolItem struct {
	OccurredAt  time.Time
	Line        string
	ApprovalID  string
	DisplayKey  string
	CurrentLive bool
}

func (m *model) buildLiveToolItems(state *sessionState, width int) []liveToolItem {
	if state == nil {
		return nil
	}
	approvals := m.currentApprovals()
	commands := m.currentRunningCommands()
	items := make([]liveToolItem, 0, len(approvals)+len(commands)+3)
	currentKeys := map[string]struct{}{}
	selectedApproval := min(max(m.approvalCursor, 0), max(0, len(approvals)-1))
	for idx, approval := range approvals {
		base := compactApprovalInvocation(approval)
		line := composeLiveStatusLine(approval.OccurredAt, base, "APPROVAL", "1;30;48;5;214", width)
		if idx == selectedApproval {
			line = "> " + line
		} else if len(approvals) > 1 {
			line = "  " + line
		}
		items = append(items, liveToolItem{
			OccurredAt:  approval.OccurredAt,
			Line:        line,
			ApprovalID:  approval.ApprovalID,
			DisplayKey:  base,
			CurrentLive: true,
		})
		currentKeys[base] = struct{}{}
	}
	for _, command := range commands {
		base := compactRunningCommandInvocation(command)
		line := composeLiveStatusLine(command.OccurredAt, base, compactShellCommandStatus(command), compactShellCommandStatusSGR(command), width)
		items = append(items, liveToolItem{
			OccurredAt:  command.OccurredAt,
			Line:        line,
			DisplayKey:  base,
			CurrentLive: true,
		})
		currentKeys[base] = struct{}{}
	}
	for _, activity := range collapseLiveToolActivities(state.ToolLog) {
		if compactToolStatusKey(activity) == "run" {
			continue
		}
		base := compactToolInvocation(activity.Name, activity.Arguments)
		if _, dup := currentKeys[base]; dup {
			continue
		}
		items = append(items, liveToolItem{
			OccurredAt: activity.OccurredAt,
			Line:       compactLiveToolActivityLine(activity, approvals, m.approvalCursor, width),
			DisplayKey: base,
		})
	}
	sort.SliceStable(items, func(i, j int) bool {
		if items[i].OccurredAt.Equal(items[j].OccurredAt) {
			if items[i].CurrentLive != items[j].CurrentLive {
				return items[i].CurrentLive
			}
			return items[i].DisplayKey < items[j].DisplayKey
		}
		return items[i].OccurredAt.Before(items[j].OccurredAt)
	})
	return items
}

func liveToolItemWindowBounds(items []liveToolItem, approvalCursor int) (int, int) {
	if len(items) <= 3 {
		return 0, len(items)
	}
	selectedIndex := -1
	currentApproval := 0
	for i, item := range items {
		if item.ApprovalID == "" {
			continue
		}
		if currentApproval == approvalCursor {
			selectedIndex = i
			break
		}
		currentApproval++
	}
	if selectedIndex < 0 {
		return len(items) - 3, len(items)
	}
	start := selectedIndex - 1
	if start < 0 {
		start = 0
	}
	if start > len(items)-3 {
		start = len(items) - 3
	}
	return start, start + 3
}

func compactRunningCommandInvocation(command projections.ShellCommandView) string {
	toolName := strings.TrimSpace(command.ToolName)
	if toolName == "" {
		toolName = "shell_start"
	}
	return compactShellInvocation(toolName, map[string]any{
		"command": command.Command,
		"args":    command.Args,
	})
}

func compactShellCommandStatus(command projections.ShellCommandView) string {
	switch strings.TrimSpace(command.Status) {
	case "killing":
		return "KILLING"
	default:
		return "RUNNING"
	}
}

func compactShellCommandStatusSGR(command projections.ShellCommandView) string {
	switch strings.TrimSpace(command.Status) {
	case "killing":
		return "1;38;5;245"
	default:
		return "1;38;5;81"
	}
}

func composeLiveStatusLine(occurredAt time.Time, base, status, sgr string, width int) string {
	suffix := " | " + ansiToolAccent(status, sgr)
	if width <= 0 {
		return prefixTimestamp(occurredAt, base) + suffix
	}
	visibleSuffix := " | " + status
	baseWidth := width - len(visibleSuffix)
	if baseWidth < 8 {
		baseWidth = 8
	}
	return ellipsizeForWidth(prefixTimestamp(occurredAt, base), baseWidth) + suffix
}

func formatElapsed(d time.Duration) string {
	if d < 0 {
		d = 0
	}
	total := int(d.Seconds())
	return fmt.Sprintf("%02d:%02d", total/60, total%60)
}

func approximateContextTokens(state *sessionState) int {
	if state == nil {
		return 0
	}
	totalChars := 0
	for _, msg := range state.Snapshot.Transcript {
		totalChars += len([]rune(msg.Content))
	}
	totalChars += len([]rune(state.Input.Value()))
	for _, item := range state.Queue {
		totalChars += len([]rune(item.Text))
	}
	return max(1, totalChars/4)
}

func summarizeChatText(input string) string {
	text := strings.TrimSpace(strings.ReplaceAll(input, "\n", " "))
	if len(text) > 80 {
		return text[:77] + "..."
	}
	return text
}
