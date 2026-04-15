package runtime

import (
	"context"
	"testing"
	"time"
)

type delegateRuntimeStub struct {
	lastSpawn DelegateSpawnRequest
	lastMsg   DelegateMessageRequest
	lastWait  DelegateWaitRequest
}

func (s *delegateRuntimeStub) Spawn(_ context.Context, req DelegateSpawnRequest) (DelegateView, error) {
	s.lastSpawn = req
	now := time.Now().UTC()
	return DelegateView{
		DelegateID:     req.DelegateID,
		Backend:        req.Backend,
		OwnerSessionID: req.OwnerSessionID,
		Status:         DelegateStatusQueued,
		CreatedAt:      now,
		UpdatedAt:      now,
	}, nil
}

func (s *delegateRuntimeStub) Message(_ context.Context, _ string, req DelegateMessageRequest) (DelegateView, error) {
	s.lastMsg = req
	now := time.Now().UTC()
	return DelegateView{DelegateID: "delegate-1", Backend: DelegateBackendLocalWorker, Status: DelegateStatusRunning, CreatedAt: now, UpdatedAt: now}, nil
}

func (s *delegateRuntimeStub) Wait(_ context.Context, req DelegateWaitRequest) (DelegateWaitResult, bool, error) {
	s.lastWait = req
	now := time.Now().UTC()
	return DelegateWaitResult{
		Delegate: DelegateView{DelegateID: req.DelegateID, Backend: DelegateBackendLocalWorker, Status: DelegateStatusIdle, CreatedAt: now, UpdatedAt: now},
		Handoff:  &DelegateHandoff{DelegateID: req.DelegateID, Backend: DelegateBackendLocalWorker, Summary: "done", CreatedAt: now, UpdatedAt: now},
		Messages: []DelegateMessage{{Cursor: 1, Role: "assistant", Content: "done"}},
		Events:   []DelegateEventRef{{EventID: 7, Kind: "delegate.completed"}},
	}, true, nil
}

func (s *delegateRuntimeStub) Close(_ context.Context, id string) (DelegateView, bool, error) {
	now := time.Now().UTC()
	return DelegateView{DelegateID: id, Backend: DelegateBackendLocalWorker, Status: DelegateStatusClosed, CreatedAt: now, UpdatedAt: now, ClosedAt: &now}, true, nil
}

func (s *delegateRuntimeStub) Handoff(_ context.Context, id string) (DelegateHandoff, bool, error) {
	now := time.Now().UTC()
	return DelegateHandoff{DelegateID: id, Backend: DelegateBackendLocalWorker, Summary: "done", CreatedAt: now, UpdatedAt: now}, true, nil
}

func TestDelegateRuntimeContractCoversLifecycleShape(t *testing.T) {
	t.Parallel()

	stub := &delegateRuntimeStub{}
	_, err := stub.Spawn(context.Background(), DelegateSpawnRequest{
		DelegateID:     "delegate-1",
		Backend:        DelegateBackendLocalWorker,
		OwnerSessionID: "session-1",
		Prompt:         "investigate",
		PolicySnapshot: map[string]any{"model": "test"},
	})
	if err != nil {
		t.Fatalf("Spawn returned error: %v", err)
	}
	if stub.lastSpawn.Backend != DelegateBackendLocalWorker {
		t.Fatalf("spawn backend = %q, want %q", stub.lastSpawn.Backend, DelegateBackendLocalWorker)
	}

	wait, ok, err := stub.Wait(context.Background(), DelegateWaitRequest{DelegateID: "delegate-1", AfterCursor: 1, AfterEventID: 6, EventLimit: 25})
	if err != nil || !ok {
		t.Fatalf("Wait returned ok=%v err=%v", ok, err)
	}
	if wait.Handoff == nil || wait.Handoff.Summary != "done" {
		t.Fatalf("handoff = %+v, want summary", wait.Handoff)
	}
	if len(wait.Events) != 1 || wait.Events[0].Kind != "delegate.completed" {
		t.Fatalf("events = %+v", wait.Events)
	}
}
