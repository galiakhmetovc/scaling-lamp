package tui

import (
	"context"
	"io"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/textarea"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"teamd/internal/runtime"
	"teamd/internal/runtime/daemon"
	"teamd/internal/runtime/projections"
)

func Run(ctx context.Context, agent *runtime.Agent, resumeID string, stdin io.Reader, stdout io.Writer) error {
	client, err := newDaemonClientFromAgent(agent)
	if err != nil {
		return err
	}
	m, err := newModelWithClient(ctx, client, resumeID)
	if err != nil {
		return err
	}
	program := tea.NewProgram(&m, tea.WithAltScreen(), tea.WithMouseCellMotion(), tea.WithInput(stdin), tea.WithOutput(stdout))
	_, err = program.Run()
	return err
}

func newModel(ctx context.Context, agent *runtime.Agent, resumeID string) (model, error) {
	return newModelWithClient(ctx, newLocalClient(agent), resumeID)
}

func newModelWithClient(ctx context.Context, client OperatorClient, resumeID string) (model, error) {
	m := model{
		ctx:                 ctx,
		client:              client,
		now:                 time.Now,
		tab:                 tabChat,
		sessions:            map[string]*sessionState{},
		mouseCaptureEnabled: true,
	}
	m.clockNow = m.now()
	m.rawEditor = textarea.New()
	m.rawEditor.SetHeight(20)
	m.rawEditor.ShowLineNumbers = true
	m.rawEditor.Prompt = ""
	m.formMaxRounds = textinput.New()
	m.formStyle = textinput.New()
	m.sessionTitleInput = textinput.New()
	m.planGoalInput = textinput.New()
	m.planDescInput = textinput.New()
	m.planDepsInput = textinput.New()
	m.planNoteInput = textinput.New()
	m.planView = viewport.New(80, 20)
	m.planView.MouseWheelEnabled = true
	m.planView.MouseWheelDelta = 3
	m.headView = viewport.New(80, 20)
	m.headView.MouseWheelEnabled = true
	m.headView.MouseWheelDelta = 3
	m.settingsView = viewport.New(80, 20)
	m.settingsView.MouseWheelEnabled = true
	m.settingsView.MouseWheelDelta = 3
	m.promptEditor = textarea.New()
	m.promptEditor.Prompt = ""
	m.promptEditor.SetHeight(20)
	m.headExpanded = map[string]bool{}
	wsCh, stopWS, err := client.Subscribe(ctx)
	if err != nil {
		return model{}, err
	}
	m.wsCh, m.stopWS = wsCh, stopWS
	if err := m.loadSessions(resumeID); err != nil {
		return model{}, err
	}
	settings, err := client.GetSettings(ctx)
	if err == nil {
		m.settingsSnapshot = settings
	}
	if err := m.loadRawFileList(); err != nil && !strings.Contains(err.Error(), "unsupported") {
		return model{}, err
	}
	m.resetFormDraft()
	return m, nil
}

func (m *model) Init() tea.Cmd {
	cmds := []tea.Cmd{tickClockCmd()}
	if m.wsCh != nil {
		cmds = append(cmds, waitForDaemonEnvelope(m.wsCh))
	}
	return tea.Batch(cmds...)
}

