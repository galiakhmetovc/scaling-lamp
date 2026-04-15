package runtime

import (
	"context"
	"fmt"
	"sync"
	"time"

	"teamd/internal/runtime/eventing"
)

type localDelegateState struct {
	DelegateID        string
	Backend           DelegateBackend
	OwnerSessionID    string
	DelegateSessionID string
	PolicySnapshot    map[string]any
	Session           *ChatSession
	Running           bool
	Closed            bool
}

type LocalDelegateRuntime struct {
	agent     *Agent
	mu        sync.Mutex
	delegates map[string]*localDelegateState
}

func NewLocalDelegateRuntime(agent *Agent) *LocalDelegateRuntime {
	return &LocalDelegateRuntime{
		agent:     agent,
		delegates: map[string]*localDelegateState{},
	}
}

func (r *LocalDelegateRuntime) Spawn(ctx context.Context, req DelegateSpawnRequest) (DelegateView, error) {
	if r == nil || r.agent == nil {
		return DelegateView{}, fmt.Errorf("delegate runtime is nil")
	}
	delegateID := req.DelegateID
	if delegateID == "" {
		delegateID = r.agent.newID("delegate")
	}
	backend := req.Backend
	if backend == "" {
		backend = DelegateBackendLocalWorker
	}

	session, err := r.agent.NewChatSession()
	if err != nil {
		return DelegateView{}, fmt.Errorf("new delegate session: %w", err)
	}

	state := &localDelegateState{
		DelegateID:        delegateID,
		Backend:           backend,
		OwnerSessionID:    req.OwnerSessionID,
		DelegateSessionID: session.SessionID,
		PolicySnapshot:    cloneAnyMap(req.PolicySnapshot),
		Session:           session,
	}

	r.mu.Lock()
	if _, exists := r.delegates[delegateID]; exists {
		r.mu.Unlock()
		return DelegateView{}, fmt.Errorf("delegate %q already exists", delegateID)
	}
	r.delegates[delegateID] = state
	r.mu.Unlock()

	if err := r.agent.RecordEvent(ctx, eventing.Event{
		ID:            r.agent.newID("evt-delegate-spawned"),
		Kind:          eventing.EventDelegateSpawned,
		OccurredAt:    r.agent.now(),
		AggregateID:   delegateID,
		AggregateType: eventing.AggregateDelegate,
		Source:        "delegate.local",
		ActorID:       r.agent.Config.ID,
		ActorType:     "agent",
		TraceSummary:  "delegate spawned",
		Payload: map[string]any{
			"backend":             string(backend),
			"owner_session_id":    req.OwnerSessionID,
			"delegate_session_id": session.SessionID,
			"policy_snapshot":     cloneAnyMap(req.PolicySnapshot),
			"metadata":            cloneAnyMap(req.Metadata),
		},
	}); err != nil {
		return DelegateView{}, err
	}

	if req.Prompt != "" {
		if err := r.startTurn(ctx, state, req.Prompt); err != nil {
			return DelegateView{}, err
		}
	}

	view, ok := r.agent.delegateView(delegateID)
	if !ok {
		return DelegateView{}, fmt.Errorf("delegate %q view missing after spawn", delegateID)
	}
	return view, nil
}

func (r *LocalDelegateRuntime) Message(ctx context.Context, delegateID string, req DelegateMessageRequest) (DelegateView, error) {
	state, err := r.materializeState(ctx, delegateID)
	if err != nil {
		return DelegateView{}, err
	}
	if state.Closed {
		return DelegateView{}, fmt.Errorf("delegate %q is closed", delegateID)
	}
	if state.Running {
		return DelegateView{}, fmt.Errorf("delegate %q is already running", delegateID)
	}
	if err := r.startTurn(ctx, state, req.Content); err != nil {
		return DelegateView{}, err
	}
	view, ok := r.agent.delegateView(delegateID)
	if !ok {
		return DelegateView{}, fmt.Errorf("delegate %q view missing after message", delegateID)
	}
	return view, nil
}

