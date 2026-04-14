package telegram

import (
	"context"
	"fmt"
	"log/slog"
	"strings"

	"teamd/internal/llmtrace"
	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
)

func (a *Adapter) PrepareStart(ctx context.Context, req runtimex.StartRunRequest, prepared runtimex.PreparedRun) error {
	a.runs.CreateWithID(req.ChatID, prepared.Run.RunID, strings.TrimSpace(req.Query), prepared.Run.StartedAt)
	a.rememberDebugProfile(prepared.Run.RunID, req.DebugProfile)
	if err := a.store.Append(req.ChatID, provider.Message{Role: "user", Content: req.Query}); err != nil {
		a.runs.Finish(req.ChatID)
		a.forgetDebugProfile(prepared.Run.RunID)
		return err
	}
	if req.Interactive {
		ackID, err := a.sendMessage(ctx, req.ChatID, formatStatusCard(mustRun(a.runs.Active(req.ChatID))), runKeyboard(mustRun(a.runs.Active(req.ChatID))))
		if err != nil {
			a.runs.Finish(req.ChatID)
			a.forgetDebugProfile(prepared.Run.RunID)
			return err
		}
		a.runs.Update(req.ChatID, func(run *RunState) {
			run.AckMessageID = ackID
			run.StatusMessageID = ackID
			run.Stage = "Готовлю контекст"
		})
	}
	slog.Info("run_created", "chat_id", req.ChatID, "run_id", prepared.Run.RunID, "query", strings.TrimSpace(req.Query))
	if a.traceEnabled && a.traceDir != "" {
		a.traceCollectors.Store(prepared.Run.RunID, llmtrace.NewCollector(llmtrace.RunMeta{
			RunID:  prepared.Run.RunID,
			ChatID: req.ChatID,
			Query:  strings.TrimSpace(req.Query),
		}))
	}
	return nil
}

func (a *Adapter) ConversationHooks(ctx context.Context, chatID int64) runtimex.ConversationHooks {
	return a.conversationHooks(ctx, chatID)
}

func (a *Adapter) PrepareRunContext(ctx context.Context, runID string) context.Context {
	ctx = withDebugRunID(ctx, runID)
	collector := a.takeTraceCollector(runID)
	if collector == nil {
		return ctx
	}
	return llmtrace.WithCollector(ctx, collector)
}

func (a *Adapter) HandleRunSuccess(ctx context.Context, chatID int64, runID string, interactive bool, resp provider.PromptResponse) error {
	collector := llmtrace.FromContext(ctx)
	defer a.persistTrace(chatID, collector)
	defer a.forgetDebugProfile(runID)
	a.runs.Update(chatID, func(run *RunState) {
		run.Completed = true
		run.Stage = "Ответ отправлен"
	})
	var err error
	if interactive {
		transportCtx, cancel := detachedContext(ctx)
		defer cancel()
		_ = a.syncStatusCard(transportCtx, chatID)
		_, err = a.sendMessageWithMode(transportCtx, chatID, a.formatReply(resp, chatID), nil, "HTML")
	}
	if err == nil {
		a.persistContinuity(chatID, resp)
		run := mustRun(a.runs.Active(chatID))
		slog.Info("run_completed", "chat_id", chatID, "run_id", run.ID, "model", resp.Model, "tool_calls", run.ToolCalls)
	}
	a.maybeStartQueuedMessage(chatID)
	return err
}

func (a *Adapter) PrepareApprovalResume(ctx context.Context, cont runtimex.ApprovalContinuation, prepared runtimex.PreparedRun) error {
	a.runs.CreateWithID(cont.ChatID, cont.RunID, cont.Query, prepared.Run.StartedAt)
	if a.traceEnabled && a.traceDir != "" {
		a.traceCollectors.Store(prepared.Run.RunID, llmtrace.NewCollector(llmtrace.RunMeta{
			RunID:  prepared.Run.RunID,
			ChatID: cont.ChatID,
			Query:  strings.TrimSpace(cont.Query),
		}))
	}
	return nil
}

