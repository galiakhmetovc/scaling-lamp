package telegram

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"sync"
	"time"

	"teamd/internal/approvals"
	"teamd/internal/artifacts"
	"teamd/internal/compaction"
	"teamd/internal/events"
	"teamd/internal/mcp"
	"teamd/internal/memory"
	"teamd/internal/mesh"
	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
	"teamd/internal/skills"
	"teamd/internal/workspace"
)

const skillsToolListName = "skills_list"
const skillsToolReadName = "skills_read"
const skillsToolActivateName = "activate_skill"
const memoryToolSearchName = "memory_search"
const memoryToolReadName = "memory_read"
const jobStartToolName = "job_start"
const jobStatusToolName = "job_status"
const jobCancelToolName = "job_cancel"
const agentSpawnToolName = "agent_spawn"
const agentMessageToolName = "agent_message"
const agentWaitToolName = "agent_wait"
const planCreateToolName = "plan_create"
const planReplaceItemsToolName = "plan_replace_items"
const planAnnotateToolName = "plan_annotate"
const planItemAddToolName = "plan_item_add"
const planItemInsertAfterToolName = "plan_item_insert_after"
const planItemInsertBeforeToolName = "plan_item_insert_before"
const planItemUpdateToolName = "plan_item_update"
const planItemRemoveToolName = "plan_item_remove"
const planItemStartToolName = "plan_item_start"
const planItemCompleteToolName = "plan_item_complete"
const projectCaptureRecentToolName = "project_capture_recent"

type Deps struct {
	BaseURL                 string
	Token                   string
	HTTPClient              *http.Client
	Provider                provider.Provider
	ProviderRoundTimeout    time.Duration
	LLMCompactionEnabled    bool
	LLMCompactionTimeout    time.Duration
	ContextWindowTokens     int
	PromptBudgetTokens      int
	CompactionTriggerTokens int
	MaxToolContextChars     int
	RunStore                runtimex.Store
	Store                   Store
	Tools                   ToolRuntime
	Mesh                    MeshRuntime
	Memory                  memory.Store
	Artifacts               artifacts.Store
	MemoryPolicy            runtimex.MemoryPolicy
	Approvals               *approvals.Service
	ActionPolicy            runtimex.ActionPolicy
	MCPPolicy               runtimex.MCPPolicy
	Skills                  skills.Catalog
	WorkspaceRoot           string
	RuntimeDefaults         provider.RequestConfig
	TraceEnabled            bool
	TraceDir                string
}

type Adapter struct {
	baseURL              string
	token                string
	httpClient           *http.Client
	provider             provider.Provider
	store                Store
	tools                ToolRuntime
	mesh                 MeshRuntime
	memory               memory.Store
	artifacts            artifacts.Store
	skills               skills.Catalog
	skillState           *skills.SessionState
	workspaceRoot        string
	workspaceContext     string
	budget               compaction.Budget
	compactor            *compaction.Service
	sessionMu            sync.Map
	meshPolicyMu         sync.RWMutex
	meshPolicies         map[string]mesh.OrchestrationPolicy
	runtimeConfigMu      sync.RWMutex
	runtimeConfigs       map[string]provider.RequestConfig
	runs                 *RunStateStore
	activeRuns           *runtimex.ActiveRegistry
	refreshInterval      time.Duration
	runtimeDefaults      provider.RequestConfig
	traceEnabled         bool
	traceDir             string
	providerRoundTimeout time.Duration
	runStore             runtimex.Store
	runtimeAPI           *runtimex.API
	execution            *runtimex.ExecutionService
	agentCore            runtimex.AgentCore
	memoryPolicy         runtimex.MemoryPolicy
	approvals            *approvals.Service
	actionPolicy         runtimex.ActionPolicy
	mcpPolicy            runtimex.MCPPolicy
	traceCollectors      sync.Map
	debugProfiles        sync.Map
	busyMessages         sync.Map
	jobControl           JobControl
	workerControl        WorkerControl
	sessionActions       *runtimex.SessionActions
}

