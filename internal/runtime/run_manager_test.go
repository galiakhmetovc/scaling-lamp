package runtime

import (
	"context"
	"errors"
	"os"
	"slices"
	"testing"
	"time"
)

func localRuntimeDBPath(t *testing.T) string {
	t.Helper()
	dir, err := os.MkdirTemp(".", ".runtime-test-")
	if err != nil {
		t.Fatalf("mkdir temp: %v", err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(dir) })
	return dir + "/runtime.db"
}

func TestRunManagerStartsCancelsAndPersistsLifecycle(t *testing.T) {
	store, err := NewSQLiteStore(localRuntimeDBPath(t))
	if err != nil {
		t.Fatalf("new sqlite store: %v", err)
	}
	manager := NewRunManager(store, nil)
	started := make(chan struct{})
	done := make(chan struct{})

	runID, ok, err := manager.Start(context.Background(), 1001, "1001:default", "hello", func(ctx context.Context, runID string) error {
		close(started)
		<-ctx.Done()
		close(done)
		return ctx.Err()
	})
	if err != nil || !ok {
		t.Fatalf("start: %v %v", ok, err)
	}
	if _, ok := manager.Active(1001); !ok {
		t.Fatal("expected active run")
	}
	<-started
	if !manager.Cancel(1001) {
		t.Fatal("expected cancel")
	}
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("expected cancelled run to finish")
	}

	deadline := time.Now().Add(time.Second)
	for {
		record, ok, err := store.Run(runID)
		if err != nil {
			t.Fatalf("load run: %v", err)
		}
		if ok && record.Status == StatusCancelled && record.CancelRequested {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("expected cancelled record, got %#v", record)
		}
		time.Sleep(10 * time.Millisecond)
	}

	events, err := store.ListEvents(EventQuery{EntityType: "run", EntityID: runID, Limit: 10})
	if err != nil {
		t.Fatalf("list events: %v", err)
	}
	kinds := make([]string, 0, len(events))
	for _, event := range events {
		kinds = append(kinds, event.Kind)
	}
	for _, kind := range []string{"run.started", "run.cancel_requested", "run.cancelled"} {
		if !slices.Contains(kinds, kind) {
			t.Fatalf("expected event %q in %+v", kind, kinds)
		}
	}
}

func TestRunManagerRejectsSecondActiveRunPerChat(t *testing.T) {
	manager := NewRunManager(nil, nil)
	block := make(chan struct{})

	_, ok, err := manager.Start(context.Background(), 1001, "1001:default", "first", func(ctx context.Context, runID string) error {
		<-block
		return nil
	})
	if err != nil || !ok {
		t.Fatalf("start first: %v %v", ok, err)
	}
	if _, ok, err := manager.Start(context.Background(), 1001, "1001:default", "second", func(ctx context.Context, runID string) error {
		return nil
	}); err != nil || ok {
		t.Fatalf("expected second run to be rejected, got ok=%v err=%v", ok, err)
	}
	close(block)
}

func TestRunManagerPersistsFailure(t *testing.T) {
	store, err := NewSQLiteStore(localRuntimeDBPath(t))
	if err != nil {
		t.Fatalf("new sqlite store: %v", err)
	}
	manager := NewRunManager(store, nil)

	runID, ok, err := manager.Start(context.Background(), 1001, "1001:default", "boom", func(ctx context.Context, runID string) error {
		return errors.New("boom")
	})
	if err != nil || !ok {
		t.Fatalf("start: %v %v", ok, err)
	}

	deadline := time.Now().Add(time.Second)
	for {
		record, ok, err := store.Run(runID)
		if err != nil {
			t.Fatalf("load run: %v", err)
		}
		if ok && record.Status == StatusFailed && record.FailureReason == "boom" {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("expected failed record, got %#v", record)
		}
		time.Sleep(10 * time.Millisecond)
	}

	events, err := store.ListEvents(EventQuery{EntityType: "run", EntityID: runID, Limit: 10})
	if err != nil {
		t.Fatalf("list events: %v", err)
	}
	kinds := make([]string, 0, len(events))
	for _, event := range events {
		kinds = append(kinds, event.Kind)
	}
	for _, kind := range []string{"run.started", "run.failed"} {
		if !slices.Contains(kinds, kind) {
			t.Fatalf("expected event %q in %+v", kind, kinds)
		}
	}
}

func TestRunManagerGeneratesRestartSafeRunIDs(t *testing.T) {
	first := NewRunManager(nil, nil)
	firstPrepared, ok, err := first.Prepare(context.Background(), "", 1001, "1001:default", "hello", PolicySnapshot{})
	if err != nil || !ok {
		t.Fatalf("prepare first: ok=%v err=%v", ok, err)
	}
	second := NewRunManager(nil, nil)
	secondPrepared, ok, err := second.Prepare(context.Background(), "", 1001, "1001:default", "hello again", PolicySnapshot{})
	if err != nil || !ok {
		t.Fatalf("prepare second: ok=%v err=%v", ok, err)
	}
	if firstPrepared.Run.RunID == secondPrepared.Run.RunID {
		t.Fatalf("expected unique run ids across manager restarts, got %q", firstPrepared.Run.RunID)
	}
}
