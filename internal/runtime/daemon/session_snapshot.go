package daemon

import (
	"fmt"
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
	MainRunActive     bool                           `json:"main_run_active"`
	MainRun           MainRunSnapshot                `json:"main_run"`
	QueuedDrafts      []QueuedDraft                  `json:"queued_drafts"`
	History           ChatHistorySnapshot            `json:"history"`
	BaseContextTokens int                            `json:"base_context_tokens"`
	Transcript        []contracts.Message            `json:"transcript"`
	Timeline          []projections.ChatTimelineItem `json:"timeline"`
	Plan              projections.PlanHeadSnapshot   `json:"plan"`
	PendingApprovals  []shell.PendingApprovalView    `json:"pending_approvals"`
	RunningCommands   []projections.ShellCommandView `json:"running_commands"`
	Delegates         []projections.DelegateView     `json:"delegates"`
}

type ChatHistorySnapshot struct {
	LoadedCount int  `json:"loaded_count"`
	TotalCount  int  `json:"total_count"`
	HasMore     bool `json:"has_more"`
	WindowLimit int  `json:"window_limit"`
}

type MainRunSnapshot struct {
	Active       bool      `json:"active"`
	StartedAt    time.Time `json:"started_at"`
	Provider     string    `json:"provider"`
	Model        string    `json:"model"`
	InputTokens  int       `json:"input_tokens"`
	OutputTokens int       `json:"output_tokens"`
	TotalTokens  int       `json:"total_tokens"`
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
	return SessionSnapshot{
		SessionID:     entry.SessionID,
		Title:         entry.Title,
		CreatedAt:     entry.CreatedAt,
		LastActivity:  entry.LastActivity,
		MessageCount:  entry.MessageCount,
		MainRunActive: s.mainRunActive(sessionID),
		MainRun:       s.mainRunSnapshot(sessionID),
		QueuedDrafts:  s.queuedDrafts(sessionID),
		History: ChatHistorySnapshot{
			LoadedCount: len(timelineWindow),
			TotalCount:  len(timeline),
			HasMore:     len(timelineWindow) < len(timeline),
			WindowLimit: windowLimit,
		},
		BaseContextTokens: approximateContextTokens(transcript),
		Transcript:        transcriptWindow,
		Timeline:          timelineWindow,
		Plan:              plan,
		PendingApprovals:  agent.PendingShellApprovals(sessionID),
		RunningCommands:   agent.CurrentRunningShellCommands(sessionID),
		Delegates:         agent.CurrentDelegates(sessionID),
	}, nil
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