func (a *Adapter) HandleRunError(ctx context.Context, chatID int64, interactive bool, err error) error {
	collector := llmtrace.FromContext(ctx)
	defer a.persistTrace(chatID, collector)
	defer a.forgetDebugProfile(debugRunIDFromContext(ctx))
	a.runs.Update(chatID, func(run *RunState) {
		run.Failed = true
		run.FailureText = err.Error()
		run.Stage = "Ошибка выполнения"
	})
	if interactive {
		transportCtx, cancel := detachedContext(ctx)
		defer cancel()
		_ = a.syncStatusCard(transportCtx, chatID)
	}
	slog.Error("run_failed", "chat_id", chatID, "error", err.Error())
	a.maybeStartQueuedMessage(chatID)
	return err
}

func (a *Adapter) HandleRunCancelled(ctx context.Context, chatID int64, interactive bool) error {
	collector := llmtrace.FromContext(ctx)
	defer a.persistTrace(chatID, collector)
	defer a.forgetDebugProfile(debugRunIDFromContext(ctx))
	a.runs.Update(chatID, func(run *RunState) {
		run.Completed = true
		run.Stage = "Отменено"
	})
	if interactive {
		transportCtx, cancel := detachedContext(ctx)
		defer cancel()
		_ = a.syncStatusCard(transportCtx, chatID)
		_, _ = a.sendMessage(transportCtx, chatID, "Выполнение отменено", nil)
	}
	slog.Info("run_aborted", "chat_id", chatID, "reason", "cancelled")
	a.maybeStartQueuedMessage(chatID)
	return nil
}

func (a *Adapter) ExecuteApprovedTool(ctx context.Context, chatID int64, allowedTools []string, call provider.ToolCall) (string, error) {
	ctx = withChatID(ctx, chatID)
	if len(allowedTools) > 0 {
		ctx = withRawAllowedTools(ctx, allowedTools)
	}
	return a.executeApprovedTool(ctx, runtimeToolName(call.Name), call)
}

func (a *Adapter) FinishApprovalResume(approvalID string) error {
	a.deleteApprovalContinuation(approvalID)
	return nil
}

func (a *Adapter) requestRunCancel(chatID int64) bool {
	run, ok := a.runs.Active(chatID)
	if !ok || run.Completed || run.Failed {
		return false
	}
	a.runs.Update(chatID, func(run *RunState) {
		run.CancelRequested = true
		run.Stage = "Отменяю выполнение"
	})
	return a.runtimeAPI.CancelRun(chatID)
}

func (a *Adapter) takeTraceCollector(runID string) *llmtrace.Collector {
	if strings.TrimSpace(runID) == "" {
		return nil
	}
	value, ok := a.traceCollectors.Load(runID)
	if !ok {
		return nil
	}
	a.traceCollectors.Delete(runID)
	collector, _ := value.(*llmtrace.Collector)
	return collector
}

func (a *Adapter) persistTrace(chatID int64, collector *llmtrace.Collector) {
	if collector == nil || strings.TrimSpace(a.traceDir) == "" {
		return
	}
	path, err := collector.WriteFile(a.traceDir)
	if err != nil {
		a.runs.Update(chatID, func(run *RunState) {
			run.Trace = append(run.Trace, TraceEntry{
				Section: "LLM Trace",
				Summary: "Не удалось сохранить trace",
				Payload: err.Error(),
			})
		})
		return
	}
	a.runs.Update(chatID, func(run *RunState) {
		run.Trace = append(run.Trace, TraceEntry{
			Section: "LLM Trace",
			Summary: "Trace сохранён на диск",
			Payload: path,
		})
	})
}

func (a *Adapter) runMeshConversation(ctx context.Context, chatID int64, prompt string) (provider.PromptResponse, error) {
	activeSession, err := a.store.ActiveSession(chatID)
	if err != nil {
		return provider.PromptResponse{}, err
	}
	a.runs.Update(chatID, func(run *RunState) {
		run.Stage = "Mesh: выбираю агента"
	})
	reply, err := a.mesh.HandleOwnerTask(ctx, fmt.Sprintf("telegram:%d/%s", chatID, activeSession), prompt, a.meshPolicy(chatID))
	if err != nil {
		return provider.PromptResponse{}, err
	}
	a.runs.Update(chatID, func(run *RunState) {
		run.Trace = convertMeshTrace(reply.Trace)
	})
	if err := a.store.Append(chatID, provider.Message{Role: "assistant", Content: reply.Text}); err != nil {
		return provider.PromptResponse{}, err
	}
	a.runs.Update(chatID, func(run *RunState) {
		run.Stage = "Mesh: оцениваю ответы"
	})
	return provider.PromptResponse{Text: reply.Text, Model: reply.AgentID}, nil
}
