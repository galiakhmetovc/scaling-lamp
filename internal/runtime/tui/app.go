package tui

import (
	"context"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"

	"github.com/charmbracelet/bubbles/textarea"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/glamour"
	"github.com/charmbracelet/lipgloss"
	"gopkg.in/yaml.v3"

	"teamd/internal/contracts"
	"teamd/internal/runtime"
	"teamd/internal/runtime/projections"
)

var topTabs = []string{"Sessions", "Chat", "Plan", "Tools", "Settings"}

type tabIndex int

const (
	tabSessions tabIndex = iota
	tabChat
	tabPlan
	tabTools
	tabSettings
)

type settingsMode int

const (
	settingsSession settingsMode = iota
	settingsForm
	settingsRaw
)

type planEditorMode int

const (
	planEditorBrowse planEditorMode = iota
	planEditorCreatePlan
	planEditorAddTask
	planEditorEditTask
	planEditorEditDeps
	planEditorStatus
	planEditorNote
)

type sessionOverrides struct {
	MaxToolRounds          int
	RenderMarkdown         bool
	MarkdownStyle          string
	ShowToolCalls          bool
	ShowToolResults        bool
	ShowPlanAfterPlanTools bool
}

type toolLogEntry struct {
	Activity runtime.ToolActivity
}

type sessionState struct {
	Session     *runtime.ChatSession
	Input       textarea.Model
	Streaming   strings.Builder
	ToolLog     []toolLogEntry
	Status      string
	LastError   string
	Busy        bool
	Overrides   sessionOverrides
	Loaded      bool
	ChatView    viewport.Model
	ToolsView   viewport.Model
}

type configFormDraft struct {
	MaxToolRounds          string
	RenderMarkdown         bool
	MarkdownStyle          string
	ShowToolCalls          bool
	ShowToolResults        bool
	ShowPlanAfterPlanTools bool
}

type model struct {
	ctx             context.Context
	agent           *runtime.Agent
	width           int
	height          int
	tab             tabIndex
	sessions        map[string]*sessionState
	sessionOrder    []string
	activeSessionID string
	sessionCursor   int

	uiSubID int
	uiCh    <-chan runtime.UIEvent

	rawFiles        []string
	rawCursor       int
	rawEditor       textarea.Model
	rawLoadedPath   string
	settingsMode    settingsMode
	sessionField    int
	formField       int
	formDraft       configFormDraft
	formMaxRounds   textinput.Model
	formStyle       textinput.Model
	planView        viewport.Model
	settingsView    viewport.Model
	planMode        planEditorMode
	planCursor      int
	planGoalInput   textinput.Model
	planDescInput   textinput.Model
	planDepsInput   textinput.Model
	planNoteInput   textinput.Model
	planStatusIndex int
	statusMessage   string
	errMessage      string
	mouseTabBounds  []tabBound
	mouseSessionTop int
}

type tabBound struct {
	left  int
	right int
	tab   tabIndex
}

type uiEventMsg runtime.UIEvent

type chatTurnFinishedMsg struct {
	SessionID string
	Err       error
}

type rebuildFinishedMsg struct {
	Agent *runtime.Agent
	Err   error
}

func Run(ctx context.Context, agent *runtime.Agent, resumeID string, stdin io.Reader, stdout io.Writer) error {
	m, err := newModel(ctx, agent, resumeID)
	if err != nil {
		return err
	}
	program := tea.NewProgram(&m, tea.WithAltScreen(), tea.WithMouseAllMotion(), tea.WithInput(stdin), tea.WithOutput(stdout))
	_, err = program.Run()
	return err
}

func newModel(ctx context.Context, agent *runtime.Agent, resumeID string) (model, error) {
	m := model{
		ctx:      ctx,
		agent:    agent,
		tab:      tabChat,
		sessions: map[string]*sessionState{},
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
	m.settingsView = viewport.New(80, 20)

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

func waitForUIEvent(ch <-chan runtime.UIEvent) tea.Cmd {
	return func() tea.Msg {
		event, ok := <-ch
		if !ok {
			return nil
		}
		return uiEventMsg(event)
	}
}

func runChatTurnCmd(agent *runtime.Agent, session *runtime.ChatSession, prompt string, overrides sessionOverrides) tea.Cmd {
	return func() tea.Msg {
		working := &runtime.ChatSession{
			SessionID: session.SessionID,
			Messages:  append([]contracts.Message{}, session.Messages...),
		}
		_, err := agent.ChatTurn(context.Background(), working, runtime.ChatTurnInput{
			Prompt:                prompt,
			MaxToolRoundsOverride: overrides.MaxToolRounds,
		})
		return chatTurnFinishedMsg{SessionID: session.SessionID, Err: err}
	}
}

func rebuildAgentCmd(configPath string) tea.Cmd {
	return func() tea.Msg {
		agent, err := runtime.BuildAgent(configPath)
		return rebuildFinishedMsg{Agent: agent, Err: err}
	}
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
	}
	return nil
}

func (m *model) updateSessions(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "up", "k":
		if m.sessionCursor > 0 {
			m.sessionCursor--
		}
	case "down", "j":
		if m.sessionCursor < len(m.sessionOrder)-1 {
			m.sessionCursor++
		}
	case "enter":
		if len(m.sessionOrder) == 0 {
			return m, nil
		}
		m.activeSessionID = m.sessionOrder[m.sessionCursor]
		if state := m.currentSessionState(); state != nil {
			state.Input.Focus()
		}
		m.tab = tabChat
	case "n":
		session, err := m.agent.NewChatSession()
		if err != nil {
			m.errMessage = err.Error()
			return m, nil
		}
		state := newSessionState(m.defaultOverrides())
		state.Session = session
		state.Loaded = true
		m.sessions[session.SessionID] = state
		m.sessionOrder = append([]string{session.SessionID}, m.sessionOrder...)
		m.activeSessionID = session.SessionID
		m.sessionCursor = 0
		m.tab = tabChat
	}
	return m, nil
}

