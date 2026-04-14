package approvals

import (
	"context"
	"testing"
	"time"
)

func TestServiceHandleCallbackIsIdempotentByUpdateID(t *testing.T) {
	svc := New(TestDeps())
	record, err := svc.Create(Request{
		WorkerID:  "shell.exec",
		SessionID: "1001:default",
		Payload:   "{}",
	})
	if err != nil {
		t.Fatalf("create: %v", err)
	}

	first, err := svc.HandleCallback(Callback{
		ApprovalID: record.ID,
		Action:     ActionApprove,
		UpdateID:   "42",
	})
	if err != nil {
		t.Fatalf("first callback: %v", err)
	}
	second, err := svc.HandleCallback(Callback{
		ApprovalID: record.ID,
		Action:     ActionReject,
		UpdateID:   "42",
	})
	if err != nil {
		t.Fatalf("second callback: %v", err)
	}

	if first.Status != StatusApproved || second.Status != StatusApproved {
		t.Fatalf("expected idempotent approved status, got first=%s second=%s", first.Status, second.Status)
	}
}

func TestServiceWaitReturnsApprovedRecord(t *testing.T) {
	svc := New(TestDeps())
	record, err := svc.Create(Request{
		WorkerID:  "shell.exec",
		SessionID: "1001:default",
		Payload:   "{}",
	})
	if err != nil {
		t.Fatalf("create: %v", err)
	}

	done := make(chan Record, 1)
	go func() {
		got, err := svc.Wait(context.Background(), record.ID)
		if err != nil {
			t.Errorf("wait: %v", err)
			return
		}
		done <- got
	}()

	time.Sleep(10 * time.Millisecond)
	if _, err := svc.HandleCallback(Callback{
		ApprovalID: record.ID,
		Action:     ActionApprove,
		UpdateID:   "43",
	}); err != nil {
		t.Fatalf("approve: %v", err)
	}

	select {
	case got := <-done:
		if got.Status != StatusApproved {
			t.Fatalf("unexpected wait result: %+v", got)
		}
	case <-time.After(time.Second):
		t.Fatal("wait did not unblock")
	}
}

func TestServicePersistsApprovalAuditFields(t *testing.T) {
	svc := New(TestDeps())
	record, err := svc.Create(Request{
		WorkerID:   "shell.exec",
		SessionID:  "1001:default",
		Payload:    "{}",
		Reason:     "shell.exec requires approval by action policy",
		TargetType: "run",
		TargetID:   "run-1",
	})
	if err != nil {
		t.Fatalf("create: %v", err)
	}
	if record.Reason == "" || record.TargetType != "run" || record.TargetID != "run-1" || record.RequestedAt.IsZero() {
		t.Fatalf("missing approval audit fields on create: %+v", record)
	}

	decided, err := svc.HandleCallback(Callback{
		ApprovalID: record.ID,
		Action:     ActionApprove,
		UpdateID:   "update-1",
	})
	if err != nil {
		t.Fatalf("approve: %v", err)
	}
	if decided.DecisionUpdateID != "update-1" || decided.DecidedAt == nil || decided.Status != StatusApproved {
		t.Fatalf("missing approval decision audit fields: %+v", decided)
	}
}
