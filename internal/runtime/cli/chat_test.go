package cli_test

import (
	"bytes"
	"context"
	"io"
	"net/http"
	"strings"
	"testing"
	"time"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/filesystem"
	"teamd/internal/provider"
	"teamd/internal/runtime"
	"teamd/internal/runtime/cli"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
	"teamd/internal/tools"
)

func TestRunChatDisplaysToolActivityAndPlan(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	call := 0
	agent := &runtime.Agent{
		Config: runtimeConfigForCLITest(),
		Contracts: contracts.ResolvedContracts{
			ProviderRequest: contracts.ProviderRequestContract{
				Transport: contracts.TransportContract{
					Endpoint: contracts.EndpointPolicy{
						Enabled:  true,
						Strategy: "static",
						Params: contracts.EndpointParams{
							BaseURL: "http://example.invalid",
							Path:    "/chat/completions",
							Method:  "POST",
						},
					},
					Auth: contracts.AuthPolicy{
						Enabled:  false,
						Strategy: "none",
					},
				},
				RequestShape: contracts.RequestShapeContract{
					Model:     contracts.ModelPolicy{Enabled: true, Strategy: "static_model", Params: contracts.ModelParams{Model: "glm-5-turbo"}},
					Messages:  contracts.MessagePolicy{Enabled: true, Strategy: "raw_messages"},
					Tools:     contracts.ToolPolicy{Enabled: true, Strategy: "tools_inline"},
					Streaming: contracts.StreamingPolicy{Enabled: true, Strategy: "static_stream", Params: contracts.StreamingParams{Stream: false}},
				},
			},
			PlanTools: contracts.PlanToolContract{
				PlanTool: contracts.PlanToolPolicy{
					Enabled:  true,
					Strategy: "default_plan_tools",
					Params: contracts.PlanToolParams{
						ToolIDs: []string{"init_plan"},
					},
				},
			},
			Tools: contracts.ToolContract{
				Catalog:       contracts.ToolCatalogPolicy{Enabled: true, Strategy: "static_allowlist", Params: contracts.ToolCatalogParams{ToolIDs: []string{"init_plan"}, Dedupe: true}},
				Serialization: contracts.ToolSerializationPolicy{Enabled: true, Strategy: "openai_function_tools"},
			},
			ToolExecution: contracts.ToolExecutionContract{
				Access:   contracts.ToolAccessPolicy{Enabled: true, Strategy: "static_allowlist", Params: contracts.ToolAccessParams{ToolIDs: []string{"init_plan"}}},
				Approval: contracts.ToolApprovalPolicy{Enabled: true, Strategy: "always_allow"},
				Sandbox:  contracts.ToolSandboxPolicy{Enabled: true, Strategy: "default_runtime"},
			},
			Chat: contracts.ChatContract{
				Input:  contracts.ChatInputPolicy{Strategy: "multiline_buffer", Params: contracts.ChatInputParams{PrimaryPrompt: "> ", ContinuationPrompt: ". "}},
				Submit: contracts.ChatSubmitPolicy{Strategy: "double_enter", Params: contracts.ChatSubmitParams{EmptyLineThreshold: 1}},
				Output: contracts.ChatOutputPolicy{Strategy: "streaming_text", Params: contracts.ChatOutputParams{ShowFinalNewline: true}},
				Status: contracts.ChatStatusPolicy{Params: contracts.ChatStatusParams{
					ShowHeader:             true,
					ShowUsage:              true,
					ShowToolCalls:          true,
					ShowToolResults:        true,
					ShowPlanAfterPlanTools: true,
				}},
				Command: contracts.ChatCommandPolicy{Strategy: "slash_commands", Params: contracts.ChatCommandParams{ExitCommand: "/exit", HelpCommand: "/help", SessionCommand: "/session"}},
			},
		},
		MaxToolRounds:   4,
		PromptAssets:    provider.NewPromptAssetExecutor(),
		RequestShape:    provider.NewRequestShapeExecutor(),
		PlanTools:       tools.NewPlanToolExecutor(),
		FilesystemTools: filesystem.NewDefinitionExecutor(),
		ShellTools:      shell.NewDefinitionExecutor(),
		ToolCatalog:     tools.NewCatalogExecutor(),
		ToolExecution:   tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(fakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				call++
				if call == 1 {
					return &http.Response{
						StatusCode: http.StatusOK,
						Header:     http.Header{"Content-Type": []string{"application/json"}},
						Body: io.NopCloser(strings.NewReader(`{
  "id":"resp-tools-1",
  "model":"glm-5-turbo",
  "choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":"","tool_calls":[{"id":"call-1","function":{"name":"init_plan","arguments":{"goal":"Refactor auth"}}}]}}],
  "usage":{"prompt_tokens":8,"completion_tokens":2,"total_tokens":10}
}`)),
					}, nil
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body: io.NopCloser(strings.NewReader(`{
  "id":"resp-final",
  "model":"glm-5-turbo",
  "choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Done"}}],
  "usage":{"prompt_tokens":12,"completion_tokens":4,"total_tokens":16}
}`)),
				}, nil
			},
		}),
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewRunProjection(),
			projections.NewTranscriptProjection(),
			projections.NewActivePlanProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewPlanArchiveProjection(),
		},
		Now: func() time.Time { return time.Date(2026, 4, 14, 19, 30, 0, 0, time.UTC) },
		NewID: func(prefix string) string {
			return prefix + "-1"
		},
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, agent.FilesystemTools, agent.ShellTools, agent.ToolCatalog, agent.ToolExecution, agent.Transport)

	var stdout bytes.Buffer
	err := cli.RunChat(context.Background(), agent, "", strings.NewReader("make a plan\n\n/exit\n"), &stdout)
	if err != nil {
		t.Fatalf("RunChat returned error: %v", err)
	}
	out := stdout.String()
	if !strings.Contains(out, "[tool] init_plan") {
		t.Fatalf("stdout = %q, want tool block", out)
	}
	if !strings.Contains(out, "goal: Refactor auth") {
		t.Fatalf("stdout = %q, want tool arg summary", out)
	}
	if !strings.Contains(out, "[plan]") || !strings.Contains(out, "goal: Refactor auth") {
		t.Fatalf("stdout = %q, want plan block", out)
	}
}

type fakeDoer struct {
	do func(*http.Request) (*http.Response, error)
}

func (d fakeDoer) Do(req *http.Request) (*http.Response, error) {
	return d.do(req)
}

func runtimeConfigForCLITest() config.AgentConfig {
	return config.AgentConfig{ID: "zai-chat"}
}
