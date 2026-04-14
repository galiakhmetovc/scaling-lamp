package runtime

import (
	"context"
	"encoding/json"
	"strconv"
	"strings"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/provider"
)

type API struct {
	runs          *RunManager
	plans         *PlansService
	store         RunLifecycleStore
	overrideStore SessionOverrideStore
	approvals     *approvals.Service
	timeouts      *TimeoutDecisions
}

func NewAPI(store RunLifecycleStore, registry *ActiveRegistry, approvalService *approvals.Service) *API {
	var overrideStore SessionOverrideStore
	if s, ok := store.(SessionOverrideStore); ok {
		overrideStore = s
	}
	var plans *PlansService
	if s, ok := store.(PlanStore); ok {
		plans = NewPlansService(s)
	}
	return &API{
		runs:          NewRunManager(store, registry),
		plans:         plans,
		store:         store,
		overrideStore: overrideStore,
		approvals:     approvalService,
		timeouts:      NewTimeoutDecisions(timeoutStore(store)),
	}
}

func (a *API) Store() RunLifecycleStore {
	if a == nil {
		return nil
	}
	return a.store
}

func timeoutStore(store RunLifecycleStore) TimeoutDecisionStore {
	if s, ok := store.(TimeoutDecisionStore); ok {
		return s
	}
	return nil
}

func (a *API) CreateTimeoutDecision(runID string, chatID int64, sessionID string, roundIndex int, autoUsed bool, autoDeadline time.Time) (TimeoutDecisionRecord, error) {
	if a == nil || a.timeouts == nil {
		return TimeoutDecisionRecord{}, NewControlError(ErrRuntimeUnavailable, "timeout decisions are not configured")
	}
	return a.timeouts.CreateOrUpdatePending(runID, chatID, sessionID, roundIndex, autoUsed, autoDeadline)
}

func (a *API) ResolveTimeoutDecision(runID string, action TimeoutDecisionAction, failureReason string) (TimeoutDecisionRecord, bool, error) {
	if a == nil || a.timeouts == nil {
		return TimeoutDecisionRecord{}, false, NewControlError(ErrRuntimeUnavailable, "timeout decisions are not configured")
	}
	return a.timeouts.Resolve(runID, action, failureReason)
}

func (a *API) WaitTimeoutDecision(ctx context.Context, runID string) (TimeoutDecisionRecord, error) {
	if a == nil || a.timeouts == nil {
		return TimeoutDecisionRecord{}, NewControlError(ErrRuntimeUnavailable, "timeout decisions are not configured")
	}
	return a.timeouts.Wait(ctx, runID)
}

func (a *API) TimeoutDecision(runID string) (TimeoutDecisionRecord, bool, error) {
	if a == nil || a.timeouts == nil {
		return TimeoutDecisionRecord{}, false, NewControlError(ErrRuntimeUnavailable, "timeout decisions are not configured")
	}
	return a.timeouts.Get(runID)
}

func (a *API) PrepareRun(ctx context.Context, runID string, chatID int64, sessionID, query string, snapshot PolicySnapshot) (PreparedRun, bool, error) {
	return a.runs.Prepare(ctx, runID, chatID, sessionID, query, snapshot)
}

func (a *API) LaunchRun(prepared PreparedRun, exec func(context.Context, string) error) {
	a.runs.Launch(prepared, exec)
}

func (a *API) FailRunStart(prepared PreparedRun, err error) error {
	return a.runs.FailStart(prepared, err)
}

func (a *API) CancelRun(chatID int64) bool {
	return a.runs.Cancel(chatID)
}

func (a *API) ActiveRun(chatID int64) (ActiveRun, bool) {
	return a.runs.Active(chatID)
}

func (a *API) ActiveRunView(chatID int64) (RunView, bool) {
	run, ok := a.runs.Active(chatID)
	if !ok {
		return RunView{}, false
	}
	refs, _ := a.artifactRefs("run", run.RunID)
	return RunView{
		RunID:          run.RunID,
		ChatID:         run.ChatID,
		SessionID:      run.SessionID,
		Query:          run.Query,
		FinalResponse:  "",
		PromptBudget:   PromptBudgetMetrics{},
		ArtifactRefs:   refs,
		Status:         StatusRunning,
		StartedAt:      run.StartedAt,
		Active:         true,
		PolicySnapshot: run.PolicySnapshot,
	}, true
}

