package runtime

import (
	"encoding/json"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/provider"
)

type RunView struct {
	RunID           string
	ChatID          int64
	SessionID       string
	Query           string
	FinalResponse   string
	PromptBudget    PromptBudgetMetrics
	ArtifactRefs    []string
	Status          RunStatus
	StartedAt       time.Time
	EndedAt         *time.Time
	FailureReason   string
	CancelRequested bool
	Active          bool
	PolicySnapshot  PolicySnapshot
}

type PlanItemStatus string

const (
	PlanItemPending    PlanItemStatus = "pending"
	PlanItemInProgress PlanItemStatus = "in_progress"
	PlanItemCompleted  PlanItemStatus = "completed"
	PlanItemCancelled  PlanItemStatus = "cancelled"
)

type PlanItem struct {
	ItemID    string
	Content   string
	Status    PlanItemStatus
	Position  int
	CreatedAt time.Time
	UpdatedAt time.Time
}

type PlanItemMutation struct {
	ItemID string
	Content string
	Status PlanItemStatus
}

type PlanRecord struct {
	PlanID    string
	OwnerType string
	OwnerID   string
	Title     string
	Notes     []string
	Items     []PlanItem
	CreatedAt time.Time
	UpdatedAt time.Time
}

type PlanQuery struct {
	OwnerType string
	OwnerID   string
	Limit     int
}

type ArtifactOffloadPolicy struct {
	MaxInlineChars int
	MaxInlineLines int
	PreviewLines   int
}

type ArtifactOwnerRef struct {
	OwnerType string
	OwnerID   string
}

type OffloadedToolResult struct {
	Content     string
	ArtifactRef string
	Offloaded   bool
}

type PolicySnapshot struct {
	Runtime      provider.RequestConfig
	MemoryPolicy MemoryPolicy
	ActionPolicy ActionPolicy
}

type PromptBudgetMetrics struct {
	ContextWindowTokens     int
	PromptBudgetTokens      int
	CompactionTriggerTokens int
	RawTranscriptTokens     int
	CheckpointTokens        int
	WorkspaceTokens         int
	SessionHeadTokens       int
	MemoryRecallTokens      int
	SkillsCatalogTokens     int
	ActiveSkillsTokens      int
	BasePromptTokens        int
	SystemOverheadTokens    int
	FinalPromptTokens       int
	PromptBudgetPercent     int
	ContextWindowPercent    int
	Layers                  []PromptBudgetLayer
}

type PromptContextParts struct {
	Workspace     string
	SessionHead   string
	MemoryRecall  string
	SkillsCatalog string
	ActiveSkills  string
}

type PromptContextBuild struct {
	Messages []provider.Message
	Parts    PromptContextParts
	Layers   []PromptContextLayer
}

type DebugExecutionProfile struct {
	Transcript     bool     `json:"transcript"`
	SessionHead    bool     `json:"session_head"`
	RecentWork     bool     `json:"recent_work"`
	MemoryRecall   bool     `json:"memory_recall"`
	Checkpoint     bool     `json:"checkpoint"`
	Workspace      bool     `json:"workspace"`
	Skills         bool     `json:"skills"`
	Tools          bool     `json:"tools"`
	AllowedTools   []string `json:"allowed_tools,omitempty"`
	WorkspaceFiles []string `json:"workspace_files,omitempty"`
}

func DefaultDebugExecutionProfile() DebugExecutionProfile {
	return DebugExecutionProfile{
		Transcript:   true,
		SessionHead:  true,
		RecentWork:   true,
		MemoryRecall: true,
		Checkpoint:   true,
		Workspace:    true,
		Skills:       true,
		Tools:        true,
	}
}

type MCPPolicyMode string

const (
	MCPPolicyAllowlist MCPPolicyMode = "allowlist"
)

type MCPPolicy struct {
	Mode           MCPPolicyMode
	AllowedTools   []string
	ShellTimeout   time.Duration
	MaxOutputBytes int
	MaxOutputLines int
}

type EffectivePolicy struct {
	Summary RuntimeSummary
	MCP     MCPPolicy
}

type MCPToolPolicy struct {
	Name           string
	Timeout        time.Duration
	MaxOutputBytes int
	MaxOutputLines int
}

type ToolExecutionDecision struct {
	Allowed          bool
	RequiresApproval bool
	Reason           string
	Policy           MCPToolPolicy
}

func NormalizePolicySnapshot(snapshot PolicySnapshot) PolicySnapshot {
	snapshot.MemoryPolicy = NormalizeMemoryPolicy(snapshot.MemoryPolicy)
	snapshot.ActionPolicy = NormalizeActionPolicy(snapshot.ActionPolicy)
	return snapshot
}

