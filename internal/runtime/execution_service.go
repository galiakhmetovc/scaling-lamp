package runtime

import (
	"context"
	"errors"
	"strings"
	"time"

	"teamd/internal/provider"
)

type StartRunRequest struct {
	RunID          string
	ChatID         int64
	SessionID      string
	Query          string
	PolicySnapshot PolicySnapshot
	DebugProfile   *DebugExecutionProfile
	Interactive    bool
}

type ExecutionHooks interface {
	PrepareStart(context.Context, StartRunRequest, PreparedRun) error
	PrepareApprovalResume(context.Context, ApprovalContinuation, PreparedRun) error
	PrepareRunContext(context.Context, string) context.Context
	ConversationHooks(context.Context, int64) ConversationHooks
	HandleRunSuccess(context.Context, int64, string, bool, provider.PromptResponse) error
	HandleRunError(context.Context, int64, bool, error) error
	HandleRunCancelled(context.Context, int64, bool) error
	ExecuteApprovedTool(context.Context, int64, []string, provider.ToolCall) (string, error)
	FinishApprovalResume(string) error
}

type ExecutionService struct {
	api                 *API
	hooks               ExecutionHooks
	timeoutDecisionWait time.Duration
}

func NewExecutionService(api *API, hooks ExecutionHooks) *ExecutionService {
	return &ExecutionService{api: api, hooks: hooks, timeoutDecisionWait: 5 * time.Minute}
}

func (s *ExecutionService) Start(ctx context.Context, req StartRunRequest) (RunView, bool, <-chan error, error) {
	if s == nil || s.api == nil {
		return RunView{}, false, nil, nil
	}
	prepared, ok, err := s.api.PrepareRun(ctx, req.RunID, req.ChatID, req.SessionID, req.Query, NormalizePolicySnapshot(req.PolicySnapshot))
	if err != nil || !ok {
		return RunView{}, ok, nil, err
	}
	if s.hooks != nil {
		if err := s.hooks.PrepareStart(ctx, req, prepared); err != nil {
			return RunView{}, false, nil, s.api.FailRunStart(prepared, err)
		}
	}
	errCh := make(chan error, 1)
	s.api.LaunchRun(prepared, func(runCtx context.Context, runID string) error {
		if s.hooks != nil {
			runCtx = s.hooks.PrepareRunContext(runCtx, runID)
		}
		err := s.executeRun(runCtx, req.ChatID, req.SessionID, runID, req.Query, req.Interactive)
		errCh <- err
		return err
	})
	view, _, err := s.api.RunView(prepared.Run.RunID)
	return view, true, errCh, err
}

func (s *ExecutionService) StartDetached(ctx context.Context, req StartRunRequest) (RunView, bool, error) {
	view, ok, _, err := s.Start(context.WithoutCancel(ctx), req)
	return view, ok, err
}

func (s *ExecutionService) StartAndWait(ctx context.Context, req StartRunRequest) (RunView, bool, error) {
	view, ok, errCh, err := s.Start(ctx, req)
	if err != nil || !ok {
		return view, ok, err
	}
	return view, ok, <-errCh
}

func (s *ExecutionService) ResumeApprovalContinuation(ctx context.Context, approvalID string) (bool, error) {
	if s == nil || s.api == nil || s.api.store == nil {
		return false, nil
	}
	store, ok := s.api.store.(ApprovalStateStore)
	if !ok {
		return false, nil
	}
	cont, ok, err := store.ApprovalContinuation(approvalID)
	if err != nil || !ok {
		return false, err
	}
	prepared, ok, err := s.api.PrepareRun(context.WithoutCancel(ctx), cont.RunID, cont.ChatID, cont.SessionID, cont.Query, PolicySnapshot{})
	if err != nil || !ok {
		return false, err
	}
	if s.hooks != nil {
		if err := s.hooks.PrepareApprovalResume(ctx, cont, prepared); err != nil {
			return false, s.api.FailRunStart(prepared, err)
		}
	}
	s.api.LaunchRun(prepared, func(runCtx context.Context, runID string) error {
		if s.hooks != nil {
			runCtx = s.hooks.PrepareRunContext(runCtx, runID)
		}
		return s.executeApprovalResume(runCtx, cont, runID)
	})
	return true, nil
}