func (a *API) RunView(runID string) (RunView, bool, error) {
	if a.store == nil {
		return RunView{}, false, nil
	}
	record, ok, err := a.store.Run(runID)
	if err != nil || !ok {
		return RunView{}, ok, err
	}
	active, activeOK := a.runs.Active(record.ChatID)
	refs, err := a.artifactRefs("run", record.RunID)
	if err != nil {
		return RunView{}, false, err
	}
	return RunView{
		RunID:           record.RunID,
		ChatID:          record.ChatID,
		SessionID:       record.SessionID,
		Query:           record.Query,
		FinalResponse:   record.FinalResponse,
		PromptBudget:    record.PromptBudget,
		ArtifactRefs:    refs,
		Status:          record.Status,
		StartedAt:       record.StartedAt,
		EndedAt:         record.EndedAt,
		FailureReason:   record.FailureReason,
		CancelRequested: record.CancelRequested,
		Active:          activeOK && active.RunID == record.RunID,
		PolicySnapshot:  record.PolicySnapshot,
	}, true, nil
}

func (a *API) ListRuns(query RunQuery) ([]RunView, error) {
	if a.store == nil {
		return nil, nil
	}
	records, err := a.store.ListRuns(query)
	if err != nil {
		return nil, err
	}
	out := make([]RunView, 0, len(records))
	for _, record := range records {
		active, activeOK := a.runs.Active(record.ChatID)
		refs, err := a.artifactRefs("run", record.RunID)
		if err != nil {
			return nil, err
		}
		out = append(out, RunView{
			RunID:           record.RunID,
			ChatID:          record.ChatID,
			SessionID:       record.SessionID,
			Query:           record.Query,
			FinalResponse:   record.FinalResponse,
			PromptBudget:    record.PromptBudget,
			ArtifactRefs:    refs,
			Status:          record.Status,
			StartedAt:       record.StartedAt,
			EndedAt:         record.EndedAt,
			FailureReason:   record.FailureReason,
			CancelRequested: record.CancelRequested,
			Active:          activeOK && active.RunID == record.RunID,
			PolicySnapshot:  record.PolicySnapshot,
		})
	}
	return out, nil
}

func (a *API) ListEvents(query EventQuery) ([]RuntimeEvent, error) {
	if a.store == nil {
		return nil, nil
	}
	if query.Limit <= 0 {
		query.Limit = 50
	}
	return a.store.ListEvents(query)
}

func (a *API) CreatePlan(ctx context.Context, ownerType, ownerID, title string) (PlanRecord, error) {
	if a == nil || a.plans == nil {
		return PlanRecord{}, NewControlError(ErrRuntimeUnavailable, "plans service is not configured")
	}
	return a.plans.Create(ctx, ownerType, ownerID, title)
}

func (a *API) Plan(planID string) (PlanRecord, bool, error) {
	if a == nil || a.plans == nil {
		return PlanRecord{}, false, NewControlError(ErrRuntimeUnavailable, "plans service is not configured")
	}
	return a.plans.Plan(planID)
}

func (a *API) ListPlans(query PlanQuery) ([]PlanRecord, error) {
	if a == nil || a.plans == nil {
		return nil, NewControlError(ErrRuntimeUnavailable, "plans service is not configured")
	}
	return a.plans.List(query)
}

func (a *API) WorkerHandoff(workerID string) (WorkerHandoff, bool, error) {
	if a == nil || a.store == nil {
		return WorkerHandoff{}, false, NewControlError(ErrRuntimeUnavailable, "runtime api is not configured")
	}
	store, ok := a.store.(WorkerStore)
	if !ok {
		return WorkerHandoff{}, false, NewControlError(ErrRuntimeUnavailable, "worker store is not configured")
	}
	return store.WorkerHandoff(workerID)
}

func (a *API) ReplacePlanItems(planID string, items []PlanItem) (PlanRecord, error) {
	if a == nil || a.plans == nil {
		return PlanRecord{}, NewControlError(ErrRuntimeUnavailable, "plans service is not configured")
	}
	return a.plans.ReplaceItems(planID, items)
}

func (a *API) AppendPlanNote(planID, note string) (PlanRecord, error) {
	if a == nil || a.plans == nil {
		return PlanRecord{}, NewControlError(ErrRuntimeUnavailable, "plans service is not configured")
	}
	return a.plans.AppendNote(planID, note)
}

func (a *API) AddPlanItem(planID string, item PlanItem) (PlanRecord, error) {
	if a == nil || a.plans == nil {
		return PlanRecord{}, NewControlError(ErrRuntimeUnavailable, "plans service is not configured")
	}
	return a.plans.AddItem(planID, item)
}

