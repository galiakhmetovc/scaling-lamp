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
	Scope       string
	ScopeID     string
	Events      []eventing.Event
	Failure     *FailureSummary
	Diagnostics *DiagnosticsSummary
	Matching    int
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

type DiagnosticsSummary struct {
	StuckRuns     []string
	ShellCommands []ShellCommandDiagnostic
	RecoveryHints []string
}

type ShellCommandDiagnostic struct {
	CommandID string
	SessionID string
	RunID     string
	Command   string
	Status    string
	LastChunk string
	Error     string
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
		Scope:       scope,
		ScopeID:     scopeID,
		Events:      filtered,
		Failure:     summarizeFailure(events),
		Diagnostics: summarizeDiagnostics(events),
		Matching:    matching,
	}, nil
}

func summarizeDiagnostics(events []eventing.Event) *DiagnosticsSummary {
	runStarted := map[string]bool{}
	runFinished := map[string]bool{}
	shellCommands := map[string]ShellCommandDiagnostic{}
	for _, event := range events {
		switch event.Kind {
		case eventing.EventRunStarted:
			runStarted[event.AggregateID] = true
		case eventing.EventRunCompleted, eventing.EventRunFailed:
			runFinished[event.AggregateID] = true
		case eventing.EventShellCommandApprovalRequested, eventing.EventShellCommandApprovalGranted, eventing.EventShellCommandApprovalDenied, eventing.EventShellCommandStarted, eventing.EventShellCommandOutputChunk, eventing.EventShellCommandKillRequested, eventing.EventShellCommandCompleted:
			current := shellCommands[event.AggregateID]
			current.CommandID = event.AggregateID
			current.SessionID = firstNonEmptyString(current.SessionID, payloadString(event.Payload, "session_id"))
			current.RunID = firstNonEmptyString(current.RunID, payloadString(event.Payload, "run_id"))
			current.Command = firstNonEmptyString(current.Command, payloadString(event.Payload, "command"))
			switch event.Kind {
			case eventing.EventShellCommandApprovalRequested:
				current.Status = "approval_pending"
			case eventing.EventShellCommandApprovalGranted:
				current.Status = "approved"
			case eventing.EventShellCommandApprovalDenied:
				current.Status = "approval_denied"
				current.Error = firstNonEmptyString(payloadString(event.Payload, "reason"), current.Error)
			case eventing.EventShellCommandStarted:
				current.Status = firstNonEmptyString(payloadString(event.Payload, "status"), "running")
			case eventing.EventShellCommandOutputChunk:
				current.LastChunk = firstNonEmptyString(payloadString(event.Payload, "text"), current.LastChunk)
				if current.Status == "" {
					current.Status = "running"
				}
			case eventing.EventShellCommandKillRequested:
				current.Status = "killing"
			case eventing.EventShellCommandCompleted:
				current.Status = firstNonEmptyString(payloadString(event.Payload, "status"), "completed")
				current.Error = firstNonEmptyString(payloadString(event.Payload, "error"), current.Error)
			}
			shellCommands[event.AggregateID] = current
		}
	}

	diagnostics := &DiagnosticsSummary{}
	for runID := range runStarted {
		if !runFinished[runID] {
			diagnostics.StuckRuns = append(diagnostics.StuckRuns, runID)
		}
	}
	sort.Strings(diagnostics.StuckRuns)

	commandIDs := make([]string, 0, len(shellCommands))
	for commandID, command := range shellCommands {
		if command.Status == "running" || command.Status == "killing" || command.Status == "approval_pending" || command.Status == "approved" || command.Status == "failed" || command.Status == "approval_denied" {
			commandIDs = append(commandIDs, commandID)
		}
	}
	sort.Strings(commandIDs)
	for _, commandID := range commandIDs {
		diagnostics.ShellCommands = append(diagnostics.ShellCommands, shellCommands[commandID])
	}
	if len(diagnostics.StuckRuns) > 0 {
		diagnostics.RecoveryHints = append(diagnostics.RecoveryHints, "Resume the affected session with --chat --resume <session-id> and retry or continue the last prompt.")
	}
	for _, command := range diagnostics.ShellCommands {
		switch command.Status {
		case "approval_pending":
			diagnostics.RecoveryHints = appendUniqueHint(diagnostics.RecoveryHints, "Open the TUI Tools pane and press a/x to approve or deny pending shell commands.")
		case "running", "killing", "approved":
			diagnostics.RecoveryHints = appendUniqueHint(diagnostics.RecoveryHints, "Open the TUI Tools pane and press k to kill a running shell command.")
		}
	}
	if len(diagnostics.StuckRuns) == 0 && len(diagnostics.ShellCommands) == 0 {
		return nil
	}
	return diagnostics
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

func appendUniqueHint(hints []string, hint string) []string {
	for _, existing := range hints {
		if existing == hint {
			return hints
		}
	}
	return append(hints, hint)
}

func firstNonEmptyString(values ...string) string {
	for _, value := range values {
		if value != "" {
			return value
		}
	}
	return ""
}

func payloadString(payload map[string]any, key string) string {
	if payload == nil {
		return ""
	}
	value, _ := payload[key].(string)
	return value
}
