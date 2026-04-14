package skills

import (
	"sort"
	"sync"
)

type SessionState struct {
	mu     sync.RWMutex
	active map[string]map[string]struct{}
}

func NewSessionState() *SessionState {
	return &SessionState{active: map[string]map[string]struct{}{}}
}

func (s *SessionState) Active(sessionKey string) []string {
	s.mu.RLock()
	defer s.mu.RUnlock()
	set := s.active[sessionKey]
	out := make([]string, 0, len(set))
	for skill := range set {
		out = append(out, skill)
	}
	sort.Strings(out)
	return out
}

func (s *SessionState) Activate(sessionKey, skill string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.active[sessionKey] == nil {
		s.active[sessionKey] = map[string]struct{}{}
	}
	s.active[sessionKey][skill] = struct{}{}
}

func (s *SessionState) Deactivate(sessionKey, skill string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.active[sessionKey] == nil {
		return
	}
	delete(s.active[sessionKey], skill)
	if len(s.active[sessionKey]) == 0 {
		delete(s.active, sessionKey)
	}
}

func (s *SessionState) Reset(sessionKey string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.active, sessionKey)
}