func (a *API) InsertPlanItemAfter(planID, afterItemID string, item PlanItem) (PlanRecord, error) {
	if a == nil || a.plans == nil {
		return PlanRecord{}, NewControlError(ErrRuntimeUnavailable, "plans service is not configured")
	}
	return a.plans.InsertItemAfter(planID, afterItemID, item)
}

func (a *API) InsertPlanItemBefore(planID, beforeItemID string, item PlanItem) (PlanRecord, error) {
	if a == nil || a.plans == nil {
		return PlanRecord{}, NewControlError(ErrRuntimeUnavailable, "plans service is not configured")
	}
	return a.plans.InsertItemBefore(planID, beforeItemID, item)
}

func (a *API) UpdatePlanItem(planID, itemID string, patch PlanItemMutation) (PlanRecord, error) {
	if a == nil || a.plans == nil {
		return PlanRecord{}, NewControlError(ErrRuntimeUnavailable, "plans service is not configured")
	}
	return a.plans.UpdateItem(planID, itemID, patch)
}

func (a *API) RemovePlanItem(planID, itemID string) (PlanRecord, error) {
	if a == nil || a.plans == nil {
		return PlanRecord{}, NewControlError(ErrRuntimeUnavailable, "plans service is not configured")
	}
	return a.plans.RemoveItem(planID, itemID)
}

func (a *API) StartPlanItem(planID, itemID string) (PlanRecord, error) {
	if a == nil || a.plans == nil {
		return PlanRecord{}, NewControlError(ErrRuntimeUnavailable, "plans service is not configured")
	}
	return a.plans.StartItem(planID, itemID)
}

func (a *API) CompletePlanItem(planID, itemID string) (PlanRecord, error) {
	if a == nil || a.plans == nil {
		return PlanRecord{}, NewControlError(ErrRuntimeUnavailable, "plans service is not configured")
	}
	return a.plans.CompleteItem(planID, itemID)
}

func (a *API) artifactRefs(entityType, entityID string) ([]string, error) {
	if a == nil || a.store == nil || entityType == "" || entityID == "" {
		return nil, nil
	}
	events, err := a.store.ListEvents(EventQuery{
		EntityType: entityType,
		EntityID:   entityID,
		Limit:      200,
	})
	if err != nil {
		return nil, err
	}
	refs := make([]string, 0, len(events))
	seen := make(map[string]struct{}, len(events))
	for _, event := range events {
		if event.Kind != "artifact.offloaded" {
			continue
		}
		ref := artifactRefFromPayload(event.Payload)
		if ref == "" {
			continue
		}
		if _, ok := seen[ref]; ok {
			continue
		}
		seen[ref] = struct{}{}
		refs = append(refs, ref)
	}
	return refs, nil
}

func artifactRefFromPayload(payload json.RawMessage) string {
	if len(payload) == 0 {
		return ""
	}
	var body map[string]any
	if err := json.Unmarshal(payload, &body); err != nil {
		return ""
	}
	ref, _ := body["artifact_ref"].(string)
	return ref
}

func (a *API) ListSessions(query SessionQuery, runtimeConfig provider.RequestConfig, memoryPolicy MemoryPolicy, actionPolicy ActionPolicy) ([]SessionState, error) {
	if a.store == nil {
		return nil, nil
	}
	records, err := a.store.ListSessions(query)
	if err != nil {
		return nil, err
	}
	out := make([]SessionState, 0, len(records))
	for _, record := range records {
		chatID := sessionChatID(record.SessionID)
		summary, err := a.RuntimeSummary(record.SessionID, runtimeConfig, memoryPolicy, actionPolicy)
		if err != nil {
			return nil, err
		}
		runs, err := a.ListRuns(RunQuery{SessionID: record.SessionID, Limit: 1})
		if err != nil {
			return nil, err
		}
		var latest *RunView
		if len(runs) > 0 {
			latest = &runs[0]
			if chatID == 0 {
				chatID = latest.ChatID
			}
		}
		head, _ := a.sessionHead(chatID, record.SessionID)
		out = append(out, SessionState{
			SessionID:        record.SessionID,
			ChatID:           chatID,
			LastActivityAt:   record.LastActivityAt,
			HasOverrides:     record.HasOverrides,
			RuntimeSummary:   summary,
			LatestRun:        latest,
			Head:             head,
			PendingApprovals: len(a.PendingApprovals(record.SessionID)),
		})
	}
	return out, nil
}