func (m *model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		for _, state := range m.sessions {
			m.resizeChatState(state)
			state.ToolsView.Width = max(20, m.width-6)
			state.ToolsView.Height = max(10, m.height-8)
		}
		m.planView.Width = max(20, m.width-6)
		m.planView.Height = max(10, m.height-8)
		m.headView.Width = max(20, m.width-6)
		m.headView.Height = max(10, m.height-8)
		m.settingsView.Width = max(20, m.width-6)
		m.settingsView.Height = max(10, m.height-8)
		m.planGoalInput.Width = max(20, m.width/3)
		m.planDescInput.Width = max(20, m.width/3)
		m.planDepsInput.Width = max(20, m.width/3)
		m.planNoteInput.Width = max(20, m.width/3)
		m.rawEditor.SetWidth(max(20, m.width/2))
		m.rawEditor.SetHeight(max(10, m.height-12))
		m.promptEditor.SetWidth(max(20, m.width-6))
		m.promptEditor.SetHeight(max(10, m.height-12))
		m.sessionTitleInput.Width = max(20, m.width/3)
		return m, nil
	case tea.KeyMsg:
		if msg.String() == "ctrl+c" || msg.String() == "ctrl+q" {
			if m.stopWS != nil {
				m.stopWS()
			}
			return m, tea.Quit
		}
		if cmd := m.handleGlobalKey(msg); cmd != nil {
			return m, cmd
		}
		switch m.tab {
		case tabSessions:
			return m.updateSessions(msg)
		case tabChat:
			return m.updateChat(msg)
		case tabHead:
			return m.updateHead(msg)
		case tabPrompt:
			return m.updatePrompt(msg)
		case tabWorkspace:
			return m.updateWorkspace(msg)
		case tabPlan:
			return m.updatePlan(msg)
		case tabTools:
			return m.updateTools(msg)
		case tabSettings:
			return m.updateSettings(msg)
		}
	case tea.MouseMsg:
		if m.handleMouseTabs(msg) {
			return m, nil
		}
		if m.tab == tabSessions && m.handleMouseSessions(msg) {
			return m, nil
		}
		if m.tab == tabTools && m.handleMouseTools(msg) {
			return m, nil
		}
		if m.tab == tabPlan && m.handleMousePlan(msg) {
			return m, nil
		}
		if m.tab == tabChat {
			if state := m.currentSessionState(); state != nil && isWheelUp(msg) {
				state.ChatView.ScrollUp(state.ChatView.MouseWheelDelta)
				if state.ChatView.YOffset == 0 && state.Snapshot.History.HasMore {
					return m, loadOlderHistoryCmd(m.ctx, m.client, state)
				}
				return m, nil
			}
		}
		if m.tab == tabChat && m.handleMouseChat(msg) {
			return m, nil
		}
		if m.tab == tabSettings && m.handleMouseSettings(msg) {
			return m, nil
		}
	case daemonEnvelopeMsg:
		envelope := daemon.WebsocketEnvelope(msg)
		m.handleDaemonEnvelope(envelope)
		return m, waitForDaemonEnvelope(m.wsCh)
	case clockTickMsg:
		m.clockNow = time.Time(msg)
		if m.hasActiveRuns() {
			return m, tickClockCmd()
		}
		return m, nil
	case chatTurnFinishedMsg:
		state := m.sessions[msg.SessionID]
		if state == nil {
			return m, nil
		}
		state.PendingPrompt = ""
		state.Busy = false
		state.MainRun.Active = false
		state.RunCancel = nil
		state.MainRun.CompletedAt = m.now()
		if msg.Result.Provider != "" {
			state.MainRun.Provider = msg.Result.Provider
		}
		if msg.Result.Model != "" {
			state.MainRun.Model = msg.Result.Model
		}
		state.MainRun.InputTokens = msg.Result.InputTokens
		state.MainRun.OutputTokens = msg.Result.OutputTokens
		state.MainRun.TotalTokens = msg.Result.TotalTokens
		if msg.Err != nil {
			state.LastError = msg.Err.Error()
			m.errMessage = msg.Err.Error()
			if cmd := m.dispatchNextQueued(state); cmd != nil {
				return m, tea.Batch(cmd, tickClockCmd())
			}
			return m, nil
		}
		if msg.Queued && msg.Draft != nil {
			state.Queue = append(state.Queue, queuedDraft{Text: msg.Draft.Text, QueuedAt: msg.Draft.QueuedAt})
		}
		state.Snapshot = msg.Session
		state.SessionID = msg.SessionID
		state.Status = "idle"
		m.renderChatViewport(state)
		m.renderToolsViewport(state)
		if cmd := m.dispatchNextQueued(state); cmd != nil {
			return m, tea.Batch(cmd, tickClockCmd())
		}
		return m, nil
	case btwTurnFinishedMsg:
		state := m.sessions[msg.SessionID]
		if state == nil {
			return m, nil
		}
		for i := range state.BtwRuns {
			if state.BtwRuns[i].ID != msg.RunID {
				continue
			}
			state.BtwRuns[i].Active = false
			state.BtwRuns[i].CompletedAt = m.now()
			state.BtwRuns[i].Provider = msg.Result.Provider
			state.BtwRuns[i].Model = msg.Result.Model
			state.BtwRuns[i].InputTokens = msg.Result.InputTokens
			state.BtwRuns[i].OutputTokens = msg.Result.OutputTokens
			state.BtwRuns[i].TotalTokens = msg.Result.TotalTokens
			if msg.Err != nil {
				state.BtwRuns[i].Error = msg.Err.Error()
			} else {
				state.BtwRuns[i].Response = msg.Result.Content
			}
			break
		}
		m.renderChatViewport(state)
		if m.hasActiveRuns() {
			return m, tickClockCmd()
		}
		return m, nil
	case historyLoadedMsg:
		state := m.sessions[msg.SessionID]
		if state == nil {
			return m, nil
		}
		if msg.Err != nil {
			m.errMessage = msg.Err.Error()
			return m, nil
		}
		state.Snapshot.Timeline = append(append([]projections.ChatTimelineItem{}, msg.Chunk.Timeline...), state.Snapshot.Timeline...)
		state.Snapshot.History.LoadedCount = msg.Chunk.LoadedCount
		state.Snapshot.History.TotalCount = msg.Chunk.TotalCount
		state.Snapshot.History.HasMore = msg.Chunk.HasMore
		state.Snapshot.History.WindowLimit = msg.Chunk.WindowLimit
		m.renderChatViewport(state)
		return m, nil
	case sessionRenamedMsg:
		if msg.Err != nil {
			m.errMessage = msg.Err.Error()
			return m, nil
		}
		state := m.sessions[msg.Session.SessionID]
		if state != nil {
			state.Snapshot = mergeSessionSnapshot(state.Snapshot, msg.Session)
		}
		m.sessionMode = sessionsModeBrowse
		m.statusMessage = "session renamed"
		return m, nil
	case sessionDeletedMsg:
		if msg.Err != nil {
			m.errMessage = msg.Err.Error()
			return m, nil
		}
		delete(m.sessions, msg.SessionID)
		nextOrder := m.sessionOrder[:0]
		for _, sessionID := range m.sessionOrder {
			if sessionID != msg.SessionID {
				nextOrder = append(nextOrder, sessionID)
			}
		}
		m.sessionOrder = nextOrder
		if m.sessionCursor >= len(m.sessionOrder) && m.sessionCursor > 0 {
			m.sessionCursor--
		}
		if m.activeSessionID == msg.SessionID {
			m.activeSessionID = ""
			if len(m.sessionOrder) > 0 {
				m.activeSessionID = m.sessionOrder[min(m.sessionCursor, len(m.sessionOrder)-1)]
			}
		}
		m.sessionMode = sessionsModeBrowse
		m.statusMessage = "session deleted"
		return m, nil
	case promptSavedMsg:
		if msg.Err != nil {
			m.errMessage = msg.Err.Error()
			return m, nil
		}
		if state := m.sessions[msg.Session.SessionID]; state != nil {
			state.Snapshot = mergeSessionSnapshot(state.Snapshot, msg.Session)
		}
		m.promptLoadedSession = ""
		m.promptDirty = false
		m.statusMessage = "prompt override saved"
		return m, nil
	case promptResetMsg:
		if msg.Err != nil {
			m.errMessage = msg.Err.Error()
			return m, nil
		}
		if state := m.sessions[msg.Session.SessionID]; state != nil {
			state.Snapshot = mergeSessionSnapshot(state.Snapshot, msg.Session)
		}
		m.promptLoadedSession = ""
		m.promptDirty = false
		m.statusMessage = "prompt override reset"
		return m, nil
	}
	return m, nil
}

