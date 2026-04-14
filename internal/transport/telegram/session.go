package telegram

import (
	"strings"
	"sync"

	"teamd/internal/provider"
	"teamd/internal/worker"
)

// limit counts individual provider.Message items, not user/assistant pairs.
type SessionStore struct {
	mu       sync.RWMutex
	limit    int
	stateByChat map[int64]chatSessionState
}

type chatSessionState struct {
	active      string
	messages    map[string][]provider.Message
	checkpoints map[string]worker.Checkpoint
}

func NewSessionStore(limit int) *SessionStore {
	if limit <= 0 {
		limit = 1
	}
	return &SessionStore{
		limit:       limit,
		stateByChat: map[int64]chatSessionState{},
	}
}

func (s *SessionStore) ensureChatLocked(chatID int64) chatSessionState {
	state, ok := s.stateByChat[chatID]
	if !ok {
		state = chatSessionState{
			active:      "default",
			messages:    map[string][]provider.Message{"default": nil},
			checkpoints: map[string]worker.Checkpoint{},
		}
	}
	if state.active == "" {
		state.active = "default"
	}
	if state.messages == nil {
		state.messages = map[string][]provider.Message{}
	}
	if state.checkpoints == nil {
		state.checkpoints = map[string]worker.Checkpoint{}
	}
	if _, ok := state.messages[state.active]; !ok {
		state.messages[state.active] = nil
	}
	return state
}

func (s *SessionStore) ensureChat(chatID int64) chatSessionState {
	state, ok := s.stateByChat[chatID]
	if !ok {
		return chatSessionState{
			active:      "default",
			messages:    map[string][]provider.Message{"default": nil},
			checkpoints: map[string]worker.Checkpoint{},
		}
	}
	if state.active == "" {
		state.active = "default"
	}
	if state.messages == nil {
		state.messages = map[string][]provider.Message{"default": nil}
	}
	if state.checkpoints == nil {
		state.checkpoints = map[string]worker.Checkpoint{}
	}
	if _, ok := state.messages[state.active]; !ok {
		state.messages[state.active] = nil
	}
	return state
}

func normalizeSessionName(session string) string {
	return strings.TrimSpace(strings.ToLower(session))
}