func (a *API) SessionState(sessionID string, chatID int64, runtimeConfig provider.RequestConfig, memoryPolicy MemoryPolicy, actionPolicy ActionPolicy) (SessionState, error) {
	summary, err := a.RuntimeSummary(sessionID, runtimeConfig, memoryPolicy, actionPolicy)
	if err != nil {
		return SessionState{}, err
	}
	runs, err := a.ListRuns(RunQuery{SessionID: sessionID, Limit: 1})
	if err != nil {
		return SessionState{}, err
	}
	if chatID == 0 {
		chatID = sessionChatID(sessionID)
	}
	var latest *RunView
	var lastActivity time.Time
	if len(runs) > 0 {
		latest = &runs[0]
		lastActivity = runs[0].StartedAt
		if chatID == 0 {
			chatID = runs[0].ChatID
		}
	}
	head, _ := a.sessionHead(chatID, sessionID)
	return SessionState{
		SessionID:        sessionID,
		ChatID:           chatID,
		LastActivityAt:   lastActivity,
		HasOverrides:     summary.HasOverrides,
		RuntimeSummary:   summary,
		LatestRun:        latest,
		Head:             head,
		PendingApprovals: len(a.PendingApprovals(sessionID)),
	}, nil
}

func (a *API) ControlState(sessionID string, chatID int64, runtimeConfig provider.RequestConfig, memoryPolicy MemoryPolicy, actionPolicy ActionPolicy) (ControlState, error) {
	session, err := a.SessionState(sessionID, chatID, runtimeConfig, memoryPolicy, actionPolicy)
	if err != nil {
		return ControlState{}, err
	}
	control := ControlState{
		Session:   session,
		Approvals: a.PendingApprovals(sessionID),
	}
	if a.store == nil {
		return control, nil
	}
	if workerStore, ok := a.store.(WorkerStore); ok {
		items, err := workerStore.ListWorkers(WorkerQuery{ParentChatID: session.ChatID, HasParentChatID: session.ChatID != 0, Limit: 100})
		if err != nil {
			return ControlState{}, err
		}
		for _, record := range items {
			if record.ParentSessionID != sessionID {
				continue
			}
			view, err := a.workerView(workerStore, record)
			if err != nil {
				return ControlState{}, err
			}
			control.Workers = append(control.Workers, view)
			for _, item := range a.PendingApprovals(workerSessionKey(record)) {
				if !containsApproval(control.Approvals, item.ID) {
					control.Approvals = append(control.Approvals, item)
				}
			}
		}
	}
	if jobStore, ok := a.store.(JobStore); ok {
		items, err := jobStore.ListJobs(200)
		if err != nil {
			return ControlState{}, err
		}
		for _, record := range items {
			if record.SessionID != sessionID {
				continue
			}
			view := jobView(record, record.Status == JobQueued || record.Status == JobRunning)
			control.Jobs = append(control.Jobs, view)
		}
	}
	return control, nil
}

func containsApproval(items []ApprovalView, id string) bool {
	for _, item := range items {
		if item.ID == id {
			return true
		}
	}
	return false
}

func (a *API) sessionHead(chatID int64, sessionID string) (*SessionHead, error) {
	if a == nil || a.store == nil || strings.TrimSpace(sessionID) == "" {
		return nil, nil
	}
	store, ok := a.store.(SessionStateStore)
	if !ok {
		return nil, nil
	}
	head, ok, err := store.SessionHead(chatID, sessionID)
	if err != nil || !ok {
		return nil, err
	}
	return &head, nil
}

func (a *API) RecentWorkSnapshot(chatID int64, sessionID, query string) (RecentWorkSnapshot, bool, error) {
	intent := DetectRecentWorkIntent(query)
	if intent == RecentWorkIntentNone {
		return RecentWorkSnapshot{}, false, nil
	}
	head, err := a.sessionHead(chatID, sessionID)
	if err != nil || head == nil {
		return RecentWorkSnapshot{}, false, err
	}
	snapshot := RecentWorkSnapshot{
		Query:  strings.TrimSpace(query),
		Intent: intent,
		Head:   *head,
	}
	if strings.TrimSpace(head.LastCompletedRunID) != "" {
		replay, ok, err := a.RunReplay(head.LastCompletedRunID)
		if err != nil {
			return RecentWorkSnapshot{}, false, err
		}
		if ok {
			snapshot.Replay = &replay
		}
	}
	return snapshot, true, nil
}