func (m *model) updateChat(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	state := m.currentSessionState()
	if state == nil {
		return m, nil
	}
	switch msg.String() {
	case "pgup":
		state.ChatView.LineUp(max(1, state.ChatView.Height/2))
		return m, nil
	case "pgdown":
		state.ChatView.LineDown(max(1, state.ChatView.Height/2))
		return m, nil
	case "ctrl+s":
		if state.Busy {
			return m, nil
		}
		prompt := strings.TrimSpace(state.Input.Value())
		if prompt == "" {
			return m, nil
		}
		state.Input.Reset()
		state.Streaming.Reset()
		state.LastError = ""
		state.Status = "sending"
		state.Busy = true
		return m, runChatTurnCmd(m.agent, state.Session, prompt, state.Overrides)
	}
	var cmd tea.Cmd
	state.Input, cmd = state.Input.Update(msg)
	return m, cmd
}

func (m *model) updatePlan(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	head, ok := m.agent.CurrentPlanHead(m.activeSessionID)
	flat := flattenedPlanTasks(head)
	selected, _ := m.selectedPlanTask(head)
	switch msg.String() {
	case "pgup":
		m.planView.LineUp(max(1, m.planView.Height/2))
	case "pgdown":
		m.planView.LineDown(max(1, m.planView.Height/2))
	case "up", "k":
		if m.planMode == planEditorBrowse {
			if m.planCursor > 0 {
				m.planCursor--
			}
			return m, nil
		}
	case "down", "j":
		if m.planMode == planEditorBrowse {
			if m.planCursor < len(flat)-1 {
				m.planCursor++
			}
			return m, nil
		}
	case "c":
		m.planMode = planEditorCreatePlan
		m.planGoalInput.Focus()
		m.planGoalInput.SetValue("")
		return m, nil
	case "a":
		m.planMode = planEditorAddTask
		m.planDescInput.Focus()
		m.planDescInput.SetValue("")
		return m, nil
	case "e":
		if ok {
			m.planMode = planEditorEditTask
			m.planDescInput.Focus()
			m.planDescInput.SetValue(selected.Description)
		}
		return m, nil
	case "d":
		if ok {
			m.planMode = planEditorEditDeps
			m.planDepsInput.Focus()
			m.planDepsInput.SetValue(strings.Join(selected.DependsOn, ","))
		}
		return m, nil
	case "n":
		if ok {
			m.planMode = planEditorNote
			m.planNoteInput.Focus()
			m.planNoteInput.SetValue("")
		}
		return m, nil
	case "s":
		if ok {
			m.planMode = planEditorStatus
			m.planStatusIndex = indexOfPlanStatus(selected.Status)
		}
		return m, nil
	case "esc":
		m.planMode = planEditorBrowse
		return m, nil
	case "ctrl+s":
		var err error
		switch m.planMode {
		case planEditorCreatePlan:
			err = m.agent.CreatePlan(m.ctx, m.activeSessionID, strings.TrimSpace(m.planGoalInput.Value()))
		case planEditorAddTask:
			err = m.agent.AddPlanTask(m.ctx, m.activeSessionID, strings.TrimSpace(m.planDescInput.Value()), "", nil)
		case planEditorEditTask:
			if ok {
				err = m.agent.EditPlanTask(m.ctx, m.activeSessionID, selected.ID, strings.TrimSpace(m.planDescInput.Value()), selected.DependsOn)
			}
		case planEditorEditDeps:
			if ok {
				err = m.agent.EditPlanTask(m.ctx, m.activeSessionID, selected.ID, selected.Description, parseCSV(m.planDepsInput.Value()))
			}
		case planEditorStatus:
			if ok {
				err = m.agent.SetPlanTaskStatus(m.ctx, m.activeSessionID, selected.ID, planStatuses()[m.planStatusIndex], "")
			}
		case planEditorNote:
			if ok {
				err = m.agent.AddPlanTaskNote(m.ctx, m.activeSessionID, selected.ID, strings.TrimSpace(m.planNoteInput.Value()))
			}
		}
		if err != nil {
			m.errMessage = err.Error()
			return m, nil
		}
		m.planMode = planEditorBrowse
		if state := m.currentSessionState(); state != nil {
			m.renderChatViewport(state)
		}
		m.statusMessage = "plan updated"
		return m, nil
	case "left", "h":
		if m.planMode == planEditorStatus && m.planStatusIndex > 0 {
			m.planStatusIndex--
			return m, nil
		}
	case "right", "l":
		if m.planMode == planEditorStatus && m.planStatusIndex < len(planStatuses())-1 {
			m.planStatusIndex++
			return m, nil
		}
	}
	switch m.planMode {
	case planEditorCreatePlan:
		var cmd tea.Cmd
		m.planGoalInput, cmd = m.planGoalInput.Update(msg)
		return m, cmd
	case planEditorAddTask, planEditorEditTask:
		var cmd tea.Cmd
		m.planDescInput, cmd = m.planDescInput.Update(msg)
		return m, cmd
	case planEditorEditDeps:
		var cmd tea.Cmd
		m.planDepsInput, cmd = m.planDepsInput.Update(msg)
		return m, cmd
	case planEditorNote:
		var cmd tea.Cmd
		m.planNoteInput, cmd = m.planNoteInput.Update(msg)
		return m, cmd
	}
	return m, nil
}

