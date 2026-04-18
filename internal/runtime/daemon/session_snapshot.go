package daemon

import (
	"fmt"
	"strings"
	"time"

	"teamd/internal/contracts"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
)

type SessionSnapshot struct {
	SessionID         string                         `json:"session_id"`
	Title             string                         `json:"title"`
	CreatedAt         time.Time                      `json:"created_at"`
	LastActivity      time.Time                      `json:"last_activity"`
	MessageCount      int                            `json:"message_count"`
	ArtifactStorePath string                         `json:"artifact_store_path"`
	ExecutionVersion  string                         `json:"execution_version"`
	MainRunActive     bool                           `json:"main_run_active"`
	MainRun           MainRunSnapshot                `json:"main_run"`
	QueuedDrafts      []QueuedDraft                  `json:"queued_drafts"`
	History           ChatHistorySnapshot            `json:"history"`
	BaseContextTokens int                            `json:"base_context_tokens"`
	ContextBudget     ContextBudgetSnapshot          `json:"context_budget"`
	Prompt            SessionPromptSnapshot          `json:"prompt"`
	Transcript        []contracts.Message            `json:"transcript"`
	Timeline          []projections.ChatTimelineItem `json:"timeline"`
	Plan              projections.PlanHeadSnapshot   `json:"plan"`
	ToolGovernance    ToolGovernanceSnapshot         `json:"tool_governance"`
	PendingApprovals  []shell.PendingApprovalView    `json:"pending_approvals"`
	RunningCommands   []projections.ShellCommandView `json:"running_commands"`
	Delegates         []projections.DelegateView     `json:"delegates"`
}

type SessionPromptSnapshot struct {
	Default     string `json:"default"`
	Override    string `json:"override"`
	Effective   string `json:"effective"`
	HasOverride bool   `json:"has_override"`
}

type ChatHistorySnapshot struct {
	LoadedCount int  `json:"loaded_count"`
	TotalCount  int  `json:"total_count"`
	HasMore     bool `json:"has_more"`
	WindowLimit int  `json:"window_limit"`
}

type MainRunSnapshot struct {
	Active           bool      `json:"active"`
	Phase            string    `json:"phase"`
	StartedAt        time.Time `json:"started_at"`
	ExecutionVersion string    `json:"execution_version"`
	Provider         string    `json:"provider"`
	Model            string    `json:"model"`
	InputTokens      int       `json:"input_tokens"`
	OutputTokens     int       `json:"output_tokens"`
	TotalTokens      int       `json:"total_tokens"`
}

type ContextBudgetSnapshot struct {
	LastInputTokens          int    `json:"last_input_tokens"`
	LastOutputTokens         int    `json:"last_output_tokens"`
	LastTotalTokens          int    `json:"last_total_tokens"`
	CurrentContextTokens     int    `json:"current_context_tokens"`
	EstimatedNextInputTokens int    `json:"estimated_next_input_tokens"`
	DraftTokens              int    `json:"draft_tokens"`
	QueuedDraftTokens        int    `json:"queued_draft_tokens"`
	SummaryTokens            int    `json:"summary_tokens"`
	SummarizationCount       int    `json:"summarization_count"`
	CompactedMessageCount    int    `json:"compacted_message_count"`
	Source                   string `json:"source"`
	BudgetState              string `json:"budget_state"`
}

func (s *Server) buildSessionSnapshot(sessionID string) (SessionSnapshot, error) {
	entry, ok := s.lookupSessionSummary(sessionID)
	if !ok {
		return SessionSnapshot{}, fmt.Errorf("session %q not found", sessionID)
	}
	agent := s.currentAgent()
	plan, _ := agent.CurrentPlanHead(sessionID)
	transcript := agent.CurrentTranscript(sessionID)
	timeline := agent.CurrentChatTimeline(sessionID)
	windowLimit := s.chatHistoryWindowLimit()
	transcriptWindow := tailMessages(transcript, windowLimit)
	timelineWindow := tailTimeline(timeline, windowLimit)
	prompt := s.sessionPromptSnapshot(sessionID)
	return SessionSnapshot{
		SessionID:         entry.SessionID,
		Title:             entry.Title,
		CreatedAt:         entry.CreatedAt,
		LastActivity:      entry.LastActivity,
		MessageCount:      entry.MessageCount,
		ArtifactStorePath: s.artifactStorePath(),
		ExecutionVersion:  string(s.sessionExecutionVersion(sessionID)),
		MainRunActive:     s.mainRunActive(sessionID),
		MainRun:           s.mainRunSnapshot(sessionID),
		QueuedDrafts:      s.queuedDrafts(sessionID),
		History: ChatHistorySnapshot{
			LoadedCount: len(timelineWindow),
			TotalCount:  len(timeline),
			HasMore:     len(timelineWindow) < len(timeline),
			WindowLimit: windowLimit,
		},
		BaseContextTokens: approximateContextTokens(transcript),
		ContextBudget:     s.contextBudgetSnapshot(sessionID, transcript),
		Prompt:            prompt,
		Transcript:        transcriptWindow,
		Timeline:          timelineWindow,
		Plan:              plan,
		ToolGovernance:    buildToolGovernanceSnapshot(agent),
		PendingApprovals:  agent.PendingShellApprovals(sessionID),
		RunningCommands:   agent.CurrentRunningShellCommands(sessionID),
		Delegates:         agent.CurrentDelegates(sessionID),
	}, nil
}

func (s *Server) sessionPromptSnapshot(sessionID string) SessionPromptSnapshot {
	defaultPrompt, _ := s.currentAgent().DefaultSystemPrompt()
	override := s.currentAgent().CurrentSessionPromptOverride(sessionID)
	effective := defaultPrompt
	if strings.TrimSpace(override) != "" {
		effective = override
	}
	return SessionPromptSnapshot{
		Default:     defaultPrompt,
		Override:    override,
		Effective:   effective,
		HasOverride: strings.TrimSpace(override) != "",
	}
}