func PolicySnapshotForSummary(summary RuntimeSummary) PolicySnapshot {
	return NormalizePolicySnapshot(PolicySnapshot{
		Runtime:      summary.Runtime,
		MemoryPolicy: summary.MemoryPolicy,
		ActionPolicy: summary.ActionPolicy,
	})
}

type EventQuery struct {
	EntityType string
	EntityID   string
	RunID      string
	SessionID  string
	AfterID    int64
	Limit      int
}

type RuntimeEvent struct {
	ID         int64
	EntityType string
	EntityID   string
	ChatID     int64
	SessionID  string
	RunID      string
	Kind       string
	Payload    json.RawMessage
	CreatedAt  time.Time
}

type ReplayStep struct {
	Index     int
	Kind      string
	Message   string
	EventID   int64
	CreatedAt time.Time
}

type RunReplay struct {
	Run   RunView
	Steps []ReplayStep
}

type RecentWorkSnapshot struct {
	Query  string
	Intent RecentWorkIntent
	Head   SessionHead
	Replay *RunReplay
}

type JobStatus string

const (
	JobQueued    JobStatus = "queued"
	JobRunning   JobStatus = "running"
	JobCompleted JobStatus = "completed"
	JobFailed    JobStatus = "failed"
	JobCancelled JobStatus = "cancelled"
)

type JobRecord struct {
	JobID           string
	Kind            string
	OwnerRunID      string
	OwnerWorkerID   string
	ChatID          int64
	SessionID       string
	Command         string
	Args            []string
	Cwd             string
	Status          JobStatus
	StartedAt       time.Time
	EndedAt         *time.Time
	ExitCode        *int
	FailureReason   string
	CancelRequested bool
	PolicySnapshot  PolicySnapshot
}

type JobView struct {
	JobID           string
	Kind            string
	OwnerRunID      string
	OwnerWorkerID   string
	ChatID          int64
	SessionID       string
	ArtifactRefs    []string
	Command         string
	Args            []string
	Cwd             string
	Status          JobStatus
	StartedAt       time.Time
	EndedAt         *time.Time
	ExitCode        *int
	FailureReason   string
	CancelRequested bool
	Active          bool
	PolicySnapshot  PolicySnapshot
}

type JobStartRequest struct {
	JobID          string
	Kind           string
	OwnerRunID     string
	OwnerWorkerID  string
	ChatID         int64
	SessionID      string
	Command        string
	Args           []string
	Cwd            string
	PolicySnapshot PolicySnapshot
}

type JobLogQuery struct {
	JobID   string
	Stream  string
	AfterID int64
	Limit   int
}

type JobLogChunk struct {
	ID        int64
	JobID     string
	Stream    string
	Content   string
	CreatedAt time.Time
}

type WorkerStatus string

const (
	WorkerIdle            WorkerStatus = "idle"
	WorkerRunning         WorkerStatus = "running"
	WorkerWaitingApproval WorkerStatus = "waiting_approval"
	WorkerFailed          WorkerStatus = "failed"
	WorkerClosed          WorkerStatus = "closed"
)

type WorkerProcessState string

const (
	WorkerProcessStopped  WorkerProcessState = "stopped"
	WorkerProcessStarting WorkerProcessState = "starting"
	WorkerProcessRunning  WorkerProcessState = "running"
	WorkerProcessFailed   WorkerProcessState = "failed"
)

type WorkerProcessRuntime struct {
	PID             int
	State           WorkerProcessState
	StartedAt       *time.Time
	LastHeartbeatAt *time.Time
	ExitedAt        *time.Time
	ExitReason      string
}

type WorkerRecord struct {
	WorkerID        string
	ParentChatID    int64
	ParentSessionID string
	WorkerChatID    int64
	WorkerSessionID string
	Status          WorkerStatus
	LastRunID       string
	LastError       string
	CreatedAt       time.Time
	UpdatedAt       time.Time
	LastMessageAt   *time.Time
	ClosedAt        *time.Time
	PolicySnapshot  PolicySnapshot
	Process         WorkerProcessRuntime
}

type PromotedFact struct {
	Fact   string `json:"fact"`
	Source string `json:"source,omitempty"`
}

type WorkerHandoff struct {
	WorkerID            string
	LastRunID           string
	Summary             string
	Artifacts           []string
	PromotedFacts       []PromotedFact
	OpenQuestions       []string
	RecommendedNextStep string
	CreatedAt           time.Time
	UpdatedAt           time.Time
}

