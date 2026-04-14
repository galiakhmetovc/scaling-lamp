package runtime

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync/atomic"
	"time"

	"teamd/internal/provider"
)

type WorkerConversationStore interface {
	CreateSession(chatID int64, session string) error
	UseSession(chatID int64, session string) error
	Messages(chatID int64) ([]provider.Message, error)
}

type WorkerRunControl interface {
	StartDetached(ctx context.Context, req StartRunRequest) (RunView, bool, error)
	RunView(runID string) (RunView, bool, error)
	CancelRunByID(runID string) (bool, error)
}

type WorkersService struct {
	store       WorkerStore
	transcripts WorkerConversationStore
	runs        WorkerRunControl
	approvals   interface{ PendingApprovals(string) []ApprovalView }
	supervisor  WorkerSupervisor
	nextID      atomic.Int64
}

func NewWorkersService(store WorkerStore, transcripts WorkerConversationStore, runs WorkerRunControl, approvals interface{ PendingApprovals(string) []ApprovalView }, supervisor WorkerSupervisor) *WorkersService {
	return &WorkersService{store: store, transcripts: transcripts, runs: runs, approvals: approvals, supervisor: supervisor}
}

func (s *WorkersService) Spawn(ctx context.Context, req WorkerSpawnRequest) (WorkerView, error) {
	if s.store == nil || s.transcripts == nil {
		return WorkerView{}, NewControlError(ErrRuntimeUnavailable, "workers service is not configured")
	}
	if req.ParentChatID == 0 {
		return WorkerView{}, NewControlError(ErrValidation, "parent chat id is required")
	}
	if strings.TrimSpace(req.ParentSessionID) == "" {
		req.ParentSessionID = "default"
	}
	workerID, workerChatID, err := s.allocateIdentity(req.WorkerID)
	if err != nil {
		return WorkerView{}, err
	}
	record := WorkerRecord{
		WorkerID:        workerID,
		ParentChatID:    req.ParentChatID,
		ParentSessionID: req.ParentSessionID,
		WorkerChatID:    workerChatID,
		WorkerSessionID: workerID,
		Status:          WorkerIdle,
		CreatedAt:       time.Now().UTC(),
		UpdatedAt:       time.Now().UTC(),
		PolicySnapshot:  NormalizePolicySnapshot(req.PolicySnapshot),
		Process: WorkerProcessRuntime{
			State: WorkerProcessStopped,
		},
	}
	if err := s.transcripts.CreateSession(record.WorkerChatID, record.WorkerSessionID); err != nil {
		return WorkerView{}, err
	}
	if err := s.transcripts.UseSession(record.WorkerChatID, record.WorkerSessionID); err != nil {
		return WorkerView{}, err
	}
	if err := s.store.SaveWorker(record); err != nil {
		return WorkerView{}, err
	}
	if s.supervisor != nil {
		processRuntime, err := s.supervisor.Start(context.WithoutCancel(ctx), record)
		if err != nil {
			record.Process.State = WorkerProcessFailed
			record.Process.ExitReason = err.Error()
			record.LastError = err.Error()
			record.UpdatedAt = time.Now().UTC()
			_ = s.store.SaveWorker(record)
			return WorkerView{}, err
		}
		record.Process = processRuntime
		record.UpdatedAt = time.Now().UTC()
		if err := s.store.SaveWorker(record); err != nil {
			return WorkerView{}, err
		}
		_ = s.store.SaveEvent(workerEvent(record, "worker.process_started", map[string]any{
			"pid":   processRuntime.PID,
			"state": processRuntime.State,
		}))
	}
	_ = s.store.SaveEvent(workerEvent(record, "worker.spawned", map[string]any{
		"parent_chat_id":    record.ParentChatID,
		"parent_session_id": record.ParentSessionID,
	}))
	if strings.TrimSpace(req.Prompt) != "" {
		return s.Message(ctx, record.WorkerID, WorkerMessageRequest{Content: req.Prompt})
	}
	return s.view(record)
}