func (s *ExecutionService) executeRun(ctx context.Context, chatID int64, sessionID, runID, query string, interactive bool) error {
	if s.hooks == nil {
		return nil
	}
	for {
		resp, err := ExecuteConversation(ctx, chatID, s.runtimeConversationHooks(ctx, chatID, sessionID, runID))
		if err != nil {
			if errors.Is(err, context.Canceled) {
				return s.hooks.HandleRunCancelled(ctx, chatID, interactive)
			}
			var timeoutErr ProviderRoundTimeoutError
			if errors.As(err, &timeoutErr) {
				action, resolveErr := s.awaitProviderTimeoutDecision(ctx, chatID, sessionID, runID)
				if resolveErr != nil {
					return s.hooks.HandleRunError(ctx, chatID, interactive, resolveErr)
				}
				switch action {
				case TimeoutDecisionActionContinue, TimeoutDecisionActionRetry:
					continue
				case TimeoutDecisionActionCancel:
					return s.hooks.HandleRunCancelled(ctx, chatID, interactive)
				case TimeoutDecisionActionFail:
					return s.hooks.HandleRunError(ctx, chatID, interactive, err)
				default:
					return s.hooks.HandleRunError(ctx, chatID, interactive, err)
				}
			}
			_ = query
			return s.hooks.HandleRunError(ctx, chatID, interactive, err)
		}
		s.saveAssistantFinalEvent(runID, chatID, sessionID, resp)
		s.saveSessionHead(chatID, sessionID, runID, query, resp)
		return s.hooks.HandleRunSuccess(ctx, chatID, runID, interactive, resp)
	}
}

func (s *ExecutionService) executeApprovalResume(ctx context.Context, cont ApprovalContinuation, runID string) error {
	if s.hooks == nil {
		return nil
	}
	call := provider.ToolCall{
		ID:        cont.ToolCallID,
		Name:      cont.ToolName,
		Arguments: cloneMap(cont.ToolArguments),
	}
	content, err := s.hooks.ExecuteApprovedTool(ctx, cont.ChatID, nil, call)
	if err != nil {
		return err
	}
	hooks := s.runtimeConversationHooks(ctx, cont.ChatID, cont.SessionID, runID)
	if err := hooks.Store.Append(cont.ChatID, provider.Message{
		Role:       "tool",
		Name:       call.Name,
		ToolCallID: call.ID,
		Content:    content,
	}); err != nil {
		return err
	}
	for {
		resp, err := ExecuteConversation(ctx, cont.ChatID, hooks)
		if err != nil {
			if errors.Is(err, context.Canceled) {
				cancelErr := s.hooks.HandleRunCancelled(ctx, cont.ChatID, true)
				_ = s.hooks.FinishApprovalResume(cont.ApprovalID)
				return cancelErr
			}
			var timeoutErr ProviderRoundTimeoutError
			if errors.As(err, &timeoutErr) {
				action, resolveErr := s.awaitProviderTimeoutDecision(ctx, cont.ChatID, cont.SessionID, runID)
				if resolveErr != nil {
					runErr := s.hooks.HandleRunError(ctx, cont.ChatID, true, resolveErr)
					_ = s.hooks.FinishApprovalResume(cont.ApprovalID)
					return runErr
				}
				switch action {
				case TimeoutDecisionActionContinue, TimeoutDecisionActionRetry:
					continue
				case TimeoutDecisionActionCancel:
					cancelErr := s.hooks.HandleRunCancelled(ctx, cont.ChatID, true)
					_ = s.hooks.FinishApprovalResume(cont.ApprovalID)
					return cancelErr
				case TimeoutDecisionActionFail:
					runErr := s.hooks.HandleRunError(ctx, cont.ChatID, true, err)
					_ = s.hooks.FinishApprovalResume(cont.ApprovalID)
					return runErr
				}
			}
			runErr := s.hooks.HandleRunError(ctx, cont.ChatID, true, err)
			_ = s.hooks.FinishApprovalResume(cont.ApprovalID)
			return runErr
		}
		s.saveAssistantFinalEvent(runID, cont.ChatID, cont.SessionID, resp)
		s.saveSessionHead(cont.ChatID, cont.SessionID, runID, cont.Query, resp)
		successErr := s.hooks.HandleRunSuccess(ctx, cont.ChatID, runID, true, resp)
		_ = s.hooks.FinishApprovalResume(cont.ApprovalID)
		return successErr
	}
}

