package runtime

import (
	"context"

	"teamd/internal/approvals"
	"teamd/internal/provider"
)

type AgentCore interface {
	StartRun(ctx context.Context, req StartRunRequest) (RunView, bool, <-chan error, error)
	StartRunDetached(ctx context.Context, req StartRunRequest) (RunView, bool, error)
	ResumeApprovalContinuation(ctx context.Context, approvalID string) (bool, error)

	Run(runID string) (RunView, bool, error)
	CancelRunByID(runID string) (bool, error)
	ListRuns(query RunQuery) ([]RunView, error)
	ListEvents(query EventQuery) ([]RuntimeEvent, error)
	ListSessions(query SessionQuery) ([]SessionState, error)
	SessionState(sessionID string, chatID int64) (SessionState, error)
	DebugSession(sessionID string, chatID int64, eventLimit int) (DebugSessionView, error)
	DebugRun(runID string, eventLimit int) (DebugRunView, bool, error)
	DebugContextProvenance(runID string) (DebugContextProvenance, error)
	RecentWorkSnapshot(chatID int64, sessionID, query string) (RecentWorkSnapshot, bool, error)
	RuntimeSummary(sessionID string) (RuntimeSummary, error)
	SessionOverrides(sessionID string) (SessionOverrides, bool, error)
	SaveSessionOverrides(overrides SessionOverrides) error
	ClearSessionOverrides(sessionID string) error

	ControlState(sessionID string, chatID int64) (ControlState, error)
	ExecuteControlAction(sessionID string, req ControlActionRequest) (ControlActionResult, error)
	ExecuteSessionAction(req SessionActionRequest) (SessionActionResult, error)

	ListApprovals(sessionID string) []ApprovalView
	Approve(approvalID string) (ApprovalView, bool, error)
	Reject(approvalID string) (ApprovalView, bool, error)

	Plan(planID string) (PlanRecord, bool, error)
	ListPlans(query PlanQuery) ([]PlanRecord, error)
	CreatePlan(ctx context.Context, ownerType, ownerID, title string) (PlanRecord, error)
	ReplacePlanItems(planID string, items []PlanItem) (PlanRecord, error)
	AppendPlanNote(planID, note string) (PlanRecord, error)
	AddPlanItem(planID string, item PlanItem) (PlanRecord, error)
	InsertPlanItemAfter(planID, afterItemID string, item PlanItem) (PlanRecord, error)
	InsertPlanItemBefore(planID, beforeItemID string, item PlanItem) (PlanRecord, error)
	UpdatePlanItem(planID, itemID string, patch PlanItemMutation) (PlanRecord, error)
	RemovePlanItem(planID, itemID string) (PlanRecord, error)
	StartPlanItem(planID, itemID string) (PlanRecord, error)
	CompletePlanItem(planID, itemID string) (PlanRecord, error)

	ListJobs(limit int) ([]JobView, error)
	StartJobDetached(ctx context.Context, req JobStartRequest) (JobView, error)
	Job(jobID string) (JobView, bool, error)
	JobLogs(query JobLogQuery) ([]JobLogChunk, error)
	CancelJob(jobID string) (bool, error)

	ListWorkers(query WorkerQuery) ([]WorkerView, error)
	SpawnWorker(ctx context.Context, req WorkerSpawnRequest) (WorkerView, error)
	MessageWorker(ctx context.Context, workerID string, req WorkerMessageRequest) (WorkerView, error)
	WaitWorker(workerID string, afterCursor int, afterEventID int64, eventLimit int) (WorkerWaitResult, bool, error)
	Worker(workerID string) (WorkerView, bool, error)
	CloseWorker(workerID string) (WorkerView, bool, error)
	WorkerHandoff(workerID string) (WorkerHandoff, bool, error)
}

type RuntimeCore struct {
	api            *API
	execution      *ExecutionService
	jobs           *JobsService
	workers        *WorkersService
	sessionActions *SessionActions
	runtimeConfig  provider.RequestConfig
	memoryPolicy   MemoryPolicy
	actionPolicy   ActionPolicy
}

