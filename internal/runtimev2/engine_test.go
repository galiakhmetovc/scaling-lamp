package runtimev2

import "testing"

func TestRunSnapshotV2States(t *testing.T) {
	tests := []struct {
		name          string
		snapshot      RunSnapshotV2
		wantStatus    RunStatusV2
		wantApprovals int
		wantProcesses int
		wantMessages  int
		wantStream    bool
		wantResult    bool
		wantError     bool
	}{
		{
			name: "running",
			snapshot: RunSnapshotV2{
				RunID:              "run-1",
				SessionID:          "session-1",
				Status:             RunStatusRunning,
				QueuedUserMessages: []QueuedUserMessageV2{{Role: "user", Content: "hello"}},
				ProviderStream:     &ProviderStreamV2{Phase: "streaming"},
			},
			wantStatus:   RunStatusRunning,
			wantMessages: 1,
			wantStream:   true,
		},
		{
			name: "waiting approval",
			snapshot: RunSnapshotV2{
				RunID:            "run-2",
				SessionID:        "session-1",
				Status:           RunStatusWaitingApproval,
				PendingApprovals: []PendingApprovalV2{{ID: "approval-1", Reason: "need approval"}},
			},
			wantStatus:    RunStatusWaitingApproval,
			wantApprovals: 1,
		},
		{
			name: "waiting process",
			snapshot: RunSnapshotV2{
				RunID:           "run-3",
				SessionID:       "session-1",
				Status:          RunStatusWaitingProcess,
				ActiveProcesses: []ActiveProcessV2{{ID: "proc-1", Command: "sleep 1"}},
			},
			wantStatus:    RunStatusWaitingProcess,
			wantProcesses: 1,
		},
		{
			name: "resuming",
			snapshot: RunSnapshotV2{
				RunID:              "run-4",
				SessionID:          "session-1",
				Status:             RunStatusResuming,
				QueuedUserMessages: []QueuedUserMessageV2{{Role: "user", Content: "resume"}},
				ProviderStream:     &ProviderStreamV2{Phase: "resuming"},
			},
			wantStatus:   RunStatusResuming,
			wantMessages: 1,
			wantStream:   true,
		},
		{
			name: "completed",
			snapshot: RunSnapshotV2{
				RunID:     "run-5",
				SessionID: "session-1",
				Status:    RunStatusCompleted,
				Result:    &RunResultV2{State: "completed", Summary: "done"},
			},
			wantStatus: RunStatusCompleted,
			wantResult: true,
		},
		{
			name: "failed",
			snapshot: RunSnapshotV2{
				RunID:     "run-6",
				SessionID: "session-1",
				Status:    RunStatusFailed,
				Result:    &RunResultV2{State: "failed"},
				Error:     "boom",
			},
			wantStatus: RunStatusFailed,
			wantResult: true,
			wantError:  true,
		},
		{
			name: "cancelled",
			snapshot: RunSnapshotV2{
				RunID:     "run-7",
				SessionID: "session-1",
				Status:    RunStatusCancelled,
				Result:    &RunResultV2{State: "cancelled"},
				Error:     "cancelled by user",
			},
			wantStatus: RunStatusCancelled,
			wantResult: true,
			wantError:  true,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if tc.snapshot.Status != tc.wantStatus {
				t.Fatalf("status = %q, want %q", tc.snapshot.Status, tc.wantStatus)
			}
			if got := len(tc.snapshot.PendingApprovals); got != tc.wantApprovals {
				t.Fatalf("pending approvals = %d, want %d", got, tc.wantApprovals)
			}
			if got := len(tc.snapshot.ActiveProcesses); got != tc.wantProcesses {
				t.Fatalf("active processes = %d, want %d", got, tc.wantProcesses)
			}
			if got := len(tc.snapshot.QueuedUserMessages); got != tc.wantMessages {
				t.Fatalf("queued user messages = %d, want %d", got, tc.wantMessages)
			}
			if (tc.snapshot.ProviderStream != nil) != tc.wantStream {
				t.Fatalf("provider stream presence = %t, want %t", tc.snapshot.ProviderStream != nil, tc.wantStream)
			}
			if (tc.snapshot.Result != nil) != tc.wantResult {
				t.Fatalf("result presence = %t, want %t", tc.snapshot.Result != nil, tc.wantResult)
			}
			if (tc.snapshot.Error != "") != tc.wantError {
				t.Fatalf("error presence = %t, want %t", tc.snapshot.Error != "", tc.wantError)
			}
		})
	}
}

