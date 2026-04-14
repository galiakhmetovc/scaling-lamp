package api

import (
	"errors"
	"testing"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/runtime"
)

func TestAPITypesExposeStableRuntimeContract(t *testing.T) {
	now := time.Now().UTC()
	resp := CreateRunResponse{
		RunID:    "run-1",
		Accepted: true,
		Run: runtime.RunView{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Query:     "hello",
			Status:    runtime.StatusRunning,
			StartedAt: now,
			Active:    true,
		},
	}
	if resp.Run.RunID != "run-1" || !resp.Run.Active {
		t.Fatalf("unexpected create run response: %+v", resp)
	}

	approval := ApprovalRecordResponse{
		ID:               "approval-1",
		WorkerID:         "shell.exec",
		SessionID:        "1001:default",
		Payload:          "{}",
		Status:           approvals.StatusPending,
		Reason:           "shell.exec requires approval by action policy",
		TargetType:       "run",
		TargetID:         "run-1",
		DecisionUpdateID: "api-1",
	}
	if approval.Status != approvals.StatusPending {
		t.Fatalf("unexpected approval response: %+v", approval)
	}
	if approval.Reason == "" || approval.TargetType != "run" || approval.TargetID != "run-1" {
		t.Fatalf("missing approval audit fields: %+v", approval)
	}

	errResp := NewErrorResponse("bad_request", "invalid request")
	if errResp.Error.Code != "bad_request" || errResp.Error.Message != "invalid request" || errResp.Time.IsZero() {
		t.Fatalf("unexpected error response: %+v", errResp)
	}

	runtimeErr := NewRuntimeErrorResponse(&runtime.ControlError{
		Code:       runtime.ErrTool,
		Message:    "tool failed",
		EntityType: "run",
		EntityID:   "run-1",
		Retryable:  true,
		Cause:      errors.New("boom"),
	}, "internal_error", "internal error")
	if runtimeErr.Error.Code != string(runtime.ErrTool) || runtimeErr.Error.EntityType != "run" || runtimeErr.Error.EntityID != "run-1" || !runtimeErr.Error.Retryable {
		t.Fatalf("unexpected runtime error response: %+v", runtimeErr)
	}
}