func (s *WorkersService) Message(ctx context.Context, workerID string, req WorkerMessageRequest) (WorkerView, error) {
	if s.store == nil || s.runs == nil {
		return WorkerView{}, NewControlError(ErrRuntimeUnavailable, "workers service is not configured")
	}
	if strings.TrimSpace(req.Content) == "" {
		return WorkerView{}, NewControlError(ErrValidation, "worker message is required")
	}
	record, ok, err := s.store.Worker(workerID)
	if err != nil {
		return WorkerView{}, err
	}
	if !ok {
		return WorkerView{}, NewControlError(ErrNotFound, "worker not found")
	}
	if record.Status == WorkerClosed {
		return WorkerView{}, NewControlError(ErrConflict, "worker is closed")
	}
	if _, running := s.runStillActive(record); running {
		return WorkerView{}, NewControlError(ErrConflict, "worker already has an active run")
	}
	now := time.Now().UTC()
	record.Status = WorkerRunning
	record.UpdatedAt = now
	record.LastMessageAt = &now
	if err := s.store.SaveWorker(record); err != nil {
		return WorkerView{}, err
	}
	_ = s.store.SaveEvent(workerEvent(record, "worker.message_received", map[string]any{"content": req.Content}))
	runID := fmt.Sprintf("%s-run-%d", record.WorkerID, now.UnixNano())
	runView, ok, err := s.runs.StartDetached(context.WithoutCancel(ctx), StartRunRequest{
		RunID:          runID,
		ChatID:         record.WorkerChatID,
		SessionID:      record.WorkerSessionID,
		Query:          req.Content,
		PolicySnapshot: record.PolicySnapshot,
		Interactive:    false,
	})
	if err != nil {
		record.Status = WorkerFailed
		record.LastError = err.Error()
		record.UpdatedAt = time.Now().UTC()
		_ = s.store.SaveWorker(record)
		return WorkerView{}, err
	}
	if !ok {
		return WorkerView{}, NewControlError(ErrConflict, "worker run was not accepted")
	}
	record.LastRunID = runView.RunID
	record.Status = mapRunStatusToWorker(runView.Status)
	record.LastError = ""
	record.UpdatedAt = time.Now().UTC()
	if err := s.store.SaveWorker(record); err != nil {
		return WorkerView{}, err
	}
	_ = s.store.SaveEvent(workerEvent(record, "worker.run_started", map[string]any{"run_id": runView.RunID}))
	return s.view(record)
}

func (s *WorkersService) Wait(workerID string, afterCursor int, afterEventID int64, eventLimit int) (WorkerWaitResult, bool, error) {
	record, ok, err := s.syncRecord(workerID)
	if err != nil || !ok {
		return WorkerWaitResult{}, ok, err
	}
	view, err := s.view(record)
	if err != nil {
		return WorkerWaitResult{}, false, err
	}
	history, err := s.transcripts.Messages(record.WorkerChatID)
	if err != nil {
		return WorkerWaitResult{}, false, err
	}
	if afterCursor < 0 {
		afterCursor = 0
	}
	if afterCursor > len(history) {
		afterCursor = len(history)
	}
	messages := make([]WorkerMessage, 0, len(history)-afterCursor)
	for i := afterCursor; i < len(history); i++ {
		msg := history[i]
		messages = append(messages, WorkerMessage{
			Cursor:     i + 1,
			Role:       msg.Role,
			Content:    msg.Content,
			Name:       msg.Name,
			ToolCallID: msg.ToolCallID,
		})
	}
	if eventLimit <= 0 {
		eventLimit = 50
	}
	events, err := s.store.ListEvents(EventQuery{
		EntityType: "worker",
		EntityID:   workerID,
		AfterID:    afterEventID,
		Limit:      eventLimit,
	})
	if err != nil {
		return WorkerWaitResult{}, false, err
	}
	nextEvent := afterEventID
	if len(events) > 0 {
		nextEvent = events[len(events)-1].ID
	}
	var handoff *WorkerHandoff
	if view.Handoff != nil {
		copy := *view.Handoff
		handoff = &copy
	}
	return WorkerWaitResult{
		Worker:         view,
		Handoff:        handoff,
		Messages:       messages,
		Events:         events,
		NextCursor:     len(history),
		NextEventAfter: nextEvent,
	}, true, nil
}

func (s *WorkersService) Close(workerID string) (WorkerView, bool, error) {
	record, ok, err := s.store.Worker(workerID)
	if err != nil || !ok {
		return WorkerView{}, ok, err
	}
	if record.LastRunID != "" {
		_, _ = s.runs.CancelRunByID(record.LastRunID)
	}
	if s.supervisor != nil {
		if err := s.supervisor.Stop(context.Background(), workerID, record); err != nil {
			record.LastError = err.Error()
		}
	}
	now := time.Now().UTC()
	record.Status = WorkerClosed
	if s.supervisor != nil {
		if process, ok := s.supervisor.Runtime(workerID); ok {
			record.Process = process
		}
	}
	record.ClosedAt = &now
	record.UpdatedAt = now
	if err := s.store.SaveWorker(record); err != nil {
		return WorkerView{}, false, err
	}
	_ = s.store.SaveEvent(workerEvent(record, "worker.closed", nil))
	view, err := s.view(record)
	return view, true, err
}

