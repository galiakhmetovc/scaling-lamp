package telegram

import (
	"fmt"
	"sort"
)

func (s *SessionStore) ActiveSession(chatID int64) (string, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.ensureChat(chatID).active, nil
}

func (s *SessionStore) CreateSession(chatID int64, session string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	session = normalizeSessionName(session)
	if session == "" {
		return fmt.Errorf("session name is required")
	}

	state := s.ensureChatLocked(chatID)
	if _, ok := state.messages[session]; !ok {
		state.messages[session] = nil
	}
	s.stateByChat[chatID] = state
	return nil
}

func (s *SessionStore) UseSession(chatID int64, session string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	session = normalizeSessionName(session)
	if session == "" {
		return fmt.Errorf("session name is required")
	}

	state := s.ensureChatLocked(chatID)
	if _, ok := state.messages[session]; !ok {
		return fmt.Errorf("session %q not found", session)
	}
	state.active = session
	s.stateByChat[chatID] = state
	return nil
}

func (s *SessionStore) ListSessions(chatID int64) ([]string, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	state := s.ensureChat(chatID)
	out := make([]string, 0, len(state.messages))
	for name := range state.messages {
		out = append(out, name)
	}
	sort.Strings(out)
	return out, nil
}
