package eventing

import "time"

type AggregateType string

const (
	AggregateSession      AggregateType = "session"
	AggregateRun          AggregateType = "run"
	AggregatePlan         AggregateType = "plan"
	AggregatePlanTask     AggregateType = "plan_task"
	AggregateShellCommand AggregateType = "shell_command"
)

type EventKind string

const (
	EventSessionCreated            EventKind = "session.created"
	EventMessageRecorded           EventKind = "message.recorded"
	EventRunStarted                EventKind = "run.started"
	EventProviderRequestCaptured   EventKind = "provider.request.captured"
	EventTransportAttemptCompleted EventKind = "transport.attempt.completed"
	EventToolCallStarted           EventKind = "tool.call.started"
	EventToolCallCompleted         EventKind = "tool.call.completed"
	EventRunCompleted              EventKind = "run.completed"
	EventRunFailed                 EventKind = "run.failed"
	EventPlanCreated               EventKind = "plan.created"
	EventPlanArchived              EventKind = "plan.archived"
	EventTaskAdded                 EventKind = "task.added"
	EventTaskStatusChanged         EventKind = "task.status_changed"
	EventTaskNoteAdded             EventKind = "task.note_added"
	EventTaskEdited                EventKind = "task.edited"
	EventShellCommandStarted       EventKind = "shell.command.started"
	EventShellCommandOutputChunk   EventKind = "shell.command.output.chunk"
	EventShellCommandKillRequested EventKind = "shell.command.kill_requested"
	EventShellCommandCompleted     EventKind = "shell.command.completed"
)

type Event struct {
	Sequence         uint64
	ID               string
	Kind             EventKind
	OccurredAt       time.Time
	AggregateID      string
	AggregateType    AggregateType
	AggregateVersion uint64
	CorrelationID    string
	CausationID      string
	Source           string
	ActorID          string
	ActorType        string
	TraceSummary     string
	TraceRefs        []string
	ArtifactRefs     []string
	Payload          map[string]any
}