func (s *WorkersService) Worker(workerID string) (WorkerView, bool, error) {
	record, ok, err := s.syncRecord(workerID)
	if err != nil || !ok {
		return WorkerView{}, ok, err
	}
	view, err := s.view(record)
	return view, true, err
}

func (s *WorkersService) Handoff(workerID string) (WorkerHandoff, bool, error) {
	if s.store == nil {
		return WorkerHandoff{}, false, NewControlError(ErrRuntimeUnavailable, "workers service is not configured")
	}
	return s.store.WorkerHandoff(workerID)
}

func (s *WorkersService) List(query WorkerQuery) ([]WorkerView, error) {
	if s.store == nil {
		return nil, NewControlError(ErrRuntimeUnavailable, "workers service is not configured")
	}
	records, err := s.store.ListWorkers(query)
	if err != nil {
		return nil, err
	}
	out := make([]WorkerView, 0, len(records))
	for _, record := range records {
		record, _, err = s.syncRecord(record.WorkerID)
		if err != nil {
			return nil, err
		}
		view, err := s.view(record)
		if err != nil {
			return nil, err
		}
		out = append(out, view)
	}
	return out, nil
}

func (s *WorkersService) allocateIdentity(requested string) (string, int64, error) {
	for {
		ordinal := s.nextID.Add(1)
		workerID := strings.TrimSpace(requested)
		if workerID == "" {
			workerID = fmt.Sprintf("worker-%d", ordinal)
		}
		if current, ok, err := s.store.Worker(workerID); err != nil {
			return "", 0, err
		} else if ok && current.WorkerID != "" {
			if requested != "" {
				return "", 0, NewControlError(ErrConflict, "worker id already exists")
			}
			continue
		}
		return workerID, -ordinal, nil
	}
}

func (s *WorkersService) syncRecord(workerID string) (WorkerRecord, bool, error) {
	record, ok, err := s.store.Worker(workerID)
	if err != nil || !ok {
		return WorkerRecord{}, ok, err
	}
	runView, active := s.runStillActive(record)
	if s.supervisor != nil {
		if process, ok := s.supervisor.Runtime(workerID); ok {
			record.Process = process
			if process.State == WorkerProcessFailed && record.Status != WorkerClosed {
				record.Status = WorkerFailed
				if process.ExitReason != "" {
					record.LastError = process.ExitReason
				}
			}
		}
	}
	nextStatus := record.Status
	nextError := record.LastError
	if active {
		nextStatus = mapRunStatusToWorker(runView.Status)
		if runView.Status == StatusFailed {
			nextError = runView.FailureReason
		}
	} else if record.Status != WorkerClosed {
		switch {
		case runView.RunID == "":
			nextStatus = WorkerIdle
		case runView.Status == StatusFailed:
			nextStatus = WorkerFailed
			nextError = runView.FailureReason
		default:
			nextStatus = WorkerIdle
			nextError = ""
		}
	}
	if nextStatus != record.Status || nextError != record.LastError {
		record.Status = nextStatus
		record.LastError = nextError
		record.UpdatedAt = time.Now().UTC()
		if err := s.store.SaveWorker(record); err != nil {
			return WorkerRecord{}, false, err
		}
	}
	if active && nextStatus == WorkerWaitingApproval {
		if err := s.ensureApprovalEvent(record); err != nil {
			return WorkerRecord{}, false, err
		}
	}
	if !active && record.LastRunID != "" {
		if err := s.ensureHandoff(record, runView); err != nil {
			return WorkerRecord{}, false, err
		}
	}
	return record, true, nil
}

func (s *WorkersService) ensureApprovalEvent(record WorkerRecord) error {
	if s.store == nil || s.approvals == nil || strings.TrimSpace(record.LastRunID) == "" {
		return nil
	}
	var pending *ApprovalView
	for _, item := range s.approvals.PendingApprovals(workerSessionKey(record)) {
		if item.TargetType == "run" && item.TargetID == record.LastRunID {
			copy := item
			pending = &copy
			break
		}
	}
	if pending == nil {
		return nil
	}
	events, err := s.store.ListEvents(EventQuery{EntityType: "worker", EntityID: record.WorkerID, Limit: 100})
	if err != nil {
		return err
	}
	for _, item := range events {
		if item.Kind != "worker.approval_requested" {
			continue
		}
		payload := decodeRuntimeEventPayload(item.Payload)
		if approvalID, _ := payload["approval_id"].(string); approvalID == pending.ID {
			return nil
		}
	}
	return s.store.SaveEvent(workerEvent(record, "worker.approval_requested", map[string]any{
		"worker_id":   record.WorkerID,
		"approval_id": pending.ID,
		"tool":        pending.WorkerID,
		"reason":      pending.Reason,
		"run_id":      record.LastRunID,
	}))
}

