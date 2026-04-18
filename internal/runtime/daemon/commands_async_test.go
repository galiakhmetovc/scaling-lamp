package daemon

import (
	"testing"
	"time"

	"teamd/internal/config"
	"teamd/internal/runtime"
)

func TestRunGuardedShellApprovalPublishesFailureOnPanic(t *testing.T) {
	t.Parallel()

	server := &Server{
		agent:          &runtime.Agent{Config: config.AgentConfig{ID: "agent-1"}, Now: func() time.Time { return time.Date(2026, 4, 18, 18, 0, 0, 0, time.UTC) }},
		sessionRuntime: map[string]*sessionRuntimeState{},
		daemonBus:      newDaemonBus(),
	}
	if !server.startMainRun("session-1") {
		t.Fatal("startMainRun returned false")
	}
	subID, ch := server.daemonBus.Subscribe(4)
	defer server.daemonBus.Unsubscribe(subID)

	server.runGuardedShellApproval("session-1", func() {
		panic("boom")
	})

	select {
	case evt := <-ch:
		if evt.Type != "shell_approval_failed" {
			t.Fatalf("event type = %q, want shell_approval_failed", evt.Type)
		}
		if evt.Error == "" {
			t.Fatal("event error is empty, want panic text")
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for shell_approval_failed event")
	}

	server.runtimeMu.RLock()
	defer server.runtimeMu.RUnlock()
	state := server.sessionRuntime["session-1"]
	if state == nil || state.mainRun.Phase != mainRunPhaseFailed {
		t.Fatalf("main run phase = %#v, want failed", state.mainRun.Phase)
	}
}
