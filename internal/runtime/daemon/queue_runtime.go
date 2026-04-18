package daemon

import (
	"context"
	"sync"
	"time"

	"teamd/internal/runtime"
)

type sessionRuntimeState struct {
	mainRun mainRunState
	queue   []QueuedDraft
}

type mainRunPhase string

const (
	mainRunPhaseIdle            mainRunPhase = "idle"
	mainRunPhaseRunning         mainRunPhase = "running"
	mainRunPhaseWaitingApproval mainRunPhase = "waiting_approval"
	mainRunPhaseWaitingShell    mainRunPhase = "waiting_shell"
	mainRunPhaseCompleted       mainRunPhase = "completed"
	mainRunPhaseFailed          mainRunPhase = "failed"
)

type mainRunState struct {
	Active       bool
	Phase        mainRunPhase
	StartedAt    time.Time
	Provider     string
	Model        string
	InputTokens  int
	OutputTokens int
	TotalTokens  int
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
	return ok && state.mainRun.isActive()
}

func (s *Server) mainRunSnapshot(sessionID string) MainRunSnapshot {
	s.runtimeMu.RLock()
	defer s.runtimeMu.RUnlock()
	state, ok := s.sessionRuntime[sessionID]
	if !ok {
		return MainRunSnapshot{
			Provider: s.providerLabel(),
			Model:    s.currentAgent().Contracts.ProviderRequest.RequestShape.Model.Params.Model,
		}
	}
	return MainRunSnapshot{
		Active:       state.mainRun.isActive(),
		Phase:        string(state.mainRun.effectivePhase()),
		StartedAt:    state.mainRun.StartedAt,
		Provider:     state.mainRun.Provider,
		Model:        state.mainRun.Model,
		InputTokens:  state.mainRun.InputTokens,
		OutputTokens: state.mainRun.OutputTokens,
		TotalTokens:  state.mainRun.TotalTokens,
	}
}

func (s *Server) queuedDrafts(sessionID string) []QueuedDraft {
	s.runtimeMu.RLock()
	defer s.runtimeMu.RUnlock()
	state, ok := s.sessionRuntime[sessionID]
	if !ok || len(state.queue) == 0 {
		return []QueuedDraft{}
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
	if state.mainRun.Active {
		return false
	}
	state.mainRun.Active = true
	state.mainRun.Phase = mainRunPhaseRunning
	state.mainRun.StartedAt = s.currentAgent().Now().UTC()
	state.mainRun.Provider = s.providerLabel()
	state.mainRun.Model = s.currentAgent().Contracts.ProviderRequest.RequestShape.Model.Params.Model
	state.mainRun.InputTokens = 0
	state.mainRun.OutputTokens = 0
	state.mainRun.TotalTokens = 0
	return true
}

func (s *Server) finishMainRun(sessionID string, result *providerResultPayload) {
	s.finishMainRunWithPhase(sessionID, result, mainRunPhaseCompleted)
}

func (s *Server) failMainRun(sessionID string, result *providerResultPayload) {
	s.finishMainRunWithPhase(sessionID, result, mainRunPhaseFailed)
}

func (s *Server) finishMainRunWithPhase(sessionID string, result *providerResultPayload, phase mainRunPhase) {
	s.runtimeMu.Lock()
	defer s.runtimeMu.Unlock()
	state := s.ensureSessionRuntimeLocked(sessionID)
	state.mainRun.Active = false
	state.mainRun.Phase = phase
	if result == nil {
		return
	}
	if result.Provider != "" {
		state.mainRun.Provider = result.Provider
	}
	if result.Model != "" {
		state.mainRun.Model = result.Model
	}
	state.mainRun.InputTokens = result.InputTokens
	state.mainRun.OutputTokens = result.OutputTokens
	state.mainRun.TotalTokens = result.TotalTokens
}

func (s *Server) settleMainRunAfterChatTurn(sessionID string, result providerResultPayload, finishReason string) bool {
	if finishReason == "approval_pending" {
		s.setMainRunPhase(sessionID, mainRunPhaseWaitingApproval)
		return true
	}
	if len(s.currentAgent().CurrentRunningShellCommands(sessionID)) > 0 {
		s.updateMainRunResult(sessionID, &result)
		s.setMainRunPhase(sessionID, mainRunPhaseWaitingShell)
		return true
	}
	s.finishMainRunWithPhase(sessionID, &result, mainRunPhaseCompleted)
	return false
}

func (s *Server) syncMainRunAfterShellContinuation(agent *runtime.Agent, sessionID string) {
	if len(agent.PendingShellApprovals(sessionID)) > 0 {
		s.setMainRunPhase(sessionID, mainRunPhaseWaitingApproval)
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_updated", Payload: map[string]any{"session_id": sessionID}})
		return
	}
	if len(agent.CurrentRunningShellCommands(sessionID)) > 0 {
		s.setMainRunPhase(sessionID, mainRunPhaseWaitingShell)
		s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_updated", Payload: map[string]any{"session_id": sessionID}})
		return
	}
	s.finishMainRun(sessionID, nil)
	s.publishDaemon(WebsocketEnvelope{Type: "shell_approval_updated", Payload: map[string]any{"session_id": sessionID}})
	s.maybeDispatchQueuedDrafts(sessionID)
}

func (s *Server) setMainRunPhase(sessionID string, phase mainRunPhase) {
	s.runtimeMu.Lock()
	defer s.runtimeMu.Unlock()
	state := s.ensureSessionRuntimeLocked(sessionID)
	state.mainRun.Phase = phase
	state.mainRun.Active = phase == mainRunPhaseRunning || phase == mainRunPhaseWaitingApproval || phase == mainRunPhaseWaitingShell
}

func (s *Server) updateMainRunResult(sessionID string, result *providerResultPayload) {
	if result == nil {
		return
	}
	s.runtimeMu.Lock()
	defer s.runtimeMu.Unlock()
	state := s.ensureSessionRuntimeLocked(sessionID)
	if result.Provider != "" {
		state.mainRun.Provider = result.Provider
	}
	if result.Model != "" {
		state.mainRun.Model = result.Model
	}
	state.mainRun.InputTokens = result.InputTokens
	state.mainRun.OutputTokens = result.OutputTokens
	state.mainRun.TotalTokens = result.TotalTokens
}

func (m mainRunState) effectivePhase() mainRunPhase {
	if m.Phase != "" {
		return m.Phase
	}
	if m.Active {
		return mainRunPhaseRunning
	}
	return mainRunPhaseIdle
}

func (m mainRunState) isActive() bool {
	switch m.effectivePhase() {
	case mainRunPhaseRunning, mainRunPhaseWaitingApproval, mainRunPhaseWaitingShell:
		return true
	default:
		return false
	}
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
		s.finishMainRun(sessionID, nil)
		return
	}
	go s.dispatchQueuedDraft(context.Background(), sessionID, draft)
}

