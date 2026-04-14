package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"slices"
	"sync"
	"testing"
	"time"

	"teamd/internal/provider"
)

type jobStoreStub struct {
	mu     sync.Mutex
	jobs   map[string]JobRecord
	logs   []JobLogChunk
	events []RuntimeEvent
}

func newJobStoreStub() *jobStoreStub {
	return &jobStoreStub{jobs: make(map[string]JobRecord)}
}

func (s *jobStoreStub) SaveJob(record JobRecord) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.jobs[record.JobID] = record
	return nil
}

func (s *jobStoreStub) Job(jobID string) (JobRecord, bool, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	record, ok := s.jobs[jobID]
	return record, ok, nil
}

func (s *jobStoreStub) ListJobs(limit int) ([]JobRecord, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	items := make([]JobRecord, 0, len(s.jobs))
	for _, item := range s.jobs {
		items = append(items, item)
	}
	slices.SortFunc(items, func(a, b JobRecord) int {
		return b.StartedAt.Compare(a.StartedAt)
	})
	if limit > 0 && len(items) > limit {
		items = items[:limit]
	}
	return items, nil
}

func (s *jobStoreStub) MarkJobCancelRequested(jobID string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	record, ok := s.jobs[jobID]
	if !ok {
		return errors.New("missing job")
	}
	record.CancelRequested = true
	s.jobs[jobID] = record
	return nil
}

func (s *jobStoreStub) SaveEvent(event RuntimeEvent) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.events = append(s.events, event)
	return nil
}

func (s *jobStoreStub) SaveJobLog(chunk JobLogChunk) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	chunk.ID = int64(len(s.logs) + 1)
	s.logs = append(s.logs, chunk)
	return nil
}

func (s *jobStoreStub) JobLogs(query JobLogQuery) ([]JobLogChunk, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]JobLogChunk, 0, len(s.logs))
	for _, item := range s.logs {
		if item.JobID != query.JobID {
			continue
		}
		if query.Stream != "" && item.Stream != query.Stream {
			continue
		}
		if query.AfterID > 0 && item.ID <= query.AfterID {
			continue
		}
		out = append(out, item)
	}
	if query.Limit > 0 && len(out) > query.Limit {
		out = out[:query.Limit]
	}
	return out, nil
}

func (s *jobStoreStub) RecoverInterruptedJobs(string) (int, error) {
	return 0, nil
}

func waitForJobStatus(t *testing.T, jobs *JobsService, jobID string, want JobStatus) JobView {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		view, ok, err := jobs.Job(jobID)
		if err != nil {
			t.Fatalf("job lookup failed: %v", err)
		}
		if ok && view.Status == want {
			return view
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatalf("job %s did not reach status %s", jobID, want)
	return JobView{}
}

func eventKinds(events []RuntimeEvent) []string {
	out := make([]string, 0, len(events))
	for _, item := range events {
		out = append(out, item.Kind)
	}
	return out
}

func TestJobsServiceStartDetachedCapturesLogsAndEvents(t *testing.T) {
	store := newJobStoreStub()
	jobs := NewJobsService(store)

	view, err := jobs.StartDetached(context.Background(), JobStartRequest{
		JobID:     "job-test-1",
		ChatID:    11,
		SessionID: "session-1",
		Command:   "bash",
		Args:      []string{"-lc", "printf 'hello\\n'; printf 'oops\\n' >&2"},
	})
	if err != nil {
		t.Fatalf("start job: %v", err)
	}
	if view.Status != JobQueued {
		t.Fatalf("unexpected initial status: %s", view.Status)
	}

	final := waitForJobStatus(t, jobs, "job-test-1", JobCompleted)
	if final.ExitCode == nil || *final.ExitCode != 0 {
		t.Fatalf("unexpected exit code: %#v", final.ExitCode)
	}

	logs, err := jobs.Logs(JobLogQuery{JobID: "job-test-1"})
	if err != nil {
		t.Fatalf("logs lookup failed: %v", err)
	}
	if len(logs) != 2 {
		t.Fatalf("expected 2 logs, got %d", len(logs))
	}
	gotStreams := []string{logs[0].Stream, logs[1].Stream}
	slices.Sort(gotStreams)
	if !slices.Equal(gotStreams, []string{"stderr", "stdout"}) {
		t.Fatalf("unexpected streams: %#v", gotStreams)
	}

	store.mu.Lock()
	kinds := eventKinds(store.events)
	store.mu.Unlock()
	for _, want := range []string{"job.created", "job.started", "job.completed"} {
		if !slices.Contains(kinds, want) {
			t.Fatalf("missing event %s in %#v", want, kinds)
		}
	}
}

func TestJobsServiceCancelMarksJobCancelled(t *testing.T) {
	store := newJobStoreStub()
	jobs := NewJobsService(store)

	_, err := jobs.StartDetached(context.Background(), JobStartRequest{
		JobID:     "job-test-2",
		ChatID:    22,
		SessionID: "session-2",
		Command:   "bash",
		Args:      []string{"-lc", "sleep 10"},
	})
	if err != nil {
		t.Fatalf("start job: %v", err)
	}

	time.Sleep(100 * time.Millisecond)
	if _, err := jobs.Cancel("job-test-2"); err != nil {
		t.Fatalf("cancel job: %v", err)
	}

	final := waitForJobStatus(t, jobs, "job-test-2", JobCancelled)
	if !final.CancelRequested {
		t.Fatalf("expected cancel_requested to be true")
	}

	store.mu.Lock()
	kinds := eventKinds(store.events)
	store.mu.Unlock()
	for _, want := range []string{"job.cancel_requested", "job.cancelled"} {
		if !slices.Contains(kinds, want) {
			t.Fatalf("missing event %s in %#v", want, kinds)
		}
	}
}

func TestJobEventPayloadDefaultsToObject(t *testing.T) {
	record := JobRecord{JobID: "job-payload", ChatID: 1, SessionID: "s"}
	event := jobEvent(record, "job.created", nil)
	var payload map[string]any
	if err := json.Unmarshal(event.Payload, &payload); err != nil {
		t.Fatalf("payload is not valid json: %v", err)
	}
	if len(payload) != 0 {
		t.Fatalf("expected empty object payload, got %#v", payload)
	}
}

func TestJobsServicePersistsPolicySnapshot(t *testing.T) {
	store := newJobStoreStub()
	jobs := NewJobsService(store)

	view, err := jobs.StartDetached(context.Background(), JobStartRequest{
		JobID:     "job-policy-1",
		ChatID:    11,
		SessionID: "session-1",
		Command:   "bash",
		Args:      []string{"-lc", "exit 0"},
		PolicySnapshot: PolicySnapshot{
			Runtime:      provider.RequestConfig{Model: "glm-5.1"},
			MemoryPolicy: MemoryPolicy{Profile: "standard"},
			ActionPolicy: ActionPolicy{ApprovalRequiredTools: []string{"shell.exec"}},
		},
	})
	if err != nil {
		t.Fatalf("start job: %v", err)
	}
	final := waitForJobStatus(t, jobs, view.JobID, JobCompleted)
	if final.PolicySnapshot.Runtime.Model != "glm-5.1" {
		t.Fatalf("missing job policy snapshot in view: %+v", final.PolicySnapshot)
	}

	record, ok, err := store.Job(view.JobID)
	if err != nil || !ok {
		t.Fatalf("job record lookup failed: ok=%v err=%v", ok, err)
	}
	if record.PolicySnapshot.MemoryPolicy.Profile != "standard" {
		t.Fatalf("missing job policy snapshot in record: %+v", record.PolicySnapshot)
	}
}
