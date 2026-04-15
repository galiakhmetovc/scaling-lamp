package tui

import (
	"context"
	"strings"
	"time"

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

type runMeta struct {
	Active       bool
	StartedAt    time.Time
	CompletedAt  time.Time
	Provider     string
	Model        string
	InputTokens  int
	OutputTokens int
	TotalTokens  int
}

type queuedDraft struct {
	Text     string
	QueuedAt time.Time
}

type btwRun struct {
	ID           string
	Prompt       string
	Response     string
	Error        string
	StartedAt    time.Time
	CompletedAt  time.Time
	Active       bool
	Provider     string
	Model        string
	InputTokens  int
	OutputTokens int
	TotalTokens  int
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
	MainRun   runMeta
	Queue     []queuedDraft
	QueueCursor int
	BtwRuns   []btwRun
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
	now             func() time.Time
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
	clockNow            time.Time
}

type tabBound struct {
	left  int
	right int
	tab   tabIndex
}

type runtimeResultMeta struct {
	Provider     string
	Model        string
	InputTokens  int
	OutputTokens int
	TotalTokens  int
	Content      string
}

type uiEventMsg runtime.UIEvent

type chatTurnFinishedMsg struct {
	SessionID string
	Result    runtimeResultMeta
	Err       error
}

type btwTurnFinishedMsg struct {
	SessionID string
	RunID     string
	Prompt    string
	Result    runtimeResultMeta
	Err       error
}

type rebuildFinishedMsg struct {
	Agent *runtime.Agent
	Err   error
}

type clockTickMsg time.Time
