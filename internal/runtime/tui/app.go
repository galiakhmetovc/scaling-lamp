package tui

import (
	"context"
	"io"
	"strings"

	"github.com/charmbracelet/bubbles/textarea"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"teamd/internal/runtime"
)

func Run(ctx context.Context, agent *runtime.Agent, resumeID string, stdin io.Reader, stdout io.Writer) error {
	m, err := newModel(ctx, agent, resumeID)
	if err != nil {
		return err
	}
	program := tea.NewProgram(&m, tea.WithAltScreen(), tea.WithMouseCellMotion(), tea.WithInput(stdin), tea.WithOutput(stdout))
	_, err = program.Run()
	return err
}

func newModel(ctx context.Context, agent *runtime.Agent, resumeID string) (model, error) {
	m := model{
		ctx:      ctx,
		agent:    agent,
		tab:      tabChat,
		sessions: map[string]*sessionState{},
		mouseCaptureEnabled: true,
	}
	m.rawEditor = textarea.New()
	m.rawEditor.SetHeight(20)
	m.rawEditor.ShowLineNumbers = true
	m.rawEditor.Prompt = ""
	m.formMaxRounds = textinput.New()
	m.formStyle = textinput.New()
	m.planGoalInput = textinput.New()
	m.planDescInput = textinput.New()
	m.planDepsInput = textinput.New()
	m.planNoteInput = textinput.New()
	m.planView = viewport.New(80, 20)
	m.planView.MouseWheelEnabled = true
	m.planView.MouseWheelDelta = 3
	m.settingsView = viewport.New(80, 20)
	m.settingsView.MouseWheelEnabled = true
	m.settingsView.MouseWheelDelta = 3

	id, ch := agent.UIBus.Subscribe(128)
	m.uiSubID, m.uiCh = id, ch
	if err := m.loadSessions(resumeID); err != nil {
		return model{}, err
	}
	if err := m.loadRawFileList(); err != nil {
		return model{}, err
	}
	m.resetFormDraft()
	return m, nil
}

func (m *model) Init() tea.Cmd {
	return waitForUIEvent(m.uiCh)
}

func (m *model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		for _, state := range m.sessions {
			state.Input.SetWidth(max(20, m.width-6))
			state.Input.SetHeight(6)
			state.ChatView.Width = max(20, m.width-6)
			state.ChatView.Height = max(10, m.height-14)
			state.ToolsView.Width = max(20, m.width-6)
			state.ToolsView.Height = max(10, m.height-8)
		}
		m.planView.Width = max(20, m.width-6)
		m.planView.Height = max(10, m.height-8)
		m.settingsView.Width = max(20, m.width-6)
		m.settingsView.Height = max(10, m.height-8)
		m.planGoalInput.Width = max(20, m.width/3)
		m.planDescInput.Width = max(20, m.width/3)
		m.planDepsInput.Width = max(20, m.width/3)
		m.planNoteInput.Width = max(20, m.width/3)
		m.rawEditor.SetWidth(max(20, m.width/2))
		m.rawEditor.SetHeight(max(10, m.height-12))
		return m, nil
	case tea.KeyMsg:
		if msg.String() == "ctrl+c" || msg.String() == "ctrl+q" {
			if m.agent.UIBus != nil {
				m.agent.UIBus.Unsubscribe(m.uiSubID)
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
		if m.tab == tabChat && m.handleMouseChat(msg) {
			return m, nil
		}
		if m.tab == tabSettings && m.handleMouseSettings(msg) {
			return m, nil
		}
	case uiEventMsg:
		event := runtime.UIEvent(msg)
		if state := m.sessions[event.SessionID]; state != nil {
			switch event.Kind {
			case runtime.UIEventStreamText:
				state.Streaming.WriteString(event.Text)
			case runtime.UIEventToolStarted, runtime.UIEventToolCompleted:
				state.ToolLog = append(state.ToolLog, toolLogEntry{Activity: event.Tool})
				if len(state.ToolLog) > 200 {
					state.ToolLog = state.ToolLog[len(state.ToolLog)-200:]
				}
				m.renderToolsViewport(state)
			case runtime.UIEventStatusChanged:
				state.Status = event.Status
			case runtime.UIEventRunCompleted:
				state.Status = "done"
				state.Streaming.Reset()
			}
			m.renderChatViewport(state)
		}
		return m, waitForUIEvent(m.uiCh)
	case chatTurnFinishedMsg:
		state := m.sessions[msg.SessionID]
		if state == nil {
			return m, nil
		}
		state.Busy = false
		if msg.Err != nil {
			state.LastError = msg.Err.Error()
			m.errMessage = msg.Err.Error()
			return m, nil
		}
		resumed, err := m.agent.ResumeChatSession(context.Background(), msg.SessionID)
		if err == nil {
			state.Session = resumed
		}
		state.Status = "idle"
		m.renderChatViewport(state)
		m.renderToolsViewport(state)
		return m, nil
	case rebuildFinishedMsg:
		if msg.Err != nil {
			m.errMessage = msg.Err.Error()
			return m, nil
		}
		if m.agent.UIBus != nil {
			m.agent.UIBus.Unsubscribe(m.uiSubID)
		}
		m.agent = msg.Agent
		m.uiSubID, m.uiCh = m.agent.UIBus.Subscribe(128)
		if err := m.loadRawFileList(); err != nil {
			m.errMessage = err.Error()
		}
		for sessionID, state := range m.sessions {
			if resumed, err := m.agent.ResumeChatSession(context.Background(), sessionID); err == nil {
				state.Session = resumed
				m.renderChatViewport(state)
				m.renderToolsViewport(state)
			}
		}
		m.resetFormDraft()
		m.statusMessage = "config applied and agent reloaded"
		return m, waitForUIEvent(m.uiCh)
	}
	return m, nil
}

func (m *model) handleGlobalKey(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "shift+tab", "left":
		if m.tab == 0 {
			m.tab = tabIndex(len(topTabs) - 1)
		} else {
			m.tab--
		}
		return nil
	case "tab", "right":
		m.tab = tabIndex((int(m.tab) + 1) % len(topTabs))
		return nil
	case "f1":
		m.tab = tabSessions
	case "f2":
		m.tab = tabChat
	case "f3":
		m.tab = tabPlan
	case "f4":
		m.tab = tabTools
	case "f5":
		m.tab = tabSettings
	case "f6":
		m.mouseCaptureEnabled = !m.mouseCaptureEnabled
		if m.mouseCaptureEnabled {
			m.statusMessage = "mouse capture enabled"
			return enableMouseCaptureCmd()
		}
		m.statusMessage = "mouse capture disabled; native text selection available"
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
	mouseMode := "Mouse: on (F6 toggle)"
	if !m.mouseCaptureEnabled {
		mouseMode = "Mouse: off (F6 toggle, select text)"
	}
	parts = append(parts, mouseMode, "Tabs: F1..F5, Ctrl+Q quit")
	return strings.Join(parts, " | ")
}

func enableMouseCaptureCmd() tea.Cmd {
	return func() tea.Msg {
		return tea.EnableMouseCellMotion()
	}
}

func disableMouseCaptureCmd() tea.Cmd {
	return func() tea.Msg {
		return tea.DisableMouse()
	}
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
