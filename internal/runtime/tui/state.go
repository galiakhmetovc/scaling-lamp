package tui

import (
	"context"
	"strings"

	"github.com/charmbracelet/bubbles/textarea"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"

	"teamd/internal/runtime"
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

type toolsFocusMode int

const (
	toolsFocusApprovals toolsFocusMode = iota
	toolsFocusCommands
	toolsFocusLog
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
	Session   *runtime.ChatSession
	Input     textarea.Model
	Streaming strings.Builder
	ToolLog   []toolLogEntry
	Status    string
	LastError string
	Busy      bool
	Overrides sessionOverrides
	Loaded    bool
	ChatView  viewport.Model
	ToolsView viewport.Model
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
	toolCursor      int
	approvalCursor  int
	commandCursor   int
	toolsFocus      toolsFocusMode

	uiSubID int
	uiCh    <-chan runtime.UIEvent

	rawFiles            []string
	rawCursor           int
	rawEditor           textarea.Model
	rawLoadedPath       string
	settingsMode        settingsMode
	sessionField        int
	formField           int
	formDraft           configFormDraft
	formMaxRounds       textinput.Model
	formStyle           textinput.Model
	planView            viewport.Model
	settingsView        viewport.Model
	planMode            planEditorMode
	planCursor          int
	planGoalInput       textinput.Model
	planDescInput       textinput.Model
	planDepsInput       textinput.Model
	planNoteInput       textinput.Model
	planStatusIndex     int
	mouseCaptureEnabled bool
	statusMessage       string
	errMessage          string
	mouseTabBounds      []tabBound
	mouseSessionTop     int
	mouseToolTop        int
	mousePlanTop        int
	mousePlanLeftWidth  int
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
