package telegram

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
)

func errorsIsCanceled(err error) bool {
	return err == context.Canceled || strings.Contains(err.Error(), context.Canceled.Error())
}

func lastUserMessage(messages []provider.Message) string {
	for i := len(messages) - 1; i >= 0; i-- {
		if messages[i].Role == "user" {
			return strings.TrimSpace(messages[i].Content)
		}
	}
	return ""
}

func shouldStopForAdvisoryDraft(userPrompt string, resp provider.PromptResponse) bool {
	if len(resp.ToolCalls) == 0 || strings.TrimSpace(resp.Text) == "" {
		return false
	}
	if !looksLikeAdvisoryPrompt(userPrompt) || !looksLikeAdvisoryDraft(resp.Text) {
		return false
	}
	for _, call := range resp.ToolCalls {
		if !looksLikeGeneralResearchToolCall(call) {
			return false
		}
	}
	return true
}

func looksLikeAdvisoryPrompt(text string) bool {
	normalized := strings.ToLower(strings.TrimSpace(text))
	if normalized == "" {
		return false
	}
	hints := []string{
		"что посоветуешь", "что лучше", "как бы ты сделал", "что думаешь",
		"как лучше", "посоветуй", "recommend", "what would you do",
	}
	for _, hint := range hints {
		if strings.Contains(normalized, hint) {
			return true
		}
	}
	return false
}

func looksLikeAdvisoryDraft(text string) bool {
	normalized := strings.ToLower(strings.TrimSpace(text))
	if normalized == "" {
		return false
	}
	hints := []string{
		"я бы рекомендовал", "я бы советовал", "мой совет", "рекомендую",
		"лучше", "стоит", "советую", "i would recommend", "my recommendation",
	}
	for _, hint := range hints {
		if strings.Contains(normalized, hint) {
			return true
		}
	}
	return false
}

func looksLikeGeneralResearchToolCall(call provider.ToolCall) bool {
	if runtimeToolName(call.Name) != "shell.exec" {
		return false
	}
	command, _ := call.Arguments["command"].(string)
	normalized := strings.ToLower(strings.TrimSpace(command))
	if normalized == "" {
		return false
	}
	return strings.Contains(normalized, "curl ") ||
		strings.Contains(normalized, "wget ") ||
		strings.Contains(normalized, "search?q=") ||
		strings.Contains(normalized, "best practices") ||
		strings.Contains(normalized, "google") ||
		strings.Contains(normalized, "bing")
}

func toolCallSignature(call provider.ToolCall) string {
	body, err := json.Marshal(call.Arguments)
	if err != nil {
		return runtimeToolName(call.Name)
	}
	return runtimeToolName(call.Name) + ":" + string(body)
}

func shouldBreakRepeatedToolLoop(call provider.ToolCall, repeatedCount int) bool {
	if repeatedCount < 2 {
		return false
	}
	switch runtimeToolName(call.Name) {
	case "shell.exec", "filesystem.read_file", "filesystem.list_dir", "skills.list", "skills.read":
		return true
	default:
		return false
	}
}

func syntheticLoopBreakerToolOutput(call provider.ToolCall, repeatedCount int) string {
	return fmt.Sprintf(
		"tool guard triggered: repeated identical call to %s detected %d times. Stop probing the same thing. Choose an alternative approach or ask the user for clarification.",
		runtimeToolName(call.Name),
		repeatedCount,
	)
}

func mergeRuntimeConfig(base, override provider.RequestConfig) provider.RequestConfig {
	return runtimex.MergeRequestConfig(base, override)
}
