package runtime

import (
	"context"
	"testing"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/provider"
)

func TestRuntimeCoreStartRunDelegatesToExecutionService(t *testing.T) {
	store := &executionTestStore{}
	hooks := &executionTestHooks{}
	api := NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	exec := NewExecutionService(api, hooks)
	core := NewRuntimeCore(api, exec, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, MemoryPolicy{Profile: "conservative"}, ActionPolicy{})

	view, ok, _, err := core.StartRun(context.Background(), StartRunRequest{
		RunID:       "run-1",
		ChatID:      1001,
		SessionID:   "1001:default",
		Query:       "hello",
		Interactive: true,
	})
	if err != nil || !ok {
		t.Fatalf("start run: ok=%v err=%v", ok, err)
	}
	if view.RunID != "run-1" || hooks.preparedReq.RunID != "run-1" {
		t.Fatalf("expected execution service to receive start request, view=%+v hooks=%+v", view, hooks.preparedReq)
	}
}

func TestRuntimeCoreExposesRunViewsAndControlState(t *testing.T) {
	now := time.Now().UTC()
	store := &runtimeAPITestStore{
		run: RunRecord{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Query:     "hello",
			Status:    StatusRunning,
			StartedAt: now,
		},
		ok: true,
	}
	api := NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	core := NewRuntimeCore(api, nil, nil, nil, nil, provider.RequestConfig{Model: "glm-5-turbo"}, MemoryPolicy{Profile: "conservative"}, ActionPolicy{})

	run, ok, err := core.Run("run-1")
	if err != nil || !ok {
		t.Fatalf("run: ok=%v err=%v", ok, err)
	}
	if run.RunID != "run-1" {
		t.Fatalf("unexpected run view: %+v", run)
	}
	control, err := core.ControlState("1001:default", 1001)
	if err != nil {
		t.Fatalf("control state: %v", err)
	}
	if control.Session.SessionID != "1001:default" {
		t.Fatalf("unexpected control state: %+v", control)
	}
}

func TestRuntimeCoreExecutesControlAndSessionActions(t *testing.T) {
	store := &runtimeAPITestStore{
		run: RunRecord{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Query:     "hello",
			Status:    StatusRunning,
			StartedAt: time.Now().UTC(),
		},
		ok: true,
	}
	api := NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps()))
	sessions := NewSessionActions(&sessionActionStoreStub{
		active:   "default",
		sessions: []string{"default", "debug"},
		messages: []provider.Message{{Role: "user", Content: "hello"}},
	})
	core := NewRuntimeCore(api, nil, nil, nil, sessions, provider.RequestConfig{Model: "glm-5-turbo"}, MemoryPolicy{Profile: "conservative"}, ActionPolicy{})

	controlResult, err := core.ExecuteControlAction("1001:default", ControlActionRequest{
		Action: ControlActionRunStatus,
		ChatID: 1001,
	})
	if err != nil {
		t.Fatalf("control action: %v", err)
	}
	if controlResult.Action != ControlActionRunStatus {
		t.Fatalf("unexpected control action result: %+v", controlResult)
	}

	sessionResult, err := core.ExecuteSessionAction(SessionActionRequest{
		ChatID: 1001,
		Action: SessionActionList,
	})
	if err != nil {
		t.Fatalf("session action: %v", err)
	}
	if len(sessionResult.Sessions) != 2 {
		t.Fatalf("unexpected session action result: %+v", sessionResult)
	}
}

type sessionActionStoreStub struct {
	active   string
	sessions []string
	messages []provider.Message
}

func (s *sessionActionStoreStub) ActiveSession(chatID int64) (string, error) {
	return s.active, nil
}

func (s *sessionActionStoreStub) CreateSession(chatID int64, session string) error {
	s.sessions = append(s.sessions, session)
	s.active = session
	return nil
}

func (s *sessionActionStoreStub) UseSession(chatID int64, session string) error {
	s.active = session
	return nil
}

func (s *sessionActionStoreStub) ListSessions(chatID int64) ([]string, error) {
	return append([]string(nil), s.sessions...), nil
}

func (s *sessionActionStoreStub) Reset(chatID int64) error {
	s.messages = nil
	return nil
}

func (s *sessionActionStoreStub) Messages(chatID int64) ([]provider.Message, error) {
	return append([]provider.Message(nil), s.messages...), nil
}