func (m *model) updateTools(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	state := m.currentSessionState()
	if state == nil {
		return m, nil
	}
	switch msg.String() {
	case "pgup":
		state.ToolsView.LineUp(max(1, state.ToolsView.Height/2))
	case "pgdown":
		state.ToolsView.LineDown(max(1, state.ToolsView.Height/2))
	}
	return m, nil
}

func (m *model) updateSettings(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "1":
		m.settingsMode = settingsSession
	case "2":
		m.settingsMode = settingsForm
	case "3":
		m.settingsMode = settingsRaw
	}
	switch m.settingsMode {
	case settingsSession:
		return m.updateSessionOverrides(msg)
	case settingsForm:
		return m.updateConfigForm(msg)
	case settingsRaw:
		return m.updateRawEditor(msg)
	}
	return m, nil
}

func (m *model) updateSessionOverrides(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	state := m.currentSessionState()
	if state == nil {
		return m, nil
	}
	switch msg.String() {
	case "pgup":
		m.settingsView.LineUp(max(1, m.settingsView.Height/2))
		return m, nil
	case "pgdown":
		m.settingsView.LineDown(max(1, m.settingsView.Height/2))
		return m, nil
	case "up", "k":
		if m.sessionField > 0 {
			m.sessionField--
		}
	case "down", "j":
		if m.sessionField < 5 {
			m.sessionField++
		}
	case "left", "h":
		if m.sessionField == 0 && state.Overrides.MaxToolRounds > 1 {
			state.Overrides.MaxToolRounds--
		}
	case "right", "l":
		if m.sessionField == 0 {
			state.Overrides.MaxToolRounds++
		}
	case " ":
		switch m.sessionField {
		case 1:
			state.Overrides.RenderMarkdown = !state.Overrides.RenderMarkdown
		case 3:
			state.Overrides.ShowToolCalls = !state.Overrides.ShowToolCalls
		case 4:
			state.Overrides.ShowToolResults = !state.Overrides.ShowToolResults
		case 5:
			state.Overrides.ShowPlanAfterPlanTools = !state.Overrides.ShowPlanAfterPlanTools
		}
	case "enter":
		if m.sessionField == 2 {
			if state.Overrides.MarkdownStyle == "dark" {
				state.Overrides.MarkdownStyle = "light"
			} else {
				state.Overrides.MarkdownStyle = "dark"
			}
		}
	}
	return m, nil
}

func (m *model) updateConfigForm(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "pgup":
		m.settingsView.LineUp(max(1, m.settingsView.Height/2))
		return m, nil
	case "pgdown":
		m.settingsView.LineDown(max(1, m.settingsView.Height/2))
		return m, nil
	case "up", "k":
		if m.formField > 0 {
			m.formField--
		}
	case "down", "j":
		if m.formField < 5 {
			m.formField++
		}
	case "left", "h":
		if m.formField == 0 {
			if value, err := strconv.Atoi(strings.TrimSpace(m.formDraft.MaxToolRounds)); err == nil && value > 1 {
				m.formDraft.MaxToolRounds = strconv.Itoa(value - 1)
				m.formMaxRounds.SetValue(m.formDraft.MaxToolRounds)
			}
		}
	case "right", "l":
		if m.formField == 0 {
			value, _ := strconv.Atoi(strings.TrimSpace(m.formDraft.MaxToolRounds))
			if value <= 0 {
				value = 100
			}
			m.formDraft.MaxToolRounds = strconv.Itoa(value + 1)
			m.formMaxRounds.SetValue(m.formDraft.MaxToolRounds)
		}
	case " ":
		switch m.formField {
		case 1:
			m.formDraft.RenderMarkdown = !m.formDraft.RenderMarkdown
		case 3:
			m.formDraft.ShowToolCalls = !m.formDraft.ShowToolCalls
		case 4:
			m.formDraft.ShowToolResults = !m.formDraft.ShowToolResults
		case 5:
			m.formDraft.ShowPlanAfterPlanTools = !m.formDraft.ShowPlanAfterPlanTools
		}
	case "enter":
		if m.formField == 2 {
			if m.formDraft.MarkdownStyle == "dark" {
				m.formDraft.MarkdownStyle = "light"
			} else {
				m.formDraft.MarkdownStyle = "dark"
			}
			m.formStyle.SetValue(m.formDraft.MarkdownStyle)
		}
	case "ctrl+s":
		if err := m.saveFormDraft(); err != nil {
			m.errMessage = err.Error()
		} else {
			m.statusMessage = "config draft saved"
		}
	case "ctrl+a":
		if err := m.saveFormDraft(); err != nil {
			m.errMessage = err.Error()
			return m, nil
		}
		return m, rebuildAgentCmd(m.agent.ConfigPath)
	}
	return m, nil
}

