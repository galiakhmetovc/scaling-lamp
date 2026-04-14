package telegram

import (
	"context"
	"strings"
	"testing"

	"teamd/internal/mcp"
	"teamd/internal/provider"
	runtimex "teamd/internal/runtime"
)

func TestAdapterProviderToolsFiltersDisallowedMCPTools(t *testing.T) {
	adapter := New(Deps{
		Tools: &fakeToolRuntime{
			tools: []mcp.Tool{
				{Name: "filesystem.read_file", Description: "read"},
				{Name: "shell.exec", Description: "exec"},
			},
		},
		MCPPolicy: runtimex.MCPPolicy{
			Mode:         runtimex.MCPPolicyAllowlist,
			AllowedTools: []string{"filesystem.read_file"},
		},
	})

	tools, err := adapter.providerTools("telegram")
	if err != nil {
		t.Fatalf("provider tools: %v", err)
	}
	names := []string{}
	for _, tool := range tools {
		names = append(names, tool.Name)
	}
	joined := strings.Join(names, ",")
	if strings.Contains(joined, providerToolName("shell.exec")) {
		t.Fatalf("expected shell.exec to be filtered out, got %q", joined)
	}
	if !strings.Contains(joined, providerToolName("filesystem.read_file")) {
		t.Fatalf("expected filesystem.read_file to remain, got %q", joined)
	}
}

func TestAdapterExecuteToolDeniesDisallowedTool(t *testing.T) {
	tools := &fakeToolRuntime{
		tools: []mcp.Tool{{Name: "shell.exec", Description: "exec"}},
	}
	adapter := New(Deps{
		Tools: tools,
		MCPPolicy: runtimex.MCPPolicy{
			Mode:         runtimex.MCPPolicyAllowlist,
			AllowedTools: []string{"filesystem.read_file"},
		},
	})

	out, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name:      providerToolName("shell.exec"),
		Arguments: map[string]any{"command": "echo hello"},
	})
	if err != nil {
		t.Fatalf("execute tool: %v", err)
	}
	if !strings.Contains(out, "tool denied:") {
		t.Fatalf("expected denial output, got %q", out)
	}
	if len(tools.calls) != 0 {
		t.Fatalf("expected denied tool to never execute, got %d calls", len(tools.calls))
	}
}

func TestAdapterExecuteToolLimitsOutputByPolicy(t *testing.T) {
	tools := &fakeToolRuntime{
		tools:  []mcp.Tool{{Name: "shell.exec", Description: "exec"}},
		result: mcp.CallResult{Content: "line1\nline2\nline3\nline4"},
	}
	adapter := New(Deps{
		Tools: tools,
		MCPPolicy: runtimex.MCPPolicy{
			Mode:           runtimex.MCPPolicyAllowlist,
			AllowedTools:   []string{"shell.exec"},
			MaxOutputLines: 2,
			MaxOutputBytes: 1024,
		},
	})

	out, err := adapter.executeTool(context.Background(), 1001, provider.ToolCall{
		Name:      providerToolName("shell.exec"),
		Arguments: map[string]any{"command": "echo hello"},
	})
	if err != nil {
		t.Fatalf("execute tool: %v", err)
	}
	if !strings.Contains(out, "output truncated by policy") {
		t.Fatalf("expected truncated output, got %q", out)
	}
	if !strings.Contains(out, "line1\nline2") {
		t.Fatalf("expected preserved prefix, got %q", out)
	}
}

func TestAdapterExecuteApprovedToolUsesRawAllowedToolsOverride(t *testing.T) {
	tools := &fakeToolRuntime{
		tools:  []mcp.Tool{{Name: "shell.exec", Description: "exec"}},
		result: mcp.CallResult{Content: "allowed"},
	}
	adapter := New(Deps{
		Tools: tools,
		MCPPolicy: runtimex.MCPPolicy{
			Mode:         runtimex.MCPPolicyAllowlist,
			AllowedTools: []string{"filesystem.read_file"},
		},
	})

	out, err := adapter.ExecuteApprovedTool(context.Background(), 1001, []string{"shell.exec"}, provider.ToolCall{
		Name:      providerToolName("shell.exec"),
		Arguments: map[string]any{"command": "echo ok"},
	})
	if err != nil {
		t.Fatalf("execute approved tool: %v", err)
	}
	if out != "allowed" {
		t.Fatalf("unexpected tool output: %q", out)
	}
	if len(tools.calls) != 1 || tools.calls[0].name != "shell.exec" {
		t.Fatalf("expected shell.exec call, got %#v", tools.calls)
	}
}

func TestAdapterExecuteApprovedToolRejectsToolOutsideRawAllowedTools(t *testing.T) {
	tools := &fakeToolRuntime{
		tools: []mcp.Tool{{Name: "plan_create", Description: "plan"}},
	}
	adapter := New(Deps{
		Tools: tools,
		MCPPolicy: runtimex.MCPPolicy{
			Mode:         runtimex.MCPPolicyAllowlist,
			AllowedTools: []string{"filesystem.read_file"},
		},
	})

	out, err := adapter.ExecuteApprovedTool(context.Background(), 1001, []string{"shell.exec"}, provider.ToolCall{
		Name:      "plan_create",
		Arguments: map[string]any{"title": "Test"},
	})
	if err != nil {
		t.Fatalf("execute approved tool: %v", err)
	}
	if !strings.Contains(out, `tool denied: tool "plan_create" is not allowed by mcp policy`) {
		t.Fatalf("unexpected denial output: %q", out)
	}
	if len(tools.calls) != 0 {
		t.Fatalf("expected denied tool to never execute, got %#v", tools.calls)
	}
}
