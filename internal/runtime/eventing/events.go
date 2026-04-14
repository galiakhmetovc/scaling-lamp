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
	ID            string
	Kind          EventKind
	OccurredAt    time.Time
	AggregateID   string
	AggregateType AggregateType
	Payload       map[string]any
}
