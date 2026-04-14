package runtime

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os/exec"
	"sync"
	"sync/atomic"
	"time"
)

type JobsService struct {
	store  JobStore
	nextID atomic.Int64
	active sync.Map // job_id -> context.CancelFunc
}

func NewJobsService(store JobStore) *JobsService {
	return &JobsService{store: store}
}

func (s *JobsService) StartDetached(ctx context.Context, req JobStartRequest) (JobView, error) {
	if s.store == nil {
		return JobView{}, NewControlError(ErrRuntimeUnavailable, "job store is not configured")
	}
	if req.Command == "" {
		return JobView{}, NewControlError(ErrValidation, "job command is required")
	}
	if req.JobID == "" {
		req.JobID = fmt.Sprintf("job-%d", s.nextID.Add(1))
	}
	startedAt := time.Now().UTC()
	record := JobRecord{
		JobID:         req.JobID,
		Kind:          defaultJobKind(req.Kind),
		OwnerRunID:    req.OwnerRunID,
		OwnerWorkerID: req.OwnerWorkerID,
		ChatID:        req.ChatID,
		SessionID:     req.SessionID,
		Command:       req.Command,
		Args:          append([]string(nil), req.Args...),
		Cwd:           req.Cwd,
		Status:        JobQueued,
		StartedAt:     startedAt,
		PolicySnapshot: NormalizePolicySnapshot(req.PolicySnapshot),
	}
	if err := s.store.SaveJob(record); err != nil {
		return JobView{}, err
	}
	_ = s.store.SaveEvent(jobEvent(record, "job.created", map[string]any{
		"command": record.Command,
		"args":    record.Args,
	}))

	jobCtx, cancel := context.WithCancel(context.Background())
	s.active.Store(record.JobID, cancel)
	go s.run(jobCtx, record)
	return jobView(record, true), nil
}

func (s *JobsService) Cancel(jobID string) (bool, error) {
	if s.store == nil {
		return false, NewControlError(ErrRuntimeUnavailable, "job store is not configured")
	}
	record, ok, err := s.store.Job(jobID)
	if err != nil {
		return false, err
	}
	if !ok {
		return false, NewControlError(ErrNotFound, "job not found")
	}
	_ = s.store.MarkJobCancelRequested(jobID)
	_ = s.store.SaveEvent(jobEvent(record, "job.cancel_requested", nil))
	if cancel, ok := s.active.Load(jobID); ok {
		cancel.(context.CancelFunc)()
	}
	return true, nil
}

func (s *JobsService) Job(jobID string) (JobView, bool, error) {
	if s.store == nil {
		return JobView{}, false, NewControlError(ErrRuntimeUnavailable, "job store is not configured")
	}
	record, ok, err := s.store.Job(jobID)
	if err != nil || !ok {
		return JobView{}, ok, err
	}
	_, active := s.active.Load(jobID)
	return jobView(record, active), true, nil
}

func (s *JobsService) List(limit int) ([]JobView, error) {
	if s.store == nil {
		return nil, NewControlError(ErrRuntimeUnavailable, "job store is not configured")
	}
	items, err := s.store.ListJobs(limit)
	if err != nil {
		return nil, err
	}
	out := make([]JobView, 0, len(items))
	for _, item := range items {
		_, active := s.active.Load(item.JobID)
		out = append(out, jobView(item, active))
	}
	return out, nil
}

func (s *JobsService) Logs(query JobLogQuery) ([]JobLogChunk, error) {
	if s.store == nil {
		return nil, NewControlError(ErrRuntimeUnavailable, "job store is not configured")
	}
	return s.store.JobLogs(query)
}

func (s *JobsService) Recover(reason string) (int, error) {
	if s.store == nil {
		return 0, nil
	}
	return s.store.RecoverInterruptedJobs(reason)
}