func (m *model) handleGlobalKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "ctrl+left":
		if m.tab == 0 {
			m.tab = tabIndex(len(topTabs) - 1)
		} else {
			m.tab--
		}
		return nil
	case "ctrl+right":
		m.tab = tabIndex((int(m.tab) + 1) % len(topTabs))
		return nil
	case "f1":
		m.tab = tabSessions
	case "f2":
		m.tab = tabChat
	case "f3":
		m.tab = tabHead
	case "f4":
		m.tab = tabPrompt
	case "f5":
		m.tab = tabWorkspace
	case "f6":
		m.tab = tabPlan
	case "f7":
		m.tab = tabTools
	case "f8":
		m.tab = tabSettings
	case "f9":
		m.mouseCaptureEnabled = !m.mouseCaptureEnabled
		if m.mouseCaptureEnabled {
			m.statusMessage = "interactive mouse mode enabled"
			return enableMouseCaptureCmd()
		}
		m.statusMessage = "selection mode enabled"
		return disableMouseCaptureCmd()
	}
	return nil
}

func (m *model) View() string {
	if m.width == 0 || m.height == 0 {
		return "loading..."
	}
	top := m.renderTopTabs()
	body := ""
	switch m.tab {
	case tabSessions:
		body = m.viewSessions()
	case tabChat:
		body = m.viewChat()
	case tabHead:
		body = m.viewHead()
	case tabPrompt:
		body = m.viewPrompt()
	case tabWorkspace:
		body = m.viewWorkspace()
	case tabPlan:
		body = m.viewPlan()
	case tabTools:
		body = m.viewTools()
	case tabSettings:
		body = m.viewSettings()
	}
	footer := m.viewFooter()
	return lipgloss.JoinVertical(lipgloss.Left, top, body, footer)
}