func (r *LocalDelegateRuntime) Wait(ctx context.Context, req DelegateWaitRequest) (DelegateWaitResult, bool, error) {
	view, ok := r.agent.delegateView(req.DelegateID)
	if !ok {
		return DelegateWaitResult{}, false, nil
	}
	if view.Status == DelegateStatusRunning {
		view = r.waitForStableDelegate(req.DelegateID, 500*time.Millisecond)
	}

	state, err := r.materializeState(ctx, req.DelegateID)
	if err != nil {
		return DelegateWaitResult{}, false, err
	}
	session, err := r.ensureSession(ctx, state)
	if err != nil {
		return DelegateWaitResult{}, false, err
	}

	result := DelegateWaitResult{
		Delegate: view,
	}
	if handoff, ok := r.agent.delegateHandoff(req.DelegateID); ok {
		result.Handoff = &handoff
	}

	for index, message := range session.Messages {
		cursor := index + 1
		if cursor <= req.AfterCursor {
			continue
		}
		result.Messages = append(result.Messages, DelegateMessage{
			Cursor:     cursor,
			Role:       message.Role,
			Content:    message.Content,
			Name:       message.Name,
			ToolCallID: message.ToolCallID,
		})
		result.NextCursor = cursor
	}

	events, err := r.agent.EventLog.ListByAggregate(ctx, eventing.AggregateDelegate, req.DelegateID)
	if err != nil {
		return DelegateWaitResult{}, false, fmt.Errorf("load delegate events: %w", err)
	}
	limit := req.EventLimit
	if limit <= 0 {
		limit = len(events)
	}
	for _, event := range events {
		eventID := int64(event.Sequence)
		if eventID <= req.AfterEventID {
			continue
		}
		result.Events = append(result.Events, DelegateEventRef{EventID: eventID, Kind: string(event.Kind)})
		result.NextEventAfter = eventID
		if len(result.Events) >= limit {
			break
		}
	}
	if result.NextCursor == 0 {
		result.NextCursor = req.AfterCursor
	}
	if result.NextEventAfter == 0 {
		result.NextEventAfter = req.AfterEventID
	}
	return result, true, nil
}

func (r *LocalDelegateRuntime) waitForStableDelegate(delegateID string, maxWait time.Duration) DelegateView {
	deadline := time.Now().Add(maxWait)
	current, _ := r.agent.delegateView(delegateID)
	for current.Status == DelegateStatusRunning && time.Now().Before(deadline) {
		time.Sleep(10 * time.Millisecond)
		next, ok := r.agent.delegateView(delegateID)
		if !ok {
			return current
		}
		current = next
	}
	return current
}

func (r *LocalDelegateRuntime) Close(ctx context.Context, delegateID string) (DelegateView, bool, error) {
	state, err := r.materializeState(ctx, delegateID)
	if err != nil {
		return DelegateView{}, false, err
	}
	if state.Running {
		return DelegateView{}, true, fmt.Errorf("delegate %q is still running", delegateID)
	}

	r.mu.Lock()
	state.Closed = true
	r.mu.Unlock()

	if err := r.agent.RecordEvent(ctx, eventing.Event{
		ID:            r.agent.newID("evt-delegate-closed"),
		Kind:          eventing.EventDelegateClosed,
		OccurredAt:    r.agent.now(),
		AggregateID:   delegateID,
		AggregateType: eventing.AggregateDelegate,
		Source:        "delegate.local",
		ActorID:       r.agent.Config.ID,
		ActorType:     "agent",
		TraceSummary:  "delegate closed",
		Payload:       map[string]any{},
	}); err != nil {
		return DelegateView{}, true, err
	}
	view, _ := r.agent.delegateView(delegateID)
	return view, true, nil
}

func (r *LocalDelegateRuntime) Handoff(_ context.Context, delegateID string) (DelegateHandoff, bool, error) {
	handoff, ok := r.agent.delegateHandoff(delegateID)
	return handoff, ok, nil
}

func (r *LocalDelegateRuntime) materializeState(ctx context.Context, delegateID string) (*localDelegateState, error) {
	r.mu.Lock()
	if state, ok := r.delegates[delegateID]; ok {
		r.mu.Unlock()
		return state, nil
	}
	r.mu.Unlock()

	view, ok := r.agent.delegateView(delegateID)
	if !ok {
		return nil, fmt.Errorf("delegate %q not found", delegateID)
	}
	state := &localDelegateState{
		DelegateID:     delegateID,
		Backend:        view.Backend,
		OwnerSessionID: view.OwnerSessionID,
		PolicySnapshot: cloneAnyMap(view.PolicySnapshot),
		Closed:         view.Status == DelegateStatusClosed,
		Running:        view.Status == DelegateStatusRunning,
	}
	if projection := r.agent.delegateProjection(); projection != nil {
		if stored, ok := projection.View(delegateID); ok {
			state.DelegateSessionID = stored.DelegateSessionID
		}
	}
	if _, err := r.ensureSession(ctx, state); err != nil && state.DelegateSessionID != "" {
		return nil, err
	}

	r.mu.Lock()
	r.delegates[delegateID] = state
	r.mu.Unlock()
	return state, nil
}

func (r *LocalDelegateRuntime) ensureSession(ctx context.Context, state *localDelegateState) (*ChatSession, error) {
	r.mu.Lock()
	if state.Session != nil {
		session := state.Session
		r.mu.Unlock()
		return session, nil
	}
	sessionID := state.DelegateSessionID
	r.mu.Unlock()

	if sessionID == "" {
		return nil, fmt.Errorf("delegate %q session id is empty", state.DelegateID)
	}
	session, err := r.agent.ResumeChatSession(ctx, sessionID)
	if err != nil {
		return nil, fmt.Errorf("resume delegate session %q: %w", sessionID, err)
	}

	r.mu.Lock()
	state.Session = session
	r.mu.Unlock()
	return session, nil
}