func TestRunStore(t *testing.T) {
	store := NewRunStore()

	running := RunSnapshotV2{
		RunID:     "run-1",
		SessionID: "session-1",
		Status:    RunStatusRunning,
	}
	waitingApproval := RunSnapshotV2{
		RunID:            "run-2",
		SessionID:        "session-1",
		Status:           RunStatusWaitingApproval,
		PendingApprovals: []PendingApprovalV2{{ID: "approval-1"}},
	}
	completed := RunSnapshotV2{
		RunID:     "run-3",
		SessionID: "session-1",
		Status:    RunStatusCompleted,
		Result:    &RunResultV2{State: "completed"},
	}
	otherSession := RunSnapshotV2{
		RunID:     "run-4",
		SessionID: "session-2",
		Status:    RunStatusRunning,
	}

	for _, snapshot := range []RunSnapshotV2{running, waitingApproval, completed, otherSession} {
		if err := store.Create(snapshot); err != nil {
			t.Fatalf("create %s: %v", snapshot.RunID, err)
		}
	}

	got, ok := store.Get("run-2")
	if !ok {
		t.Fatal("expected run-2 to exist")
	}
	if got.RunID != waitingApproval.RunID || got.SessionID != waitingApproval.SessionID {
		t.Fatalf("get returned %+v, want %+v", got, waitingApproval)
	}

	if err := store.Update("run-1", func(snapshot *RunSnapshotV2) error {
		snapshot.Status = RunStatusWaitingProcess
		snapshot.ActiveProcesses = []ActiveProcessV2{{ID: "proc-1"}}
		return nil
	}); err != nil {
		t.Fatalf("update run-1: %v", err)
	}

	updated, ok := store.Get("run-1")
	if !ok {
		t.Fatal("expected updated run-1 to exist")
	}
	if updated.Status != RunStatusWaitingProcess || len(updated.ActiveProcesses) != 1 {
		t.Fatalf("unexpected updated snapshot: %+v", updated)
	}

	active := store.ListActiveBySession("session-1")
	if len(active) != 2 {
		t.Fatalf("active runs = %d, want 2", len(active))
	}

	if err := store.Delete("run-2"); err != nil {
		t.Fatalf("delete run-2: %v", err)
	}
	if _, ok := store.Get("run-2"); ok {
		t.Fatal("expected run-2 to be deleted")
	}
}

func TestRunStoreCreateNormalizesIdentifiers(t *testing.T) {
	store := NewRunStore()

	snapshot := RunSnapshotV2{
		RunID:     "  run-1  ",
		SessionID: "  session-1  ",
		Status:    RunStatusRunning,
	}
	if err := store.Create(snapshot); err != nil {
		t.Fatalf("create: %v", err)
	}

	got, ok := store.Get("run-1")
	if !ok {
		t.Fatal("expected lookup by normalized key to succeed")
	}
	if got.RunID != "run-1" {
		t.Fatalf("run id = %q, want %q", got.RunID, "run-1")
	}
	if got.SessionID != "session-1" {
		t.Fatalf("session id = %q, want %q", got.SessionID, "session-1")
	}

	active := store.ListActiveBySession("session-1")
	if len(active) != 1 {
		t.Fatalf("active runs = %d, want 1", len(active))
	}
	if active[0].RunID != "run-1" {
		t.Fatalf("active run id = %q, want %q", active[0].RunID, "run-1")
	}

	if _, ok := store.Get("  run-1  "); ok {
		t.Fatal("expected padded lookup to miss after normalization")
	}
}

func TestRunStoreUpdateRejectsKeyFieldChanges(t *testing.T) {
	store := NewRunStore()
	if err := store.Create(RunSnapshotV2{RunID: "run-1", SessionID: "session-1", Status: RunStatusRunning}); err != nil {
		t.Fatalf("create: %v", err)
	}

	err := store.Update("run-1", func(snapshot *RunSnapshotV2) error {
		snapshot.RunID = "run-2"
		snapshot.SessionID = "session-2"
		return nil
	})
	if err == nil {
		t.Fatal("expected update to reject key field changes")
	}

	got, ok := store.Get("run-1")
	if !ok {
		t.Fatal("expected original run to remain present")
	}
	if got.RunID != "run-1" || got.SessionID != "session-1" {
		t.Fatalf("snapshot mutated after rejected update: %+v", got)
	}

	if _, ok := store.Get("run-2"); ok {
		t.Fatal("unexpected new key created after rejected update")
	}
}