func (s *ExecutionService) runtimeConversationHooks(ctx context.Context, chatID int64, sessionID, runID string) ConversationHooks {
	hooks := s.hooks.ConversationHooks(ctx, chatID)
	basePrepared := hooks.OnRoundPrepared
	hooks.OnRoundPrepared = func(chatID int64, assembled []provider.Message, metrics PromptBudgetMetrics) {
		s.savePromptBudget(runID, metrics)
		if s.api != nil && s.api.store != nil {
			_ = s.api.store.SaveEvent(runEvent(runID, chatID, sessionID, "prompt.assembled", map[string]any{
				"final_prompt_tokens":       metrics.FinalPromptTokens,
				"prompt_budget_tokens":      metrics.PromptBudgetTokens,
				"context_window_tokens":     metrics.ContextWindowTokens,
				"system_overhead_tokens":    metrics.SystemOverheadTokens,
				"prompt_budget_percent":     metrics.PromptBudgetPercent,
				"context_window_percent":    metrics.ContextWindowPercent,
				"raw_transcript_tokens":     metrics.RawTranscriptTokens,
				"compaction_trigger_tokens": metrics.CompactionTriggerTokens,
				"message_count":             len(assembled),
			}))
		}
		if basePrepared != nil {
			basePrepared(chatID, assembled, metrics)
		}
	}
	base := hooks.OnToolResult
	hooks.OnToolResult = func(chatID int64, call provider.ToolCall, result OffloadedToolResult, elapsed time.Duration) error {
		if result.ArtifactRef != "" && s.api != nil && s.api.store != nil {
			_ = s.api.store.SaveEvent(runEvent(runID, chatID, sessionID, "artifact.offloaded", map[string]any{
				"tool_name":    call.Name,
				"tool_call_id": call.ID,
				"artifact_ref": result.ArtifactRef,
			}))
		}
		if base != nil {
			return base(chatID, call, result, elapsed)
		}
		return nil
	}
	baseTranscriptAppend := hooks.OnTranscriptAppend
	hooks.OnTranscriptAppend = func(chatID int64, msg provider.Message) {
		if s.api != nil && s.api.store != nil {
			_ = s.api.store.SaveEvent(runEvent(runID, chatID, sessionID, "transcript.appended", map[string]any{
				"role":         msg.Role,
				"name":         msg.Name,
				"tool_call_id": msg.ToolCallID,
				"content":      msg.Content,
				"preview":      summarizeResult(msg.Content),
			}))
		}
		if baseTranscriptAppend != nil {
			baseTranscriptAppend(chatID, msg)
		}
	}
	return hooks
}

func (s *ExecutionService) savePromptBudget(runID string, metrics PromptBudgetMetrics) {
	if s == nil || s.api == nil || s.api.store == nil || strings.TrimSpace(runID) == "" {
		return
	}
	run, ok, err := s.api.store.Run(runID)
	if err != nil || !ok {
		return
	}
	run.PromptBudget = metrics
	_ = s.api.store.SaveRun(run)
}

func (s *ExecutionService) saveAssistantFinalEvent(runID string, chatID int64, sessionID string, resp provider.PromptResponse) {
	if s == nil || s.api == nil || s.api.store == nil {
		return
	}
	if existing, ok, err := s.api.store.Run(runID); err == nil && ok {
		existing.FinalResponse = resp.Text
		_ = s.api.store.SaveRun(existing)
	}
	_ = s.api.store.SaveEvent(runEvent(runID, chatID, sessionID, "assistant.final", map[string]any{
		"text":          resp.Text,
		"model":         resp.Model,
		"finish_reason": resp.FinishReason,
	}))
}

