package runtime

import "sync"

type UIEventKind string

const (
	UIEventSessionChanged UIEventKind = "session.changed"
	UIEventStreamText     UIEventKind = "stream.text"
	UIEventToolStarted    UIEventKind = "tool.started"
	UIEventToolCompleted  UIEventKind = "tool.completed"
	UIEventStatusChanged  UIEventKind = "status.changed"
	UIEventRunCompleted   UIEventKind = "run.completed"
)

type UIEvent struct {
	Kind      UIEventKind   `json:"kind"`
	SessionID string        `json:"session_id"`
	RunID     string        `json:"run_id"`
	Text      string        `json:"text"`
	Status    string        `json:"status"`
	Tool      ToolActivity  `json:"tool,omitempty"`
}

type UIEventBus struct {
	mu          sync.RWMutex
	nextID      int
	subscribers map[int]chan UIEvent
}

func NewUIEventBus() *UIEventBus {
	return &UIEventBus{subscribers: map[int]chan UIEvent{}}
}

func (b *UIEventBus) Subscribe(buffer int) (int, <-chan UIEvent) {
	if buffer <= 0 {
		buffer = 64
	}
	b.mu.Lock()
	defer b.mu.Unlock()
	id := b.nextID
	b.nextID++
	ch := make(chan UIEvent, buffer)
	b.subscribers[id] = ch
	return id, ch
}

func (b *UIEventBus) Unsubscribe(id int) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if ch, ok := b.subscribers[id]; ok {
		delete(b.subscribers, id)
		close(ch)
	}
}

func (b *UIEventBus) Publish(event UIEvent) {
	b.mu.RLock()
	defer b.mu.RUnlock()
	for _, ch := range b.subscribers {
		select {
		case ch <- event:
		default:
		}
	}
}
