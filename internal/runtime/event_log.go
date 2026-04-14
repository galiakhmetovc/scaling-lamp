package runtime

import (
	"context"
	"sync"

	"teamd/internal/runtime/eventing"
)

type EventLog interface {
	Append(ctx context.Context, event eventing.Event) error
	ListByAggregate(ctx context.Context, aggregateType eventing.AggregateType, aggregateID string) ([]eventing.Event, error)
}

type InMemoryEventLog struct {
	mu     sync.RWMutex
	events []eventing.Event
}

func NewInMemoryEventLog() *InMemoryEventLog {
	return &InMemoryEventLog{}
}

func (l *InMemoryEventLog) Append(_ context.Context, event eventing.Event) error {
	l.mu.Lock()
	defer l.mu.Unlock()

	l.events = append(l.events, event)
	return nil
}

func (l *InMemoryEventLog) ListByAggregate(_ context.Context, aggregateType eventing.AggregateType, aggregateID string) ([]eventing.Event, error) {
	l.mu.RLock()
	defer l.mu.RUnlock()

	var out []eventing.Event
	for _, event := range l.events {
		if event.AggregateType == aggregateType && event.AggregateID == aggregateID {
			out = append(out, event)
		}
	}
	return out, nil
}
