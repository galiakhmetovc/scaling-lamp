package cli

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"sync/atomic"
	"strings"
	"testing"
	"time"

	"teamd/internal/api"
	"teamd/internal/runtime"
)

type chatConsoleClientStub struct {
	startRunFn     func(chatID int64, sessionID, query string) (api.CreateRunResponse, error)
	runStatusFn    func(runID string) (api.RunStatusResponse, error)
	eventsFn       func(req api.EventListRequest) (api.EventListResponse, error)
	streamEventsFn func(ctx context.Context, req api.EventListRequest, onEvent func(runtime.RuntimeEvent) error) error
	cancelRunFn    func(runID string) error
	approvalsFn    func(sessionID string) ([]api.ApprovalRecordResponse, error)
	controlFn      func(sessionID string, chatID int64) (api.ControlStateResponse, error)
	approveFn      func(id string) (api.ApprovalRecordResponse, error)
	rejectFn       func(id string) (api.ApprovalRecordResponse, error)
	planFn         func(planID string) (api.PlanResponse, error)
	plansFn        func(ownerType, ownerID string, limit int) (api.PlanListResponse, error)
	handoffFn      func(workerID string) (api.WorkerHandoffResponse, error)
	artifactFn     func(ref string) (api.ArtifactResponse, error)
	artifactCatFn  func(ref string) (api.ArtifactContentResponse, error)
}

func (s chatConsoleClientStub) StartRun(chatID int64, sessionID, query string) (api.CreateRunResponse, error) {
	return s.startRunFn(chatID, sessionID, query)
}

func (s chatConsoleClientStub) RunStatus(runID string) (api.RunStatusResponse, error) {
	return s.runStatusFn(runID)
}

func (s chatConsoleClientStub) Events(req api.EventListRequest) (api.EventListResponse, error) {
	if s.eventsFn == nil {
		return api.EventListResponse{}, nil
	}
	return s.eventsFn(req)
}

func (s chatConsoleClientStub) StreamEvents(ctx context.Context, req api.EventListRequest, onEvent func(runtime.RuntimeEvent) error) error {
	return s.streamEventsFn(ctx, req, onEvent)
}

func (s chatConsoleClientStub) CancelRun(runID string) error {
	return s.cancelRunFn(runID)
}

func (s chatConsoleClientStub) Approvals(sessionID string) ([]api.ApprovalRecordResponse, error) {
	return s.approvalsFn(sessionID)
}

func (s chatConsoleClientStub) ControlState(sessionID string, chatID int64) (api.ControlStateResponse, error) {
	if s.controlFn == nil {
		return api.ControlStateResponse{}, nil
	}
	return s.controlFn(sessionID, chatID)
}

func (s chatConsoleClientStub) Approve(id string) (api.ApprovalRecordResponse, error) {
	return s.approveFn(id)
}

func (s chatConsoleClientStub) Reject(id string) (api.ApprovalRecordResponse, error) {
	return s.rejectFn(id)
}

func (s chatConsoleClientStub) Plan(planID string) (api.PlanResponse, error) {
	return s.planFn(planID)
}

func (s chatConsoleClientStub) Plans(ownerType, ownerID string, limit int) (api.PlanListResponse, error) {
	return s.plansFn(ownerType, ownerID, limit)
}

func (s chatConsoleClientStub) WorkerHandoff(workerID string) (api.WorkerHandoffResponse, error) {
	return s.handoffFn(workerID)
}

func (s chatConsoleClientStub) Artifact(ref string) (api.ArtifactResponse, error) {
	return s.artifactFn(ref)
}

func (s chatConsoleClientStub) ArtifactContent(ref string) (api.ArtifactContentResponse, error) {
	return s.artifactCatFn(ref)
}

