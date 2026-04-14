package projections

import (
	"encoding/json"
	"fmt"
	"time"

	"teamd/internal/runtime/eventing"
)

type SessionSnapshot struct {
	SessionID string
	CreatedAt time.Time
}

type SessionProjection struct {
	snapshot SessionSnapshot
}

func NewSessionProjection() *SessionProjection {
	return &SessionProjection{}
}

func (p *SessionProjection) ID() string {
	return "session"
}

func (p *SessionProjection) Apply(event eventing.Event) error {
	switch event.Kind {
	case eventing.EventSessionCreated:
		p.snapshot.SessionID = event.AggregateID
		p.snapshot.CreatedAt = event.OccurredAt
		return nil
	default:
		return fmt.Errorf("unsupported event kind %q", event.Kind)
	}
}

func (p *SessionProjection) Snapshot() SessionSnapshot {
	return p.snapshot
}

func (p *SessionProjection) SnapshotValue() any {
	return p.snapshot
}

func (p *SessionProjection) RestoreSnapshot(raw []byte) error {
	var snapshot SessionSnapshot
	if err := json.Unmarshal(raw, &snapshot); err != nil {
		return fmt.Errorf("restore session snapshot: %w", err)
	}
	p.snapshot = snapshot
	return nil
}
