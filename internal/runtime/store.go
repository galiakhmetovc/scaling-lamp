package runtime

import (
	"time"

	"teamd/internal/approvals"
)

type RunStatus string

const (
	StatusQueued          RunStatus = "queued"
	StatusRunning         RunStatus = "running"
	StatusWaitingApproval RunStatus = "waiting_approval"
	StatusWaitingOperator RunStatus = "waiting_operator"
	StatusCompleted       RunStatus = "completed"
	StatusFailed          RunStatus = "failed"
	StatusCancelled       RunStatus = "cancelled"
)

type RunRecord struct {
	RunID           string
	ChatID          int64
	SessionID       string
	Query           string
	FinalResponse   string
	PromptBudget    PromptBudgetMetrics
	Status          RunStatus
	StartedAt       time.Time
	EndedAt         *time.Time
	FailureReason   string
	CancelRequested bool
	PolicySnapshot  PolicySnapshot
}

type Checkpoint struct {
	ChatID            int64
	SessionID         string
	OriginatingIntent string
	WhatHappened      string
	WhatMattersNow    string
	ArchiveRefs       []string
	ArtifactRefs      []string
	UpdatedAt         time.Time
}

type Continuity struct {
	ChatID          int64
	SessionID       string
	UserGoal        string
	CurrentState    string
	ResolvedFacts   []string
	UnresolvedItems []string
	ArchiveRefs     []string
	ArtifactRefs    []string
	UpdatedAt       time.Time
}

type SessionHead struct {
	ChatID             int64
	SessionID          string
	LastCompletedRunID string
	CurrentGoal        string
	LastResultSummary  string
	CurrentPlanID      string
	CurrentPlanTitle   string
	CurrentPlanItems   []string
	ResolvedEntities   []string
	RecentArtifactRefs []string
	OpenLoops          []string
	CurrentProject     string
	UpdatedAt          time.Time
}

type RunLifecycleStore interface {
	SaveRun(RunRecord) error
	MarkCancelRequested(runID string) error
	Run(runID string) (RunRecord, bool, error)
	ListRuns(query RunQuery) ([]RunRecord, error)
	ListSessions(query SessionQuery) ([]SessionRecord, error)
	SaveEvent(RuntimeEvent) error
	ListEvents(query EventQuery) ([]RuntimeEvent, error)
	RecoverInterruptedRuns(reason string) (int, error)
}

type PlanStore interface {
	SavePlan(PlanRecord) error
	Plan(planID string) (PlanRecord, bool, error)
	ListPlans(query PlanQuery) ([]PlanRecord, error)
	SaveEvent(RuntimeEvent) error
}

type JobStore interface {
	SaveJob(JobRecord) error
	Job(jobID string) (JobRecord, bool, error)
	ListJobs(limit int) ([]JobRecord, error)
	MarkJobCancelRequested(jobID string) error
	SaveEvent(RuntimeEvent) error
	SaveJobLog(JobLogChunk) error
	JobLogs(query JobLogQuery) ([]JobLogChunk, error)
	RecoverInterruptedJobs(reason string) (int, error)
}

type WorkerStore interface {
	SaveWorker(WorkerRecord) error
	Worker(workerID string) (WorkerRecord, bool, error)
	ListWorkers(query WorkerQuery) ([]WorkerRecord, error)
	RecoverInterruptedWorkers(reason string) (int, error)
	SaveWorkerHandoff(WorkerHandoff) error
	WorkerHandoff(workerID string) (WorkerHandoff, bool, error)
	SaveEvent(RuntimeEvent) error
	ListEvents(query EventQuery) ([]RuntimeEvent, error)
}

type SessionStateStore interface {
	SaveCheckpoint(Checkpoint) error
	Checkpoint(chatID int64, sessionID string) (Checkpoint, bool, error)
	SaveContinuity(Continuity) error
	Continuity(chatID int64, sessionID string) (Continuity, bool, error)
	SaveSessionHead(SessionHead) error
	SessionHead(chatID int64, sessionID string) (SessionHead, bool, error)
}

type ProcessedUpdateStore interface {
	TryMarkUpdate(chatID int64, updateID int64) (bool, error)
}

type SessionOverrideStore interface {
	SaveSessionOverrides(overrides SessionOverrides) error
	SessionOverrides(sessionID string) (SessionOverrides, bool, error)
	ClearSessionOverrides(sessionID string) error
}

type ApprovalStateStore interface {
	SaveApproval(approvals.Record) error
	Approval(id string) (approvals.Record, bool, error)
	PendingApprovals(sessionID string) ([]approvals.Record, error)
	SaveHandledApprovalCallback(updateID string, record approvals.Record) error
	HandledApprovalCallback(updateID string) (approvals.Record, bool, error)
	SaveApprovalContinuation(ApprovalContinuation) error
	ApprovalContinuation(id string) (ApprovalContinuation, bool, error)
	DeleteApprovalContinuation(id string) error
}

type TimeoutDecisionStore interface {
	SaveTimeoutDecision(TimeoutDecisionRecord) error
	TimeoutDecision(runID string) (TimeoutDecisionRecord, bool, error)
	DeleteTimeoutDecision(runID string) error
}

type Store interface {
	RunLifecycleStore
	PlanStore
	JobStore
	WorkerStore
	SessionStateStore
	ProcessedUpdateStore
	SessionOverrideStore
	ApprovalStateStore
	TimeoutDecisionStore
}
