package projections

import (
	"encoding/json"
	"fmt"

	"teamd/internal/runtime/eventing"
)

type ShellCommandView struct {
	CommandID   string `json:"command_id"`
	SessionID   string `json:"session_id"`
	RunID       string `json:"run_id"`
	Command     string `json:"command"`
	Status      string `json:"status"`
	NextOffset  int    `json:"next_offset"`
	LastChunk   string `json:"last_chunk"`
	ExitCode    *int   `json:"exit_code,omitempty"`
	Error       string `json:"error,omitempty"`
	KillPending bool   `json:"kill_pending,omitempty"`
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
	case eventing.EventShellCommandStarted:
		view := ShellCommandView{
			CommandID: event.AggregateID,
			SessionID: stringPayload(event.Payload, "session_id"),
			RunID:     stringPayload(event.Payload, "run_id"),
			Command:   stringPayload(event.Payload, "command"),
			Status:    "running",
		}
		p.snapshot.Commands[event.AggregateID] = view
	case eventing.EventShellCommandOutputChunk:
		view := p.snapshot.Commands[event.AggregateID]
		view.CommandID = event.AggregateID
		view.SessionID = firstNonEmpty(view.SessionID, stringPayload(event.Payload, "session_id"))
		view.RunID = firstNonEmpty(view.RunID, stringPayload(event.Payload, "run_id"))
		view.Status = firstNonEmpty(view.Status, "running")
		view.NextOffset = intPayload(event.Payload, "offset")
		view.LastChunk = stringPayload(event.Payload, "text")
		p.snapshot.Commands[event.AggregateID] = view
	case eventing.EventShellCommandKillRequested:
		view := p.snapshot.Commands[event.AggregateID]
		view.CommandID = event.AggregateID
		view.SessionID = firstNonEmpty(view.SessionID, stringPayload(event.Payload, "session_id"))
		view.RunID = firstNonEmpty(view.RunID, stringPayload(event.Payload, "run_id"))
		view.Status = "killing"
		view.KillPending = true
		p.snapshot.Commands[event.AggregateID] = view
	case eventing.EventShellCommandCompleted:
		view := p.snapshot.Commands[event.AggregateID]
		view.CommandID = event.AggregateID
		view.SessionID = firstNonEmpty(view.SessionID, stringPayload(event.Payload, "session_id"))
		view.RunID = firstNonEmpty(view.RunID, stringPayload(event.Payload, "run_id"))
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
