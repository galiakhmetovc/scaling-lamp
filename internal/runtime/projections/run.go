package projections

import (
	"encoding/json"
	"fmt"

	"teamd/internal/runtime/eventing"
)

type RunStatus string

const (
	RunStatusRunning   RunStatus = "running"
	RunStatusCompleted RunStatus = "completed"
	RunStatusFailed    RunStatus = "failed"
)

type RunSnapshot struct {
	RunID     string
	SessionID string
	Status    RunStatus
}

type RunProjection struct {
	snapshot RunSnapshot
}

func NewRunProjection() *RunProjection {
	return &RunProjection{}
}

func (p *RunProjection) ID() string {
	return "run"
}

func (p *RunProjection) Apply(event eventing.Event) error {
	switch event.Kind {
	case eventing.EventRunStarted:
		p.snapshot.RunID = event.AggregateID
		if sessionID, ok := event.Payload["session_id"].(string); ok {
			p.snapshot.SessionID = sessionID
		}
		p.snapshot.Status = RunStatusRunning
		return nil
	case eventing.EventRunCompleted:
		p.snapshot.RunID = event.AggregateID
		if sessionID, ok := event.Payload["session_id"].(string); ok {
			p.snapshot.SessionID = sessionID
		}
		p.snapshot.Status = RunStatusCompleted
		return nil
	case eventing.EventRunFailed:
		p.snapshot.RunID = event.AggregateID
		if sessionID, ok := event.Payload["session_id"].(string); ok {
			p.snapshot.SessionID = sessionID
		}
		p.snapshot.Status = RunStatusFailed
		return nil
	default:
		return nil
	}
}

func (p *RunProjection) Snapshot() RunSnapshot {
	return p.snapshot
}

func (p *RunProjection) SnapshotValue() any {
	return p.snapshot
}

func (p *RunProjection) RestoreSnapshot(raw []byte) error {
	var snapshot RunSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore run snapshot: %w", err)
	}
	p.snapshot = snapshot
	return nil
}
