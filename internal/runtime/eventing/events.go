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
	EventMessageRecorded EventKind = "message.recorded"
	EventRunStarted     EventKind = "run.started"
	EventTransportAttemptCompleted EventKind = "transport.attempt.completed"
	EventRunCompleted   EventKind = "run.completed"
	EventRunFailed      EventKind = "run.failed"
)

type Event struct {
	Sequence      uint64
	ID            string
	Kind          EventKind
	OccurredAt    time.Time
	AggregateID   string
	AggregateType AggregateType
	AggregateVersion uint64
	CorrelationID string
	CausationID   string
	Source        string
	ActorID       string
	ActorType     string
	TraceSummary  string
	TraceRefs     []string
	ArtifactRefs  []string
	Payload       map[string]any
}
