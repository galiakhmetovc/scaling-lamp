package mesh

import (
	"context"
	"testing"

	"teamd/internal/mcp"
	"teamd/internal/provider"
)

type fakeToolRuntime struct {
	tools []mcp.Tool
	calls []struct {
		name string
		args map[string]any
	}
	result mcp.CallResult
	err    error
}

func (f *fakeToolRuntime) ListTools(string) ([]mcp.Tool, error) {
	return f.tools, nil
}

func (f *fakeToolRuntime) CallTool(_ context.Context, name string, args map[string]any) (mcp.CallResult, error) {
	f.calls = append(f.calls, struct {
		name string
		args map[string]any
	}{name: name, args: args})
	if f.err != nil {
		return mcp.CallResult{}, f.err
	}
	return f.result, nil
}

type scriptedMeshProvider struct {
	responses []provider.PromptResponse
}

func (s *scriptedMeshProvider) Generate(_ context.Context, _ provider.PromptRequest) (provider.PromptResponse, error) {
	resp := s.responses[0]
	s.responses = s.responses[1:]
	return resp, nil
}

func TestToolExecutorRunsModelRequestedToolAndReturnsFinalReply(t *testing.T) {
	runtime := &fakeToolRuntime{
		tools: []mcp.Tool{
			{Name: "shell.exec", Description: "run shell", Parameters: map[string]any{"type": "object"}},
		},
		result: mcp.CallResult{Content: "Mem: 16Gi used 4Gi"},
	}
	providerClient := &scriptedMeshProvider{
		responses: []provider.PromptResponse{
			{
				FinishReason: "tool_calls",
				ToolCalls: []provider.ToolCall{
					{ID: "call-1", Name: "shell_exec", Arguments: map[string]any{"command": "free -h"}},
				},
			},
			{
				Text: "На сервере 16Gi памяти, используется 4Gi.",
				Usage: provider.Usage{TotalTokens: 42},
			},
		},
	}

	exec := ToolExecutor{
		AgentID:  "agent-peer",
		Provider: providerClient,
		Tools:    runtime,
	}

	reply, err := exec.Execute(context.Background(), Envelope{Prompt: "проверь память на сервере"})
	if err != nil {
		t.Fatalf("execute: %v", err)
	}
	if len(runtime.calls) != 1 || runtime.calls[0].name != "shell.exec" {
		t.Fatalf("expected shell.exec call, got %#v", runtime.calls)
	}
	if reply.Stage != "final" || reply.Text != "На сервере 16Gi памяти, используется 4Gi." {
		t.Fatalf("unexpected reply: %#v", reply)
	}
}

func TestToolExecutorRejectsProposalModeBeforeAnyToolCall(t *testing.T) {
	runtime := &fakeToolRuntime{
		tools: []mcp.Tool{
			{Name: "shell.exec", Description: "run shell", Parameters: map[string]any{"type": "object"}},
		},
		result: mcp.CallResult{Content: "ignored"},
	}
	providerClient := &scriptedMeshProvider{
		responses: []provider.PromptResponse{{
			Text: "should not be used",
		}},
	}

	exec := ToolExecutor{
		AgentID:  "agent-peer",
		Provider: providerClient,
		Tools:    runtime,
	}

	reply, err := exec.Execute(context.Background(), Envelope{Kind: "proposal", Prompt: "проверь память на сервере"})
	if err != nil {
		t.Fatalf("execute: %v", err)
	}
	if reply.Stage != "error" {
		t.Fatalf("expected proposal mode rejection, got %#v", reply)
	}
	if len(runtime.calls) != 0 {
		t.Fatalf("expected no tool calls in proposal mode, got %#v", runtime.calls)
	}
}