func NewRuntimeCore(api *API, execution *ExecutionService, jobs *JobsService, workers *WorkersService, sessionActions *SessionActions, runtimeConfig provider.RequestConfig, memoryPolicy MemoryPolicy, actionPolicy ActionPolicy) *RuntimeCore {
	return &RuntimeCore{
		api:            api,
		execution:      execution,
		jobs:           jobs,
		workers:        workers,
		sessionActions: sessionActions,
		runtimeConfig:  runtimeConfig,
		memoryPolicy:   NormalizeMemoryPolicy(memoryPolicy),
		actionPolicy:   NormalizeActionPolicy(actionPolicy),
	}
}

func (c *RuntimeCore) StartRun(ctx context.Context, req StartRunRequest) (RunView, bool, <-chan error, error) {
	if c == nil || c.execution == nil {
		return RunView{}, false, nil, nil
	}
	return c.execution.Start(ctx, req)
}

func (c *RuntimeCore) StartRunDetached(ctx context.Context, req StartRunRequest) (RunView, bool, error) {
	if c == nil || c.execution == nil {
		return RunView{}, false, nil
	}
	return c.execution.StartDetached(ctx, req)
}

func (c *RuntimeCore) ResumeApprovalContinuation(ctx context.Context, approvalID string) (bool, error) {
	if c == nil || c.execution == nil {
		return false, nil
	}
	return c.execution.ResumeApprovalContinuation(ctx, approvalID)
}

func (c *RuntimeCore) Run(runID string) (RunView, bool, error) {
	if c == nil || c.api == nil {
		return RunView{}, false, nil
	}
	return c.api.RunView(runID)
}

func (c *RuntimeCore) CancelRunByID(runID string) (bool, error) {
	if c == nil || c.api == nil {
		return false, nil
	}
	return c.api.CancelRunByID(runID)
}

func (c *RuntimeCore) ListRuns(query RunQuery) ([]RunView, error) {
	if c == nil || c.api == nil {
		return nil, nil
	}
	return c.api.ListRuns(query)
}

func (c *RuntimeCore) ListEvents(query EventQuery) ([]RuntimeEvent, error) {
	if c == nil || c.api == nil {
		return nil, nil
	}
	return c.api.ListEvents(query)
}

func (c *RuntimeCore) ListSessions(query SessionQuery) ([]SessionState, error) {
	if c == nil || c.api == nil {
		return nil, nil
	}
	return c.api.ListSessions(query, c.runtimeConfig, c.memoryPolicy, c.actionPolicy)
}

func (c *RuntimeCore) SessionState(sessionID string, chatID int64) (SessionState, error) {
	if c == nil || c.api == nil {
		return SessionState{}, nil
	}
	return c.api.SessionState(sessionID, chatID, c.runtimeConfig, c.memoryPolicy, c.actionPolicy)
}

func (c *RuntimeCore) DebugSession(sessionID string, chatID int64, eventLimit int) (DebugSessionView, error) {
	if c == nil || c.api == nil {
		return DebugSessionView{}, nil
	}
	return NewDebugService(c.api).SessionView(sessionID, chatID, eventLimit)
}

func (c *RuntimeCore) DebugRun(runID string, eventLimit int) (DebugRunView, bool, error) {
	if c == nil || c.api == nil {
		return DebugRunView{}, false, nil
	}
	return NewDebugService(c.api).RunView(runID, eventLimit)
}

func (c *RuntimeCore) DebugContextProvenance(runID string) (DebugContextProvenance, error) {
	if c == nil || c.api == nil {
		return DebugContextProvenance{}, nil
	}
	return NewDebugService(c.api).ContextProvenance(runID)
}

func (c *RuntimeCore) RecentWorkSnapshot(chatID int64, sessionID, query string) (RecentWorkSnapshot, bool, error) {
	if c == nil || c.api == nil {
		return RecentWorkSnapshot{}, false, nil
	}
	return c.api.RecentWorkSnapshot(chatID, sessionID, query)
}

func (c *RuntimeCore) RuntimeSummary(sessionID string) (RuntimeSummary, error) {
	if c == nil || c.api == nil {
		return RuntimeSummary{}, nil
	}
	return c.api.RuntimeSummary(sessionID, c.runtimeConfig, c.memoryPolicy, c.actionPolicy)
}

