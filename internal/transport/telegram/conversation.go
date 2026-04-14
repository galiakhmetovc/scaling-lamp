package telegram

import (
	"context"
	"log/slog"
	"slices"
	"strings"
	"time"

	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
	"teamd/internal/workspace"
)

func (a *Adapter) conversationHooks(ctx context.Context, chatID int64) runtimex.ConversationHooks {
	profile := a.debugProfileForContext(ctx)
	var store runtimex.ConversationStore = a.store
	toolsProvider := a.providerTools
	workspaceContext := func(chatID int64) string { return a.workspaceContext }
	sessionHead := a.sessionHeadPrompt
	recentWork := a.recentWorkPrompt
	memoryRecall := a.memoryRecallPrompt
	skillsCatalog := a.skillsCatalogPrompt
	activeSkills := a.activeSkillsPrompt
	if profile != nil {
		store = debugProfileConversationStore{base: a.store, profile: *profile}
		if !profile.Tools {
			toolsProvider = func(role string) ([]provider.ToolDefinition, error) { return nil, nil }
		} else if len(profile.AllowedTools) > 0 {
			allowed := normalizeAllowedToolNames(profile.AllowedTools)
			toolsProvider = func(role string) ([]provider.ToolDefinition, error) {
				items, err := a.providerTools(role)
				if err != nil {
					return nil, err
				}
				out := make([]provider.ToolDefinition, 0, len(items))
				for _, item := range items {
					if slices.Contains(allowed, item.Name) {
						out = append(out, item)
					}
				}
				return out, nil
			}
		}
		if !profile.Workspace {
			workspaceContext = nil
		} else if len(profile.WorkspaceFiles) > 0 {
			workspaceContext = func(chatID int64) string {
				return workspace.BuildSelectedContext(a.workspaceRoot, profile.WorkspaceFiles)
			}
		}
		if !profile.SessionHead {
			sessionHead = nil
		}
		if !profile.RecentWork {
			recentWork = nil
		}
		if !profile.MemoryRecall {
			memoryRecall = nil
		}
		if !profile.Skills {
			skillsCatalog = nil
			activeSkills = nil
		}
	}
	assembler := runtimex.PromptContextAssembler{
		WorkspaceContext: workspaceContext,
		SessionHead:      sessionHead,
		RecentWork:       recentWork,
		MemoryRecall:     memoryRecall,
		SkillsCatalog:    skillsCatalog,
		ActiveSkills:     activeSkills,
	}
	return runtimex.ConversationHooks{
		Provider:             a.provider,
		Store:                store,
		Budget:               a.budget,
		Compactor:            a.compactor,
		ProviderRoundTimeout: a.providerRoundTimeout,
		ProviderTools:        toolsProvider,
		ToolRole:             func(chatID int64) string { return "telegram" },
		RequestConfig:        a.runtimeConfig,
		BuildPromptContext:   assembler.Build,
		InjectPromptContext:  assembler.Inject,
		ExecuteTool:          a.executeTool,
		ShapeToolResult:      a.shapeToolResult,
		LastUserMessage:      lastUserMessage,
		ShouldStopAdvisory:   shouldStopForAdvisoryDraft,
		ToolCallSignature:    toolCallSignature,
		ShouldBreakLoop:      shouldBreakRepeatedToolLoop,
		SyntheticLoopOutput:  syntheticLoopBreakerToolOutput,
		OnRoundPrepared: func(chatID int64, assembled []provider.Message, metrics runtimex.PromptBudgetMetrics) {
			a.runs.Update(chatID, func(run *RunState) {
				run.Stage = "Думаю над ответом"
				run.WaitingOn = "model"
				run.RoundIndex++
				run.LastProgressAt = time.Now().UTC()
				run.ContextEstimate = metrics.FinalPromptTokens
				run.ContextPercent = metrics.ContextWindowPercent
				run.ContextPercentDelta = run.ContextPercent
				run.PromptBudgetPercent = metrics.PromptBudgetPercent
				run.PromptBudgetPercentDelta = run.PromptBudgetPercent
				run.SystemOverheadTokens = metrics.SystemOverheadTokens
			})
		},
		OnProviderTimeout: func(chatID int64) {
			a.runs.Update(chatID, func(run *RunState) {
				run.Stage = "Жду решения по timeout"
				run.WaitingOn = "provider-timeout"
				run.LastProgressAt = time.Now().UTC()
			})
			slog.Warn("provider_round_timeout", "chat_id", chatID, "timeout", a.providerRoundTimeout.String())
			_ = a.syncStatusCard(ctx, chatID)
		},
		OnFinalResponse: func(chatID int64, resp provider.PromptResponse) {
			a.runs.Update(chatID, func(run *RunState) {
				run.PromptTokens += resp.Usage.PromptTokens
				run.CompletionTokens += resp.Usage.CompletionTokens
				run.Stage = "Формирую финальный ответ"
				run.WaitingOn = ""
				run.LastProgressAt = time.Now().UTC()
			})
		},
		OnAdvisoryStop: func(chatID int64, resp provider.PromptResponse) {
			a.runs.Update(chatID, func(run *RunState) {
				run.PromptTokens += resp.Usage.PromptTokens
				run.CompletionTokens += resp.Usage.CompletionTokens
				run.Stage = "Завершаю advisory-ответ"
				run.WaitingOn = ""
				run.LastProgressAt = time.Now().UTC()
				run.Trace = append(run.Trace, TraceEntry{
					Section: "Runtime Guard",
					Summary: "advisory_stop_applied",
					Payload: strings.TrimSpace(resp.Text),
				})
			})
			slog.Info("advisory_stop_applied", "chat_id", chatID)
		},
		OnToolStart: func(chatID int64, call provider.ToolCall) {
			a.runs.Update(chatID, func(run *RunState) {
				run.Stage = "Выполняю инструмент"
				run.WaitingOn = "tool"
				run.CurrentTool = runtimeToolName(call.Name)
				run.LastProgressAt = time.Now().UTC()
			})
		},
		OnToolLoopBreak: func(chatID int64, call provider.ToolCall, repeatedCount int, content string) error {
			a.runs.Update(chatID, func(run *RunState) {
				run.Trace = append(run.Trace, TraceEntry{
					Section: "Runtime Guard",
					Summary: "guard_triggered",
					Payload: content,
				})
				run.Steps = append(run.Steps, RunStep{
					Title:   runtimeToolName(call.Name),
					Detail:  "loop breaker triggered",
					Icon:    "⛔",
					Elapsed: 0,
				})
				run.WaitingOn = "model"
				run.LastProgressAt = time.Now().UTC()
			})
			slog.Warn("guard_triggered",
				"chat_id", chatID,
				"tool", runtimeToolName(call.Name),
				"reason", "repeated_identical_tool_call",
				"repeat_count", repeatedCount,
			)
			return a.syncStatusCard(ctx, chatID)
		},
		OnToolResult: func(chatID int64, call provider.ToolCall, result runtimex.OffloadedToolResult, elapsed time.Duration) error {
			content := result.Content
			a.runs.Update(chatID, func(run *RunState) {
				run.ToolCalls++
				run.ToolCallsDelta++
				run.ToolOutputChars += len(content)
				run.ToolOutputCharsDelta += len(content)
				run.ToolDuration += elapsed
				run.ToolDurationDelta += elapsed
				run.CurrentTool = runtimeToolName(call.Name)
				run.WaitingOn = "model"
				run.LastProgressAt = time.Now().UTC()
				run.Steps = append(run.Steps, RunStep{
					Title:   runtimeToolName(call.Name),
					Detail:  summarizeToolCall(call),
					Icon:    toolIcon(runtimeToolName(call.Name)),
					Elapsed: elapsed,
				})
			})
			return a.syncStatusCard(ctx, chatID)
		},
		OnCheckpointSaved: a.persistCheckpoint,
	}
}

func normalizeAllowedToolNames(items []string) []string {
	out := make([]string, 0, len(items))
	seen := map[string]struct{}{}
	for _, item := range items {
		trimmed := strings.TrimSpace(item)
		if trimmed == "" {
			continue
		}
		candidates := []string{trimmed, providerToolName(trimmed)}
		for _, candidate := range candidates {
			if _, ok := seen[candidate]; ok {
				continue
			}
			seen[candidate] = struct{}{}
			out = append(out, candidate)
		}
	}
	return out
}

func (a *Adapter) runConversation(ctx context.Context, chatID int64) (provider.PromptResponse, error) {
	return runtimex.ExecuteConversation(ctx, chatID, a.conversationHooks(ctx, chatID))
}