func (a *API) PendingApprovals(sessionID string) []ApprovalView {
	if a.approvals == nil {
		return nil
	}
	records := a.approvals.PendingBySession(sessionID)
	out := make([]ApprovalView, 0, len(records))
	for _, record := range records {
		out = append(out, ApprovalView{
			ID:               record.ID,
			WorkerID:         record.WorkerID,
			SessionID:        record.SessionID,
			Payload:          record.Payload,
			Status:           record.Status,
			Reason:           record.Reason,
			TargetType:       record.TargetType,
			TargetID:         record.TargetID,
			RequestedAt:      record.RequestedAt,
			DecidedAt:        record.DecidedAt,
			DecisionUpdateID: record.DecisionUpdateID,
		})
	}
	return out
}

func (a *API) workerView(store WorkerStore, record WorkerRecord) (WorkerView, error) {
	var lastRun *RunView
	var artifactRefs []string
	var handoff *WorkerHandoff
	if strings.TrimSpace(record.LastRunID) != "" {
		if runView, ok, err := a.RunView(record.LastRunID); err != nil {
			return WorkerView{}, err
		} else if ok {
			lastRun = &runView
			artifactRefs = append([]string(nil), runView.ArtifactRefs...)
		}
	}
	if stored, ok, err := store.WorkerHandoff(record.WorkerID); err != nil {
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
	}, nil
}

func (a *API) CancelRunByID(runID string) (bool, error) {
	if a.store == nil {
		return false, nil
	}
	record, ok, err := a.store.Run(runID)
	if err != nil || !ok {
		return false, err
	}
	return a.CancelRun(record.ChatID), nil
}

func (a *API) Approval(id string) (ApprovalView, bool) {
	if a.approvals == nil {
		return ApprovalView{}, false
	}
	record, ok := a.approvals.Get(id)
	if !ok {
		return ApprovalView{}, false
	}
	return ApprovalView{
		ID:               record.ID,
		WorkerID:         record.WorkerID,
		SessionID:        record.SessionID,
		Payload:          record.Payload,
		Status:           record.Status,
		Reason:           record.Reason,
		TargetType:       record.TargetType,
		TargetID:         record.TargetID,
		RequestedAt:      record.RequestedAt,
		DecidedAt:        record.DecidedAt,
		DecisionUpdateID: record.DecisionUpdateID,
	}, true
}

func (a *API) DecideApproval(id, updateID string, action approvals.Action) (ApprovalView, bool, error) {
	if a.approvals == nil {
		return ApprovalView{}, false, nil
	}
	record, err := a.approvals.HandleCallback(approvals.Callback{
		ApprovalID: id,
		Action:     action,
		UpdateID:   updateID,
	})
	if err != nil {
		return ApprovalView{}, false, err
	}
	return ApprovalView{
		ID:               record.ID,
		WorkerID:         record.WorkerID,
		SessionID:        record.SessionID,
		Payload:          record.Payload,
		Status:           record.Status,
		Reason:           record.Reason,
		TargetType:       record.TargetType,
		TargetID:         record.TargetID,
		RequestedAt:      record.RequestedAt,
		DecidedAt:        record.DecidedAt,
		DecisionUpdateID: record.DecisionUpdateID,
	}, true, nil
}

func (a *API) SessionOverrides(sessionID string) (SessionOverrides, bool, error) {
	if a.overrideStore == nil {
		return SessionOverrides{}, false, nil
	}
	return a.overrideStore.SessionOverrides(sessionID)
}

func (a *API) SaveSessionOverrides(overrides SessionOverrides) error {
	if a.overrideStore == nil {
		return nil
	}
	return a.overrideStore.SaveSessionOverrides(overrides)
}

func (a *API) ClearSessionOverrides(sessionID string) error {
	if a.overrideStore == nil {
		return nil
	}
	return a.overrideStore.ClearSessionOverrides(sessionID)
}

func (a *API) RuntimeSummary(sessionID string, runtimeConfig provider.RequestConfig, memoryPolicy MemoryPolicy, actionPolicy ActionPolicy) (RuntimeSummary, error) {
	overrides, ok, err := a.SessionOverrides(sessionID)
	if err != nil {
		return RuntimeSummary{}, err
	}
	if !ok {
		overrides = SessionOverrides{SessionID: sessionID}
	}
	return ApplySessionOverrides(sessionID, runtimeConfig, memoryPolicy, actionPolicy, overrides), nil
}

func sessionChatID(sessionID string) int64 {
	prefix, _, ok := strings.Cut(strings.TrimSpace(sessionID), ":")
	if !ok {
		return 0
	}
	chatID, err := strconv.ParseInt(prefix, 10, 64)
	if err != nil {
		return 0
	}
	return chatID
}
