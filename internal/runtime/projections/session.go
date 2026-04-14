package projections

import (
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