func (s *Server) dispatchQueuedDraft(ctx context.Context, sessionID string, draft QueuedDraft) {
	s.publishDaemon(WebsocketEnvelope{Type: "queue_draft_started", Payload: map[string]any{"session_id": sessionID, "draft": draft}})
	agent := s.currentAgent()
	session, err := agent.ResumeChatSession(ctx, sessionID)
	if err != nil {
		s.failMainRun(sessionID, nil)
		s.publishDaemon(WebsocketEnvelope{Type: "queue_draft_failed", Payload: map[string]any{"session_id": sessionID, "draft": draft}, Error: err.Error()})
		return
	}
	result, err := agent.ChatTurn(ctx, session, runtime.ChatTurnInput{Prompt: draft.Text})
	if err != nil {
		s.failMainRun(sessionID, nil)
		s.publishDaemon(WebsocketEnvelope{Type: "queue_draft_failed", Payload: map[string]any{"session_id": sessionID, "draft": draft}, Error: err.Error()})
		s.maybeDispatchQueuedDrafts(sessionID)
		return
	}
	resultPayload := providerResultPayload{
		Provider:     s.providerLabel(),
		Model:        result.Provider.Model,
		InputTokens:  result.Provider.Usage.InputTokens,
		OutputTokens: result.Provider.Usage.OutputTokens,
		TotalTokens:  result.Provider.Usage.TotalTokens,
		Content:      result.Provider.Message.Content,
	}
	stillRunning := s.settleMainRunAfterChatTurn(sessionID, resultPayload, result.Provider.FinishReason)
	snapshot, snapshotErr := s.buildSessionSnapshot(sessionID)
	if snapshotErr != nil {
		s.publishDaemon(WebsocketEnvelope{Type: "queue_draft_failed", Payload: map[string]any{"session_id": sessionID, "draft": draft}, Error: snapshotErr.Error()})
		if !stillRunning {
			s.maybeDispatchQueuedDrafts(sessionID)
		}
		return
	}
	s.publishDaemon(WebsocketEnvelope{
		Type: "queue_draft_completed",
		Payload: map[string]any{
			"session": snapshot,
			"draft":   draft,
			"result":  resultPayload,
		},
	})
	if !stillRunning {
		s.maybeDispatchQueuedDrafts(sessionID)
	}
}
