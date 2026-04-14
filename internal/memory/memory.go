package memory

type SessionStore interface {
	Append(sessionID string, lines []string) error
}

type SemanticStore interface {
	Index(namespace string, text string) error
}

type KVStore interface {
	Put(key string, value string) error
	Get(key string) (string, bool)
}