const minStatusSyncInterval = 2 * time.Second

type ToolRuntime interface {
	ListTools(role string) ([]mcp.Tool, error)
	CallTool(ctx context.Context, name string, args map[string]any) (mcp.CallResult, error)
}

type JobControl interface {
	StartDetached(ctx context.Context, req runtimex.JobStartRequest) (runtimex.JobView, error)
	Job(jobID string) (runtimex.JobView, bool, error)
	Cancel(jobID string) (bool, error)
}

type WorkerControl interface {
	Spawn(ctx context.Context, req runtimex.WorkerSpawnRequest) (runtimex.WorkerView, error)
	Message(ctx context.Context, workerID string, req runtimex.WorkerMessageRequest) (runtimex.WorkerView, error)
	Wait(workerID string, afterCursor int, afterEventID int64, eventLimit int) (runtimex.WorkerWaitResult, bool, error)
}

type MeshRuntime interface {
	HandleOwnerTask(ctx context.Context, sessionID, prompt string, policy mesh.OrchestrationPolicy) (mesh.CandidateReply, error)
}

func (a *Adapter) DebugToolCatalog(role string) ([]provider.ToolDefinition, error) {
	if strings.TrimSpace(role) == "" {
		role = "telegram"
	}
	return a.providerTools(role)
}

type MessageUpdate struct {
	Text string
}

type Update struct {
	UpdateID      int64
	ChatID        int64
	Text          string
	CallbackID    string
	CallbackData  string
	CallbackQuery bool
}

type getUpdatesResponse struct {
	OK     bool            `json:"ok"`
	Result []telegramEntry `json:"result"`
}

type telegramMutationResponse struct {
	OK     bool `json:"ok"`
	Result struct {
		MessageID int64 `json:"message_id"`
	} `json:"result"`
}

type telegramBotCommand struct {
	Command     string `json:"command"`
	Description string `json:"description"`
}

type telegramEntry struct {
	UpdateID int64 `json:"update_id"`
	Message  struct {
		MessageID int64 `json:"message_id"`
		Chat      struct {
			ID int64 `json:"id"`
		} `json:"chat"`
		Text string `json:"text"`
	} `json:"message"`
	CallbackQuery struct {
		ID      string `json:"id"`
		Data    string `json:"data"`
		Message struct {
			Chat struct {
				ID int64 `json:"id"`
			} `json:"chat"`
		} `json:"message"`
	} `json:"callback_query"`
}

func TestDeps() Deps {
	return Deps{
		BaseURL:    "https://api.telegram.org",
		Token:      "test-token",
		HTTPClient: http.DefaultClient,
		Provider:   provider.FakeProvider{},
	}
}

