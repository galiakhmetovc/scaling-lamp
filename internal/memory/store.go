package memory

import "time"

type Scope string

const (
	ScopeSession Scope = "session"
	ScopeChat    Scope = "chat"
	ScopeGlobal  Scope = "global"
)

type Document struct {
	DocKey     string
	Scope      Scope
	ChatID     int64
	SessionID  string
	Kind       string
	Title      string
	Body       string
	Source     string
	UpdatedAt  time.Time
}

type RecallQuery struct {
	ChatID    int64
	SessionID string
	Text      string
	Limit     int
	Kinds     []string
}

type RecallItem struct {
	DocKey string
	Kind  string
	Title string
	Body  string
	Score float64
}

type Store interface {
	UpsertDocument(Document) error
	Search(RecallQuery) ([]RecallItem, error)
	Get(docKey string) (Document, bool, error)
}
