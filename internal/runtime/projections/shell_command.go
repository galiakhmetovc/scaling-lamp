package projections

import (
	"encoding/json"
	"fmt"
	"sort"
	"time"

	"teamd/internal/runtime/eventing"
)

type ShellCommandView struct {
	CommandID            string    `json:"command_id"`
	SessionID            string    `json:"session_id"`
	RunID                string    `json:"run_id"`
	OccurredAt           time.Time `json:"occurred_at"`
	ApprovalID           string    `json:"approval_id,omitempty"`
	ToolName             string    `json:"tool_name,omitempty"`
	Message              string    `json:"message,omitempty"`
	Command              string    `json:"command"`
	Args                 []string  `json:"args,omitempty"`
	InvocationExecutable string    `json:"invocation_executable,omitempty"`
	InvocationArgs       []string  `json:"invocation_args,omitempty"`
	Cwd                  string    `json:"cwd,omitempty"`
	Status               string    `json:"status"`
	NextOffset           int       `json:"next_offset"`
	LastChunk            string    `json:"last_chunk"`
	ExitCode             *int      `json:"exit_code,omitempty"`
	Error                string    `json:"error,omitempty"`
	KillPending          bool      `json:"kill_pending,omitempty"`
}

type ShellCommandSnapshot struct {
	Commands map[string]ShellCommandView `json:"commands"`
}

type ShellCommandProjection struct {
	snapshot ShellCommandSnapshot
}

func NewShellCommandProjection() *ShellCommandProjection {
	return &ShellCommandProjection{
		snapshot: ShellCommandSnapshot{Commands: map[string]ShellCommandView{}},
	}
}

func (p *ShellCommandProjection) ID() string { return "shell_command" }

func (p *ShellCommandProjection) Apply(event eventing.Event) error {
	if p.snapshot.Commands == nil {
		p.snapshot.Commands = map[string]ShellCommandView{}
	}
	switch event.Kind {
	case eventing.EventShellCommandApprovalRequested:
		view := ShellCommandView{
			CommandID:            event.AggregateID,
			SessionID:            stringPayload(event.Payload, "session_id"),
			RunID:                stringPayload(event.Payload, "run_id"),
			OccurredAt:           event.OccurredAt,
			ApprovalID:           stringPayload(event.Payload, "approval_id"),
			ToolName:             stringPayload(event.Payload, "tool_name"),
			Message:              stringPayload(event.Payload, "approval_message"),
			Command:              stringPayload(event.Payload, "command"),
			Args:                 stringSlicePayload(event.Payload, "args"),
			InvocationExecutable: stringPayload(event.Payload, "invocation_executable"),
			InvocationArgs:       stringSlicePayload(event.Payload, "invocation_args"),
			Cwd:                  stringPayload(event.Payload, "cwd"),
			Status:               "approval_pending",
		}
		p.snapshot.Commands[event.AggregateID] = view
	case eventing.EventShellCommandApprovalGranted:
		view := p.snapshot.Commands[event.AggregateID]
		view.CommandID = event.AggregateID
		view.SessionID = firstNonEmpty(view.SessionID, stringPayload(event.Payload, "session_id"))
		view.RunID = firstNonEmpty(view.RunID, stringPayload(event.Payload, "run_id"))
		view.OccurredAt = event.OccurredAt
		view.Status = "approved"
		p.snapshot.Commands[event.AggregateID] = view
	case eventing.EventShellCommandApprovalDenied:
		view := p.snapshot.Commands[event.AggregateID]
		view.CommandID = event.AggregateID
		view.SessionID = firstNonEmpty(view.SessionID, stringPayload(event.Payload, "session_id"))
		view.RunID = firstNonEmpty(view.RunID, stringPayload(event.Payload, "run_id"))
		view.OccurredAt = event.OccurredAt
		view.Status = "approval_denied"
		view.Error = firstNonEmpty(stringPayload(event.Payload, "reason"), "shell command denied by operator")
		view.KillPending = false
		p.snapshot.Commands[event.AggregateID] = view
	case eventing.EventShellCommandStarted:
		view := ShellCommandView{
			CommandID:  event.AggregateID,
			SessionID:  stringPayload(event.Payload, "session_id"),
			RunID:      stringPayload(event.Payload, "run_id"),
			OccurredAt: event.OccurredAt,
			Command:    stringPayload(event.Payload, "command"),
			Args:       stringSlicePayload(event.Payload, "args"),
			Cwd:        stringPayload(event.Payload, "cwd"),
			Status:     "running",
		}
		p.snapshot.Commands[event.AggregateID] = view
	case eventing.EventShellCommandOutputChunk:
		view := p.snapshot.Commands[event.AggregateID]
		view.CommandID = event.AggregateID
		view.SessionID = firstNonEmpty(view.SessionID, stringPayload(event.Payload, "session_id"))
		view.RunID = firstNonEmpty(view.RunID, stringPayload(event.Payload, "run_id"))
		view.OccurredAt = event.OccurredAt
		view.Status = firstNonEmpty(view.Status, "running")
		view.NextOffset = intPayload(event.Payload, "offset")
		view.LastChunk = stringPayload(event.Payload, "text")
		p.snapshot.Commands[event.AggregateID] = view
	case eventing.EventShellCommandKillRequested:
		view := p.snapshot.Commands[event.AggregateID]
		view.CommandID = event.AggregateID
		view.SessionID = firstNonEmpty(view.SessionID, stringPayload(event.Payload, "session_id"))
		view.RunID = firstNonEmpty(view.RunID, stringPayload(event.Payload, "run_id"))
		view.OccurredAt = event.OccurredAt
		view.Status = "killing"
		view.KillPending = true
		p.snapshot.Commands[event.AggregateID] = view
	case eventing.EventShellCommandCompleted:
		view := p.snapshot.Commands[event.AggregateID]
		view.CommandID = event.AggregateID
		view.SessionID = firstNonEmpty(view.SessionID, stringPayload(event.Payload, "session_id"))
		view.RunID = firstNonEmpty(view.RunID, stringPayload(event.Payload, "run_id"))
		view.OccurredAt = event.OccurredAt
		view.Status = firstNonEmpty(stringPayload(event.Payload, "status"), "completed")
		if value, ok := event.Payload["exit_code"].(int); ok {
			view.ExitCode = &value
		} else if value, ok := event.Payload["exit_code"].(float64); ok {
			intValue := int(value)
			view.ExitCode = &intValue
		}
		view.Error = stringPayload(event.Payload, "error")
		view.KillPending = false
		p.snapshot.Commands[event.AggregateID] = view
	}
	return nil
}

