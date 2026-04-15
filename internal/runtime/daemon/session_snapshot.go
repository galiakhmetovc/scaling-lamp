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
	Transcript       []contracts.Message            `json:"transcript"`
	Timeline         []projections.ChatTimelineItem `json:"timeline"`
	Plan             projections.PlanHeadSnapshot   `json:"plan"`
	PendingApprovals []shell.PendingApprovalView    `json:"pending_approvals"`
	RunningCommands  []projections.ShellCommandView `json:"running_commands"`
	Delegates        []projections.DelegateView     `json:"delegates"`
}

func (s *Server) buildSessionSnapshot(sessionID string) (SessionSnapshot, error) {
	entry, ok := s.lookupSessionSummary(sessionID)
	if !ok {
		return SessionSnapshot{}, fmt.Errorf("session %q not found", sessionID)
	}
	plan, _ := s.agent.CurrentPlanHead(sessionID)
	return SessionSnapshot{
		SessionID:        entry.SessionID,
		CreatedAt:        entry.CreatedAt,
		LastActivity:     entry.LastActivity,
		MessageCount:     entry.MessageCount,
		Transcript:       s.agent.CurrentTranscript(sessionID),
		Timeline:         s.agent.CurrentChatTimeline(sessionID),
		Plan:             plan,
		PendingApprovals: s.agent.PendingShellApprovals(sessionID),
		RunningCommands:  s.agent.CurrentRunningShellCommands(sessionID),
		Delegates:        s.agent.CurrentDelegates(sessionID),
	}, nil
}

func (s *Server) lookupSessionSummary(sessionID string) (projections.SessionCatalogEntry, bool) {
	for _, entry := range s.agent.ListSessions() {
		if entry.SessionID == sessionID {
			return entry, true
		}
	}
	return projections.SessionCatalogEntry{}, false
}
