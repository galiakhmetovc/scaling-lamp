package runtime

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"

	"teamd/internal/runtime/eventing"
)

type EventLog interface {
	Append(ctx context.Context, event eventing.Event) error
	ListByAggregate(ctx context.Context, aggregateType eventing.AggregateType, aggregateID string) ([]eventing.Event, error)
}

type InMemoryEventLog struct {
	mu     sync.RWMutex
	next   uint64
	events []eventing.Event
}

func NewInMemoryEventLog() *InMemoryEventLog {
	return &InMemoryEventLog{}
}

type FileEventLog struct {
	mu   sync.RWMutex
	path string
	next uint64
}

func NewFileEventLog(path string) (*FileEventLog, error) {
	if path == "" {
		return nil, fmt.Errorf("file event log path is empty")
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, fmt.Errorf("mkdir event log dir: %w", err)
	}
	file, err := os.OpenFile(path, os.O_CREATE, 0o644)
	if err != nil {
		return nil, fmt.Errorf("open event log file: %w", err)
	}
	if err := file.Close(); err != nil {
		return nil, fmt.Errorf("close event log file: %w", err)
	}

	log := &FileEventLog{path: path}
	if err := log.loadSequence(); err != nil {
		return nil, err
	}
	return log, nil
}

func (l *InMemoryEventLog) Append(_ context.Context, event eventing.Event) error {
	l.mu.Lock()
	defer l.mu.Unlock()

	l.next++
	if event.Sequence == 0 {
		event.Sequence = l.next
	}
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

func (l *FileEventLog) Append(_ context.Context, event eventing.Event) error {
	l.mu.Lock()
	defer l.mu.Unlock()

	l.next++
	if event.Sequence == 0 {
		event.Sequence = l.next
	} else if event.Sequence > l.next {
		l.next = event.Sequence
	}

	file, err := os.OpenFile(l.path, os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return fmt.Errorf("open event log for append: %w", err)
	}
	defer file.Close()

	encoded, err := json.Marshal(event)
	if err != nil {
		return fmt.Errorf("marshal event: %w", err)
	}
	if _, err := file.Write(append(encoded, '\n')); err != nil {
		return fmt.Errorf("append event: %w", err)
	}
	return nil
}

func (l *FileEventLog) ListByAggregate(_ context.Context, aggregateType eventing.AggregateType, aggregateID string) ([]eventing.Event, error) {
	l.mu.RLock()
	defer l.mu.RUnlock()

	file, err := os.Open(l.path)
	if err != nil {
		return nil, fmt.Errorf("open event log for read: %w", err)
	}
	defer file.Close()

	var out []eventing.Event
	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		var event eventing.Event
		if err := json.Unmarshal(scanner.Bytes(), &event); err != nil {
			return nil, fmt.Errorf("decode event log line: %w", err)
		}
		if event.AggregateType == aggregateType && event.AggregateID == aggregateID {
			out = append(out, event)
		}
	}
	if err := scanner.Err(); err != nil {
		return nil, fmt.Errorf("scan event log: %w", err)
	}
	return out, nil
}

func (l *FileEventLog) loadSequence() error {
	file, err := os.Open(l.path)
	if err != nil {
		return fmt.Errorf("open event log for sequence load: %w", err)
	}
	defer file.Close()

	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		var event eventing.Event
		if err := json.Unmarshal(scanner.Bytes(), &event); err != nil {
			return fmt.Errorf("decode event log line: %w", err)
		}
		if event.Sequence > l.next {
			l.next = event.Sequence
		}
	}
	if err := scanner.Err(); err != nil {
		return fmt.Errorf("scan event log for sequence load: %w", err)
	}
	return nil
}
