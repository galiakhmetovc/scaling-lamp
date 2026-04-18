package runtime

import (
	"context"
	"strings"

	"teamd/internal/runtime/eventing"
)

func (a *Agent) RecordTraceEvent(ctx context.Context, sessionID, runID, source, traceName string, fields map[string]any) error {
	if a == nil || strings.TrimSpace(sessionID) == "" || strings.TrimSpace(traceName) == "" {
		return nil
	}
	payload := map[string]any{
		"session_id": sessionID,
		"trace":      traceName,
		"fields":     cloneTraceFields(fields),
	}
	if runID != "" {
		payload["run_id"] = runID
	}
	return a.RecordEvent(ctx, eventing.Event{
		ID:               a.newID("evt-trace"),
		Kind:             eventing.EventTraceRecorded,
		OccurredAt:       a.now(),
		AggregateID:      sessionID,
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		CorrelationID:    runID,
		Source:           source,
		ActorID:          a.Config.ID,
		ActorType:        "agent",
		TraceSummary:     traceName,
		Payload:          payload,
	})
}

func cloneTraceFields(fields map[string]any) map[string]any {
	if len(fields) == 0 {
		return map[string]any{}
	}
	out := make(map[string]any, len(fields))
	for k, v := range fields {
		out[k] = v
	}
	return out
}
