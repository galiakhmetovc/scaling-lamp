package daemon

import (
	"context"
	"sync"

	"teamd/internal/runtime"
)

type sessionRuntimeState struct {
	active bool
	queue  []QueuedDraft
}

type daemonBus struct {
	mu          sync.RWMutex
	nextID      int
	subscribers map[int]chan WebsocketEnvelope
}

func newDaemonBus() *daemonBus {
	return &daemonBus{subscribers: map[int]chan WebsocketEnvelope{}}
}

func (b *daemonBus) Subscribe(buffer int) (int, <-chan WebsocketEnvelope) {
	if buffer <= 0 {
		buffer = 64
	}
	b.mu.Lock()
	defer b.mu.Unlock()
	id := b.nextID
	b.nextID++
	ch := make(chan WebsocketEnvelope, buffer)
	b.subscribers[id] = ch
	return id, ch
}

func (b *daemonBus) Unsubscribe(id int) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if ch, ok := b.subscribers[id]; ok {
		delete(b.subscribers, id)
		close(ch)
	}
}

func (b *daemonBus) Publish(event WebsocketEnvelope) {
	b.mu.RLock()
	defer b.mu.RUnlock()
	for _, ch := range b.subscribers {
		select {
		case ch <- event:
		default:
		}
	}
}

func (s *Server) ensureSessionRuntimeLocked(sessionID string) *sessionRuntimeState {
	if s.sessionRuntime == nil {
		s.sessionRuntime = map[string]*sessionRuntimeState{}
	}
	state, ok := s.sessionRuntime[sessionID]
	if !ok {
		state = &sessionRuntimeState{}
		s.sessionRuntime[sessionID] = state
	}
	return state
}

func (s *Server) mainRunActive(sessionID string) bool {
	s.runtimeMu.RLock()
	defer s.runtimeMu.RUnlock()
	state, ok := s.sessionRuntime[sessionID]
	return ok && state.active
}

func (s *Server) queuedDrafts(sessionID string) []QueuedDraft {
	s.runtimeMu.RLock()
	defer s.runtimeMu.RUnlock()
	state, ok := s.sessionRuntime[sessionID]
	if !ok || len(state.queue) == 0 {
		return nil
	}
	out := make([]QueuedDraft, len(state.queue))
	copy(out, state.queue)
	return out
}

func (s *Server) enqueueDraft(sessionID, text string) QueuedDraft {
	s.runtimeMu.Lock()
	defer s.runtimeMu.Unlock()
	state := s.ensureSessionRuntimeLocked(sessionID)
	agent := s.currentAgent()
	draft := QueuedDraft{ID: agent.NewID("draft"), Text: text, QueuedAt: agent.Now().UTC()}
	state.queue = append(state.queue, draft)
	return draft
}

func (s *Server) recallDraft(sessionID, draftID string) (QueuedDraft, bool) {
	s.runtimeMu.Lock()
	defer s.runtimeMu.Unlock()
	state, ok := s.sessionRuntime[sessionID]
	if !ok {
		return QueuedDraft{}, false
	}
	for idx, draft := range state.queue {
		if draft.ID != draftID {
			continue
		}
		state.queue = append(state.queue[:idx], state.queue[idx+1:]...)
		return draft, true
	}
	return QueuedDraft{}, false
}

func (s *Server) startMainRun(sessionID string) bool {
	s.runtimeMu.Lock()
	defer s.runtimeMu.Unlock()
	state := s.ensureSessionRuntimeLocked(sessionID)
	if state.active {
		return false
	}
	state.active = true
	return true
}

func (s *Server) finishMainRun(sessionID string) {
	s.runtimeMu.Lock()
	defer s.runtimeMu.Unlock()
	s.ensureSessionRuntimeLocked(sessionID).active = false
}

func (s *Server) popNextQueuedDraft(sessionID string) (QueuedDraft, bool) {
	s.runtimeMu.Lock()
	defer s.runtimeMu.Unlock()
	state, ok := s.sessionRuntime[sessionID]
	if !ok || len(state.queue) == 0 {
		return QueuedDraft{}, false
	}
	draft := state.queue[0]
	state.queue = state.queue[1:]
	return draft, true
}

func (s *Server) maybeDispatchQueuedDrafts(sessionID string) {
	if !s.startMainRun(sessionID) {
		return
	}
	draft, ok := s.popNextQueuedDraft(sessionID)
	if !ok {
		s.finishMainRun(sessionID)
		return
	}
	go s.dispatchQueuedDraft(context.Background(), sessionID, draft)
}

func (s *Server) dispatchQueuedDraft(ctx context.Context, sessionID string, draft QueuedDraft) {
	s.publishDaemon(WebsocketEnvelope{Type: "queue_draft_started", Payload: map[string]any{"session_id": sessionID, "draft": draft}})
	agent := s.currentAgent()
	session, err := agent.ResumeChatSession(ctx, sessionID)
	if err != nil {
		s.finishMainRun(sessionID)
		s.publishDaemon(WebsocketEnvelope{Type: "queue_draft_failed", Payload: map[string]any{"session_id": sessionID, "draft": draft}, Error: err.Error()})
		return
	}
	result, err := agent.ChatTurn(ctx, session, runtime.ChatTurnInput{Prompt: draft.Text})
	s.finishMainRun(sessionID)
	if err != nil {
		s.publishDaemon(WebsocketEnvelope{Type: "queue_draft_failed", Payload: map[string]any{"session_id": sessionID, "draft": draft}, Error: err.Error()})
		s.maybeDispatchQueuedDrafts(sessionID)
		return
	}
	snapshot, snapshotErr := s.buildSessionSnapshot(sessionID)
	if snapshotErr != nil {
		s.publishDaemon(WebsocketEnvelope{Type: "queue_draft_failed", Payload: map[string]any{"session_id": sessionID, "draft": draft}, Error: snapshotErr.Error()})
		s.maybeDispatchQueuedDrafts(sessionID)
		return
	}
	s.publishDaemon(WebsocketEnvelope{
		Type: "queue_draft_completed",
		Payload: map[string]any{
			"session": snapshot,
			"draft":   draft,
			"result": providerResultPayload{
				Provider:     s.providerLabel(),
				Model:        result.Provider.Model,
				InputTokens:  result.Provider.Usage.InputTokens,
				OutputTokens: result.Provider.Usage.OutputTokens,
				TotalTokens:  result.Provider.Usage.TotalTokens,
				Content:      result.Provider.Message.Content,
			},
		},
	})
	s.maybeDispatchQueuedDrafts(sessionID)
}
