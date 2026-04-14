package runtime

import (
	"testing"
	"time"
)

func TestSQLiteStorePersistsRunCheckpointAndContinuity(t *testing.T) {
	path := t.TempDir() + "/runtime.db"
	store, err := NewSQLiteStore(path)
	if err != nil {
		t.Fatalf("new sqlite store: %v", err)
	}

	started := time.Now().UTC().Round(time.Second)
	finished := started.Add(5 * time.Second)
	run := RunRecord{
		RunID:           "run-1",
		ChatID:          1001,
		SessionID:       "1001:default",
		Query:           "проверь память",
		Status:          StatusCompleted,
		StartedAt:       started,
		EndedAt:         &finished,
		CancelRequested: false,
	}
	if err := store.SaveRun(run); err != nil {
		t.Fatalf("save run: %v", err)
	}
	if err := store.MarkCancelRequested("run-1"); err != nil {
		t.Fatalf("mark cancel requested: %v", err)
	}

	checkpoint := Checkpoint{
		ChatID:            1001,
		SessionID:         "1001:default",
		OriginatingIntent: "проверь память",
		WhatHappened:      "прочитал /proc/meminfo",
		WhatMattersNow:    "RAM почти заполнена",
		ArchiveRefs:       []string{"archive://chat/1001/session/default#messages-1-4"},
		ArtifactRefs:      []string{"artifact://memory-snapshot/1"},
		UpdatedAt:         started,
	}
	if err := store.SaveCheckpoint(checkpoint); err != nil {
		t.Fatalf("save checkpoint: %v", err)
	}

	continuity := Continuity{
		ChatID:          1001,
		SessionID:       "1001:default",
		UserGoal:        "проверить память и дать совет",
		CurrentState:    "ответ отправлен",
		ResolvedFacts:   []string{"RAM почти заполнена"},
		UnresolvedItems: []string{"нужно решить, чистить ли кеш"},
		ArchiveRefs:     []string{"archive://chat/1001/session/default#messages-1-4"},
		ArtifactRefs:    []string{"artifact://memory-snapshot/1"},
		UpdatedAt:       started,
	}
	if err := store.SaveContinuity(continuity); err != nil {
		t.Fatalf("save continuity: %v", err)
	}

	gotRun, ok, err := store.Run("run-1")
	if err != nil {
		t.Fatalf("load run: %v", err)
	}
	if !ok {
		t.Fatal("expected run record")
	}
	if !gotRun.CancelRequested || gotRun.Status != StatusCompleted {
		t.Fatalf("unexpected run record: %#v", gotRun)
	}

	gotCheckpoint, ok, err := store.Checkpoint(1001, "1001:default")
	if err != nil {
		t.Fatalf("load checkpoint: %v", err)
	}
	if !ok || gotCheckpoint.OriginatingIntent != "проверь память" {
		t.Fatalf("unexpected checkpoint: %#v", gotCheckpoint)
	}
	if len(gotCheckpoint.ArchiveRefs) != 1 || len(gotCheckpoint.ArtifactRefs) != 1 {
		t.Fatalf("unexpected checkpoint refs: %#v", gotCheckpoint)
	}

	gotContinuity, ok, err := store.Continuity(1001, "1001:default")
	if err != nil {
		t.Fatalf("load continuity: %v", err)
	}
	if !ok || gotContinuity.UserGoal != "проверить память и дать совет" {
		t.Fatalf("unexpected continuity: %#v", gotContinuity)
	}
	if len(gotContinuity.ArchiveRefs) != 1 || len(gotContinuity.ArtifactRefs) != 1 {
		t.Fatalf("unexpected continuity refs: %#v", gotContinuity)
	}
}