func TestChatConsoleRunsMessageAndExitsOnQuit(t *testing.T) {
	var started []string
	console := NewChatConsole(chatConsoleClientStub{
		startRunFn: func(chatID int64, sessionID, query string) (api.CreateRunResponse, error) {
			started = append(started, query)
			return api.CreateRunResponse{
				RunID:    "run-1",
				Accepted: true,
				Run: runtime.RunView{
					RunID:     "run-1",
					ChatID:    chatID,
					SessionID: sessionID,
					Status:    runtime.StatusRunning,
				},
			}, nil
		},
		runStatusFn: func(runID string) (api.RunStatusResponse, error) {
			return api.RunStatusResponse{
				Run: runtime.RunView{
					RunID:     runID,
					ChatID:    1001,
					SessionID: "1001:default",
					Status:    runtime.StatusCompleted,
				},
			}, nil
		},
		streamEventsFn: func(ctx context.Context, req api.EventListRequest, onEvent func(runtime.RuntimeEvent) error) error {
			if req.SessionID != "1001:default" || req.AfterID != 0 {
				t.Fatalf("unexpected stream query: %+v", req)
			}
			if err := onEvent(runtime.RuntimeEvent{ID: 1, EntityType: "run", EntityID: "run-1", Kind: "run.started"}); err != nil {
				return err
			}
			if err := onEvent(runtime.RuntimeEvent{ID: 2, EntityType: "run", EntityID: "run-1", Kind: "run.completed"}); err != nil {
				return err
			}
			return ErrStopStream
		},
	}, bytes.NewBufferString("hello there\n/quit\n"), &bytes.Buffer{})

	if err := console.Run(context.Background(), 1001, "1001:default"); err != nil {
		t.Fatalf("Run: %v", err)
	}
	if len(started) != 1 || started[0] != "hello there" {
		t.Fatalf("unexpected started runs: %#v", started)
	}
	out := console.Output().String()
	if !strings.Contains(out, "you: hello there") {
		t.Fatalf("expected user line in output: %q", out)
	}
	if !strings.Contains(out, "system: run started") || !strings.Contains(out, "system: run completed") {
		t.Fatalf("expected run lifecycle lines: %q", out)
	}
}

func TestChatConsoleShowsUsageForEmptySessionIdentifiers(t *testing.T) {
	console := NewChatConsole(chatConsoleClientStub{}, &bytes.Buffer{}, &bytes.Buffer{})
	err := console.Run(context.Background(), 0, "")
	if err == nil || !strings.Contains(err.Error(), "chat_id and session_id are required") {
		t.Fatalf("expected usage error, got %v", err)
	}
}

func TestChatConsoleFallsBackToRunStatusAfterStreamDisconnect(t *testing.T) {
	console := NewChatConsole(chatConsoleClientStub{
		startRunFn: func(chatID int64, sessionID, query string) (api.CreateRunResponse, error) {
			return api.CreateRunResponse{
				RunID:    "run-1",
				Accepted: true,
				Run:      runtime.RunView{RunID: "run-1", Status: runtime.StatusRunning, ChatID: chatID, SessionID: sessionID},
			}, nil
		},
		runStatusFn: func(runID string) (api.RunStatusResponse, error) {
			return api.RunStatusResponse{Run: runtime.RunView{RunID: runID, Status: runtime.StatusCompleted, ChatID: 1001, SessionID: "1001:default"}}, nil
		},
		streamEventsFn: func(ctx context.Context, req api.EventListRequest, onEvent func(runtime.RuntimeEvent) error) error {
			return errors.New("stream dropped")
		},
	}, bytes.NewBufferString("hello\n/quit\n"), &bytes.Buffer{})

	if err := console.Run(context.Background(), 1001, "1001:default"); err != nil {
		t.Fatalf("Run: %v", err)
	}
	out := console.Output().String()
	if !strings.Contains(out, "system: event stream disconnected") || !strings.Contains(out, "system: run completed") {
		t.Fatalf("unexpected fallback output: %q", out)
	}
}

