package runtime

import (
	"context"
	"sync"
	"time"
)

type ActiveRun struct {
	RunID          string
	ChatID         int64
	SessionID      string
	Query          string
	StartedAt      time.Time
	PolicySnapshot PolicySnapshot
	cancel         context.CancelFunc
}

type ActiveRegistry struct {
	mu   sync.RWMutex
	runs map[int64]ActiveRun
}

func NewActiveRegistry() *ActiveRegistry {
	return &ActiveRegistry{runs: map[int64]ActiveRun{}}
}

func (r *ActiveRegistry) TryStartRun(runID string, chatID int64, sessionID, query string, startedAt time.Time, cancel context.CancelFunc) bool {
	return r.TryStart(ActiveRun{
		RunID:     runID,
		ChatID:    chatID,
		SessionID: sessionID,
		Query:     query,
		StartedAt: startedAt,
		cancel:    cancel,
	})
}

func (r *ActiveRegistry) TryStart(run ActiveRun) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	if _, exists := r.runs[run.ChatID]; exists {
		return false
	}
	r.runs[run.ChatID] = run
	return true
}

func (r *ActiveRegistry) Active(chatID int64) (ActiveRun, bool) {
	r.mu.RLock()
	defer r.mu.RUnlock()
	run, ok := r.runs[chatID]
	return run, ok
}

func (r *ActiveRegistry) Cancel(chatID int64) bool {
	r.mu.RLock()
	run, ok := r.runs[chatID]
	r.mu.RUnlock()
	if !ok {
		return false
	}
	if run.cancel != nil {
		run.cancel()
	}
	return true
}

func (r *ActiveRegistry) Finish(chatID int64) {
	r.mu.Lock()
	defer r.mu.Unlock()
	delete(r.runs, chatID)
}
