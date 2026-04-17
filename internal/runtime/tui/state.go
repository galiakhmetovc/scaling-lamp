package tui

import (
	"context"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/textarea"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"

	"teamd/internal/runtime"
	"teamd/internal/runtime/daemon"
)

var topTabs = []string{"Sessions", "Chat", "Head", "Prompt", "Plan", "Tools", "Settings"}

type tabIndex int

const (
	tabSessions tabIndex = iota
	tabChat
	tabHead
	tabPrompt
	tabPlan
	tabTools
	tabSettings
)

type sessionsMode int

const (
	sessionsModeBrowse sessionsMode = iota
	sessionsModeRename
	sessionsModeDeleteConfirm
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

type interjectionEntry struct {
	Text      string
	QueuedAt  time.Time
	StartedAt time.Time
	Status    string
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
	SessionID string
	Snapshot  daemon.SessionSnapshot
	Input     textarea.Model
	PendingPrompt string
	Streaming strings.Builder
	ToolLog   []toolLogEntry
	Status    string
	LastError string
	Busy      bool
	RunCancel context.CancelFunc
	Overrides sessionOverrides
	Loaded    bool
	ChatView  viewport.Model
	ToolsView viewport.Model
	MainRun   runMeta
	Queue     []queuedDraft
	QueueCursor int
	BtwRuns      []btwRun
	Interjections []interjectionEntry
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
	client          OperatorClient
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

	wsCh    <-chan daemon.WebsocketEnvelope
	stopWS  func()

	settingsSnapshot daemon.SettingsSnapshot

	rawFiles            []string
	rawCursor           int
	rawEditor           textarea.Model
	rawLoadedPath       string
	settingsMode        settingsMode
	sessionField        int
	sessionMode         sessionsMode
	sessionTitleInput   textinput.Model
	formField           int
	formDraft           configFormDraft
	formMaxRounds       textinput.Model
	formStyle           textinput.Model
	planView            viewport.Model
	headView            viewport.Model
	settingsView        viewport.Model
	headExpanded        map[string]bool
	headCursor          int
	promptEditor        textarea.Model
	promptLoadedSession string
	promptDirty         bool
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

type daemonEnvelopeMsg daemon.WebsocketEnvelope

type chatTurnFinishedMsg struct {
	SessionID string
	Result    runtimeResultMeta
	Queued    bool
	Draft     *daemon.QueuedDraft
	Session   daemon.SessionSnapshot
	Err       error
}

type btwTurnFinishedMsg struct {
	SessionID string
	RunID     string
	Prompt    string
	Result    runtimeResultMeta
	Err       error
}

type clockTickMsg time.Time

type historyLoadedMsg struct {
	SessionID string
	Chunk     SessionHistoryChunk
	Err       error
}

type sessionRenamedMsg struct {
	Session daemon.SessionSnapshot
	Err     error
}

type sessionDeletedMsg struct {
	SessionID string
	Err       error
}

type promptSavedMsg struct {
	Session daemon.SessionSnapshot
	Err     error
}

type promptResetMsg struct {
	Session daemon.SessionSnapshot
	Err     error
}