func TestChatConsoleStopsStreamingOnTerminalRunEvent(t *testing.T) {
	console := NewChatConsole(chatConsoleClientStub{
		startRunFn: func(chatID int64, sessionID, query string) (api.CreateRunResponse, error) {
			return api.CreateRunResponse{
				RunID:    "run-1",
				Accepted: true,
				Run:      runtime.RunView{RunID: "run-1", Status: runtime.StatusRunning, ChatID: chatID, SessionID: sessionID},
			}, nil
		},
		runStatusFn: func(runID string) (api.RunStatusResponse, error) {
			return api.RunStatusResponse{Run: runtime.RunView{RunID: runID, Status: runtime.StatusCompleted, ChatID: 1001, SessionID: "1001:default"}}, nil
		},
		streamEventsFn: func(ctx context.Context, req api.EventListRequest, onEvent func(runtime.RuntimeEvent) error) error {
			if err := onEvent(runtime.RuntimeEvent{ID: 1, EntityType: "run", EntityID: "run-1", Kind: "run.started"}); err != nil {
				return err
			}
			return onEvent(runtime.RuntimeEvent{ID: 2, EntityType: "run", EntityID: "run-1", Kind: "run.completed"})
		},
	}, bytes.NewBufferString("hello\n/quit\n"), &bytes.Buffer{})

	err := console.Run(context.Background(), 1001, "1001:default")
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	out := console.Output().String()
	if !strings.Contains(out, "system: run completed") {
		t.Fatalf("unexpected output: %q", out)
	}
}

func TestChatConsoleRendersAssistantFinalEventWithoutDuplicatingTerminalStatus(t *testing.T) {
	console := NewChatConsole(chatConsoleClientStub{
		startRunFn: func(chatID int64, sessionID, query string) (api.CreateRunResponse, error) {
			return api.CreateRunResponse{
				RunID:    "run-1",
				Accepted: true,
				Run:      runtime.RunView{RunID: "run-1", Status: runtime.StatusRunning, ChatID: chatID, SessionID: sessionID},
			}, nil
		},
		runStatusFn: func(runID string) (api.RunStatusResponse, error) {
			return api.RunStatusResponse{Run: runtime.RunView{RunID: runID, Status: runtime.StatusCompleted, ChatID: 1001, SessionID: "1001:default"}}, nil
		},
		streamEventsFn: func(ctx context.Context, req api.EventListRequest, onEvent func(runtime.RuntimeEvent) error) error {
			if err := onEvent(runtime.RuntimeEvent{ID: 1, EntityType: "run", EntityID: "run-1", Kind: "run.started"}); err != nil {
				return err
			}
			if err := onEvent(runtime.RuntimeEvent{ID: 2, EntityType: "run", EntityID: "run-1", Kind: "assistant.final", Payload: []byte(`{"text":"привет в ответ"}`)}); err != nil {
				return err
			}
			return onEvent(runtime.RuntimeEvent{ID: 3, EntityType: "run", EntityID: "run-1", Kind: "run.completed"})
		},
	}, bytes.NewBufferString("hello\n/quit\n"), &bytes.Buffer{})

	if err := console.Run(context.Background(), 1001, "1001:default"); err != nil {
		t.Fatalf("Run: %v", err)
	}
	out := console.Output().String()
	if !strings.Contains(out, "assistant: привет в ответ") {
		t.Fatalf("expected assistant final reply in output: %q", out)
	}
	if got := strings.Count(out, "system: run completed"); got != 1 {
		t.Fatalf("expected single terminal status, got %d in output %q", got, out)
	}
}

func TestChatConsoleRetriesRunBusyBeforeStartingRun(t *testing.T) {
	attempts := 0
	console := NewChatConsole(chatConsoleClientStub{
		startRunFn: func(chatID int64, sessionID, query string) (api.CreateRunResponse, error) {
			attempts++
			if attempts == 1 {
				return api.CreateRunResponse{}, errors.New("run_busy: another run is already active for this chat")
			}
			return api.CreateRunResponse{
				RunID:    "run-1",
				Accepted: true,
				Run:      runtime.RunView{RunID: "run-1", Status: runtime.StatusRunning, ChatID: chatID, SessionID: sessionID},
			}, nil
		},
		runStatusFn: func(runID string) (api.RunStatusResponse, error) {
			return api.RunStatusResponse{Run: runtime.RunView{RunID: runID, Status: runtime.StatusCompleted, ChatID: 1001, SessionID: "1001:default"}}, nil
		},
		streamEventsFn: func(ctx context.Context, req api.EventListRequest, onEvent func(runtime.RuntimeEvent) error) error {
			if err := onEvent(runtime.RuntimeEvent{ID: 1, EntityType: "run", EntityID: "run-1", Kind: "run.started"}); err != nil {
				return err
			}
			return onEvent(runtime.RuntimeEvent{ID: 2, EntityType: "run", EntityID: "run-1", Kind: "run.completed"})
		},
	}, bytes.NewBufferString("hello\n/quit\n"), &bytes.Buffer{})

	if err := console.Run(context.Background(), 1001, "1001:default"); err != nil {
		t.Fatalf("Run: %v", err)
	}
	if attempts != 2 {
		t.Fatalf("expected retry after run_busy, got %d attempts", attempts)
	}
}

