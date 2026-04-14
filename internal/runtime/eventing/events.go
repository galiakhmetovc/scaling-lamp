package eventing

import "time"

type AggregateType string

const (
	AggregateSession AggregateType = "session"
	AggregateRun     AggregateType = "run"
)

type EventKind string

const (
	EventSessionCreated EventKind = "session.created"
	EventRunStarted     EventKind = "run.started"
)

type Event struct {
	Sequence      uint64
	ID            string
	Kind          EventKind
	OccurredAt    time.Time
	AggregateID   string
	AggregateType AggregateType
	CorrelationID string
	CausationID   string
	Source        string
	Payload       map[string]any
}
