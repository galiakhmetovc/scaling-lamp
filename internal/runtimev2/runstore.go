package runtimev2

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"sync"
)

var (
	errRunSnapshotNotFound = errors.New("run snapshot not found")
	errRunSnapshotExists   = errors.New("run snapshot already exists")
)

type RunStore struct {
	mu   sync.RWMutex
	runs map[string]RunSnapshotV2
}

func NewRunStore() *RunStore {
	return &RunStore{
		runs: make(map[string]RunSnapshotV2),
	}
}

func (s *RunStore) Create(snapshot RunSnapshotV2) error {
	if strings.TrimSpace(snapshot.RunID) == "" {
		return fmt.Errorf("run id is required")
	}
	if strings.TrimSpace(snapshot.SessionID) == "" {
		return fmt.Errorf("session id is required")
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	if _, exists := s.runs[snapshot.RunID]; exists {
		return errRunSnapshotExists
	}
	s.runs[snapshot.RunID] = cloneRunSnapshotV2(snapshot)
	return nil
}

func (s *RunStore) Get(runID string) (RunSnapshotV2, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	snapshot, ok := s.runs[runID]
	if !ok {
		return RunSnapshotV2{}, false
	}
	return cloneRunSnapshotV2(snapshot), true
}

func (s *RunStore) Update(runID string, fn func(*RunSnapshotV2) error) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	snapshot, ok := s.runs[runID]
	if !ok {
		return errRunSnapshotNotFound
	}

	updated := cloneRunSnapshotV2(snapshot)
	if err := fn(&updated); err != nil {
		return err
	}
	s.runs[runID] = cloneRunSnapshotV2(updated)
	return nil
}

func (s *RunStore) Delete(runID string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if _, ok := s.runs[runID]; !ok {
		return errRunSnapshotNotFound
	}
	delete(s.runs, runID)
	return nil
}

func (s *RunStore) ListActiveBySession(sessionID string) []RunSnapshotV2 {
	s.mu.RLock()
	defer s.mu.RUnlock()

	active := make([]RunSnapshotV2, 0)
	for _, snapshot := range s.runs {
		if snapshot.SessionID != sessionID || isTerminalStatus(snapshot.Status) {
			continue
		}
		active = append(active, cloneRunSnapshotV2(snapshot))
	}

	sort.Slice(active, func(i, j int) bool {
		return active[i].RunID < active[j].RunID
	})
	return active
}

func cloneRunSnapshotV2(snapshot RunSnapshotV2) RunSnapshotV2 {
	cloned := snapshot
	cloned.QueuedUserMessages = append([]QueuedUserMessageV2(nil), snapshot.QueuedUserMessages...)
	cloned.PendingApprovals = append([]PendingApprovalV2(nil), snapshot.PendingApprovals...)
	cloned.ActiveProcesses = append([]ActiveProcessV2(nil), snapshot.ActiveProcesses...)
	cloned.RecentSteps = append([]RecentStepV2(nil), snapshot.RecentSteps...)
	if snapshot.ProviderStream != nil {
		stream := *snapshot.ProviderStream
		cloned.ProviderStream = &stream
	}
	if snapshot.Result != nil {
		result := *snapshot.Result
		cloned.Result = &result
	}
	return cloned
}

func isTerminalStatus(status RunStatusV2) bool {
	switch status {
	case RunStatusCompleted, RunStatusFailed, RunStatusCancelled:
		return true
	default:
		return false
	}
}
