package runtime

import (
	"context"
	"fmt"

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