func (s *Server) artifactStorePath() string {
	path, err := s.currentAgent().ArtifactStorePath()
	if err != nil {
		return ""
	}
	return path
}

func (s *Server) contextBudgetSnapshot(sessionID string, transcript []contracts.Message) ContextBudgetSnapshot {
	view := s.currentAgent().CurrentContextBudget(sessionID)
	compactedTranscript := s.currentAgent().CompactedMessagesForSession(sessionID, transcript)
	mainRun := s.mainRunSnapshot(sessionID)
	if view.LastTotalTokens == 0 && (mainRun.TotalTokens > 0 || mainRun.InputTokens > 0 || mainRun.OutputTokens > 0) {
		view.LastInputTokens = mainRun.InputTokens
		view.LastOutputTokens = mainRun.OutputTokens
		view.LastTotalTokens = mainRun.TotalTokens
		if view.Source == "" {
			view.Source = "provider"
		}
	}
	current := approximateContextTokens(compactedTranscript)
	queueTokens := 0
	for _, draft := range s.queuedDrafts(sessionID) {
		queueTokens += approximateTextTokens(draft.Text)
	}
	source := view.Source
	if source == "" {
		source = "mixed"
	}
	state := contextBudgetState(s.currentAgent().Contracts.ContextBudget.Compaction.Params, current)
	return ContextBudgetSnapshot{
		LastInputTokens:          view.LastInputTokens,
		LastOutputTokens:         view.LastOutputTokens,
		LastTotalTokens:          view.LastTotalTokens,
		CurrentContextTokens:     current,
		EstimatedNextInputTokens: current + queueTokens,
		QueuedDraftTokens:        queueTokens,
		SummaryTokens:            view.SummaryTokens,
		SummarizationCount:       view.SummarizationCount,
		CompactedMessageCount:    view.CompactedMessageCount,
		Source:                   source,
		BudgetState:              state,
	}
}

func contextBudgetState(params contracts.ContextBudgetCompactionParams, currentTokens int) string {
	if params.MaxContextTokens > 0 && currentTokens > 0 {
		highest := 0
		for _, guard := range params.Guards {
			if guard.Percent <= 0 {
				continue
			}
			if currentTokens*100 >= params.MaxContextTokens*guard.Percent && guard.Percent > highest {
				highest = guard.Percent
			}
		}
		switch {
		case highest >= 85:
			return "needs_compaction"
		case highest >= 70:
			return "approaching_limit"
		default:
			return "healthy"
		}
	}
	state := "healthy"
	if params.WarningTokens > 0 && currentTokens >= params.WarningTokens {
		state = "approaching_limit"
	}
	if params.CompactionTokens > 0 && currentTokens >= params.CompactionTokens {
		state = "needs_compaction"
	}
	return state
}

func approximateTextTokens(text string) int {
	if text == "" {
		return 0
	}
	return maxInt(1, (len(text)+3)/4)
}

func (s *Server) buildSessionHistoryChunk(sessionID string, loadedCount, historyLimit int) (map[string]any, error) {
	if loadedCount < 0 {
		return nil, fmt.Errorf("loaded_count must be >= 0")
	}
	if _, ok := s.lookupSessionSummary(sessionID); !ok {
		return nil, fmt.Errorf("session %q not found", sessionID)
	}
	if historyLimit <= 0 {
		historyLimit = s.chatHistoryWindowLimit()
	}
	timeline := s.currentAgent().CurrentChatTimeline(sessionID)
	totalCount := len(timeline)
	if loadedCount > totalCount {
		loadedCount = totalCount
	}
	remaining := totalCount - loadedCount
	chunkSize := minInt(historyLimit, remaining)
	start := maxInt(0, totalCount-loadedCount-chunkSize)
	end := maxInt(start, totalCount-loadedCount)
	chunk := append([]projections.ChatTimelineItem{}, timeline[start:end]...)
	return map[string]any{
		"session_id":   sessionID,
		"timeline":     chunk,
		"loaded_count": loadedCount + len(chunk),
		"total_count":  totalCount,
		"has_more":     start > 0,
		"window_limit": historyLimit,
	}, nil
}

func (s *Server) chatHistoryWindowLimit() int {
	limit := s.currentAgent().Contracts.OperatorSurface.DaemonServer.Params.InitialChatHistoryLimit
	if limit <= 0 {
		return 40
	}
	return limit
}

func approximateContextTokens(transcript []contracts.Message) int {
	chars := 0
	for _, message := range transcript {
		chars += len(message.Content)
	}
	if chars <= 0 {
		return 0
	}
	return maxInt(1, (chars+3)/4)
}

func tailMessages(messages []contracts.Message, limit int) []contracts.Message {
	if limit <= 0 || len(messages) <= limit {
		return append([]contracts.Message{}, messages...)
	}
	return append([]contracts.Message{}, messages[len(messages)-limit:]...)
}

func tailTimeline(timeline []projections.ChatTimelineItem, limit int) []projections.ChatTimelineItem {
	if limit <= 0 || len(timeline) <= limit {
		return append([]projections.ChatTimelineItem{}, timeline...)
	}
	return append([]projections.ChatTimelineItem{}, timeline[len(timeline)-limit:]...)
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func (s *Server) lookupSessionSummary(sessionID string) (projections.SessionCatalogEntry, bool) {
	for _, entry := range s.currentAgent().ListSessions() {
		if entry.SessionID == sessionID {
			return entry, true
		}
	}
	return projections.SessionCatalogEntry{}, false
}