func (m *model) updateRawEditor(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "up", "k":
		if m.rawCursor > 0 {
			m.rawCursor--
			_ = m.loadRawEditorFile()
		}
		return m, nil
	case "down", "j":
		if m.rawCursor < len(m.rawFiles)-1 {
			m.rawCursor++
			_ = m.loadRawEditorFile()
		}
		return m, nil
	case "ctrl+s":
		if err := m.saveRawEditor(); err != nil {
			m.errMessage = err.Error()
		} else {
			m.statusMessage = "raw config saved"
		}
		return m, nil
	case "ctrl+a":
		if err := m.saveRawEditor(); err != nil {
			m.errMessage = err.Error()
			return m, nil
		}
		return m, rebuildAgentCmd(m.agent.ConfigPath)
	}
	var cmd tea.Cmd
	m.rawEditor, cmd = m.rawEditor.Update(msg)
	return m, cmd
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

func (m *model) viewSessions() string {
	lines := []string{"Sessions", "", "n = new session, Enter = activate"}
	m.mouseSessionTop = len(lines)
	for i, sessionID := range m.sessionOrder {
		state := m.sessions[sessionID]
		prefix := "  "
		if i == m.sessionCursor {
			prefix = "> "
		}
		title := sessionID
		if state != nil && state.Status != "" {
			title += " [" + state.Status + "]"
		}
		lines = append(lines, prefix+title)
	}
	return strings.Join(lines, "\n")
}

func (m *model) viewChat() string {
	state := m.currentSessionState()
	if state == nil {
		return "No active session"
	}
	m.renderChatViewport(state)
	header := fmt.Sprintf("session: %s\nstatus: %s", state.Session.SessionID, coalesce(state.Status, "idle"))
	return lipgloss.JoinVertical(lipgloss.Left, header, state.ChatView.View(), "\nInput (Ctrl+S send, PgUp/PgDn scroll):", state.Input.View())
}

func (m *model) viewPlan() string {
	head, ok := m.agent.CurrentPlanHead(m.activeSessionID)
	if !ok || head.Plan.ID == "" {
		left := "No active plan\n\nPress c to create one."
		right := m.renderPlanEditor(false, projections.PlanTaskView{})
		return lipgloss.JoinHorizontal(lipgloss.Top, lipgloss.NewStyle().Width(max(30, m.width/2)).Render(left), lipgloss.NewStyle().Width(max(26, m.width/3)).Render(right))
	}
	lines := []string{"Plan", "", "goal: " + head.Plan.Goal}
	ordered := orderedPlanTasks(head.Tasks)
	flat := flattenedPlanTasks(head)
	selected, hasSelection := m.selectedPlanTask(head)
	flatIndex := 0
	for _, task := range ordered {
		if task.ParentTaskID != "" {
			continue
		}
		renderPlanTaskWithSelection(&lines, head, task, ordered, 0, flat, &flatIndex, m.planCursor)
	}
	m.planView.SetContent(strings.Join(lines, "\n"))
	left := m.planView.View()
	right := m.renderPlanEditor(hasSelection, selected)
	return lipgloss.JoinHorizontal(lipgloss.Top, lipgloss.NewStyle().Width(max(30, m.width/2)).Render(left), lipgloss.NewStyle().Width(max(26, m.width/3)).Render(right))
}

func (m *model) viewTools() string {
	state := m.currentSessionState()
	if state == nil {
		return "No active session"
	}
	lines := []string{"Tools", ""}
	for i := len(state.ToolLog) - 1; i >= 0 && len(lines) < m.height-4; i-- {
		entry := state.ToolLog[i]
		line := fmt.Sprintf("[%s] %s", entry.Activity.Phase, entry.Activity.Name)
		if entry.Activity.ErrorText != "" {
			line += " | error: " + entry.Activity.ErrorText
		} else if entry.Activity.ResultText != "" {
			line += " | ok"
		}
		lines = append(lines, line)
	}
	state.ToolsView.SetContent(strings.Join(lines, "\n"))
	return state.ToolsView.View()
}

