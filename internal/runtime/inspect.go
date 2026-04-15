package runtime

import (
	"context"
	"fmt"
	"sort"

	"teamd/internal/runtime/eventing"
)

type InspectOptions struct {
	Kind  eventing.EventKind
	Limit int
}

type InspectionReport struct {
	Scope    string
	ScopeID  string
	Events   []eventing.Event
	Failure  *FailureSummary
	Matching int
}

type FailureSummary struct {
	RunID       string
	Error       string
	ToolErrors  []ToolFailure
	ProviderIDs []string
}

type ToolFailure struct {
	Name  string
	Error string
}

func (a *Agent) InspectSession(ctx context.Context, sessionID string, opts InspectOptions) (InspectionReport, error) {
	if a == nil {
		return InspectionReport{}, fmt.Errorf("agent is nil")
	}
	if sessionID == "" {
		return InspectionReport{}, fmt.Errorf("session id is empty")
	}
	events, err := a.EventLog.ListAll(ctx)
	if err != nil {
		return InspectionReport{}, fmt.Errorf("list events: %w", err)
	}
	scoped := filterEvents(events, func(event eventing.Event) bool {
		if event.AggregateType == eventing.AggregateSession && event.AggregateID == sessionID {
			return true
		}
		return payloadString(event.Payload, "session_id") == sessionID
	})
	return buildInspectionReport("session", sessionID, scoped, opts)
}

func (a *Agent) InspectRun(ctx context.Context, runID string, opts InspectOptions) (InspectionReport, error) {
	if a == nil {
		return InspectionReport{}, fmt.Errorf("agent is nil")
	}
	if runID == "" {
		return InspectionReport{}, fmt.Errorf("run id is empty")
	}
	events, err := a.EventLog.ListByAggregate(ctx, eventing.AggregateRun, runID)
	if err != nil {
		return InspectionReport{}, fmt.Errorf("list run events: %w", err)
	}
	return buildInspectionReport("run", runID, events, opts)
}

func buildInspectionReport(scope, scopeID string, events []eventing.Event, opts InspectOptions) (InspectionReport, error) {
	sort.Slice(events, func(i, j int) bool {
		if events[i].Sequence == events[j].Sequence {
			return events[i].OccurredAt.Before(events[j].OccurredAt)
		}
		return events[i].Sequence < events[j].Sequence
	})
	if len(events) == 0 {
		return InspectionReport{}, fmt.Errorf("%s %q not found", scope, scopeID)
	}

	filtered := events
	if opts.Kind != "" {
		filtered = filterEvents(filtered, func(event eventing.Event) bool {
			return event.Kind == opts.Kind
		})
	}
	matching := len(filtered)
	if opts.Limit > 0 && len(filtered) > opts.Limit {
		filtered = append([]eventing.Event{}, filtered[len(filtered)-opts.Limit:]...)
	} else {
		filtered = append([]eventing.Event{}, filtered...)
	}

	return InspectionReport{
		Scope:    scope,
		ScopeID:  scopeID,
		Events:   filtered,
		Failure:  summarizeFailure(events),
		Matching: matching,
	}, nil
}

func summarizeFailure(events []eventing.Event) *FailureSummary {
	var failed *eventing.Event
	for i := len(events) - 1; i >= 0; i-- {
		if events[i].Kind == eventing.EventRunFailed {
			failed = &events[i]
			break
		}
	}
	if failed == nil {
		return nil
	}

	summary := &FailureSummary{
		RunID: failed.AggregateID,
		Error: payloadString(failed.Payload, "error"),
	}
	for _, event := range events {
		if event.AggregateType != eventing.AggregateRun || event.AggregateID != failed.AggregateID {
			continue
		}
		switch event.Kind {
		case eventing.EventToolCallCompleted:
			if errText := payloadString(event.Payload, "error"); errText != "" {
				summary.ToolErrors = append(summary.ToolErrors, ToolFailure{
					Name:  payloadString(event.Payload, "tool_name"),
					Error: errText,
				})
			}
		case eventing.EventProviderRequestCaptured:
			if providerID := payloadString(event.Payload, "provider_id"); providerID != "" {
				summary.ProviderIDs = append(summary.ProviderIDs, providerID)
			}
		}
	}
	return summary
}

func filterEvents(events []eventing.Event, keep func(eventing.Event) bool) []eventing.Event {
	out := make([]eventing.Event, 0, len(events))
	for _, event := range events {
		if keep(event) {
			out = append(out, event)
		}
	}
	return out
}

func payloadString(payload map[string]any, key string) string {
	if payload == nil {
		return ""
	}
	value, _ := payload[key].(string)
	return value
}
