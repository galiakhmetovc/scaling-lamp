package runtime

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"teamd/internal/runtime/eventing"
)

const maxEventLogRecordSize = 8 * 1024 * 1024
const (
	defaultEventLogRotateMaxBytes int64 = 128 * 1024 * 1024
	defaultEventLogRotateKeep           = 4
)

type eventJSONRecord struct {
	Timestamp string `json:"timestamp"`
	eventing.Event
}

type EventLog interface {
	Append(ctx context.Context, event eventing.Event) error
	ListAll(ctx context.Context) ([]eventing.Event, error)
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
	mu         sync.RWMutex
	path       string
	next       uint64
	rotateMax  int64
	rotateKeep int
}

type FileEventLogOptions struct {
	RotateMaxBytes int64
	RotateKeep     int
}

func NewFileEventLog(path string, opts ...FileEventLogOptions) (*FileEventLog, error) {
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

	options := FileEventLogOptions{
		RotateMaxBytes: defaultEventLogRotateMaxBytes,
		RotateKeep:     defaultEventLogRotateKeep,
	}
	if len(opts) > 0 {
		if opts[0].RotateMaxBytes > 0 {
			options.RotateMaxBytes = opts[0].RotateMaxBytes
		}
		if opts[0].RotateKeep > 0 {
			options.RotateKeep = opts[0].RotateKeep
		}
	}
	log := &FileEventLog{
		path:       path,
		rotateMax:  options.RotateMaxBytes,
		rotateKeep: options.RotateKeep,
	}
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

func (l *InMemoryEventLog) ListAll(_ context.Context) ([]eventing.Event, error) {
	l.mu.RLock()
	defer l.mu.RUnlock()

	out := make([]eventing.Event, len(l.events))
	copy(out, l.events)
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
	if err := l.rotateIfNeededLocked(); err != nil {
		return err
	}

	file, err := os.OpenFile(l.path, os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return fmt.Errorf("open event log for append: %w", err)
	}
	defer file.Close()

	encoded, err := json.Marshal(eventJSONRecord{
		Timestamp: event.OccurredAt.Format(time.RFC3339Nano),
		Event:     event,
	})
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

	events, err := l.readAllEventsLocked()
	if err != nil {
		return nil, err
	}
	var out []eventing.Event
	for _, event := range events {
		if event.AggregateType == aggregateType && event.AggregateID == aggregateID {
			out = append(out, event)
		}
	}
	return out, nil
}

func (l *FileEventLog) ListAll(_ context.Context) ([]eventing.Event, error) {
	l.mu.RLock()
	defer l.mu.RUnlock()

	return l.readAllEventsLocked()
}

func (l *FileEventLog) loadSequence() error {
	events, err := l.readAllEventsLocked()
	if err != nil {
		return err
	}
	for _, event := range events {
		if event.Sequence > l.next {
			l.next = event.Sequence
		}
	}
	return nil
}

func (l *FileEventLog) readAllEventsLocked() ([]eventing.Event, error) {
	var out []eventing.Event
	paths, err := l.readPathsLocked()
	if err != nil {
		return nil, err
	}
	for _, path := range paths {
		events, err := readEventsFromPath(path)
		if err != nil {
			return nil, err
		}
		out = append(out, events...)
	}
	return out, nil
}

func (l *FileEventLog) readPathsLocked() ([]string, error) {
	archivePaths, err := filepath.Glob(l.path + ".*")
	if err != nil {
		return nil, fmt.Errorf("glob event log archives: %w", err)
	}
	paths := make([]string, 0, len(archivePaths)+1)
	for _, path := range archivePaths {
		if strings.HasPrefix(filepath.Base(path), filepath.Base(l.path)+".") {
			paths = append(paths, path)
		}
	}
	sort.Strings(paths)
	paths = append(paths, l.path)
	return paths, nil
}

func readEventsFromPath(path string) ([]eventing.Event, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("open event log for read: %w", err)
	}
	defer file.Close()

	var out []eventing.Event
	scanner := bufio.NewScanner(file)
	scanner.Buffer(make([]byte, 64*1024), maxEventLogRecordSize)
	for scanner.Scan() {
		var event eventing.Event
		if err := json.Unmarshal(scanner.Bytes(), &event); err != nil {
			return nil, fmt.Errorf("decode event log line: %w", err)
		}
		out = append(out, event)
	}
	if err := scanner.Err(); err != nil {
		return nil, fmt.Errorf("scan event log: %w", err)
	}
	return out, nil
}

func (l *FileEventLog) rotateIfNeededLocked() error {
	if l.rotateMax <= 0 {
		return nil
	}
	info, err := os.Stat(l.path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return fmt.Errorf("stat event log: %w", err)
	}
	if info.Size() < l.rotateMax {
		return nil
	}
	archivePath := fmt.Sprintf("%s.%s", l.path, time.Now().UTC().Format("20060102T150405.000000000"))
	if err := os.Rename(l.path, archivePath); err != nil {
		return fmt.Errorf("rotate event log: %w", err)
	}
	file, err := os.OpenFile(l.path, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o644)
	if err != nil {
		return fmt.Errorf("create fresh event log after rotation: %w", err)
	}
	if err := file.Close(); err != nil {
		return fmt.Errorf("close fresh event log after rotation: %w", err)
	}
	if err := l.pruneArchivesLocked(); err != nil {
		return err
	}
	return nil
}

func (l *FileEventLog) pruneArchivesLocked() error {
	if l.rotateKeep <= 0 {
		return nil
	}
	archivePaths, err := filepath.Glob(l.path + ".*")
	if err != nil {
		return fmt.Errorf("glob event log archives for prune: %w", err)
	}
	filtered := make([]string, 0, len(archivePaths))
	for _, path := range archivePaths {
		if strings.HasPrefix(filepath.Base(path), filepath.Base(l.path)+".") {
			filtered = append(filtered, path)
		}
	}
	sort.Strings(filtered)
	for len(filtered) > l.rotateKeep {
		if err := os.Remove(filtered[0]); err != nil && !os.IsNotExist(err) {
			return fmt.Errorf("remove rotated event log %q: %w", filtered[0], err)
		}
		filtered = filtered[1:]
	}
	return nil
}