func (c *RuntimeCore) SessionOverrides(sessionID string) (SessionOverrides, bool, error) {
	if c == nil || c.api == nil {
		return SessionOverrides{}, false, nil
	}
	return c.api.SessionOverrides(sessionID)
}

func (c *RuntimeCore) SaveSessionOverrides(overrides SessionOverrides) error {
	if c == nil || c.api == nil {
		return nil
	}
	return c.api.SaveSessionOverrides(overrides)
}

func (c *RuntimeCore) ClearSessionOverrides(sessionID string) error {
	if c == nil || c.api == nil {
		return nil
	}
	return c.api.ClearSessionOverrides(sessionID)
}

func (c *RuntimeCore) ControlState(sessionID string, chatID int64) (ControlState, error) {
	if c == nil || c.api == nil {
		return ControlState{}, nil
	}
	return c.api.ControlState(sessionID, chatID, c.runtimeConfig, c.memoryPolicy, c.actionPolicy)
}

func (c *RuntimeCore) ExecuteControlAction(sessionID string, req ControlActionRequest) (ControlActionResult, error) {
	if c == nil || c.api == nil {
		return ControlActionResult{}, nil
	}
	return c.api.ExecuteControlAction(sessionID, req.ChatID, c.runtimeConfig, c.memoryPolicy, c.actionPolicy, ControlAction(req.Action))
}

func (c *RuntimeCore) ExecuteSessionAction(req SessionActionRequest) (SessionActionResult, error) {
	if c == nil || c.sessionActions == nil {
		return SessionActionResult{}, nil
	}
	return c.sessionActions.Execute(req.ChatID, req)
}

func (c *RuntimeCore) ListApprovals(sessionID string) []ApprovalView {
	if c == nil || c.api == nil {
		return nil
	}
	return c.api.PendingApprovals(sessionID)
}

func (c *RuntimeCore) Approve(approvalID string) (ApprovalView, bool, error) {
	if c == nil || c.api == nil {
		return ApprovalView{}, false, nil
	}
	return c.api.DecideApproval(approvalID, "agentcore-approve", approvals.ActionApprove)
}

func (c *RuntimeCore) Reject(approvalID string) (ApprovalView, bool, error) {
	if c == nil || c.api == nil {
		return ApprovalView{}, false, nil
	}
	return c.api.DecideApproval(approvalID, "agentcore-reject", approvals.ActionReject)
}

func (c *RuntimeCore) Plan(planID string) (PlanRecord, bool, error) {
	if c == nil || c.api == nil {
		return PlanRecord{}, false, nil
	}
	return c.api.Plan(planID)
}

func (c *RuntimeCore) ListPlans(query PlanQuery) ([]PlanRecord, error) {
	if c == nil || c.api == nil {
		return nil, nil
	}
	return c.api.ListPlans(query)
}

func (c *RuntimeCore) CreatePlan(ctx context.Context, ownerType, ownerID, title string) (PlanRecord, error) {
	if c == nil || c.api == nil {
		return PlanRecord{}, nil
	}
	return c.api.CreatePlan(ctx, ownerType, ownerID, title)
}

func (c *RuntimeCore) ReplacePlanItems(planID string, items []PlanItem) (PlanRecord, error) {
	if c == nil || c.api == nil {
		return PlanRecord{}, nil
	}
	return c.api.ReplacePlanItems(planID, items)
}

func (c *RuntimeCore) AppendPlanNote(planID, note string) (PlanRecord, error) {
	if c == nil || c.api == nil {
		return PlanRecord{}, nil
	}
	return c.api.AppendPlanNote(planID, note)
}

func (c *RuntimeCore) AddPlanItem(planID string, item PlanItem) (PlanRecord, error) {
	if c == nil || c.api == nil {
		return PlanRecord{}, nil
	}
	return c.api.AddPlanItem(planID, item)
}

func (c *RuntimeCore) InsertPlanItemAfter(planID, afterItemID string, item PlanItem) (PlanRecord, error) {
	if c == nil || c.api == nil {
		return PlanRecord{}, nil
	}
	return c.api.InsertPlanItemAfter(planID, afterItemID, item)
}