func (s *JobsService) run(ctx context.Context, record JobRecord) {
	defer s.active.Delete(record.JobID)

	cmd := exec.CommandContext(ctx, record.Command, record.Args...)
	if record.Cwd != "" {
		cmd.Dir = record.Cwd
	}

	stdout, err := cmd.StdoutPipe()
	if err != nil {
		s.fail(record, nil, fmt.Errorf("stdout pipe: %w", err))
		return
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		s.fail(record, nil, fmt.Errorf("stderr pipe: %w", err))
		return
	}
	if err := cmd.Start(); err != nil {
		s.fail(record, nil, err)
		return
	}

	record.Status = JobRunning
	_ = s.store.SaveJob(record)
	_ = s.store.SaveEvent(jobEvent(record, "job.started", nil))

	var wg sync.WaitGroup
	wg.Add(2)
	go func() {
		defer wg.Done()
		s.captureLogs(record.JobID, "stdout", stdout)
	}()
	go func() {
		defer wg.Done()
		s.captureLogs(record.JobID, "stderr", stderr)
	}()

	wg.Wait()
	err = cmd.Wait()
	endedAt := time.Now().UTC()
	record.EndedAt = &endedAt
	if cmd.ProcessState != nil {
		code := cmd.ProcessState.ExitCode()
		record.ExitCode = &code
	}
	if current, ok, currentErr := s.store.Job(record.JobID); currentErr == nil && ok {
		record.CancelRequested = current.CancelRequested
	}

	switch {
	case record.CancelRequested || errors.Is(ctx.Err(), context.Canceled) || errors.Is(err, context.Canceled):
		record.Status = JobCancelled
		record.CancelRequested = true
		_ = s.store.SaveEvent(jobEvent(record, "job.cancelled", nil))
	case err != nil:
		record.Status = JobFailed
		record.FailureReason = err.Error()
		_ = s.store.SaveEvent(jobEvent(record, "job.failed", map[string]any{"error": err.Error()}))
	default:
		record.Status = JobCompleted
		_ = s.store.SaveEvent(jobEvent(record, "job.completed", nil))
	}
	_ = s.store.SaveJob(record)
}

func (s *JobsService) captureLogs(jobID, stream string, reader io.Reader) {
	scanner := bufio.NewScanner(reader)
	for scanner.Scan() {
		line := scanner.Text()
		_ = s.store.SaveJobLog(JobLogChunk{
			JobID:     jobID,
			Stream:    stream,
			Content:   line,
			CreatedAt: time.Now().UTC(),
		})
	}
}

func (s *JobsService) fail(record JobRecord, exitCode *int, err error) {
	endedAt := time.Now().UTC()
	record.Status = JobFailed
	record.EndedAt = &endedAt
	record.ExitCode = exitCode
	record.FailureReason = err.Error()
	_ = s.store.SaveJob(record)
	_ = s.store.SaveEvent(jobEvent(record, "job.failed", map[string]any{"error": err.Error()}))
}

func defaultJobKind(kind string) string {
	if kind == "" {
		return "command"
	}
	return kind
}

func jobView(record JobRecord, active bool) JobView {
	return JobView{
		JobID:           record.JobID,
		Kind:            record.Kind,
		OwnerRunID:      record.OwnerRunID,
		OwnerWorkerID:   record.OwnerWorkerID,
		ChatID:          record.ChatID,
		SessionID:       record.SessionID,
		Command:         record.Command,
		Args:            append([]string(nil), record.Args...),
		Cwd:             record.Cwd,
		Status:          record.Status,
		StartedAt:       record.StartedAt,
		EndedAt:         record.EndedAt,
		ExitCode:        record.ExitCode,
		FailureReason:   record.FailureReason,
		CancelRequested: record.CancelRequested,
		Active:          active,
		PolicySnapshot:  record.PolicySnapshot,
	}
}

func jobEvent(record JobRecord, kind string, payload map[string]any) RuntimeEvent {
	return RuntimeEvent{
		EntityType: "job",
		EntityID:   record.JobID,
		ChatID:     record.ChatID,
		SessionID:  record.SessionID,
		RunID:      record.OwnerRunID,
		Kind:       kind,
		Payload:    mustJSONPayload(payload),
		CreatedAt:  time.Now().UTC(),
	}
}

func mustJSONPayload(payload map[string]any) json.RawMessage {
	if payload == nil {
		return json.RawMessage(`{}`)
	}
	encoded, err := json.Marshal(payload)
	if err != nil {
		return json.RawMessage(`{}`)
	}
	return encoded
}