type WorkerView struct {
	WorkerID        string
	ParentChatID    int64
	ParentSessionID string
	WorkerChatID    int64
	WorkerSessionID string
	ArtifactRefs    []string
	Status          WorkerStatus
	LastRunID       string
	LastRun         *RunView
	Handoff         *WorkerHandoff
	LastError       string
	CreatedAt       time.Time
	UpdatedAt       time.Time
	LastMessageAt   *time.Time
	ClosedAt        *time.Time
	PolicySnapshot  PolicySnapshot
	Process         WorkerProcessRuntime
}

type WorkerSpawnRequest struct {
	WorkerID        string
	ParentChatID    int64
	ParentSessionID string
	Prompt          string
	PolicySnapshot  PolicySnapshot
}

type WorkerMessageRequest struct {
	Content string
}

type WorkerMessage struct {
	Cursor     int    `json:"cursor"`
	Role       string `json:"role"`
	Content    string `json:"content"`
	Name       string `json:"name,omitempty"`
	ToolCallID string `json:"tool_call_id,omitempty"`
}

type WorkerWaitResult struct {
	Worker         WorkerView
	Handoff        *WorkerHandoff
	Messages       []WorkerMessage
	Events         []RuntimeEvent
	NextCursor     int
	NextEventAfter int64
}

type WorkerQuery struct {
	ParentChatID    int64
	HasParentChatID bool
	Limit           int
}

type RunQuery struct {
	ChatID    int64
	HasChatID bool
	SessionID string
	Status    RunStatus
	HasStatus bool
	Limit     int
}

type SessionQuery struct {
	ChatID    int64
	HasChatID bool
	Limit     int
}

type SessionRecord struct {
	SessionID      string
	LastActivityAt time.Time
	HasOverrides   bool
}

type ApprovalView struct {
	ID               string
	WorkerID         string
	SessionID        string
	Payload          string
	Status           approvals.Status
	Reason           string
	TargetType       string
	TargetID         string
	RequestedAt      time.Time
	DecidedAt        *time.Time
	DecisionUpdateID string
}

type SessionOverrides struct {
	SessionID    string
	Runtime      provider.RequestConfig
	MemoryPolicy MemoryPolicyOverride
	ActionPolicy ActionPolicyOverride
	UpdatedAt    time.Time
}

type MemoryPolicyOverride struct {
	Profile              string
	PromoteCheckpoint    *bool
	PromoteContinuity    *bool
	AutomaticRecallKinds []string
	MaxDocumentBodyChars *int
	MaxResolvedFacts     *int
}

type ActionPolicyOverride struct {
	ApprovalRequiredTools []string
}

type RuntimeSummary struct {
	SessionID    string
	Runtime      provider.RequestConfig
	MemoryPolicy MemoryPolicy
	ActionPolicy ActionPolicy
	HasOverrides bool
	Overrides    SessionOverrides
}

type SessionState struct {
	SessionID        string
	ChatID           int64
	LastActivityAt   time.Time
	HasOverrides     bool
	RuntimeSummary   RuntimeSummary
	LatestRun        *RunView
	Head             *SessionHead
	PendingApprovals int
}

type ControlState struct {
	Session   SessionState
	Approvals []ApprovalView
	Workers   []WorkerView
	Jobs      []JobView
}

type ApprovalContinuation struct {
	ApprovalID    string
	RunID         string
	ChatID        int64
	SessionID     string
	Query         string
	ToolCallID    string
	ToolName      string
	ToolArguments map[string]any
	RequestedAt   time.Time
}

type TimeoutDecisionStatus string

const (
	TimeoutDecisionPending   TimeoutDecisionStatus = "pending"
	TimeoutDecisionContinued TimeoutDecisionStatus = "continued"
	TimeoutDecisionRetried   TimeoutDecisionStatus = "retried"
	TimeoutDecisionCancelled TimeoutDecisionStatus = "cancelled"
	TimeoutDecisionFailed    TimeoutDecisionStatus = "failed"
	TimeoutDecisionExpired   TimeoutDecisionStatus = "expired"
)

type TimeoutDecisionAction string

const (
	TimeoutDecisionActionContinue TimeoutDecisionAction = "continue"
	TimeoutDecisionActionRetry    TimeoutDecisionAction = "retry_round"
	TimeoutDecisionActionCancel   TimeoutDecisionAction = "cancel"
	TimeoutDecisionActionFail     TimeoutDecisionAction = "fail"
)

type TimeoutDecisionRecord struct {
	RunID                string
	ChatID               int64
	SessionID            string
	Status               TimeoutDecisionStatus
	FailureReason        string
	RequestedAt          time.Time
	ResolvedAt           *time.Time
	AutoContinueDeadline *time.Time
	AutoContinueUsed     bool
	RoundIndex           int
}
