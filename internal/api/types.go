package api

import (
	"time"

	"teamd/internal/approvals"
	"teamd/internal/llmtrace"
	"teamd/internal/memory"
	"teamd/internal/provider"
	"teamd/internal/runtime"
)

type CreateRunRequest struct {
	ChatID        int64                          `json:"chat_id"`
	SessionID     string                         `json:"session_id"`
	Query         string                         `json:"query"`
	Config        provider.RequestConfig         `json:"config,omitempty"`
	ContextInputs *runtime.DebugExecutionProfile `json:"context_inputs,omitempty"`
}

type CreateRunResponse struct {
	RunID    string          `json:"run_id"`
	Accepted bool            `json:"accepted"`
	Run      runtime.RunView `json:"run"`
	Error    *APIError       `json:"error,omitempty"`
}

type RunStatusResponse struct {
	Run runtime.RunView `json:"run"`
}

type RunListResponse struct {
	Items []runtime.RunView `json:"items"`
}

type RunReplayResponse struct {
	Replay runtime.RunReplay `json:"replay"`
}

type EventListRequest struct {
	EntityType string `json:"entity_type"`
	EntityID   string `json:"entity_id"`
	RunID      string `json:"run_id"`
	SessionID  string `json:"session_id"`
	AfterID    int64  `json:"after_id"`
	Limit      int    `json:"limit"`
}

type EventListResponse struct {
	Items []runtime.RuntimeEvent `json:"items"`
}

type CreatePlanRequest struct {
	OwnerType string `json:"owner_type"`
	OwnerID   string `json:"owner_id"`
	Title     string `json:"title"`
}

type ReplacePlanItemsRequest struct {
	Items []runtime.PlanItem `json:"items"`
}

type AppendPlanNoteRequest struct {
	Note string `json:"note"`
}

type PlanResponse struct {
	Plan runtime.PlanRecord `json:"plan"`
}

type PlanListResponse struct {
	Items []runtime.PlanRecord `json:"items"`
}

type ArtifactMetadata struct {
	Ref       string    `json:"ref"`
	Name      string    `json:"name"`
	OwnerType string    `json:"owner_type,omitempty"`
	OwnerID   string    `json:"owner_id,omitempty"`
	SizeBytes int       `json:"size_bytes"`
	CreatedAt time.Time `json:"created_at,omitempty"`
}

type ArtifactResponse struct {
	Artifact ArtifactMetadata `json:"artifact"`
}

type ArtifactContentResponse struct {
	Content string `json:"content"`
}

type ArtifactSearchRequest struct {
	OwnerType string `json:"owner_type,omitempty"`
	OwnerID   string `json:"owner_id,omitempty"`
	RunID     string `json:"run_id,omitempty"`
	WorkerID  string `json:"worker_id,omitempty"`
	Query     string `json:"query"`
	Limit     int    `json:"limit,omitempty"`
	Global    bool   `json:"global,omitempty"`
}

type ArtifactSearchItem struct {
	Ref       string    `json:"ref"`
	Name      string    `json:"name"`
	OwnerType string    `json:"owner_type,omitempty"`
	OwnerID   string    `json:"owner_id,omitempty"`
	SizeBytes int       `json:"size_bytes"`
	Preview   string    `json:"preview"`
	CreatedAt time.Time `json:"created_at,omitempty"`
}

type ArtifactSearchResponse struct {
	Items []ArtifactSearchItem `json:"items"`
}

type CreateJobRequest struct {
	Kind          string   `json:"kind"`
	OwnerRunID    string   `json:"owner_run_id"`
	OwnerWorkerID string   `json:"owner_worker_id"`
	ChatID        int64    `json:"chat_id"`
	SessionID     string   `json:"session_id"`
	Command       string   `json:"command"`
	Args          []string `json:"args"`
	Cwd           string   `json:"cwd"`
}

type CreateJobResponse struct {
	Job runtime.JobView `json:"job"`
}

type JobStatusResponse struct {
	Job runtime.JobView `json:"job"`
}

type JobListResponse struct {
	Items []runtime.JobView `json:"items"`
}

type JobLogsResponse struct {
	Items []runtime.JobLogChunk `json:"items"`
}

type CreateWorkerRequest struct {
	WorkerID  string `json:"worker_id"`
	ChatID    int64  `json:"chat_id"`
	SessionID string `json:"session_id"`
	Prompt    string `json:"prompt"`
}

type WorkerMessageRequest struct {
	Content string `json:"content"`
}

type WorkerStatusResponse struct {
	Worker runtime.WorkerView `json:"worker"`
}

type WorkerHandoffResponse struct {
	Handoff runtime.WorkerHandoff `json:"handoff"`
}

type WorkerListResponse struct {
	Items []runtime.WorkerView `json:"items"`
}

type WorkerWaitResponse struct {
	Worker         runtime.WorkerView      `json:"worker"`
	Handoff        *runtime.WorkerHandoff  `json:"handoff,omitempty"`
	Messages       []runtime.WorkerMessage `json:"messages"`
	Events         []runtime.RuntimeEvent  `json:"events"`
	NextCursor     int                     `json:"next_cursor"`
	NextEventAfter int64                   `json:"next_event_after"`
}

type ApprovalRecordResponse struct {
	ID               string           `json:"id"`
	WorkerID         string           `json:"worker_id"`
	SessionID        string           `json:"session_id"`
	Payload          string           `json:"payload"`
	Status           approvals.Status `json:"status"`
	Reason           string           `json:"reason,omitempty"`
	TargetType       string           `json:"target_type,omitempty"`
	TargetID         string           `json:"target_id,omitempty"`
	RequestedAt      time.Time        `json:"requested_at"`
	DecidedAt        *time.Time       `json:"decided_at,omitempty"`
	DecisionUpdateID string           `json:"decision_update_id,omitempty"`
}

