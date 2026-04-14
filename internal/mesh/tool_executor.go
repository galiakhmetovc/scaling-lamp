package mesh

import (
	"context"
	"fmt"
	"time"
	"unicode"
	"strings"

	"teamd/internal/mcp"
	"teamd/internal/provider"
)

const maxToolRounds = 8

type ToolRuntime interface {
	ListTools(role string) ([]mcp.Tool, error)
	CallTool(ctx context.Context, name string, args map[string]any) (mcp.CallResult, error)
}

type ToolExecutor struct {
	AgentID  string
	Provider provider.Provider
	Tools    ToolRuntime
}

func (e ToolExecutor) Execute(ctx context.Context, env Envelope) (CandidateReply, error) {
	started := time.Now()
	if env.Kind == "proposal" {
		return CandidateReply{
			AgentID: e.AgentID,
			Stage:   "error",
			Err:     "tool execution is disabled in proposal mode",
			Latency: time.Since(started),
		}, nil
	}
	tools, err := e.providerTools()
	if err != nil {
		return CandidateReply{}, err
	}

	messages := []provider.Message{{Role: "user", Content: env.Prompt}}
	for round := 0; round < maxToolRounds; round++ {
		resp, err := e.Provider.Generate(ctx, provider.PromptRequest{
			WorkerID: "mesh:" + e.AgentID,
			Messages: messages,
			Tools:    tools,
		})
		if err != nil {
			return CandidateReply{
				AgentID: e.AgentID,
				Stage:   "error",
				Err:     err.Error(),
				Latency: time.Since(started),
			}, nil
		}
		if resp.FinishReason != "tool_calls" || len(resp.ToolCalls) == 0 {
			return CandidateReply{
				AgentID:            e.AgentID,
				Stage:              "final",
				Text:               resp.Text,
				Latency:            time.Since(started),
				TokensUsed:         resp.Usage.TotalTokens,
				DeterministicScore: 1,
				PassedChecks:       true,
			}, nil
		}

		messages = append(messages, provider.Message{
			Role:      "assistant",
			Content:   resp.Text,
			ToolCalls: resp.ToolCalls,
		})
		for _, call := range resp.ToolCalls {
			result, err := e.Tools.CallTool(ctx, runtimeToolName(call.Name), call.Arguments)
			if err != nil {
				result = mcp.CallResult{Content: "tool error: " + err.Error()}
			}
			messages = append(messages, provider.Message{
				Role:       "tool",
				Name:       call.Name,
				ToolCallID: call.ID,
				Content:    result.Content,
			})
		}
	}

	return CandidateReply{
		AgentID: e.AgentID,
		Stage:   "error",
		Err:     fmt.Sprintf("tool loop exceeded %d rounds", maxToolRounds),
		Latency: time.Since(started),
	}, nil
}

func (e ToolExecutor) providerTools() ([]provider.ToolDefinition, error) {
	if e.Tools == nil {
		return nil, nil
	}
	tools, err := e.Tools.ListTools("mesh")
	if err != nil {
		return nil, err
	}
	out := make([]provider.ToolDefinition, 0, len(tools))
	for _, tool := range tools {
		out = append(out, provider.ToolDefinition{
			Name:        providerToolName(tool.Name),
			Description: tool.Description,
			Parameters:  tool.Parameters,
		})
	}
	return out, nil
}

func providerToolName(name string) string {
	var b strings.Builder
	b.Grow(len(name))
	for _, r := range name {
		switch {
		case unicode.IsLetter(r), unicode.IsDigit(r), r == '_', r == '-':
			b.WriteRune(r)
		default:
			b.WriteRune('_')
		}
	}
	return b.String()
}

func runtimeToolName(name string) string {
	switch name {
	case providerToolName("filesystem.read_file"):
		return "filesystem.read_file"
	case providerToolName("filesystem.write_file"):
		return "filesystem.write_file"
	case providerToolName("filesystem.list_dir"):
		return "filesystem.list_dir"
	case providerToolName("shell.exec"):
		return "shell.exec"
	default:
		return name
	}
}