func (c *RuntimeCore) InsertPlanItemBefore(planID, beforeItemID string, item PlanItem) (PlanRecord, error) {
	if c == nil || c.api == nil {
		return PlanRecord{}, nil
	}
	return c.api.InsertPlanItemBefore(planID, beforeItemID, item)
}

func (c *RuntimeCore) UpdatePlanItem(planID, itemID string, patch PlanItemMutation) (PlanRecord, error) {
	if c == nil || c.api == nil {
		return PlanRecord{}, nil
	}
	return c.api.UpdatePlanItem(planID, itemID, patch)
}

func (c *RuntimeCore) RemovePlanItem(planID, itemID string) (PlanRecord, error) {
	if c == nil || c.api == nil {
		return PlanRecord{}, nil
	}
	return c.api.RemovePlanItem(planID, itemID)
}

func (c *RuntimeCore) StartPlanItem(planID, itemID string) (PlanRecord, error) {
	if c == nil || c.api == nil {
		return PlanRecord{}, nil
	}
	return c.api.StartPlanItem(planID, itemID)
}

func (c *RuntimeCore) CompletePlanItem(planID, itemID string) (PlanRecord, error) {
	if c == nil || c.api == nil {
		return PlanRecord{}, nil
	}
	return c.api.CompletePlanItem(planID, itemID)
}

func (c *RuntimeCore) ListJobs(limit int) ([]JobView, error) {
	if c == nil || c.jobs == nil {
		return nil, nil
	}
	return c.jobs.List(limit)
}

func (c *RuntimeCore) StartJobDetached(ctx context.Context, req JobStartRequest) (JobView, error) {
	if c == nil || c.jobs == nil {
		return JobView{}, nil
	}
	return c.jobs.StartDetached(ctx, req)
}

func (c *RuntimeCore) Job(jobID string) (JobView, bool, error) {
	if c == nil || c.jobs == nil {
		return JobView{}, false, nil
	}
	return c.jobs.Job(jobID)
}

func (c *RuntimeCore) JobLogs(query JobLogQuery) ([]JobLogChunk, error) {
	if c == nil || c.jobs == nil {
		return nil, nil
	}
	return c.jobs.Logs(query)
}

func (c *RuntimeCore) CancelJob(jobID string) (bool, error) {
	if c == nil || c.jobs == nil {
		return false, nil
	}
	return c.jobs.Cancel(jobID)
}

func (c *RuntimeCore) ListWorkers(query WorkerQuery) ([]WorkerView, error) {
	if c == nil || c.workers == nil {
		return nil, nil
	}
	return c.workers.List(query)
}

func (c *RuntimeCore) SpawnWorker(ctx context.Context, req WorkerSpawnRequest) (WorkerView, error) {
	if c == nil || c.workers == nil {
		return WorkerView{}, nil
	}
	return c.workers.Spawn(ctx, req)
}

func (c *RuntimeCore) MessageWorker(ctx context.Context, workerID string, req WorkerMessageRequest) (WorkerView, error) {
	if c == nil || c.workers == nil {
		return WorkerView{}, nil
	}
	return c.workers.Message(ctx, workerID, req)
}

func (c *RuntimeCore) WaitWorker(workerID string, afterCursor int, afterEventID int64, eventLimit int) (WorkerWaitResult, bool, error) {
	if c == nil || c.workers == nil {
		return WorkerWaitResult{}, false, nil
	}
	return c.workers.Wait(workerID, afterCursor, afterEventID, eventLimit)
}

func (c *RuntimeCore) Worker(workerID string) (WorkerView, bool, error) {
	if c == nil || c.workers == nil {
		return WorkerView{}, false, nil
	}
	return c.workers.Worker(workerID)
}

func (c *RuntimeCore) CloseWorker(workerID string) (WorkerView, bool, error) {
	if c == nil || c.workers == nil {
		return WorkerView{}, false, nil
	}
	return c.workers.Close(workerID)
}

func (c *RuntimeCore) WorkerHandoff(workerID string) (WorkerHandoff, bool, error) {
	if c == nil {
		return WorkerHandoff{}, false, nil
	}
	if c.workers != nil {
		return c.workers.Handoff(workerID)
	}
	if c.api != nil {
		return c.api.WorkerHandoff(workerID)
	}
	return WorkerHandoff{}, false, nil
}
