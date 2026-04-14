package approvals

import (
	"context"
	"fmt"
	"sync"
	"time"
)

type Status string

const (
	StatusPending  Status = "pending"
	StatusApproved Status = "approved"
	StatusRejected Status = "rejected"
	StatusExpired  Status = "expired"
	StatusCanceled Status = "canceled"
)

type Action string

const (
	ActionApprove Action = "approve"
	ActionReject  Action = "reject"
)

type Request struct {
	WorkerID   string
	SessionID  string
	Payload    string
	Reason     string
	TargetType string
	TargetID   string
}

type Record struct {
	ID               string
	WorkerID         string
	SessionID        string
	Payload          string
	Status           Status
	Reason           string
	TargetType       string
	TargetID         string
	RequestedAt      time.Time
	DecidedAt        *time.Time
	DecisionUpdateID string
}

type Callback struct {
	ApprovalID string
	Action     Action
	UpdateID   string
}

type Store interface {
	SaveApproval(Record) error
	Approval(id string) (Record, bool, error)
	PendingApprovals(sessionID string) ([]Record, error)
	SaveHandledApprovalCallback(updateID string, record Record) error
	HandledApprovalCallback(updateID string) (Record, bool, error)
}

type Deps struct {
	Store Store
}

type Service struct {
	mu              sync.RWMutex
	records         map[string]Record
	handledUpdateID map[string]Record
	waiters         map[string][]chan Record
	store           Store
}

func TestDeps() Deps {
	return Deps{}
}

func New(deps Deps) *Service {
	return &Service{
		records:         map[string]Record{},
		handledUpdateID: map[string]Record{},
		waiters:         map[string][]chan Record{},
		store:           deps.Store,
	}
}

func (s *Service) Create(req Request) (Record, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	record := Record{
		ID:          fmt.Sprintf("approval-%d", time.Now().UTC().UnixNano()),
		WorkerID:    req.WorkerID,
		SessionID:   req.SessionID,
		Payload:     req.Payload,
		Status:      StatusPending,
		Reason:      req.Reason,
		TargetType:  req.TargetType,
		TargetID:    req.TargetID,
		RequestedAt: time.Now().UTC(),
	}
	s.records[record.ID] = record
	if s.store != nil {
		if err := s.store.SaveApproval(record); err != nil {
			return Record{}, err
		}
	}
	return record, nil
}

func (s *Service) HandleCallback(cb Callback) (Record, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if existing, ok := s.handledUpdateID[cb.UpdateID]; ok {
		return existing, nil
	}
	if s.store != nil {
		if existing, ok, err := s.store.HandledApprovalCallback(cb.UpdateID); err != nil {
			return Record{}, err
		} else if ok {
			return existing, nil
		}
	}

	record, ok := s.records[cb.ApprovalID]
	if !ok && s.store != nil {
		persisted, found, err := s.store.Approval(cb.ApprovalID)
		if err != nil {
			return Record{}, err
		}
		if found {
			record = persisted
			ok = true
		}
	}
	if !ok {
		return Record{}, fmt.Errorf("approval not found: %s", cb.ApprovalID)
	}

	switch cb.Action {
	case ActionApprove:
		record.Status = StatusApproved
	case ActionReject:
		record.Status = StatusRejected
	default:
		return Record{}, fmt.Errorf("unsupported action: %s", cb.Action)
	}

	now := time.Now().UTC()
	record.DecidedAt = &now
	record.DecisionUpdateID = cb.UpdateID

	s.records[record.ID] = record
	s.notifyWaitersLocked(record)
	s.handledUpdateID[cb.UpdateID] = record
	if s.store != nil {
		if err := s.store.SaveApproval(record); err != nil {
			return Record{}, err
		}
		if err := s.store.SaveHandledApprovalCallback(cb.UpdateID, record); err != nil {
			return Record{}, err
		}
	}

	return record, nil
}

func (s *Service) Wait(ctx context.Context, id string) (Record, error) {
	s.mu.Lock()
	record, ok := s.records[id]
	if !ok && s.store != nil {
		persisted, found, err := s.store.Approval(id)
		if err != nil {
			s.mu.Unlock()
			return Record{}, err
		}
		if found {
			record = persisted
			ok = true
			s.records[id] = record
		}
	}
	if !ok {
		s.mu.Unlock()
		return Record{}, fmt.Errorf("approval not found: %s", id)
	}
	if record.Status != StatusPending {
		s.mu.Unlock()
		return record, nil
	}
	ch := make(chan Record, 1)
	s.waiters[id] = append(s.waiters[id], ch)
	s.mu.Unlock()

	select {
	case <-ctx.Done():
		return Record{}, ctx.Err()
	case record := <-ch:
		return record, nil
	}
}

func (s *Service) Get(id string) (Record, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.store != nil {
		record, ok, err := s.store.Approval(id)
		if err != nil {
			return Record{}, false
		}
		return record, ok
	}
	record, ok := s.records[id]
	return record, ok
}

func (s *Service) PendingBySession(sessionID string) []Record {
	s.mu.RLock()
	defer s.mu.RUnlock()
	if s.store != nil {
		out, err := s.store.PendingApprovals(sessionID)
		if err == nil {
			return out
		}
	}
	out := make([]Record, 0)
	for _, record := range s.records {
		if record.SessionID == sessionID && record.Status == StatusPending {
			out = append(out, record)
		}
	}
	return out
}

func (s *Service) HasWaiter(id string) bool {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return len(s.waiters[id]) > 0
}

func (s *Service) notifyWaitersLocked(record Record) {
	waiters := s.waiters[record.ID]
	if len(waiters) == 0 {
		return
	}
	delete(s.waiters, record.ID)
	for _, ch := range waiters {
		ch <- record
		close(ch)
	}
}