func (s *ExecutionService) saveSessionHead(chatID int64, sessionID, runID, query string, resp provider.PromptResponse) {
	if s == nil || s.api == nil || s.api.store == nil {
		return
	}
	store, ok := s.api.store.(SessionStateStore)
	if !ok {
		return
	}
	existing, _, _ := store.SessionHead(chatID, sessionID)
	artifactRefs, _ := s.api.artifactRefs("run", runID)
	head := SessionHead{
		ChatID:             chatID,
		SessionID:          sessionID,
		LastCompletedRunID: runID,
		CurrentGoal:        strings.TrimSpace(query),
		LastResultSummary:  summarizeResult(resp.Text),
		CurrentPlanID:      existing.CurrentPlanID,
		CurrentPlanTitle:   existing.CurrentPlanTitle,
		CurrentPlanItems:   append([]string(nil), existing.CurrentPlanItems...),
		ResolvedEntities:   append([]string(nil), existing.ResolvedEntities...),
		RecentArtifactRefs: artifactRefs,
		OpenLoops:          append([]string(nil), existing.OpenLoops...),
		CurrentProject:     existing.CurrentProject,
		UpdatedAt:          time.Now().UTC(),
	}
	_ = store.SaveSessionHead(head)
	_ = s.api.store.SaveEvent(runEvent(runID, chatID, sessionID, "session_head.updated", map[string]any{
		"last_completed_run_id": head.LastCompletedRunID,
		"current_goal":          head.CurrentGoal,
		"last_result_summary":   head.LastResultSummary,
		"current_project":       head.CurrentProject,
		"artifact_refs":         head.RecentArtifactRefs,
		"open_loops":            head.OpenLoops,
	}))
}

func summarizeResult(text string) string {
	text = strings.TrimSpace(text)
	if text == "" {
		return ""
	}
	const max = 280
	if len(text) <= max {
		return text
	}
	cut := strings.TrimSpace(text[:max])
	return cut + "..."
}

func cloneMap(src map[string]any) map[string]any {
	if len(src) == 0 {
		return map[string]any{}
	}
	dst := make(map[string]any, len(src))
	for k, v := range src {
		dst[k] = v
	}
	return dst
}

func (s *ExecutionService) awaitProviderTimeoutDecision(ctx context.Context, chatID int64, sessionID, runID string) (TimeoutDecisionAction, error) {
	if s == nil || s.api == nil || s.api.store == nil {
		return TimeoutDecisionActionFail, ProviderRoundTimeoutError{TimeoutText: "runtime unavailable"}
	}
	autoUsed := false
	if existing, ok, err := s.api.TimeoutDecision(runID); err == nil && ok {
		autoUsed = existing.AutoContinueUsed
	}
	now := time.Now().UTC()
	autoDeadline := now.Add(s.timeoutDecisionWait)
	if _, err := s.api.CreateTimeoutDecision(runID, chatID, sessionID, 0, autoUsed, autoDeadline); err != nil {
		return TimeoutDecisionActionFail, err
	}
	if run, ok, err := s.api.store.Run(runID); err == nil && ok {
		run.Status = StatusWaitingOperator
		run.FailureReason = ""
		run.EndedAt = nil
		_ = s.api.store.SaveRun(run)
	}
	_ = s.api.store.SaveEvent(runEvent(runID, chatID, sessionID, "run.provider_timeout", map[string]any{
		"auto_continue_deadline": autoDeadline,
		"auto_continue_used":     autoUsed,
	}))
	waitCtx, cancel := context.WithTimeout(ctx, s.timeoutDecisionWait)
	defer cancel()
	decisionCh := make(chan TimeoutDecisionRecord, 1)
	timer := time.NewTimer(s.timeoutDecisionWait)
	defer timer.Stop()
	go func() {
		record, err := s.api.WaitTimeoutDecision(waitCtx, runID)
		if err != nil {
			return
		}
		decisionCh <- record
	}()
	select {
	case <-ctx.Done():
		return TimeoutDecisionActionFail, ctx.Err()
	case <-timer.C:
		action := TimeoutDecisionActionContinue
		reason := ""
		if autoUsed {
			action = TimeoutDecisionActionFail
			reason = "provider timeout repeated after automatic continue"
		}
		record, _, err := s.api.ResolveTimeoutDecision(runID, action, reason)
		if err != nil {
			return TimeoutDecisionActionFail, err
		}
		cancel()
		return timeoutActionFromRecord(record), nil
	case record := <-decisionCh:
		cancel()
		return timeoutActionFromRecord(record), nil
	}
}

func timeoutActionFromRecord(record TimeoutDecisionRecord) TimeoutDecisionAction {
	switch record.Status {
	case TimeoutDecisionContinued:
		return TimeoutDecisionActionContinue
	case TimeoutDecisionRetried:
		return TimeoutDecisionActionRetry
	case TimeoutDecisionCancelled:
		return TimeoutDecisionActionCancel
	case TimeoutDecisionFailed, TimeoutDecisionExpired:
		return TimeoutDecisionActionFail
	default:
		return TimeoutDecisionActionFail
	}
}
