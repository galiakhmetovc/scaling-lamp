package memory

type InMemorySessionStore struct {
	data map[string][]string
}

func NewInMemorySessionStore() *InMemorySessionStore {
	return &InMemorySessionStore{data: map[string][]string{}}
}

func (s *InMemorySessionStore) Append(sessionID string, lines []string) error {
	s.data[sessionID] = append(s.data[sessionID], lines...)
	return nil
}