type SessionOverrideResponse struct {
	SessionID    string                       `json:"session_id"`
	Runtime      provider.RequestConfig       `json:"runtime"`
	MemoryPolicy runtime.MemoryPolicyOverride `json:"memory_policy"`
	ActionPolicy runtime.ActionPolicyOverride `json:"action_policy"`
	UpdatedAt    *time.Time                   `json:"updated_at,omitempty"`
}

type RuntimeSummaryResponse struct {
	SessionID    string                   `json:"session_id"`
	Runtime      provider.RequestConfig   `json:"runtime"`
	MemoryPolicy runtime.MemoryPolicy     `json:"memory_policy"`
	ActionPolicy runtime.ActionPolicy     `json:"action_policy"`
	HasOverrides bool                     `json:"has_overrides"`
	Overrides    *SessionOverrideResponse `json:"overrides,omitempty"`
}

type SessionStateResponse struct {
	Session runtime.SessionState `json:"session"`
}

type ControlStateResponse struct {
	Control runtime.ControlState `json:"control"`
}

type ControlActionRequest struct {
	Action string `json:"action"`
	ChatID int64  `json:"chat_id"`
}

type ControlActionResponse struct {
	Result runtime.ControlActionResult `json:"result"`
}

type SessionActionRequest struct {
	ChatID      int64  `json:"chat_id"`
	Action      string `json:"action"`
	SessionName string `json:"session_name,omitempty"`
}

type SessionActionResponse struct {
	Result runtime.SessionActionResult `json:"result"`
}

type SessionListResponse struct {
	Items []runtime.SessionState `json:"items"`
}

type DebugSessionResponse struct {
	Session runtime.SessionState   `json:"session"`
	Control runtime.ControlState   `json:"control"`
	Events  []runtime.RuntimeEvent `json:"events"`
}

type DebugRunResponse struct {
	Run    runtime.RunView        `json:"run"`
	Replay *runtime.RunReplay     `json:"replay,omitempty"`
	Events []runtime.RuntimeEvent `json:"events"`
}

type DebugContextResponse struct {
	Provenance runtime.DebugContextProvenance `json:"provenance"`
}

type DebugRawConversationTurn struct {
	Query               string                  `json:"query,omitempty"`
	Request             provider.PromptRequest  `json:"request,omitempty"`
	Response            provider.PromptResponse `json:"response,omitempty"`
	Trace               llmtrace.CallTrace      `json:"trace,omitempty"`
	LogPath             string                  `json:"log_path,omitempty"`
	SystemPrompt        string                  `json:"system_prompt,omitempty"`
	IncludeSystemPrompt bool                    `json:"include_system_prompt,omitempty"`
}

type DebugRawConversationResponse struct {
	SessionID string                     `json:"session_id"`
	Messages  []provider.Message         `json:"messages"`
	Turns     []DebugRawConversationTurn `json:"turns"`
}

type DebugRawNetworkResponse struct {
	Request  provider.PromptRequest  `json:"request"`
	Response provider.PromptResponse `json:"response"`
	Trace    llmtrace.CallTrace      `json:"trace"`
	LogPath  string                  `json:"log_path,omitempty"`
}

type DebugRawToolExecRequest struct {
	ChatID    int64             `json:"chat_id"`
	SessionID string            `json:"session_id,omitempty"`
	Tools     []string          `json:"tools,omitempty"`
	Call      provider.ToolCall `json:"call"`
}

type DebugRawToolExecResponse struct {
	Call         provider.ToolCall `json:"call"`
	Output       string            `json:"output"`
	Success      bool              `json:"success"`
	ErrorCode    string            `json:"error_code,omitempty"`
	ErrorMessage string            `json:"error_message,omitempty"`
	LogPath      string            `json:"log_path,omitempty"`
}

type DebugProviderPreviewResponse struct {
	Request provider.PromptRequest        `json:"request"`
	Metrics runtime.PromptBudgetMetrics   `json:"metrics"`
}

type ToolCatalogResponse struct {
	Items []provider.ToolDefinition `json:"items"`
}

type DebugRawNetworkRequest struct {
	ChatID    int64                  `json:"chat_id"`
	SessionID string                 `json:"session_id,omitempty"`
	Query     string                 `json:"query,omitempty"`
	SystemPrompt string              `json:"system_prompt,omitempty"`
	IncludeSystemPrompt bool         `json:"include_system_prompt,omitempty"`
	Messages  []provider.Message     `json:"messages,omitempty"`
	Tools     []string               `json:"tools,omitempty"`
	OffloadOldToolOutputs bool       `json:"offload_old_tool_outputs,omitempty"`
	Config    provider.RequestConfig `json:"config,omitempty"`
}

type SessionOverrideRequest struct {
	Runtime      provider.RequestConfig       `json:"runtime"`
	MemoryPolicy runtime.MemoryPolicyOverride `json:"memory_policy"`
	ActionPolicy runtime.ActionPolicyOverride `json:"action_policy"`
}

type MemorySearchResponse struct {
	Items []memory.RecallItem `json:"items"`
}

type MemoryDocumentResponse struct {
	Document memory.Document `json:"document"`
}

type APIError struct {
	Code       string `json:"code"`
	Message    string `json:"message"`
	RequestID  string `json:"request_id,omitempty"`
	EntityType string `json:"entity_type,omitempty"`
	EntityID   string `json:"entity_id,omitempty"`
	Retryable  bool   `json:"retryable,omitempty"`
}

type ErrorResponse struct {
	Error APIError  `json:"error"`
	Time  time.Time `json:"time"`
}