func New(deps Deps) *Adapter {
	baseURL := deps.BaseURL
	if baseURL == "" {
		baseURL = "https://api.telegram.org"
	}
	client := deps.HTTPClient
	if client == nil {
		client = http.DefaultClient
	}
	if deps.Provider == nil {
		deps.Provider = provider.FakeProvider{}
	}
	if deps.Store == nil {
		deps.Store = NewSessionStore(16)
	}
	if deps.Skills == nil && strings.TrimSpace(deps.WorkspaceRoot) != "" {
		catalog := skills.NewFilesystemCatalog(strings.TrimSpace(deps.WorkspaceRoot))
		deps.Skills = catalog
	}

	adapter := &Adapter{
		baseURL:          strings.TrimRight(baseURL, "/"),
		token:            deps.Token,
		httpClient:       client,
		provider:         deps.Provider,
		store:            deps.Store,
		tools:            deps.Tools,
		mesh:             deps.Mesh,
		memory:           deps.Memory,
		artifacts:        deps.Artifacts,
		approvals:        deps.Approvals,
		skills:           deps.Skills,
		skillState:       skills.NewSessionState(),
		workspaceRoot:    strings.TrimSpace(deps.WorkspaceRoot),
		workspaceContext: workspace.BuildAGENTSContext(strings.TrimSpace(deps.WorkspaceRoot)),
		budget: compaction.Budget{
			ContextWindowTokens:     nonZeroInt(deps.ContextWindowTokens, 200000),
			PromptBudgetTokens:      nonZeroInt(deps.PromptBudgetTokens, 150000),
			CompactionTriggerTokens: nonZeroInt(deps.CompactionTriggerTokens, 120000),
			MaxToolContextChars:     nonZeroInt(deps.MaxToolContextChars, 4096),
		},
		compactor: compaction.New(compaction.Deps{
			Provider:        deps.Provider,
			RequestConfig:   deps.RuntimeDefaults,
			ProviderTimeout: deps.LLMCompactionTimeout,
			Enabled:         deps.LLMCompactionEnabled,
		}),
		meshPolicies:         map[string]mesh.OrchestrationPolicy{},
		runtimeConfigs:       map[string]provider.RequestConfig{},
		runs:                 NewRunStateStore(),
		refreshInterval:      5 * time.Second,
		runtimeDefaults:      deps.RuntimeDefaults,
		traceEnabled:         deps.TraceEnabled,
		traceDir:             strings.TrimSpace(deps.TraceDir),
		providerRoundTimeout: deps.ProviderRoundTimeout,
		runStore:             deps.RunStore,
		memoryPolicy:         runtimex.NormalizeMemoryPolicy(deps.MemoryPolicy),
		actionPolicy:         runtimex.NormalizeActionPolicy(deps.ActionPolicy),
		mcpPolicy:            runtimex.NormalizeMCPPolicy(deps.MCPPolicy),
	}
	if len(adapter.mcpPolicy.AllowedTools) == 0 {
		adapter.mcpPolicy = runtimex.DefaultMCPPolicy()
	}
	adapter.activeRuns = runtimex.NewActiveRegistry()
	adapter.runtimeAPI = runtimex.NewAPI(deps.RunStore, adapter.activeRuns, deps.Approvals)
	adapter.execution = runtimex.NewExecutionService(adapter.runtimeAPI, adapter)
	adapter.sessionActions = runtimex.NewSessionActions(deps.Store)
	return adapter
}

func nonZeroInt(v, fallback int) int {
	if v > 0 {
		return v
	}
	return fallback
}

func (a *Adapter) RuntimeAPI() *runtimex.API {
	return a.runtimeAPI
}

func (a *Adapter) SetDelegationServices(jobs JobControl, workers WorkerControl) {
	a.jobControl = jobs
	a.workerControl = workers
}

func (a *Adapter) SetExecutionService(service *runtimex.ExecutionService) {
	a.execution = service
}

func (a *Adapter) SetAgentCore(core runtimex.AgentCore) {
	a.agentCore = core
}

func (a *Adapter) MemoryPolicy() runtimex.MemoryPolicy {
	return a.memoryPolicy
}

func (a *Adapter) ActionPolicy() runtimex.ActionPolicy {
	return a.actionPolicy
}

func (a *Adapter) RuntimeDefaults() provider.RequestConfig {
	return a.runtimeDefaults
}

func (a *Adapter) SessionActions() *runtimex.SessionActions {
	return a.sessionActions
}

func TestMessageUpdate(text string) MessageUpdate {
	return MessageUpdate{Text: text}
}

func (a *Adapter) Normalize(raw any) (events.InboundEvent, error) {
	switch update := raw.(type) {
	case MessageUpdate:
		if update.Text == "" {
			return events.InboundEvent{}, fmt.Errorf("empty message")
		}
		return events.InboundEvent{
			Source:    "telegram",
			SessionID: "telegram-session",
			Text:      update.Text,
		}, nil
	case Update:
		if update.CallbackQuery {
			if update.CallbackData == "" {
				return events.InboundEvent{}, fmt.Errorf("empty callback data")
			}
			return events.InboundEvent{
				Source:    "telegram",
				SessionID: fmt.Sprintf("telegram:%d", update.ChatID),
				Text:      update.CallbackData,
			}, nil
		}
		if update.Text == "" {
			return events.InboundEvent{}, fmt.Errorf("empty message")
		}
		return events.InboundEvent{
			Source:    "telegram",
			SessionID: fmt.Sprintf("telegram:%d", update.ChatID),
			Text:      update.Text,
		}, nil
	default:
		return events.InboundEvent{}, fmt.Errorf("unsupported update type %T", raw)
	}
}