func TestChatConsoleFallsBackToRunStatusFinalResponse(t *testing.T) {
	console := NewChatConsole(chatConsoleClientStub{
		startRunFn: func(chatID int64, sessionID, query string) (api.CreateRunResponse, error) {
			return api.CreateRunResponse{
				RunID:    "run-1",
				Accepted: true,
				Run:      runtime.RunView{RunID: "run-1", Status: runtime.StatusRunning, ChatID: chatID, SessionID: sessionID},
			}, nil
		},
		runStatusFn: func(runID string) (api.RunStatusResponse, error) {
			return api.RunStatusResponse{Run: runtime.RunView{
				RunID:         runID,
				Status:        runtime.StatusCompleted,
				ChatID:        1001,
				SessionID:     "1001:default",
				FinalResponse: "fallback final reply",
			}}, nil
		},
		streamEventsFn: func(ctx context.Context, req api.EventListRequest, onEvent func(runtime.RuntimeEvent) error) error {
			if err := onEvent(runtime.RuntimeEvent{ID: 1, EntityType: "run", EntityID: "run-1", Kind: "run.started"}); err != nil {
				return err
			}
			return onEvent(runtime.RuntimeEvent{ID: 2, EntityType: "run", EntityID: "run-1", Kind: "run.completed"})
		},
	}, bytes.NewBufferString("hello\n/quit\n"), &bytes.Buffer{})

	if err := console.Run(context.Background(), 1001, "1001:default"); err != nil {
		t.Fatalf("Run: %v", err)
	}
	out := console.Output().String()
	if !strings.Contains(out, "assistant: fallback final reply") {
		t.Fatalf("expected fallback final reply in output: %q", out)
	}
}

func TestChatConsoleHandlesLocalCommands(t *testing.T) {
	cancelled := false
	console := NewChatConsole(chatConsoleClientStub{
		cancelRunFn: func(runID string) error {
			cancelled = true
			if runID != "run-1" {
				t.Fatalf("unexpected cancelled run: %s", runID)
			}
			return nil
		},
		approvalsFn: func(sessionID string) ([]api.ApprovalRecordResponse, error) {
			return []api.ApprovalRecordResponse{{ID: "approval-1", WorkerID: "shell.exec", SessionID: sessionID, Status: "pending"}}, nil
		},
		approveFn: func(id string) (api.ApprovalRecordResponse, error) {
			return api.ApprovalRecordResponse{ID: id, Status: "approved"}, nil
		},
		plansFn: func(ownerType, ownerID string, limit int) (api.PlanListResponse, error) {
			return api.PlanListResponse{Items: []runtime.PlanRecord{{PlanID: "plan-1", OwnerType: ownerType, OwnerID: ownerID, Title: "Investigate rollout", Items: []runtime.PlanItem{{ItemID: "item-1", Content: "Inspect runtime", Status: runtime.PlanItemInProgress}}}}}, nil
		},
		handoffFn: func(workerID string) (api.WorkerHandoffResponse, error) {
			return api.WorkerHandoffResponse{Handoff: runtime.WorkerHandoff{WorkerID: workerID, Summary: "done", Artifacts: []string{"artifact://worker-output-1"}}}, nil
		},
		artifactFn: func(ref string) (api.ArtifactResponse, error) {
			return api.ArtifactResponse{Artifact: api.ArtifactMetadata{Ref: ref, Name: "worker-output", SizeBytes: 12}}, nil
		},
		artifactCatFn: func(ref string) (api.ArtifactContentResponse, error) {
			return api.ArtifactContentResponse{Content: "artifact body"}, nil
		},
	}, bytes.NewBufferString("/approve approval-1\n/plan\n/handoff worker-1\n/artifact artifact://worker-output-1\n/cancel\n/quit\n"), &bytes.Buffer{})
	console.lastRunID = "run-1"
	if err := console.Run(context.Background(), 1001, "1001:default"); err != nil {
		t.Fatalf("Run: %v", err)
	}
	if !cancelled {
		t.Fatal("expected cancel command to call CancelRun")
	}
	out := console.Output().String()
	for _, want := range []string{
		"approval: approval-1 approved",
		"plan: Investigate rollout",
		"worker: worker-1 handoff",
		"artifact: artifact://worker-output-1",
		"system: cancel requested for run-1",
	} {
		if !strings.Contains(out, want) {
			t.Fatalf("missing %q in output %q", want, out)
		}
	}
}