func (r *LocalDelegateRuntime) startTurn(ctx context.Context, state *localDelegateState, prompt string) error {
	if prompt == "" {
		return fmt.Errorf("delegate prompt is empty")
	}
	session, err := r.ensureSession(ctx, state)
	if err != nil {
		return err
	}
	runID := r.agent.newID("run-delegate")
	now := r.agent.now()
	if err := r.agent.RecordEvent(ctx, eventing.Event{
		ID:            r.agent.newID("evt-delegate-msg"),
		Kind:          eventing.EventDelegateMessageReceived,
		OccurredAt:    now,
		AggregateID:   state.DelegateID,
		AggregateType: eventing.AggregateDelegate,
		Source:        "delegate.local",
		ActorID:       r.agent.Config.ID,
		ActorType:     "agent",
		TraceSummary:  "delegate message received",
		Payload: map[string]any{
			"content": prompt,
		},
	}); err != nil {
		return err
	}
	if err := r.agent.RecordEvent(ctx, eventing.Event{
		ID:            r.agent.newID("evt-delegate-run-start"),
		Kind:          eventing.EventDelegateRunStarted,
		OccurredAt:    now,
		AggregateID:   state.DelegateID,
		AggregateType: eventing.AggregateDelegate,
		Source:        "delegate.local",
		ActorID:       r.agent.Config.ID,
		ActorType:     "agent",
		TraceSummary:  "delegate run started",
		Payload: map[string]any{
			"delegate_run_id": runID,
		},
	}); err != nil {
		return err
	}

	r.mu.Lock()
	state.Running = true
	r.mu.Unlock()

	go r.finishTurn(state, session, runID, prompt)
	return nil
}

func (r *LocalDelegateRuntime) finishTurn(state *localDelegateState, session *ChatSession, runID, prompt string) {
	_, err := r.agent.ChatTurn(context.Background(), session, ChatTurnInput{Prompt: prompt})
	if err != nil {
		_ = r.agent.RecordEvent(context.Background(), eventing.Event{
			ID:            r.agent.newID("evt-delegate-failed"),
			Kind:          eventing.EventDelegateFailed,
			OccurredAt:    r.agent.now(),
			AggregateID:   state.DelegateID,
			AggregateType: eventing.AggregateDelegate,
			Source:        "delegate.local",
			ActorID:       r.agent.Config.ID,
			ActorType:     "agent",
			TraceSummary:  "delegate run failed",
			Payload: map[string]any{
				"delegate_run_id": runID,
				"error":           err.Error(),
			},
		})
		r.mu.Lock()
		state.Running = false
		r.mu.Unlock()
		return
	}

	summary := "delegate completed"
	if len(session.Messages) > 0 {
		summary = session.Messages[len(session.Messages)-1].Content
	}
	_ = r.agent.RecordEvent(context.Background(), eventing.Event{
		ID:            r.agent.newID("evt-delegate-completed"),
		Kind:          eventing.EventDelegateCompleted,
		OccurredAt:    r.agent.now(),
		AggregateID:   state.DelegateID,
		AggregateType: eventing.AggregateDelegate,
		Source:        "delegate.local",
		ActorID:       r.agent.Config.ID,
		ActorType:     "agent",
		TraceSummary:  "delegate run completed",
		Payload: map[string]any{
			"delegate_run_id": runID,
			"artifacts":       []any{},
		},
	})
	_ = r.agent.RecordEvent(context.Background(), eventing.Event{
		ID:            r.agent.newID("evt-delegate-handoff"),
		Kind:          eventing.EventDelegateHandoffCreated,
		OccurredAt:    r.agent.now(),
		AggregateID:   state.DelegateID,
		AggregateType: eventing.AggregateDelegate,
		Source:        "delegate.local",
		ActorID:       r.agent.Config.ID,
		ActorType:     "agent",
		TraceSummary:  "delegate handoff created",
		Payload: map[string]any{
			"backend":               string(state.Backend),
			"delegate_run_id":       runID,
			"summary":               summary,
			"artifacts":             []any{},
			"promoted_facts":        []any{},
			"open_questions":        []any{},
			"recommended_next_step": "review delegate handoff",
			"created_at":            r.agent.now().Format(timeFormatRFC3339Nano),
		},
	})

	r.mu.Lock()
	state.Running = false
	r.mu.Unlock()
}

const timeFormatRFC3339Nano = "2006-01-02T15:04:05.999999999Z07:00"
