package daemon

import (
	"fmt"
	"time"

	"teamd/internal/contracts"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
)

type SessionSnapshot struct {
	SessionID        string                         `json:"session_id"`
	CreatedAt        time.Time                      `json:"created_at"`
	LastActivity     time.Time                      `json:"last_activity"`
	MessageCount     int                            `json:"message_count"`
	MainRunActive    bool                           `json:"main_run_active"`
	MainRun          MainRunSnapshot                `json:"main_run"`
	QueuedDrafts     []QueuedDraft                  `json:"queued_drafts"`
	Transcript       []contracts.Message            `json:"transcript"`
	Timeline         []projections.ChatTimelineItem `json:"timeline"`
	Plan             projections.PlanHeadSnapshot   `json:"plan"`
	PendingApprovals []shell.PendingApprovalView    `json:"pending_approvals"`
	RunningCommands  []projections.ShellCommandView `json:"running_commands"`
	Delegates        []projections.DelegateView     `json:"delegates"`
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
	return SessionSnapshot{
		SessionID:        entry.SessionID,
		CreatedAt:        entry.CreatedAt,
		LastActivity:     entry.LastActivity,
		MessageCount:     entry.MessageCount,
		MainRunActive:    s.mainRunActive(sessionID),
		MainRun:          s.mainRunSnapshot(sessionID),
		QueuedDrafts:     s.queuedDrafts(sessionID),
		Transcript:       agent.CurrentTranscript(sessionID),
		Timeline:         agent.CurrentChatTimeline(sessionID),
		Plan:             plan,
		PendingApprovals: agent.PendingShellApprovals(sessionID),
		RunningCommands:  agent.CurrentRunningShellCommands(sessionID),
		Delegates:        agent.CurrentDelegates(sessionID),
	}, nil
}

func (s *Server) lookupSessionSummary(sessionID string) (projections.SessionCatalogEntry, bool) {
	for _, entry := range s.currentAgent().ListSessions() {
		if entry.SessionID == sessionID {
			return entry, true
		}
	}
	return projections.SessionCatalogEntry{}, false
}