func (m *model) viewSettings() string {
	modeTitle := []string{"Session Overrides", "Config Form", "Raw YAML"}[m.settingsMode]
	head := "Settings\n\n1=session overrides  2=config form  3=raw yaml\nmode: " + modeTitle
	switch m.settingsMode {
	case settingsSession:
		state := m.currentSessionState()
		if state == nil {
			return head + "\n\nNo active session"
		}
		rows := []string{
			fmt.Sprintf("%s max_tool_rounds: %d", cursor(m.sessionField, 0), state.Overrides.MaxToolRounds),
			fmt.Sprintf("%s render_markdown: %t", cursor(m.sessionField, 1), state.Overrides.RenderMarkdown),
			fmt.Sprintf("%s markdown_style: %s", cursor(m.sessionField, 2), state.Overrides.MarkdownStyle),
			fmt.Sprintf("%s show_tool_calls: %t", cursor(m.sessionField, 3), state.Overrides.ShowToolCalls),
			fmt.Sprintf("%s show_tool_results: %t", cursor(m.sessionField, 4), state.Overrides.ShowToolResults),
			fmt.Sprintf("%s show_plan_after_plan_tools: %t", cursor(m.sessionField, 5), state.Overrides.ShowPlanAfterPlanTools),
		}
		m.settingsView.SetContent(head + "\n\n" + strings.Join(rows, "\n"))
		return m.settingsView.View()
	case settingsForm:
		rows := []string{
			fmt.Sprintf("%s max_tool_rounds: %s", cursor(m.formField, 0), m.formDraft.MaxToolRounds),
			fmt.Sprintf("%s render_markdown: %t", cursor(m.formField, 1), m.formDraft.RenderMarkdown),
			fmt.Sprintf("%s markdown_style: %s", cursor(m.formField, 2), m.formDraft.MarkdownStyle),
			fmt.Sprintf("%s show_tool_calls: %t", cursor(m.formField, 3), m.formDraft.ShowToolCalls),
			fmt.Sprintf("%s show_tool_results: %t", cursor(m.formField, 4), m.formDraft.ShowToolResults),
			fmt.Sprintf("%s show_plan_after_plan_tools: %t", cursor(m.formField, 5), m.formDraft.ShowPlanAfterPlanTools),
			"",
			"Ctrl+S save to disk",
			"Ctrl+A save and reload agent",
		}
		m.settingsView.SetContent(head + "\n\n" + strings.Join(rows, "\n"))
		return m.settingsView.View()
	default:
		var fileLines []string
		for i, path := range m.rawFiles {
			prefix := cursor(m.rawCursor, i)
			fileLines = append(fileLines, prefix+filepath.Base(path))
		}
		left := "Files\n" + strings.Join(fileLines, "\n")
		right := "Editor\n" + m.rawEditor.View() + "\nCtrl+S save  Ctrl+A save+reload"
		return head + "\n\n" + lipgloss.JoinHorizontal(lipgloss.Top, lipgloss.NewStyle().Width(max(24, m.width/4)).Render(left), lipgloss.NewStyle().Width(max(30, m.width-(m.width/4)-4)).Render(right))
	}
}

