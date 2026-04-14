package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"sync/atomic"
	"time"
)

type RunManager struct {
	store    RunLifecycleStore
	registry *ActiveRegistry
	nextID   atomic.Int64
}

type PreparedRun struct {
	Run ActiveRun
	Ctx context.Context
}

func NewRunManager(store RunLifecycleStore, registry *ActiveRegistry) *RunManager {
	if registry == nil {
		registry = NewActiveRegistry()
	}
	return &RunManager{store: store, registry: registry}
}

func (m *RunManager) Prepare(ctx context.Context, runID string, chatID int64, sessionID, query string, snapshot PolicySnapshot) (PreparedRun, bool, error) {
	if strings.TrimSpace(runID) == "" {
		runID = fmt.Sprintf("runmgr-%d-%d", time.Now().UTC().UnixNano(), m.nextID.Add(1))
	}
	snapshot = NormalizePolicySnapshot(snapshot)
	runCtx, cancel := context.WithCancel(ctx)
	run := ActiveRun{
		RunID:          runID,
		ChatID:         chatID,
		SessionID:      sessionID,
		Query:          query,
		StartedAt:      time.Now().UTC(),
		PolicySnapshot: snapshot,
		cancel:         cancel,
	}
	if !m.registry.TryStart(run) {
		cancel()
		return PreparedRun{}, false, nil
	}
	if m.store != nil {
		if err := m.store.SaveRun(RunRecord{
			RunID:          runID,
			ChatID:         chatID,
			SessionID:      sessionID,
			Query:          query,
			Status:         StatusRunning,
			StartedAt:      run.StartedAt,
			PolicySnapshot: snapshot,
		}); err != nil {
			m.registry.Finish(chatID)
			cancel()
			return PreparedRun{}, false, err
		}
		_ = m.store.SaveEvent(runEvent(run.RunID, chatID, sessionID, "run.started", map[string]any{
			"query": query,
		}))
	}
	return PreparedRun{Run: run, Ctx: runCtx}, true, nil
}

func (m *RunManager) Launch(prepared PreparedRun, exec func(context.Context, string) error) {
	go func() {
		defer m.registry.Finish(prepared.Run.ChatID)
		err := exec(prepared.Ctx, prepared.Run.RunID)
		if m.store == nil {
			return
		}
		endedAt := time.Now().UTC()
		record := RunRecord{
			RunID:          prepared.Run.RunID,
			ChatID:         prepared.Run.ChatID,
			SessionID:      prepared.Run.SessionID,
			Query:          prepared.Run.Query,
			StartedAt:      prepared.Run.StartedAt,
			EndedAt:        &endedAt,
			PolicySnapshot: prepared.Run.PolicySnapshot,
		}
		if existing, ok, readErr := m.store.Run(prepared.Run.RunID); readErr == nil && ok {
			record.FinalResponse = existing.FinalResponse
		}
		switch {
		case errors.Is(err, context.Canceled):
			record.Status = StatusCancelled
			record.CancelRequested = true
			_ = m.store.SaveEvent(runEvent(prepared.Run.RunID, prepared.Run.ChatID, prepared.Run.SessionID, "run.cancelled", map[string]any{
				"cancel_requested": true,
			}))
		case err != nil:
			record.Status = StatusFailed
			record.FailureReason = err.Error()
			_ = m.store.SaveEvent(runEvent(prepared.Run.RunID, prepared.Run.ChatID, prepared.Run.SessionID, "run.failed", map[string]any{
				"error": err.Error(),
			}))
		default:
			record.Status = StatusCompleted
			_ = m.store.SaveEvent(runEvent(prepared.Run.RunID, prepared.Run.ChatID, prepared.Run.SessionID, "run.completed", nil))
		}
		_ = m.store.SaveRun(record)
	}()
}

func (m *RunManager) FailStart(prepared PreparedRun, err error) error {
	if prepared.Run.cancel != nil {
		prepared.Run.cancel()
	}
	m.registry.Finish(prepared.Run.ChatID)
	if m.store == nil {
		return err
	}
	endedAt := time.Now().UTC()
	record := RunRecord{
		RunID:          prepared.Run.RunID,
		ChatID:         prepared.Run.ChatID,
		SessionID:      prepared.Run.SessionID,
		Query:          prepared.Run.Query,
		Status:         StatusFailed,
		StartedAt:      prepared.Run.StartedAt,
		EndedAt:        &endedAt,
		FailureReason:  err.Error(),
		PolicySnapshot: prepared.Run.PolicySnapshot,
	}
	_ = m.store.SaveEvent(runEvent(prepared.Run.RunID, prepared.Run.ChatID, prepared.Run.SessionID, "run.failed", map[string]any{
		"error": err.Error(),
	}))
	if saveErr := m.store.SaveRun(record); saveErr != nil {
		return saveErr
	}
	return err
}

func (m *RunManager) Start(ctx context.Context, chatID int64, sessionID, query string, exec func(context.Context, string) error) (string, bool, error) {
	prepared, ok, err := m.Prepare(ctx, "", chatID, sessionID, query, PolicySnapshot{})
	if err != nil || !ok {
		return "", ok, err
	}
	m.Launch(prepared, exec)
	return prepared.Run.RunID, true, nil
}

func (m *RunManager) Cancel(chatID int64) bool {
	run, ok := m.registry.Active(chatID)
	if !ok {
		return false
	}
	if m.store != nil {
		_ = m.store.MarkCancelRequested(run.RunID)
		_ = m.store.SaveEvent(runEvent(run.RunID, run.ChatID, run.SessionID, "run.cancel_requested", nil))
	}
	return m.registry.Cancel(chatID)
}

func (m *RunManager) Active(chatID int64) (ActiveRun, bool) {
	return m.registry.Active(chatID)
}

func runEvent(runID string, chatID int64, sessionID, kind string, payload map[string]any) RuntimeEvent {
	data := json.RawMessage(`{}`)
	if payload != nil {
		if encoded, err := json.Marshal(payload); err == nil {
			data = encoded
		}
	}
	return RuntimeEvent{
		EntityType: "run",
		EntityID:   runID,
		ChatID:     chatID,
		SessionID:  sessionID,
		RunID:      runID,
		Kind:       kind,
		Payload:    data,
		CreatedAt:  time.Now().UTC(),
	}
}
