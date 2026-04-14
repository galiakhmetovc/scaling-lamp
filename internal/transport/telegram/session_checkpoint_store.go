package telegram

import "teamd/internal/worker"

func (s *SessionStore) Checkpoint(chatID int64) (worker.Checkpoint, bool, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	state := s.ensureChat(chatID)
	checkpoint, ok := state.checkpoints[state.active]
	return checkpoint, ok, nil
}

func (s *SessionStore) SaveCheckpoint(chatID int64, checkpoint worker.Checkpoint) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	state := s.ensureChatLocked(chatID)
	checkpoint = sanitizeCheckpoint(checkpoint)
	state.checkpoints[state.active] = checkpoint
	s.stateByChat[chatID] = state
	return nil
}