func TestChatConsoleCompletionCandidates(t *testing.T) {
	console := NewChatConsole(chatConsoleClientStub{}, &bytes.Buffer{}, &bytes.Buffer{})
	console.knownApprovals = []string{"approval-1", "approval-2"}
	console.knownWorkers = []string{"worker-1"}
	console.knownArtifacts = []string{"artifact://worker-output-1"}

	if got := console.completeLine("/appr"); len(got) != 2 || got[0] != "/approve" || got[1] != "/approve " {
		t.Fatalf("unexpected command completion: %#v", got)
	}
	if got := console.completeLine("/approve appr"); len(got) != 2 || got[0] != "approval-1" || got[1] != "approval-2" {
		t.Fatalf("unexpected approval completion: %#v", got)
	}
	if got := console.completeLine("/handoff work"); len(got) != 1 || got[0] != "worker-1" {
		t.Fatalf("unexpected worker completion: %#v", got)
	}
	if got := console.completeLine("/artifact art"); len(got) != 1 || got[0] != "artifact://worker-output-1" {
		t.Fatalf("unexpected artifact completion: %#v", got)
	}
}

func TestChatConsoleAllowsApproveWhileRunIsActive(t *testing.T) {
	var approved atomic.Bool
	started := make(chan struct{}, 1)
	release := make(chan struct{})
	console := NewChatConsole(chatConsoleClientStub{
		startRunFn: func(chatID int64, sessionID, query string) (api.CreateRunResponse, error) {
			return api.CreateRunResponse{
				RunID:    "run-1",
				Accepted: true,
				Run:      runtime.RunView{RunID: "run-1", Status: runtime.StatusRunning, ChatID: chatID, SessionID: sessionID},
			}, nil
		},
		runStatusFn: func(runID string) (api.RunStatusResponse, error) {
			status := runtime.StatusWaitingApproval
			final := ""
			if approved.Load() {
				status = runtime.StatusCompleted
				final = "approved and finished"
			}
			return api.RunStatusResponse{Run: runtime.RunView{RunID: runID, Status: status, ChatID: 1001, SessionID: "1001:default", FinalResponse: final}}, nil
		},
		streamEventsFn: func(ctx context.Context, req api.EventListRequest, onEvent func(runtime.RuntimeEvent) error) error {
			if err := onEvent(runtime.RuntimeEvent{ID: 1, EntityType: "run", EntityID: "run-1", Kind: "run.started"}); err != nil {
				return err
			}
			if err := onEvent(runtime.RuntimeEvent{ID: 2, EntityType: "run", EntityID: "run-1", Kind: "approval.requested", Payload: []byte(`{"approval_id":"approval-1","tool":"shell.exec"}`)}); err != nil {
				return err
			}
			started <- struct{}{}
			<-release
			if err := onEvent(runtime.RuntimeEvent{ID: 3, EntityType: "run", EntityID: "run-1", Kind: "assistant.final", Payload: []byte(`{"text":"approved and finished"}`)}); err != nil {
				return err
			}
			return onEvent(runtime.RuntimeEvent{ID: 4, EntityType: "run", EntityID: "run-1", Kind: "run.completed"})
		},
		approveFn: func(id string) (api.ApprovalRecordResponse, error) {
			if id != "approval-1" {
				t.Fatalf("unexpected approval id: %s", id)
			}
			approved.Store(true)
			close(release)
			return api.ApprovalRecordResponse{ID: id, Status: "approved"}, nil
		},
	}, &bytes.Buffer{}, &bytes.Buffer{})

	go func() {
		if _, err := console.handleInputLine(context.Background(), 1001, "1001:default", "нужен ls -la"); err != nil {
			t.Errorf("start line: %v", err)
		}
	}()
	select {
	case <-started:
	case <-time.After(time.Second):
		t.Fatal("run did not reach approval state")
	}
	if _, err := console.handleInputLine(context.Background(), 1001, "1001:default", "/approve approval-1"); err != nil {
		t.Fatalf("approve line: %v", err)
	}
	console.waitForActiveRun()

	out := console.Output().String()
	for _, want := range []string{
		"approval: requested approval-1 for shell.exec",
		"approval: approval-1 approved",
		"assistant: approved and finished",
		"system: run completed",
	} {
		if !strings.Contains(out, want) {
			t.Fatalf("missing %q in output %q", want, out)
		}
	}
}