func (s *WorkersService) runStillActive(record WorkerRecord) (RunView, bool) {
	if s.runs == nil || strings.TrimSpace(record.LastRunID) == "" {
		return RunView{}, false
	}
	runView, ok, err := s.runs.RunView(record.LastRunID)
	if err != nil || !ok {
		return RunView{}, false
	}
	switch runView.Status {
	case StatusQueued, StatusRunning, StatusWaitingApproval:
		return runView, true
	default:
		return runView, false
	}
}

func (s *WorkersService) view(record WorkerRecord) (WorkerView, error) {
	var lastRun *RunView
	var artifactRefs []string
	var handoff *WorkerHandoff
	if s.runs != nil && strings.TrimSpace(record.LastRunID) != "" {
		if runView, ok, err := s.runs.RunView(record.LastRunID); err != nil {
			return WorkerView{}, err
		} else if ok {
			lastRun = &runView
			artifactRefs = append([]string(nil), runView.ArtifactRefs...)
		}
	}
	if stored, ok, err := s.store.WorkerHandoff(record.WorkerID); err != nil {
		return WorkerView{}, err
	} else if ok {
		copy := stored
		handoff = &copy
		if len(artifactRefs) == 0 {
			artifactRefs = append([]string(nil), stored.Artifacts...)
		}
	}
	return WorkerView{
		WorkerID:        record.WorkerID,
		ParentChatID:    record.ParentChatID,
		ParentSessionID: record.ParentSessionID,
		WorkerChatID:    record.WorkerChatID,
		WorkerSessionID: record.WorkerSessionID,
		ArtifactRefs:    artifactRefs,
		Status:          record.Status,
		LastRunID:       record.LastRunID,
		LastRun:         lastRun,
		Handoff:         handoff,
		LastError:       record.LastError,
		CreatedAt:       record.CreatedAt,
		UpdatedAt:       record.UpdatedAt,
		LastMessageAt:   record.LastMessageAt,
		ClosedAt:        record.ClosedAt,
		PolicySnapshot:  record.PolicySnapshot,
		Process:         record.Process,
	}, nil
}

func (s *WorkersService) ensureHandoff(record WorkerRecord, runView RunView) error {
	if s.store == nil || s.transcripts == nil {
		return nil
	}
	existing, ok, err := s.store.WorkerHandoff(record.WorkerID)
	if err != nil {
		return err
	}
	if ok && existing.LastRunID == runView.RunID {
		return nil
	}
	history, err := s.transcripts.Messages(record.WorkerChatID)
	if err != nil {
		return err
	}
	handoff := BuildWorkerHandoff(record.WorkerID, &runView, history, func() *WorkerHandoff {
		if ok {
			return &existing
		}
		return nil
	}())
	if handoff == nil {
		return nil
	}
	if err := s.store.SaveWorkerHandoff(*handoff); err != nil {
		return err
	}
	_ = s.store.SaveEvent(workerEvent(record, "worker.handoff_created", map[string]any{
		"last_run_id": handoff.LastRunID,
		"artifacts":   handoff.Artifacts,
	}))
	return nil
}

func mapRunStatusToWorker(status RunStatus) WorkerStatus {
	switch status {
	case StatusQueued, StatusRunning:
		return WorkerRunning
	case StatusWaitingApproval:
		return WorkerWaitingApproval
	case StatusFailed:
		return WorkerFailed
	default:
		return WorkerIdle
	}
}

func workerEvent(record WorkerRecord, kind string, payload map[string]any) RuntimeEvent {
	return RuntimeEvent{
		EntityType: "worker",
		EntityID:   record.WorkerID,
		ChatID:     record.ParentChatID,
		SessionID:  record.ParentSessionID,
		RunID:      record.LastRunID,
		Kind:       kind,
		Payload:    mustJSONPayload(payload),
		CreatedAt:  time.Now().UTC(),
	}
}

func decodeRuntimeEventPayload(payload []byte) map[string]any {
	out := map[string]any{}
	_ = json.Unmarshal(payload, &out)
	return out
}

func workerSessionKey(record WorkerRecord) string {
	return fmt.Sprintf("%d:%s", record.WorkerChatID, record.WorkerSessionID)
}
