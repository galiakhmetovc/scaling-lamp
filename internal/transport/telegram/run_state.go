package telegram

import (
	"fmt"
	"sync"
	"time"
)

type RunStep struct {
	Title   string
	Detail  string
	Elapsed time.Duration
	Icon    string
}

type TraceEntry struct {
	Section string
	Summary string
	Payload string
}

type RunState struct {
	ID               string
	ChatID           int64
	Query            string
	Stage            string
	StartedAt        time.Time
	LastProgressAt   time.Time
	AckMessageID     int64
	StatusMessageID  int64
	CurrentTool      string
	WaitingOn        string
	RoundIndex       int
	Steps            []RunStep
	CancelRequested  bool
	Completed        bool
	Failed           bool
	FailureText      string
	PromptTokens     int
	CompletionTokens int
	ContextEstimate  int
	ContextPercent   int
	ContextPercentDelta int
	PromptBudgetPercent int
	PromptBudgetPercentDelta int
	SystemOverheadTokens int
	ToolCalls        int
	ToolOutputChars  int
	ToolDuration     time.Duration
	ToolCallsDelta      int
	ToolOutputCharsDelta int
	ToolDurationDelta    time.Duration
	Trace            []TraceEntry
	LastStatusSyncAt time.Time
	StatusRetryAfterUntil time.Time
}

func (r *RunState) Elapsed(now time.Time) time.Duration {
	if r == nil || r.StartedAt.IsZero() {
		return 0
	}
	return now.Sub(r.StartedAt)
}

type RunStateStore struct {
	mu     sync.RWMutex
	nextID int64
	active map[int64]*RunState
}

func NewRunStateStore() *RunStateStore {
	return &RunStateStore{active: map[int64]*RunState{}}
}

func (s *RunStateStore) AllocateID() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.nextID++
	return fmt.Sprintf("run-%d", s.nextID)
}

func (s *RunStateStore) CreateWithID(chatID int64, runID, query string, startedAt time.Time) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.active[chatID] = &RunState{
		ID:             runID,
		ChatID:         chatID,
		Query:          query,
		Stage:          "Запускаю агента",
		StartedAt:      startedAt.UTC(),
		LastProgressAt: startedAt.UTC(),
	}
}

func (s *RunStateStore) Active(chatID int64) (*RunState, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	run, ok := s.active[chatID]
	if !ok {
		return nil, false
	}
	copy := *run
	copy.Steps = append([]RunStep(nil), run.Steps...)
	copy.Trace = append([]TraceEntry(nil), run.Trace...)
	return &copy, true
}

func (s *RunStateStore) Update(chatID int64, fn func(*RunState)) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if run, ok := s.active[chatID]; ok {
		fn(run)
	}
}

func (s *RunStateStore) Running(chatID int64) bool {
	s.mu.RLock()
	defer s.mu.RUnlock()
	run, ok := s.active[chatID]
	return ok && !run.Completed && !run.Failed
}

func (s *RunStateStore) Finish(chatID int64) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.active, chatID)
}