func (a *Adapter) Poll(ctx context.Context, offset int64) (Update, error) {
	query := url.Values{}
	query.Set("offset", strconv.FormatInt(offset, 10))
	query.Set("timeout", "30")

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, a.methodURL("getUpdates")+"?"+query.Encode(), nil)
	if err != nil {
		return Update{}, err
	}

	resp, err := a.httpClient.Do(req)
	if err != nil {
		return Update{}, err
	}
	defer resp.Body.Close()

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		body, _ := io.ReadAll(resp.Body)
		return Update{}, fmt.Errorf("telegram api error: status=%d body=%s", resp.StatusCode, strings.TrimSpace(string(body)))
	}

	var payload getUpdatesResponse
	if err := json.NewDecoder(resp.Body).Decode(&payload); err != nil {
		return Update{}, err
	}
	if len(payload.Result) == 0 {
		return Update{}, nil
	}

	entry := payload.Result[0]
	if entry.CallbackQuery.ID != "" {
		return Update{
			UpdateID:      entry.UpdateID,
			ChatID:        entry.CallbackQuery.Message.Chat.ID,
			CallbackID:    entry.CallbackQuery.ID,
			CallbackData:  entry.CallbackQuery.Data,
			CallbackQuery: true,
		}, nil
	}
	return Update{
		UpdateID: entry.UpdateID,
		ChatID:   entry.Message.Chat.ID,
		Text:     entry.Message.Text,
	}, nil
}

func (a *Adapter) Dispatch(ctx context.Context, update Update) error {
	if a.runStore != nil && update.UpdateID != 0 {
		ok, err := a.runStore.TryMarkUpdate(update.ChatID, update.UpdateID)
		if err != nil {
			return err
		}
		if !ok {
			slog.Debug("telegram duplicate update ignored", "chat_id", update.ChatID, "update_id", update.UpdateID)
			return nil
		}
	}
	handled, err := a.handleImmediateUpdate(ctx, update)
	if handled || err != nil {
		return err
	}
	if a.execution == nil {
		return fmt.Errorf("runtime execution service is not configured")
	}
	_, ok, err := a.execution.StartDetached(ctx, runtimex.StartRunRequest{
		RunID:          a.runs.AllocateID(),
		ChatID:         update.ChatID,
		SessionID:      a.meshSessionID(update.ChatID),
		Query:          strings.TrimSpace(update.Text),
		PolicySnapshot: runtimex.PolicySnapshotForSummary(a.runtimeSummary(update.ChatID)),
		Interactive:    true,
	})
	if err != nil || !ok {
		if !ok && err == nil {
			err = a.handleBusyRun(ctx, update.ChatID, update.Text)
		}
		return err
	}
	return nil
}

func (a *Adapter) Reply(ctx context.Context, update Update) error {
	handled, err := a.handleImmediateUpdate(ctx, update)
	if handled || err != nil {
		return err
	}
	if a.execution == nil {
		return fmt.Errorf("runtime execution service is not configured")
	}
	_, ok, err := a.execution.StartAndWait(ctx, runtimex.StartRunRequest{
		RunID:          a.runs.AllocateID(),
		ChatID:         update.ChatID,
		SessionID:      a.meshSessionID(update.ChatID),
		Query:          strings.TrimSpace(update.Text),
		PolicySnapshot: runtimex.PolicySnapshotForSummary(a.runtimeSummary(update.ChatID)),
		Interactive:    true,
	})
	if err != nil || !ok {
		if !ok && err == nil {
			err = a.handleBusyRun(ctx, update.ChatID, update.Text)
		}
		return err
	}
	return nil
}
