package runtime

import (
	"testing"

	"teamd/internal/provider"
)

type sessionActionTestStore struct {
	active   string
	sessions []string
	messages []provider.Message
	reset    bool
	used     string
	created  string
}

func (s *sessionActionTestStore) ActiveSession(chatID int64) (string, error) { return s.active, nil }
func (s *sessionActionTestStore) CreateSession(chatID int64, session string) error {
	s.created = session
	found := false
	for _, item := range s.sessions {
		if item == session {
			found = true
			break
		}
	}
	if !found {
		s.sessions = append(s.sessions, session)
	}
	return nil
}
func (s *sessionActionTestStore) UseSession(chatID int64, session string) error {
	s.used = session
	s.active = session
	return nil
}
func (s *sessionActionTestStore) ListSessions(chatID int64) ([]string, error) {
	return append([]string(nil), s.sessions...), nil
}
func (s *sessionActionTestStore) Reset(chatID int64) error {
	s.reset = true
	s.messages = nil
	return nil
}
func (s *sessionActionTestStore) Messages(chatID int64) ([]provider.Message, error) {
	return append([]provider.Message(nil), s.messages...), nil
}

func TestSessionActionsExecuteCreateAndUse(t *testing.T) {
	store := &sessionActionTestStore{active: "default", sessions: []string{"default"}}
	svc := NewSessionActions(store)

	result, err := svc.Execute(1001, SessionActionRequest{Action: SessionActionCreate, SessionName: "deploy"})
	if err != nil {
		t.Fatalf("execute create: %v", err)
	}
	if result.ActiveSession != "deploy" || store.created != "deploy" || store.used != "deploy" {
		t.Fatalf("unexpected create result: %+v store=%+v", result, store)
	}
}

func TestSessionActionsExecuteStatsAndReset(t *testing.T) {
	store := &sessionActionTestStore{
		active:   "default",
		sessions: []string{"default", "deploy"},
		messages: []provider.Message{{Role: "user", Content: "hello"}, {Role: "assistant", Content: "hi"}},
	}
	svc := NewSessionActions(store)

	stats, err := svc.Execute(1001, SessionActionRequest{Action: SessionActionStats})
	if err != nil {
		t.Fatalf("stats: %v", err)
	}
	if stats.ActiveSession != "default" || stats.MessageCount != 2 {
		t.Fatalf("unexpected stats: %+v", stats)
	}

	reset, err := svc.Execute(1001, SessionActionRequest{Action: SessionActionReset})
	if err != nil {
		t.Fatalf("reset: %v", err)
	}
	if !store.reset || reset.MessageCount != 0 {
		t.Fatalf("unexpected reset result: %+v store=%+v", reset, store)
	}
}
