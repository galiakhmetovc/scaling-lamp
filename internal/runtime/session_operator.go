package runtime

import (
	"context"
	"fmt"
	"strings"

	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func (a *Agent) CreateChatSession(ctx context.Context) (*ChatSession, error) {
	if a == nil {
		return nil, fmt.Errorf("agent is nil")
	}
	session, err := a.NewChatSession()
	if err != nil {
		return nil, err
	}
	if a.sessionExists(session.SessionID) {
		return session, nil
	}
	if err := a.RecordEvent(ctx, eventing.Event{
		ID:               a.newID("evt-session-created"),
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       a.now(),
		AggregateID:      session.SessionID,
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		CorrelationID:    session.SessionID,
		Source:           "runtime.session",
		ActorID:          a.Config.ID,
		ActorType:        "agent",
		TraceSummary:     "chat session created",
		Payload: map[string]any{
			"session_id": session.SessionID,
		},
	}); err != nil {
		return nil, err
	}
	return session, nil
}

func (a *Agent) RenameSession(ctx context.Context, sessionID, title string) error {
	if a == nil {
		return fmt.Errorf("agent is nil")
	}
	if strings.TrimSpace(sessionID) == "" {
		return fmt.Errorf("session id is empty")
	}
	title = strings.TrimSpace(title)
	if title == "" {
		return fmt.Errorf("session title is empty")
	}
	if !a.sessionExists(sessionID) {
		return fmt.Errorf("session %q not found", sessionID)
	}
	return a.RecordEvent(ctx, eventing.Event{
		ID:               a.newID("evt-session-renamed"),
		Kind:             eventing.EventSessionRenamed,
		OccurredAt:       a.now(),
		AggregateID:      sessionID,
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 2,
		CorrelationID:    sessionID,
		Source:           "runtime.session",
		ActorID:          a.Config.ID,
		ActorType:        "agent",
		TraceSummary:     "chat session renamed",
		Payload: map[string]any{
			"session_id": sessionID,
			"title":      title,
		},
	})
}

func (a *Agent) CurrentShellCommand(commandID string) (projections.ShellCommandView, bool) {
	projection := a.shellCommandProjection()
	if projection == nil {
		return projections.ShellCommandView{}, false
	}
	for _, view := range projection.SnapshotForSession("") {
		if view.CommandID == commandID {
			return view, true
		}
	}
	return projections.ShellCommandView{}, false
}

func (a *Agent) CurrentDelegates(sessionID string) []projections.DelegateView {
	for _, projection := range a.Projections {
		delegates, ok := projection.(*projections.DelegateProjection)
		if ok {
			return delegates.SnapshotForOwnerSession(sessionID)
		}
	}
	return nil
}
