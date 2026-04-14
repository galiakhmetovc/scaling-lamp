package mesh

import "time"

type PeerDescriptor struct {
	AgentID    string
	Addr       string
	Model      string
	Status     string
	StartedAt  time.Time
	LastSeenAt time.Time
	Metadata   map[string]any
}

type ScoreRecord struct {
	AgentID        string
	TaskClass      string
	TasksSeen      int
	TasksWon       int
	SuccessCount   int
	FailureCount   int
	AvgLatencyMS   int64
	LastScoreAt    time.Time
}

type Envelope struct {
	Version    string
	MessageID  string
	TraceID    string
	SessionID  string
	OwnerAgent string
	FromAgent  string
	ToAgent    string
	TaskClass  string
	TaskShape  string
	Mode       string
	ParentStepID string
	Kind       string
	TTL        int
	Prompt     string
	ExecutionBrief ExecutionBrief
	Metadata   map[string]any
}

type CandidateReply struct {
	AgentID            string
	Stage              string
	Text               string
	Latency            time.Duration
	TokensUsed         int
	DeterministicScore int
	JudgeScore         int
	PassedChecks       bool
	Err                string
	Proposal           Proposal
	ProposalMetadata   ProposalMetadata
	ExecutionNotes     []string
	Artifacts          []string
	RejectionReason    string
	Trace              []TraceEvent
}

type ContextPolicy struct {
	MaxTokensHard int
	TrackStats    bool
}

type TimeoutPolicy struct {
	PeerTimeout  time.Duration
	OwnerTimeout time.Duration
	LogOnly      bool
}

type MemoryScope struct {
	SessionID          string
	PrivateSemanticRO  []string
	SharedSemanticRW   []string
	CrossAgentReadOnly bool
}

type TaskShape string

const (
	TaskShapeSingle    TaskShape = "single"
	TaskShapeComposite TaskShape = "composite"
)

type ClarifiedTask struct {
	Goal              string   `json:"goal"`
	Deliverables      []string `json:"deliverables"`
	Constraints       []string `json:"constraints"`
	Assumptions       []string `json:"assumptions"`
	MissingInfo       []string `json:"missing_info"`
	TaskClass         string   `json:"task_class"`
	TaskShape         string   `json:"task_shape"`
	RequiresFollowUp  bool     `json:"-"`
	FollowUpQuestion  string   `json:"-"`
	LowConfidence     bool     `json:"-"`
}

type ClarificationInput struct {
	Mode                      string
	Prompt                    string
	CriticalMissingInfo       bool
	MaxClarificationRounds    int
	CurrentClarificationRound int
}

type Proposal struct {
	Understanding  string
	PlannedChecks  []string
	SuggestedTools []string
	Risks          []string
	DraftConclusion string
}

type ProposalMetadata struct {
	EstimatedTokens int
	SuggestedTools  []string
	Confidence      float64
	RiskFlags       []string
}

type ExecutionBrief struct {
	Goal               string
	RequiredSteps      []string
	Constraints        []string
	AdoptedIdeas       []string
	ConflictsToResolve []string
	RequiredChecks     []string
}

type TaskPlan struct {
	TaskShape string        `json:"task_shape"`
	Steps     []PlannedStep `json:"steps"`
}

type PlannedStep struct {
	StepID        string   `json:"step_id"`
	Title         string   `json:"title"`
	TaskClass     string   `json:"task_class"`
	Description   string   `json:"description"`
	RequiresTools bool     `json:"requires_tools"`
	Dependencies  []string `json:"dependencies,omitempty"`
}

type TraceEvent struct {
	Section string
	Summary string
	Payload string
}

type ProposalPolicy struct {
	ProposalTimeout time.Duration
	MinQuorumSize   int
	RetryCount      int
}

func (p ProposalPolicy) SatisfiesQuorum(count int) bool {
	if p.MinQuorumSize <= 0 {
		return count > 0
	}
	return count >= p.MinQuorumSize
}