func TestChatConsoleApproveHandlesUnknownApprovalGracefully(t *testing.T) {
	console := NewChatConsole(chatConsoleClientStub{
		approveFn: func(id string) (api.ApprovalRecordResponse, error) {
			return api.ApprovalRecordResponse{}, fmt.Errorf("approval_error: approval not found: %s", id)
		},
	}, &bytes.Buffer{}, &bytes.Buffer{})

	if _, err := console.handleInputLine(context.Background(), 1001, "1001:default", "/approve approval-17759"); err != nil {
		t.Fatalf("approve line should not fail: %v", err)
	}
	out := console.Output().String()
	if !strings.Contains(out, "system: approval_error: approval not found: approval-17759") {
		t.Fatalf("unexpected output: %q", out)
	}
}

func TestChatConsoleApproveResolvesUniquePrefix(t *testing.T) {
	console := NewChatConsole(chatConsoleClientStub{
		approveFn: func(id string) (api.ApprovalRecordResponse, error) {
			if id != "approval-1775984578647293815" {
				t.Fatalf("unexpected approval id: %s", id)
			}
			return api.ApprovalRecordResponse{ID: id, Status: "approved"}, nil
		},
	}, &bytes.Buffer{}, &bytes.Buffer{})
	console.knownApprovals = []string{"approval-1775984578647293815"}

	if _, err := console.handleInputLine(context.Background(), 1001, "1001:default", "/approve approval-17759"); err != nil {
		t.Fatalf("approve line: %v", err)
	}
	out := console.Output().String()
	if !strings.Contains(out, "approval: approval-1775984578647293815 approved") {
		t.Fatalf("unexpected output: %q", out)
	}
}

func TestChatConsoleStatusShowsWaitingWorkerApproval(t *testing.T) {
	console := NewChatConsole(chatConsoleClientStub{
		controlFn: func(sessionID string, chatID int64) (api.ControlStateResponse, error) {
			return api.ControlStateResponse{Control: runtime.ControlState{
				Session: runtime.SessionState{
					SessionID: sessionID,
					LatestRun: &runtime.RunView{RunID: "run-1", Status: runtime.StatusRunning},
				},
				Approvals: []runtime.ApprovalView{{
					ID:         "approval-1",
					SessionID:  sessionID,
					Status:     "pending",
					TargetType: "run",
					TargetID:   "worker-run-1",
				}},
				Workers: []runtime.WorkerView{{
					WorkerID:  "worker-3",
					Status:    runtime.WorkerWaitingApproval,
					LastRunID: "worker-run-1",
				}},
			}}, nil
		},
	}, bytes.NewBufferString("/status\n/quit\n"), &bytes.Buffer{})
	console.sessionID = "1001:default"
	console.lastRunID = "run-1"
	if err := console.Run(context.Background(), 1001, "1001:default"); err != nil {
		t.Fatalf("Run: %v", err)
	}
	out := console.Output().String()
	for _, want := range []string{
		"system: run run-1 running",
		"approval: approval-1 pending target=worker-run-1",
		"worker: worker-3 waiting_approval run=worker-run-1 approval=approval-1",
	} {
		if !strings.Contains(out, want) {
			t.Fatalf("missing %q in output %q", want, out)
		}
	}
}