func (p *ShellCommandProjection) Snapshot() ShellCommandSnapshot { return p.snapshot }
func (p *ShellCommandProjection) SnapshotValue() any             { return p.snapshot }

func (p *ShellCommandProjection) SnapshotForSession(sessionID string) []ShellCommandView {
	if p.snapshot.Commands == nil {
		return nil
	}
	out := make([]ShellCommandView, 0, len(p.snapshot.Commands))
	for _, command := range p.snapshot.Commands {
		if sessionID != "" && command.SessionID != sessionID {
			continue
		}
		out = append(out, command)
	}
	sort.Slice(out, func(i, j int) bool {
		return out[i].CommandID < out[j].CommandID
	})
	return out
}

func (p *ShellCommandProjection) PendingForSession(sessionID string) []ShellCommandView {
	commands := p.SnapshotForSession(sessionID)
	out := make([]ShellCommandView, 0, len(commands))
	for _, command := range commands {
		if command.Status == "approval_pending" {
			out = append(out, command)
		}
	}
	return out
}

func (p *ShellCommandProjection) ActiveForSession(sessionID string) []ShellCommandView {
	commands := p.SnapshotForSession(sessionID)
	out := make([]ShellCommandView, 0, len(commands))
	for _, command := range commands {
		switch command.Status {
		case "running", "killing":
			out = append(out, command)
		}
	}
	return out
}

func (p *ShellCommandProjection) RestoreSnapshot(raw []byte) error {
	var snapshot ShellCommandSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore shell command snapshot: %w", err)
	}
	if snapshot.Commands == nil {
		snapshot.Commands = map[string]ShellCommandView{}
	}
	p.snapshot = snapshot
	return nil
}

func stringPayload(payload map[string]any, key string) string {
	if payload == nil {
		return ""
	}
	value, _ := payload[key].(string)
	return value
}

func intPayload(payload map[string]any, key string) int {
	if payload == nil {
		return 0
	}
	if value, ok := payload[key].(int); ok {
		return value
	}
	if value, ok := payload[key].(float64); ok {
		return int(value)
	}
	return 0
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if value != "" {
			return value
		}
	}
	return ""
}

func stringSlicePayload(payload map[string]any, key string) []string {
	if payload == nil {
		return nil
	}
	raw, ok := payload[key]
	if !ok || raw == nil {
		return nil
	}
	switch value := raw.(type) {
	case []string:
		return append([]string{}, value...)
	case []any:
		out := make([]string, 0, len(value))
		for _, item := range value {
			text, ok := item.(string)
			if !ok {
				continue
			}
			out = append(out, text)
		}
		return out
	default:
		return nil
	}
}
