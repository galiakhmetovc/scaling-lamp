package runtime

import (
	"strings"
	"testing"
	"time"

	"teamd/internal/approvals"
)

func TestRunReplayIncludesEventsAndFinalResponse(t *testing.T) {
	store := &apiTestReplayStore{
		run: RunRecord{
			RunID:         "run-1",
			ChatID:        1001,
			SessionID:     "1001:default",
			Query:         "hello",
			FinalResponse: "done",
			Status:        StatusCompleted,
			StartedAt:     time.Now().UTC().Add(-time.Minute),
		},
		ok: true,
		events: []RuntimeEvent{
			{ID: 1, RunID: "run-1", Kind: "tool.started", Payload: []byte(`{"tool":"shell.exec"}`), CreatedAt: time.Now().UTC().Add(-30 * time.Second)},
			{ID: 2, RunID: "run-1", Kind: "run.completed", CreatedAt: time.Now().UTC()},
		},
	}
	api := NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps()))

	replay, ok, err := api.RunReplay("run-1")
	if err != nil {
		t.Fatalf("run replay: %v", err)
	}
	if !ok {
		t.Fatal("expected replay to exist")
	}
	if len(replay.Steps) != 4 {
		t.Fatalf("expected 4 steps, got %d", len(replay.Steps))
	}
	if replay.Steps[1].Kind != "tool.started" {
		t.Fatalf("unexpected second step: %#v", replay.Steps[1])
	}
	if replay.Steps[len(replay.Steps)-1].Kind != "assistant.final" {
		t.Fatalf("unexpected final step: %#v", replay.Steps[len(replay.Steps)-1])
	}
}

func TestRunReplaySummarizesArchiveAndArtifactRefs(t *testing.T) {
	store := &apiTestReplayStore{
		run: RunRecord{
			RunID:     "run-1",
			ChatID:    1001,
			SessionID: "1001:default",
			Query:     "hello",
			Status:    StatusCompleted,
			StartedAt: time.Now().UTC().Add(-time.Minute),
		},
		ok: true,
		events: []RuntimeEvent{
			{ID: 1, RunID: "run-1", Kind: "checkpoint.saved", Payload: []byte(`{"archive_refs":["archive://chat/1001/session/default#messages-1-4"],"artifact_refs":["artifact://tool-output/1"]}`), CreatedAt: time.Now().UTC()},
		},
	}
	api := NewAPI(store, NewActiveRegistry(), approvals.New(approvals.TestDeps()))

	replay, ok, err := api.RunReplay("run-1")
	if err != nil {
		t.Fatalf("run replay: %v", err)
	}
	if !ok {
		t.Fatal("expected replay to exist")
	}
	if !strings.Contains(replay.Steps[1].Message, "archive_refs=archive://chat/1001/session/default#messages-1-4") {
		t.Fatalf("expected archive refs in replay message, got %#v", replay.Steps[1])
	}
	if !strings.Contains(replay.Steps[1].Message, "artifact_refs=artifact://tool-output/1") {
		t.Fatalf("expected artifact refs in replay message, got %#v", replay.Steps[1])
	}
}

type apiTestReplayStore struct {
	run    RunRecord
	ok     bool
	events []RuntimeEvent
}

func (s *apiTestReplayStore) SaveRun(run RunRecord) error            { s.run = run; s.ok = true; return nil }
func (s *apiTestReplayStore) MarkCancelRequested(runID string) error { return nil }
func (s *apiTestReplayStore) Run(runID string) (RunRecord, bool, error) {
	return s.run, s.ok && s.run.RunID == runID, nil
}
func (s *apiTestReplayStore) ListRuns(query RunQuery) ([]RunRecord, error) {
	return []RunRecord{s.run}, nil
}
func (s *apiTestReplayStore) ListSessions(query SessionQuery) ([]SessionRecord, error) {
	return nil, nil
}
func (s *apiTestReplayStore) SaveEvent(RuntimeEvent) error { return nil }
func (s *apiTestReplayStore) ListEvents(query EventQuery) ([]RuntimeEvent, error) {
	return append([]RuntimeEvent(nil), s.events...), nil
}
func (s *apiTestReplayStore) RecoverInterruptedRuns(reason string) (int, error) { return 0, nil }