func (m *model) viewFooter() string {
	parts := []string{}
	if m.statusMessage != "" {
		parts = append(parts, "ok: "+m.statusMessage)
	}
	if m.errMessage != "" {
		parts = append(parts, "error: "+m.errMessage)
	}
	parts = append(parts, "Tabs: F1..F5, Ctrl+Q quit")
	return strings.Join(parts, " | ")
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

func (m *model) handleMouseSessions(msg tea.MouseMsg) bool {
	if msg.Button != tea.MouseButtonLeft || msg.Action != tea.MouseActionRelease {
		return false
	}
	row := msg.Y - m.mouseSessionTop
	if row < 0 || row >= len(m.sessionOrder) {
		return false
	}
	m.sessionCursor = row
	m.activeSessionID = m.sessionOrder[row]
	return true
}

func (m *model) loadSessions(resumeID string) error {
	entries := m.agent.ListSessions()
	for _, entry := range entries {
		state := newSessionState(m.defaultOverrides())
		resumed, err := m.agent.ResumeChatSession(context.Background(), entry.SessionID)
		if err == nil {
			state.Session = resumed
			state.Loaded = true
			m.renderChatViewport(state)
			m.renderToolsViewport(state)
			m.sessions[entry.SessionID] = state
			m.sessionOrder = append(m.sessionOrder, entry.SessionID)
		}
	}
	if resumeID != "" {
		if state, ok := m.sessions[resumeID]; ok {
			m.activeSessionID = resumeID
			state.Loaded = true
			return nil
		}
	}
	session, err := m.agent.NewChatSession()
	if err != nil {
		return err
	}
	state := newSessionState(m.defaultOverrides())
	state.Session = session
	state.Loaded = true
	m.sessions[session.SessionID] = state
	m.sessionOrder = append([]string{session.SessionID}, m.sessionOrder...)
	m.activeSessionID = session.SessionID
	return nil
}

func newSessionState(overrides sessionOverrides) *sessionState {
	input := textarea.New()
	input.Prompt = ""
	input.SetHeight(6)
	input.Focus()
	chatView := viewport.New(80, 20)
	toolsView := viewport.New(80, 20)
	return &sessionState{
		Input:       input,
		Overrides:   overrides,
		ChatView:    chatView,
		ToolsView:   toolsView,
		Status:      "idle",
	}
}

func (m *model) defaultOverrides() sessionOverrides {
	return sessionOverrides{
		MaxToolRounds:          m.agent.MaxToolRounds,
		RenderMarkdown:         m.agent.Contracts.Chat.Output.Params.RenderMarkdown,
		MarkdownStyle:          coalesce(m.agent.Contracts.Chat.Output.Params.MarkdownStyle, "dark"),
		ShowToolCalls:          m.agent.Contracts.Chat.Status.Params.ShowToolCalls,
		ShowToolResults:        m.agent.Contracts.Chat.Status.Params.ShowToolResults,
		ShowPlanAfterPlanTools: m.agent.Contracts.Chat.Status.Params.ShowPlanAfterPlanTools,
	}
}

func (m *model) currentSessionState() *sessionState {
	if m.activeSessionID == "" {
		return nil
	}
	return m.sessions[m.activeSessionID]
}

func (m *model) renderChatViewport(state *sessionState) {
	if state == nil || state.Session == nil {
		return
	}
	lines := []string{}
	for _, item := range m.agent.CurrentChatTimeline(state.Session.SessionID) {
		switch item.Kind {
		case projections.ChatTimelineItemMessage:
			lines = append(lines, strings.ToUpper(item.Role)+":")
			content := item.Content
			if item.Role == "assistant" && state.Overrides.RenderMarkdown {
				rendered, err := renderMarkdown(content, state.Overrides.MarkdownStyle)
				if err == nil {
					content = strings.TrimRight(rendered, "\n")
				}
			}
			lines = append(lines, content, "")
		default:
			rendered, err := renderMarkdown(item.Content, state.Overrides.MarkdownStyle)
			if err == nil {
				lines = append(lines, strings.TrimRight(rendered, "\n"), "")
			} else {
				lines = append(lines, item.Content, "")
			}
		}
	}
	if state.Streaming.Len() > 0 {
		lines = append(lines, "ASSISTANT:", state.Streaming.String(), "")
	}
	state.ChatView.SetContent(strings.Join(lines, "\n"))
	state.ChatView.GotoBottom()
}

func (m *model) renderToolsViewport(state *sessionState) {
	if state == nil {
		return
	}
	lines := []string{"Tools", ""}
	for i := len(state.ToolLog) - 1; i >= 0; i-- {
		entry := state.ToolLog[i]
		line := fmt.Sprintf("[%s] %s", entry.Activity.Phase, entry.Activity.Name)
		if entry.Activity.ErrorText != "" {
			line += " | error: " + entry.Activity.ErrorText
		} else if entry.Activity.ResultText != "" {
			line += " | ok"
		}
		lines = append(lines, line)
	}
	state.ToolsView.SetContent(strings.Join(lines, "\n"))
}

func renderMarkdown(input, style string) (string, error) {
	if strings.TrimSpace(input) == "" {
		return "", nil
	}
	options := []glamour.TermRendererOption{glamour.WithWordWrap(0)}
	if strings.TrimSpace(style) != "" {
		options = append(options, glamour.WithStandardStyle(style))
	} else {
		options = append(options, glamour.WithAutoStyle())
	}
	renderer, err := glamour.NewTermRenderer(options...)
	if err != nil {
		return "", err
	}
	return renderer.Render(input)
}

func (m *model) loadRawFileList() error {
	root := filepath.Dir(m.agent.ConfigPath)
	files := []string{m.agent.ConfigPath}
	if err := filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		if path == m.agent.ConfigPath {
			return nil
		}
		if strings.HasSuffix(path, ".yaml") || strings.HasSuffix(path, ".yml") {
			files = append(files, path)
		}
		return nil
	}); err != nil {
		return err
	}
	slices.Sort(files)
	m.rawFiles = files
	if m.rawCursor >= len(m.rawFiles) {
		m.rawCursor = 0
	}
	return m.loadRawEditorFile()
}

func (m *model) loadRawEditorFile() error {
	if len(m.rawFiles) == 0 {
		m.rawEditor.SetValue("")
		m.rawLoadedPath = ""
		return nil
	}
	path := m.rawFiles[m.rawCursor]
	body, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	m.rawLoadedPath = path
	m.rawEditor.SetValue(string(body))
	return nil
}

func (m *model) saveRawEditor() error {
	if m.rawLoadedPath == "" {
		return nil
	}
	return os.WriteFile(m.rawLoadedPath, []byte(m.rawEditor.Value()), 0o644)
}

