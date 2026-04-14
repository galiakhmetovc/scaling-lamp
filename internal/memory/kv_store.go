package memory

type InMemoryKVStore struct {
	data map[string]string
}

func NewInMemoryKVStore() *InMemoryKVStore {
	return &InMemoryKVStore{data: map[string]string{}}
}

func (s *InMemoryKVStore) Put(key string, value string) error {
	s.data[key] = value
	return nil
}

func (s *InMemoryKVStore) Get(key string) (string, bool) {
	value, ok := s.data[key]
	return value, ok
}
