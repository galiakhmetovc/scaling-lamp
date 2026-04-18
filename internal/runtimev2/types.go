package runtimev2

type RunStatusV2 string

const (
	RunStatusRunning         RunStatusV2 = "running"
	RunStatusWaitingApproval RunStatusV2 = "waiting_approval"
	RunStatusWaitingProcess  RunStatusV2 = "waiting_process"
	RunStatusResuming        RunStatusV2 = "resuming"
	RunStatusCompleted       RunStatusV2 = "completed"
	RunStatusFailed          RunStatusV2 = "failed"
	RunStatusCancelled       RunStatusV2 = "cancelled"
)

type RunSnapshotV2 struct {
	RunID              string
	SessionID          string
	Status             RunStatusV2
	QueuedUserMessages []QueuedUserMessageV2
	ProviderStream     *ProviderStreamV2
	PendingApprovals   []PendingApprovalV2
	ActiveProcesses    []ActiveProcessV2
	RecentSteps        []RecentStepV2
	Result             *RunResultV2
	Error              string
}

type PendingApprovalV2 struct {
	ID     string
	Reason string
}

type ActiveProcessV2 struct {
	ID      string
	Command string
}

type RecentStepV2 struct {
	Title  string
	Detail string
}

type QueuedUserMessageV2 struct {
	Role    string
	Content string
}

type ProviderStreamV2 struct {
	Phase string
}

type RunResultV2 struct {
	State   RunStatusV2
	Summary string
}