func (m *model) resetFormDraft() {
	m.formDraft = configFormDraft{
		MaxToolRounds:          strconv.Itoa(m.agent.Config.Spec.Runtime.MaxToolRounds),
		RenderMarkdown:         m.agent.Contracts.Chat.Output.Params.RenderMarkdown,
		MarkdownStyle:          coalesce(m.agent.Contracts.Chat.Output.Params.MarkdownStyle, "dark"),
		ShowToolCalls:          m.agent.Contracts.Chat.Status.Params.ShowToolCalls,
		ShowToolResults:        m.agent.Contracts.Chat.Status.Params.ShowToolResults,
		ShowPlanAfterPlanTools: m.agent.Contracts.Chat.Status.Params.ShowPlanAfterPlanTools,
	}
	m.formMaxRounds.SetValue(m.formDraft.MaxToolRounds)
	m.formStyle.SetValue(m.formDraft.MarkdownStyle)
}

func (m *model) saveFormDraft() error {
	if err := updateAgentRuntimeMaxToolRounds(m.agent.ConfigPath, m.formDraft.MaxToolRounds); err != nil {
		return err
	}
	root := filepath.Dir(m.agent.ConfigPath)
	if err := updateChatOutputPolicy(filepath.Join(root, "policies", "chat", "output.yaml"), m.formDraft.RenderMarkdown, m.formDraft.MarkdownStyle); err != nil {
		return err
	}
	if err := updateChatStatusPolicy(filepath.Join(root, "policies", "chat", "status.yaml"), m.formDraft.ShowToolCalls, m.formDraft.ShowToolResults, m.formDraft.ShowPlanAfterPlanTools); err != nil {
		return err
	}
	return nil
}

func updateAgentRuntimeMaxToolRounds(path, value string) error {
	var cfg struct {
		Kind    string `yaml:"kind"`
		Version string `yaml:"version"`
		ID      string `yaml:"id"`
		Spec    struct {
			Runtime   map[string]any    `yaml:"runtime"`
			Contracts map[string]string `yaml:"contracts"`
		} `yaml:"spec"`
	}
	body, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	if err := yaml.Unmarshal(body, &cfg); err != nil {
		return err
	}
	if cfg.Spec.Runtime == nil {
		cfg.Spec.Runtime = map[string]any{}
	}
	parsed, err := strconv.Atoi(strings.TrimSpace(value))
	if err != nil {
		return err
	}
	cfg.Spec.Runtime["max_tool_rounds"] = parsed
	out, err := yaml.Marshal(&cfg)
	if err != nil {
		return err
	}
	return os.WriteFile(path, out, 0o644)
}

func updateChatOutputPolicy(path string, renderMarkdown bool, style string) error {
	var doc map[string]any
	body, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	if err := yaml.Unmarshal(body, &doc); err != nil {
		return err
	}
	spec := ensureMap(doc, "spec")
	params := ensureMap(spec, "params")
	params["render_markdown"] = renderMarkdown
	params["markdown_style"] = style
	out, err := yaml.Marshal(doc)
	if err != nil {
		return err
	}
	return os.WriteFile(path, out, 0o644)
}

func updateChatStatusPolicy(path string, showToolCalls, showToolResults, showPlanAfter bool) error {
	var doc map[string]any
	body, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	if err := yaml.Unmarshal(body, &doc); err != nil {
		return err
	}
	spec := ensureMap(doc, "spec")
	params := ensureMap(spec, "params")
	params["show_tool_calls"] = showToolCalls
	params["show_tool_results"] = showToolResults
	params["show_plan_after_plan_tools"] = showPlanAfter
	out, err := yaml.Marshal(doc)
	if err != nil {
		return err
	}
	return os.WriteFile(path, out, 0o644)
}

func ensureMap(parent map[string]any, key string) map[string]any {
	if existing, ok := parent[key].(map[string]any); ok {
		return existing
	}
	if existing, ok := parent[key].(map[any]any); ok {
		out := map[string]any{}
		for k, v := range existing {
			if text, ok := k.(string); ok {
				out[text] = v
			}
		}
		parent[key] = out
		return out
	}
	out := map[string]any{}
	parent[key] = out
	return out
}

func cursor(current, want int) string {
	if current == want {
		return ">"
	}
	return " "
}

func renderPlanTask(lines *[]string, head projections.PlanHeadSnapshot, task projections.PlanTaskView, all []projections.PlanTaskView, depth int) {
	prefix := strings.Repeat("  ", depth)
	status := "[todo]"
	switch task.Status {
	case "done":
		status = "[done]"
	case "in_progress":
		status = "[doing]"
	case "blocked":
		status = "[blocked]"
	case "cancelled":
		status = "[cancelled]"
	default:
		if head.WaitingOnDependencies[task.ID] {
			status = "[waiting]"
		} else if head.Ready[task.ID] {
			status = "[ready]"
		}
	}
	*lines = append(*lines, fmt.Sprintf("%s%s %s", prefix, status, task.Description))
	for _, child := range all {
		if child.ParentTaskID == task.ID {
			renderPlanTask(lines, head, child, all, depth+1)
		}
	}
}

