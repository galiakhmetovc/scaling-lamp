package runtime

import (
	"context"
	"fmt"
	"time"

	"teamd/internal/compaction"
	"teamd/internal/provider"
	"teamd/internal/worker"
)

type ConversationStore interface {
	Append(chatID int64, msg provider.Message) error
	Messages(chatID int64) ([]provider.Message, error)
	Checkpoint(chatID int64) (worker.Checkpoint, bool, error)
	SaveCheckpoint(chatID int64, checkpoint worker.Checkpoint) error
	ActiveSession(chatID int64) (string, error)
}

type ConversationHooks struct {
	Provider            provider.Provider
	Store               ConversationStore
	Budget              compaction.Budget
	Compactor           *compaction.Service
	ProviderRoundTimeout time.Duration

	ProviderTools       func(role string) ([]provider.ToolDefinition, error)
	ToolRole            func(chatID int64) string
	RequestConfig       func(chatID int64) provider.RequestConfig
	InjectPromptContext func(chatID int64, messages []provider.Message) ([]provider.Message, error)
	BuildPromptContext  func(chatID int64, messages []provider.Message) (PromptContextBuild, error)
	ExecuteTool         func(ctx context.Context, chatID int64, call provider.ToolCall) (string, error)
	ShapeToolResult     func(chatID int64, call provider.ToolCall, content string) (OffloadedToolResult, error)
	LastUserMessage     func(messages []provider.Message) string
	ShouldStopAdvisory  func(userPrompt string, resp provider.PromptResponse) bool
	ToolCallSignature   func(call provider.ToolCall) string
	ShouldBreakLoop     func(call provider.ToolCall, repeatedCount int) bool
	SyntheticLoopOutput func(call provider.ToolCall, repeatedCount int) string

	OnRoundPrepared    func(chatID int64, assembled []provider.Message, metrics PromptBudgetMetrics)
	OnProviderTimeout  func(chatID int64)
	OnFinalResponse    func(chatID int64, resp provider.PromptResponse)
	OnAdvisoryStop     func(chatID int64, resp provider.PromptResponse)
	OnToolStart        func(chatID int64, call provider.ToolCall)
	OnToolLoopBreak    func(chatID int64, call provider.ToolCall, repeatedCount int, content string) error
	OnToolResult       func(chatID int64, call provider.ToolCall, result OffloadedToolResult, elapsed time.Duration) error
	OnCheckpointSaved  func(chatID int64, checkpoint worker.Checkpoint, originatingIntent string)
	OnTranscriptAppend func(chatID int64, msg provider.Message)
}

