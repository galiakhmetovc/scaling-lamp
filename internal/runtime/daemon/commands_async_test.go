package daemon

import (
	"context"
	"sync/atomic"
	"testing"
	"time"

	"teamd/internal/config"
	"teamd/internal/runtime"
	"teamd/internal/runtime/eventing"
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

func TestRunGuardedShellApprovalSerializesPerSession(t *testing.T) {
	t.Parallel()

	server := &Server{
		agent:          &runtime.Agent{Config: config.AgentConfig{ID: "agent-1"}, Now: func() time.Time { return time.Date(2026, 4, 18, 18, 5, 0, 0, time.UTC) }},
		sessionRuntime: map[string]*sessionRuntimeState{},
		daemonBus:      newDaemonBus(),
	}

	var running int32
	firstEntered := make(chan struct{}, 1)
	releaseFirst := make(chan struct{})
	secondFinished := make(chan struct{}, 1)
	concurrent := make(chan struct{}, 1)

	server.runGuardedShellApproval("session-1", func() {
		if n := atomic.AddInt32(&running, 1); n != 1 {
			concurrent <- struct{}{}
		}
		firstEntered <- struct{}{}
		<-releaseFirst
		atomic.AddInt32(&running, -1)
	})

	select {
	case <-firstEntered:
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for first approval to start")
	}

	server.runGuardedShellApproval("session-1", func() {
		if n := atomic.AddInt32(&running, 1); n != 1 {
			concurrent <- struct{}{}
		}
		atomic.AddInt32(&running, -1)
		secondFinished <- struct{}{}
	})

	select {
	case <-secondFinished:
		t.Fatal("second approval continuation ran before first finished")
	case <-concurrent:
		t.Fatal("approval continuations ran concurrently for one session")
	case <-time.After(150 * time.Millisecond):
	}

	close(releaseFirst)

	select {
	case <-secondFinished:
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for second approval continuation")
	}

	select {
	case <-concurrent:
		t.Fatal("approval continuations ran concurrently for one session")
	default:
	}
}

func TestExecuteCommandDebugTraceRecordsEvent(t *testing.T) {
	t.Parallel()

	agent := &runtime.Agent{
		Config:   config.AgentConfig{ID: "agent-1"},
		EventLog: runtime.NewInMemoryEventLog(),
		Now:      func() time.Time { return time.Date(2026, 4, 18, 19, 10, 0, 0, time.UTC) },
		NewID:    func(prefix string) string { return prefix + "-1" },
	}
	server := &Server{
		agent:          agent,
		sessionRuntime: map[string]*sessionRuntimeState{},
		daemonBus:      newDaemonBus(),
	}

	_, err := server.executeCommand(context.Background(), CommandRequest{
		Command: "debug.trace",
		Payload: map[string]any{
			"session_id": "session-1",
			"trace":      "tui.approval_menu.shown",
			"fields": map[string]any{
				"approval_id": "approval-1",
			},
		},
	})
	if err != nil {
		t.Fatalf("executeCommand returned error: %v", err)
	}

	events, err := agent.EventLog.ListByAggregate(context.Background(), eventing.AggregateSession, "session-1")
	if err != nil {
		t.Fatalf("ListByAggregate returned error: %v", err)
	}
	if len(events) != 1 {
		t.Fatalf("session events = %d, want 1", len(events))
	}
	if events[0].Kind != eventing.EventTraceRecorded {
		t.Fatalf("event kind = %q, want %q", events[0].Kind, eventing.EventTraceRecorded)
	}
	if got := events[0].Payload["trace"]; got != "tui.approval_menu.shown" {
		t.Fatalf("trace payload = %#v, want tui.approval_menu.shown", got)
	}
	fields, _ := events[0].Payload["fields"].(map[string]any)
	if got := fields["approval_id"]; got != "approval-1" {
		t.Fatalf("approval_id = %#v, want approval-1", got)
	}
}
