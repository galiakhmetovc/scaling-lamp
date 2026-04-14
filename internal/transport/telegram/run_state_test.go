package telegram

import (
	"testing"
	"time"
)

func TestRunStateTracksActiveRunAndElapsedTime(t *testing.T) {
	state := NewRunStateStore()
	startedAt := time.Now().UTC()
	runID := state.AllocateID()
	state.CreateWithID(1001, runID, "deploy", startedAt)

	run, ok := state.Active(1001)
	if !ok || run.ID != runID {
		t.Fatalf("expected active run, got %#v", run)
	}
	if run.Query != "deploy" {
		t.Fatalf("unexpected query: %#v", run)
	}
	if run.StartedAt != startedAt {
		t.Fatalf("unexpected started_at: %#v", run.StartedAt)
	}
}

func TestRunStateFinishRemovesActiveRun(t *testing.T) {
	state := NewRunStateStore()
	state.CreateWithID(1001, state.AllocateID(), "deploy", time.Now().UTC())
	state.Finish(1001)
	if _, ok := state.Active(1001); ok {
		t.Fatal("expected no active run")
	}
}