func orderedPlanTasks(tasks map[string]projections.PlanTaskView) []projections.PlanTaskView {
	out := make([]projections.PlanTaskView, 0, len(tasks))
	for _, task := range tasks {
		out = append(out, task)
	}
	slices.SortFunc(out, func(a, b projections.PlanTaskView) int {
		if a.Order == b.Order {
			return strings.Compare(a.ID, b.ID)
		}
		return a.Order - b.Order
	})
	return out
}

func flattenedPlanTasks(head projections.PlanHeadSnapshot) []projections.PlanTaskView {
	ordered := orderedPlanTasks(head.Tasks)
	out := make([]projections.PlanTaskView, 0, len(ordered))
	var walk func(parent string)
	walk = func(parent string) {
		for _, task := range ordered {
			if task.ParentTaskID != parent {
				continue
			}
			out = append(out, task)
			walk(task.ID)
		}
	}
	walk("")
	return out
}

func renderPlanTaskWithSelection(lines *[]string, head projections.PlanHeadSnapshot, task projections.PlanTaskView, all []projections.PlanTaskView, depth int, flat []projections.PlanTaskView, flatIndex *int, selectedIndex int) {
	prefix := "  "
	if *flatIndex == selectedIndex {
		prefix = "> "
	}
	rendered := fmt.Sprintf("%s%s", prefix, planTaskLine(head, task, depth))
	*lines = append(*lines, rendered)
	*flatIndex++
	for _, child := range all {
		if child.ParentTaskID == task.ID {
			renderPlanTaskWithSelection(lines, head, child, all, depth+1, flat, flatIndex, selectedIndex)
		}
	}
}

func planTaskLine(head projections.PlanHeadSnapshot, task projections.PlanTaskView, depth int) string {
	prefix := strings.Repeat("  ", depth)
	status := "[todo]"
	switch task.Status {
	case "done":
		status = "[done]"
	case "in_progress":
		status = "[doing]"
	case "blocked":
		status = "[blocked]"
	case "cancelled":
		status = "[cancelled]"
	default:
		if head.WaitingOnDependencies[task.ID] {
			status = "[waiting]"
		} else if head.Ready[task.ID] {
			status = "[ready]"
		}
	}
	return fmt.Sprintf("%s%s %s", prefix, status, task.Description)
}

func (m *model) selectedPlanTask(head projections.PlanHeadSnapshot) (projections.PlanTaskView, bool) {
	flat := flattenedPlanTasks(head)
	if len(flat) == 0 || m.planCursor < 0 || m.planCursor >= len(flat) {
		return projections.PlanTaskView{}, false
	}
	return flat[m.planCursor], true
}

func (m *model) renderPlanEditor(hasSelection bool, selected projections.PlanTaskView) string {
	lines := []string{
		"Plan Editor",
		"",
		"c=create plan  a=add task  e=edit task  d=deps  s=status  n=note  Esc=close  Ctrl+S=apply",
		"",
	}
	switch m.planMode {
	case planEditorCreatePlan:
		lines = append(lines, "Create Plan", "", "Goal:", m.planGoalInput.View())
	case planEditorAddTask:
		lines = append(lines, "Add Task", "", "Description:", m.planDescInput.View())
	case planEditorEditTask:
		lines = append(lines, "Edit Task", "", "Task ID: "+selected.ID, "Description:", m.planDescInput.View())
	case planEditorEditDeps:
		lines = append(lines, "Edit Dependencies", "", "Task ID: "+selected.ID, "Depends on (comma-separated ids):", m.planDepsInput.View())
	case planEditorStatus:
		status := "todo"
		if index := m.planStatusIndex; index >= 0 && index < len(planStatuses()) {
			status = planStatuses()[index]
		}
		lines = append(lines, "Set Status", "", "Task ID: "+selected.ID, "Use h/l then Ctrl+S", "Status: "+status)
	case planEditorNote:
		lines = append(lines, "Add Note", "", "Task ID: "+selected.ID, "Note:", m.planNoteInput.View())
	default:
		if hasSelection {
			lines = append(lines,
				"Selected Task",
				"",
				"ID: "+selected.ID,
				"Description: "+selected.Description,
				"Status: "+selected.Status,
				"Depends on: "+strings.Join(selected.DependsOn, ", "),
			)
		} else {
			lines = append(lines, "No task selected")
		}
	}
	return strings.Join(lines, "\n")
}

func planStatuses() []string {
	return []string{"todo", "in_progress", "done", "blocked", "cancelled"}
}

func indexOfPlanStatus(status string) int {
	for idx, item := range planStatuses() {
		if item == status {
			return idx
		}
	}
	return 0
}

func parseCSV(input string) []string {
	if strings.TrimSpace(input) == "" {
		return nil
	}
	parts := strings.Split(input, ",")
	out := make([]string, 0, len(parts))
	for _, part := range parts {
		text := strings.TrimSpace(part)
		if text != "" {
			out = append(out, text)
		}
	}
	return out
}

func coalesce(value, fallback string) string {
	if strings.TrimSpace(value) == "" {
		return fallback
	}
	return value
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}