func (m *model) renderTopTabs() string {
	active := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("230")).Background(lipgloss.Color("62")).Padding(0, 1)
	inactive := lipgloss.NewStyle().Foreground(lipgloss.Color("250")).Background(lipgloss.Color("238")).Padding(0, 1)
	var parts []string
	m.mouseTabBounds = m.mouseTabBounds[:0]
	x := 0
	for i, title := range topTabs {
		styled := inactive.Render(title)
		if tabIndex(i) == m.tab {
			styled = active.Render(title)
		}
		width := lipgloss.Width(styled)
		m.mouseTabBounds = append(m.mouseTabBounds, tabBound{left: x, right: x + width - 1, tab: tabIndex(i)})
		x += width + 1
		parts = append(parts, styled)
	}
	return strings.Join(parts, " ")
}

func (m *model) viewFooter() string {
	parts := []string{}
	if m.statusMessage != "" {
		parts = append(parts, "ok: "+m.statusMessage)
	}
	if m.errMessage != "" {
		parts = append(parts, "error: "+m.errMessage)
	}
	mouseMode := "Mouse: on (F9 toggle)"
	if !m.mouseCaptureEnabled {
		mouseMode = "Mouse: off (F9 toggle, select text)"
	}
	parts = append(parts, mouseMode, "Tabs: F1..F8 or Ctrl+Left/Right, Ctrl+Q quit")
	return strings.Join(parts, " | ")
}

func (m *model) resizeChatState(state *sessionState) {
	if state == nil {
		return
	}
	inputHeight := state.Input.Height()
	if inputHeight <= 0 {
		inputHeight = 5
	}
	state.Input.SetWidth(max(20, m.width-2))
	state.ChatView.Width = max(20, m.width-2)
	queueHeight := queueDisplayLines(state)
	reserved := 1 + 1 + 1 + inputHeight + 1 + queueHeight
	state.ChatView.Height = max(1, m.height-4-reserved)
}

func queueDisplayLines(state *sessionState) int {
	if state == nil || len(state.Queue) == 0 {
		return 0
	}
	lines := 2 + min(len(state.Queue), 4)
	if len(state.Queue) > 4 {
		lines++
	}
	return lines
}

func enableMouseCaptureCmd() tea.Cmd {
	return tea.Batch(
		func() tea.Msg { return tea.EnterAltScreen() },
		func() tea.Msg { return tea.EnableMouseCellMotion() },
	)
}

func disableMouseCaptureCmd() tea.Cmd {
	return tea.Batch(
		func() tea.Msg { return tea.DisableMouse() },
		func() tea.Msg { return tea.ExitAltScreen() },
	)
}

func (m *model) handleMouseTabs(msg tea.MouseMsg) bool {
	if msg.Button != tea.MouseButtonLeft || msg.Action != tea.MouseActionRelease {
		return false
	}
	if msg.Y != 0 {
		return false
	}
	for _, bound := range m.mouseTabBounds {
		if msg.X >= bound.left && msg.X <= bound.right {
			m.tab = bound.tab
			return true
		}
	}
	return false
}