func ExecuteConversation(ctx context.Context, chatID int64, hooks ConversationHooks) (provider.PromptResponse, error) {
	role := "default"
	if hooks.ToolRole != nil {
		role = hooks.ToolRole(chatID)
	}
	tools, err := hooks.ProviderTools(role)
	if err != nil {
		return provider.PromptResponse{}, err
	}
	toolCallSignature := hooks.ToolCallSignature
	if toolCallSignature == nil {
		toolCallSignature = func(call provider.ToolCall) string { return call.Name }
	}
	syntheticLoopOutput := hooks.SyntheticLoopOutput
	if syntheticLoopOutput == nil {
		syntheticLoopOutput = func(call provider.ToolCall, repeatedCount int) string { return "" }
	}

	var final provider.PromptResponse
	var (
		lastToolSignature string
		repeatedToolCalls int
	)
	for {
		if err := ctx.Err(); err != nil {
			return provider.PromptResponse{}, err
		}
		assembled, lastUserPrompt, metrics, err := prepareConversationRound(ctx, chatID, hooks)
		if err != nil {
			return provider.PromptResponse{}, err
		}
		if hooks.OnRoundPrepared != nil {
			hooks.OnRoundPrepared(chatID, assembled, metrics)
		}

		roundCtx, cancel := providerRoundContext(ctx, hooks.ProviderRoundTimeout)
		resp, err := hooks.Provider.Generate(roundCtx, provider.PromptRequest{
			WorkerID: fmt.Sprintf("telegram:%d", chatID),
			Messages: assembled,
			Tools:    tools,
			Config:   hooks.RequestConfig(chatID),
		})
		cancel()
		if err != nil {
			if errorsIsProviderRoundTimeout(err, ctx) {
				if hooks.OnProviderTimeout != nil {
					hooks.OnProviderTimeout(chatID)
				}
				return provider.PromptResponse{}, ProviderRoundTimeoutError{TimeoutText: hooks.ProviderRoundTimeout.String()}
			}
			return provider.PromptResponse{}, err
		}

		if resp.FinishReason != "tool_calls" || len(resp.ToolCalls) == 0 {
			if hooks.OnFinalResponse != nil {
				hooks.OnFinalResponse(chatID, resp)
			}
			final = resp
			break
		}
		if hooks.ShouldStopAdvisory != nil && hooks.ShouldStopAdvisory(lastUserPrompt, resp) {
			if hooks.OnAdvisoryStop != nil {
				hooks.OnAdvisoryStop(chatID, resp)
			}
			final = resp
			break
		}

		if err := hooks.Store.Append(chatID, provider.Message{
			Role:      "assistant",
			Content:   resp.Text,
			ToolCalls: resp.ToolCalls,
		}); err != nil {
			return provider.PromptResponse{}, err
		}
		if hooks.OnTranscriptAppend != nil {
			hooks.OnTranscriptAppend(chatID, provider.Message{
				Role:      "assistant",
				Content:   resp.Text,
				ToolCalls: resp.ToolCalls,
			})
		}

		for _, call := range resp.ToolCalls {
			signature := toolCallSignature(call)
			if signature == lastToolSignature {
				repeatedToolCalls++
			} else {
				lastToolSignature = signature
				repeatedToolCalls = 1
			}
			if hooks.OnToolStart != nil {
				hooks.OnToolStart(chatID, call)
			}
			if hooks.ShouldBreakLoop != nil && hooks.ShouldBreakLoop(call, repeatedToolCalls) {
				content := syntheticLoopOutput(call, repeatedToolCalls)
				if err := hooks.Store.Append(chatID, provider.Message{
					Role:       "tool",
					Name:       call.Name,
					ToolCallID: call.ID,
					Content:    content,
				}); err != nil {
					return provider.PromptResponse{}, err
				}
				if hooks.OnTranscriptAppend != nil {
					hooks.OnTranscriptAppend(chatID, provider.Message{
						Role:       "tool",
						Name:       call.Name,
						ToolCallID: call.ID,
						Content:    content,
					})
				}
				if hooks.OnToolLoopBreak != nil {
					if err := hooks.OnToolLoopBreak(chatID, call, repeatedToolCalls, content); err != nil {
						return provider.PromptResponse{}, err
					}
				}
				continue
			}
			started := time.Now()
			content, err := hooks.ExecuteTool(ctx, chatID, call)
			if err != nil {
				return provider.PromptResponse{}, err
			}
			shaped := OffloadedToolResult{Content: content}
			if hooks.ShapeToolResult != nil {
				shaped, err = hooks.ShapeToolResult(chatID, call, content)
				if err != nil {
					return provider.PromptResponse{}, err
				}
			}
			content = shaped.Content
			if err := hooks.Store.Append(chatID, provider.Message{
				Role:       "tool",
				Name:       call.Name,
				ToolCallID: call.ID,
				Content:    content,
			}); err != nil {
				return provider.PromptResponse{}, err
			}
			if hooks.OnTranscriptAppend != nil {
				hooks.OnTranscriptAppend(chatID, provider.Message{
					Role:       "tool",
					Name:       call.Name,
					ToolCallID: call.ID,
					Content:    content,
				})
			}
			if hooks.OnToolResult != nil {
				if err := hooks.OnToolResult(chatID, call, shaped, time.Since(started)); err != nil {
					return provider.PromptResponse{}, err
				}
			}
		}
	}

	if err := hooks.Store.Append(chatID, provider.Message{Role: "assistant", Content: final.Text}); err != nil {
		return provider.PromptResponse{}, err
	}
	if hooks.OnTranscriptAppend != nil {
		hooks.OnTranscriptAppend(chatID, provider.Message{Role: "assistant", Content: final.Text})
	}
	return final, nil
}