func TestSQLiteStoreRecoversInterruptedRunsAndDeduplicatesUpdates(t *testing.T) {
	path := t.TempDir() + "/runtime.db"
	store, err := NewSQLiteStore(path)
	if err != nil {
		t.Fatalf("new sqlite store: %v", err)
	}

	started := time.Now().UTC().Round(time.Second)
	if err := store.SaveRun(RunRecord{
		RunID:     "run-queued",
		ChatID:    1001,
		SessionID: "1001:default",
		Query:     "queued run",
		Status:    StatusQueued,
		StartedAt: started,
	}); err != nil {
		t.Fatalf("save queued: %v", err)
	}
	if err := store.SaveRun(RunRecord{
		RunID:     "run-running",
		ChatID:    1002,
		SessionID: "1002:default",
		Query:     "running run",
		Status:    StatusRunning,
		StartedAt: started,
	}); err != nil {
		t.Fatalf("save running: %v", err)
	}

	recovered, err := store.RecoverInterruptedRuns("process restarted")
	if err != nil {
		t.Fatalf("recover interrupted runs: %v", err)
	}
	if recovered != 2 {
		t.Fatalf("expected 2 recovered runs, got %d", recovered)
	}
	got, ok, err := store.Run("run-running")
	if err != nil || !ok {
		t.Fatalf("load recovered run: %#v %v", got, err)
	}
	if got.Status != StatusFailed || got.FailureReason != "process restarted" || got.EndedAt == nil {
		t.Fatalf("unexpected recovered run: %#v", got)
	}

	first, err := store.TryMarkUpdate(1001, 42)
	if err != nil {
		t.Fatalf("mark first update: %v", err)
	}
	second, err := store.TryMarkUpdate(1001, 42)
	if err != nil {
		t.Fatalf("mark duplicate update: %v", err)
	}
	if !first || second {
		t.Fatalf("expected first=true second=false, got %v %v", first, second)
	}
}

func TestSQLiteStorePersistsRuntimeEvents(t *testing.T) {
	path := t.TempDir() + "/runtime.db"
	store, err := NewSQLiteStore(path)
	if err != nil {
		t.Fatalf("new sqlite store: %v", err)
	}
	createdAt := time.Now().UTC().Round(time.Second)
	if err := store.SaveEvent(RuntimeEvent{
		EntityType: "run",
		EntityID:   "run-1",
		ChatID:     1001,
		SessionID:  "1001:default",
		RunID:      "run-1",
		Kind:       "run.started",
		Payload:    []byte(`{"query":"hello"}`),
		CreatedAt:  createdAt,
	}); err != nil {
		t.Fatalf("save event: %v", err)
	}
	items, err := store.ListEvents(EventQuery{EntityType: "run", EntityID: "run-1", Limit: 10})
	if err != nil {
		t.Fatalf("list events: %v", err)
	}
	if len(items) != 1 || items[0].Kind != "run.started" || items[0].RunID != "run-1" {
		t.Fatalf("unexpected events: %+v", items)
	}
}

func TestSQLiteStorePersistsSessionHead(t *testing.T) {
	path := t.TempDir() + "/runtime.db"
	store, err := NewSQLiteStore(path)
	if err != nil {
		t.Fatalf("new sqlite store: %v", err)
	}

	updatedAt := time.Now().UTC().Round(time.Second)
	head := SessionHead{
		ChatID:             1001,
		SessionID:          "1001:default",
		LastCompletedRunID: "run-1",
		CurrentGoal:        "обновить шаблон астры",
		LastResultSummary:  "шаблон включен, проверен, выключен",
		ResolvedEntities:   []string{"tpl-astra-1.7-clean", "10.31.211.17"},
		RecentArtifactRefs: []string{"artifact://run/run-1/report"},
		OpenLoops:          []string{"оформить как проект"},
		CurrentProject:     "projects/astra-template-update",
		UpdatedAt:          updatedAt,
	}
	if err := store.SaveSessionHead(head); err != nil {
		t.Fatalf("save session head: %v", err)
	}

	got, ok, err := store.SessionHead(1001, "1001:default")
	if err != nil {
		t.Fatalf("load session head: %v", err)
	}
	if !ok {
		t.Fatal("expected session head")
	}
	if got.LastCompletedRunID != "run-1" || got.CurrentGoal != "обновить шаблон астры" {
		t.Fatalf("unexpected session head: %#v", got)
	}
	if len(got.ResolvedEntities) != 2 || len(got.RecentArtifactRefs) != 1 || len(got.OpenLoops) != 1 {
		t.Fatalf("unexpected session head collections: %#v", got)
	}
}
