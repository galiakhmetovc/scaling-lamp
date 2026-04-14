package runtime

import (
	"testing"
	"time"

	"teamd/internal/approvals"
)

func TestSQLiteStorePersistsApprovalState(t *testing.T) {
	store, err := NewSQLiteStore(localRuntimeDBPath(t))
	if err != nil {
		t.Fatalf("new sqlite store: %v", err)
	}

	record := approvals.Record{
		ID:        "approval-1",
		WorkerID:  "shell.exec",
		SessionID: "1001:default",
		Payload:   "{}",
		Status:    approvals.StatusPending,
	}
	if err := store.SaveApproval(record); err != nil {
		t.Fatalf("save approval: %v", err)
	}
	got, ok, err := store.Approval("approval-1")
	if err != nil || !ok {
		t.Fatalf("approval lookup: ok=%v err=%v", ok, err)
	}
	if got.Status != approvals.StatusPending || got.WorkerID != "shell.exec" {
		t.Fatalf("unexpected approval: %+v", got)
	}

	record.Status = approvals.StatusApproved
	if err := store.SaveApproval(record); err != nil {
		t.Fatalf("save updated approval: %v", err)
	}
	if err := store.SaveHandledApprovalCallback("upd-1", record); err != nil {
		t.Fatalf("save handled callback: %v", err)
	}
	handled, ok, err := store.HandledApprovalCallback("upd-1")
	if err != nil || !ok {
		t.Fatalf("handled callback lookup: ok=%v err=%v", ok, err)
	}
	if handled.Status != approvals.StatusApproved {
		t.Fatalf("unexpected handled callback record: %+v", handled)
	}

	continuation := ApprovalContinuation{
		ApprovalID:    "approval-1",
		RunID:         "run-1",
		ChatID:        1001,
		SessionID:     "1001:default",
		Query:         "do the thing",
		ToolCallID:    "call-1",
		ToolName:      "shell.exec",
		ToolArguments: map[string]any{"command": "pwd"},
		RequestedAt:   nowForTest(),
	}
	if err := store.SaveApprovalContinuation(continuation); err != nil {
		t.Fatalf("save continuation: %v", err)
	}
	gotCont, ok, err := store.ApprovalContinuation("approval-1")
	if err != nil || !ok {
		t.Fatalf("continuation lookup: ok=%v err=%v", ok, err)
	}
	if gotCont.RunID != "run-1" || gotCont.ToolName != "shell.exec" {
		t.Fatalf("unexpected continuation: %+v", gotCont)
	}
	if err := store.DeleteApprovalContinuation("approval-1"); err != nil {
		t.Fatalf("delete continuation: %v", err)
	}
	if _, ok, err := store.ApprovalContinuation("approval-1"); err != nil || ok {
		t.Fatalf("expected deleted continuation, ok=%v err=%v", ok, err)
	}
}

func nowForTest() time.Time {
	return time.Date(2026, 4, 11, 18, 30, 0, 0, time.UTC)
}
