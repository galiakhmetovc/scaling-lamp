package runtime

import (
	"context"
	"fmt"
	"sync"
	"time"
)

type TimeoutDecisions struct {
	mu      sync.RWMutex
	records map[string]TimeoutDecisionRecord
	waiters map[string][]chan TimeoutDecisionRecord
	store   TimeoutDecisionStore
}

func NewTimeoutDecisions(store TimeoutDecisionStore) *TimeoutDecisions {
	return &TimeoutDecisions{
		records: map[string]TimeoutDecisionRecord{},
		waiters: map[string][]chan TimeoutDecisionRecord{},
		store:   store,
	}
}

func (s *TimeoutDecisions) CreateOrUpdatePending(runID string, chatID int64, sessionID string, roundIndex int, autoUsed bool, autoDeadline time.Time) (TimeoutDecisionRecord, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	record := TimeoutDecisionRecord{
		RunID:                runID,
		ChatID:               chatID,
		SessionID:            sessionID,
		Status:               TimeoutDecisionPending,
		RequestedAt:          time.Now().UTC(),
		AutoContinueDeadline: &autoDeadline,
		AutoContinueUsed:     autoUsed,
		RoundIndex:           roundIndex,
	}
	s.records[runID] = record
	if s.store != nil {
		if err := s.store.SaveTimeoutDecision(record); err != nil {
			return TimeoutDecisionRecord{}, err
		}
	}
	return record, nil
}

func (s *TimeoutDecisions) Resolve(runID string, action TimeoutDecisionAction, failureReason string) (TimeoutDecisionRecord, bool, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	record, ok := s.records[runID]
	if !ok && s.store != nil {
		persisted, found, err := s.store.TimeoutDecision(runID)
		if err != nil {
			return TimeoutDecisionRecord{}, false, err
		}
		if found {
			record = persisted
			ok = true
			s.records[runID] = record
		}
	}
	if !ok {
		return TimeoutDecisionRecord{}, false, fmt.Errorf("timeout decision not found for run %s", runID)
	}
	now := time.Now().UTC()
	record.ResolvedAt = &now
	record.FailureReason = failureReason
	switch action {
	case TimeoutDecisionActionContinue:
		record.Status = TimeoutDecisionContinued
		record.AutoContinueUsed = true
	case TimeoutDecisionActionRetry:
		record.Status = TimeoutDecisionRetried
	case TimeoutDecisionActionCancel:
		record.Status = TimeoutDecisionCancelled
	case TimeoutDecisionActionFail:
		record.Status = TimeoutDecisionFailed
	default:
		return TimeoutDecisionRecord{}, false, fmt.Errorf("unsupported timeout decision action: %s", action)
	}
	s.records[runID] = record
	if s.store != nil {
		if err := s.store.SaveTimeoutDecision(record); err != nil {
			return TimeoutDecisionRecord{}, false, err
		}
	}
	for _, ch := range s.waiters[runID] {
		ch <- record
		close(ch)
	}
	delete(s.waiters, runID)
	return record, true, nil
}

func (s *TimeoutDecisions) Wait(ctx context.Context, runID string) (TimeoutDecisionRecord, error) {
	s.mu.Lock()
	record, ok := s.records[runID]
	if !ok && s.store != nil {
		persisted, found, err := s.store.TimeoutDecision(runID)
		if err != nil {
			s.mu.Unlock()
			return TimeoutDecisionRecord{}, err
		}
		if found {
			record = persisted
			ok = true
			s.records[runID] = record
		}
	}
	if !ok {
		s.mu.Unlock()
		return TimeoutDecisionRecord{}, fmt.Errorf("timeout decision not found for run %s", runID)
	}
	if record.Status != TimeoutDecisionPending {
		s.mu.Unlock()
		return record, nil
	}
	ch := make(chan TimeoutDecisionRecord, 1)
	s.waiters[runID] = append(s.waiters[runID], ch)
	s.mu.Unlock()

	select {
	case <-ctx.Done():
		return TimeoutDecisionRecord{}, ctx.Err()
	case record := <-ch:
		return record, nil
	}
}

func (s *TimeoutDecisions) Get(runID string) (TimeoutDecisionRecord, bool, error) {
	s.mu.RLock()
	record, ok := s.records[runID]
	s.mu.RUnlock()
	if ok {
		return record, true, nil
	}
	if s.store == nil {
		return TimeoutDecisionRecord{}, false, nil
	}
	return s.store.TimeoutDecision(runID)
}
