package telegram

import "teamd/internal/provider"

func (s *SessionStore) Append(chatID int64, msg provider.Message) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	msg = sanitizeMessage(msg)
	state := s.ensureChatLocked(chatID)
	history := append(state.messages[state.active], msg)
	history = trimHistory(history, s.limit)
	state.messages[state.active] = history
	s.stateByChat[chatID] = state
	return nil
}

func (s *SessionStore) Messages(chatID int64) ([]provider.Message, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	state := s.ensureChat(chatID)
	history := state.messages[state.active]
	out := make([]provider.Message, len(history))
	copy(out, history)
	return out, nil
}

func (s *SessionStore) Reset(chatID int64) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	state := s.ensureChatLocked(chatID)
	delete(state.messages, state.active)
	state.messages[state.active] = nil
	delete(state.checkpoints, state.active)
	s.stateByChat[chatID] = state
	return nil
}
