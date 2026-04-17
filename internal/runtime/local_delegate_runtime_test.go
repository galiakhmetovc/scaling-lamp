package runtime

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"testing"
	"time"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/delegation"
	"teamd/internal/filesystem"
	"teamd/internal/provider"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
	"teamd/internal/tools"
)

type delegateFakeDoer struct {
	do func(*http.Request) (*http.Response, error)
}

func (d delegateFakeDoer) Do(req *http.Request) (*http.Response, error) {
	return d.do(req)
}

func TestLocalDelegateRuntimeSpawnWaitAndHandoff(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	agent := &Agent{
		Config:        delegateRuntimeConfigForTest(),
		MaxToolRounds: 2,
		Contracts:     delegateRuntimeContractsForTest(),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		Transport: provider.NewTransportExecutor(delegateFakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-sub-1","model":"glm-5-turbo","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Subagent result."}}]}`)),
				}, nil
			},
		}),
		EventLog: NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewSessionCatalogProjection(),
			projections.NewRunProjection(),
			projections.NewTranscriptProjection(),
			projections.NewDelegateProjection(),
		},
		Now:   func() time.Time { return time.Date(2026, 4, 15, 15, 45, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)
	runtime := NewLocalDelegateRuntime(agent)

	view, err := runtime.Spawn(context.Background(), DelegateSpawnRequest{
		Backend:        DelegateBackendLocalWorker,
		OwnerSessionID: "session-owner-1",
		Prompt:         "Investigate tests",
		PolicySnapshot: map[string]any{"backend": "local_worker"},
	})
	if err != nil {
		t.Fatalf("Spawn returned error: %v", err)
	}
	if view.DelegateID == "" {
		t.Fatal("delegate id missing")
	}

	result, ok, err := runtime.Wait(context.Background(), DelegateWaitRequest{
		DelegateID:   view.DelegateID,
		AfterCursor:  0,
		AfterEventID: 0,
		EventLimit:   25,
	})
	if err != nil {
		t.Fatalf("Wait returned error: %v", err)
	}
	if !ok {
		t.Fatal("Wait returned ok=false")
	}
	if result.Handoff == nil || result.Handoff.Summary != "Subagent result." {
		t.Fatalf("handoff = %+v, want summary", result.Handoff)
	}
	if len(result.Messages) != 2 {
		t.Fatalf("messages len = %d, want 2", len(result.Messages))
	}
	if result.Messages[0].Role != "user" || result.Messages[0].Content != "Investigate tests" {
		t.Fatalf("first message = %+v", result.Messages[0])
	}
	if result.Messages[1].Role != "assistant" || result.Messages[1].Content != "Subagent result." {
		t.Fatalf("second message = %+v", result.Messages[1])
	}
	if len(result.Events) == 0 {
		t.Fatal("delegate events missing")
	}
}

func TestLocalDelegateRuntimeUsesDelegatedApprovalSnapshot(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	var secondRequest map[string]any
	call := 0
	agent := &Agent{
		Config:        delegateRuntimeConfigForTest(),
		MaxToolRounds: 2,
		Contracts:     delegateRuntimeContractsWithShellForTest(),
		PromptAssets:  provider.NewPromptAssetExecutor(),
		RequestShape:  provider.NewRequestShapeExecutor(),
		PlanTools:     tools.NewPlanToolExecutor(),
		ToolCatalog:   tools.NewCatalogExecutor(),
		ToolExecution: tools.NewExecutionGate(),
		ShellRuntime:  shell.NewExecutor(),
		Transport: provider.NewTransportExecutor(delegateFakeDoer{
			do: func(req *http.Request) (*http.Response, error) {
				call++
				if call == 1 {
					return &http.Response{
						StatusCode: http.StatusOK,
						Header:     http.Header{"Content-Type": []string{"application/json"}},
						Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-delegate-approval-1","model":"glm-5-turbo","choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":"","tool_calls":[{"id":"call-shell-1","function":{"name":"shell_exec","arguments":{"command":"pwd"}}}]}}]}`)),
					}, nil
				}
				defer req.Body.Close()
				if err := json.NewDecoder(req.Body).Decode(&secondRequest); err != nil {
					t.Fatalf("decode second request body: %v", err)
				}
				return &http.Response{
					StatusCode: http.StatusOK,
					Header:     http.Header{"Content-Type": []string{"application/json"}},
					Body:       io.NopCloser(bytes.NewBufferString(`{"id":"resp-delegate-approval-2","model":"glm-5-turbo","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Need approval."}}]}`)),
				}, nil
			},
		}),
		EventLog: NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewSessionCatalogProjection(),
			projections.NewRunProjection(),
			projections.NewTranscriptProjection(),
			projections.NewDelegateProjection(),
			projections.NewShellCommandProjection(),
		},
		Now:   func() time.Time { return time.Date(2026, 4, 15, 16, 0, 0, 0, time.UTC) },
		NewID: func(prefix string) string { return prefix + "-approval" },
	}
	agent.ProviderClient = provider.NewClient(agent.PromptAssets, agent.RequestShape, agent.PlanTools, filesystem.NewDefinitionExecutor(), shell.NewDefinitionExecutor(), delegation.NewDefinitionExecutor(), agent.ToolCatalog, agent.ToolExecution, agent.Transport)
	runtime := NewLocalDelegateRuntime(agent)

	snapshotMap, err := encodeDelegatePolicySnapshot(DelegatePolicySnapshot{
		Tools: agent.Contracts.Tools,
		ToolExecution: contracts.ToolExecutionContract{
			Access: contracts.ToolAccessPolicy{
				Enabled:  true,
				Strategy: "static_allowlist",
				Params:   contracts.ToolAccessParams{ToolIDs: []string{"shell_exec"}},
			},
			Approval: contracts.ToolApprovalPolicy{
				Enabled:  true,
				Strategy: "always_allow",
			},
			Sandbox: contracts.ToolSandboxPolicy{
				Enabled:  true,
				Strategy: "default_runtime",
			},
		},
		ShellTools: agent.Contracts.ShellTools,
		ShellExecution: contracts.ShellExecutionContract{
			Command: agent.Contracts.ShellExecution.Command,
			Approval: contracts.ShellApprovalPolicy{
				Enabled:  true,
				Strategy: "always_require",
				Params: contracts.ShellApprovalParams{
					ApprovalMessageTemplate: "delegate shell approval required: {{command}}",
				},
			},
			Runtime: agent.Contracts.ShellExecution.Runtime,
		},
		PlanTools:       contracts.PlanToolContract{},
		FilesystemTools: contracts.FilesystemToolContract{},
		DelegationTools: contracts.DelegationToolContract{},
	})
	if err != nil {
		t.Fatalf("encodeDelegatePolicySnapshot returned error: %v", err)
	}

	view, err := runtime.Spawn(context.Background(), DelegateSpawnRequest{
		Backend:        DelegateBackendLocalWorker,
		OwnerSessionID: "session-owner-approval",
		Prompt:         "Investigate tests",
		PolicySnapshot: snapshotMap,
	})
	if err != nil {
		t.Fatalf("Spawn returned error: %v", err)
	}

	result, ok, err := runtime.Wait(context.Background(), DelegateWaitRequest{
		DelegateID:   view.DelegateID,
		AfterCursor:  0,
		AfterEventID: 0,
		EventLimit:   25,
	})
	if err != nil {
		t.Fatalf("Wait returned error: %v", err)
	}
	if !ok {
		t.Fatal("Wait returned ok=false")
	}
	if result.Handoff == nil || result.Handoff.Summary != "Need approval." {
		t.Fatalf("handoff = %+v, want Need approval.", result.Handoff)
	}
	if secondRequest != nil && !requestContainsToolErrorFragment(secondRequest, "shell_exec", "\"approval_pending\"") {
		t.Fatalf("second request missing delegated shell approval result: %#v", secondRequest["messages"])
	}
}

func delegateRuntimeConfigForTest() config.AgentConfig {
	return config.AgentConfig{ID: "agent-delegate-test"}
}

func delegateRuntimeContractsForTest() contracts.ResolvedContracts {
	return contracts.ResolvedContracts{
		ProviderRequest: contracts.ProviderRequestContract{
			Transport: contracts.TransportContract{
				ID: "transport-delegate-runtime",
				Endpoint: contracts.EndpointPolicy{
					Enabled:  true,
					Strategy: "static",
					Params: contracts.EndpointParams{
						BaseURL: "https://api.z.ai/api/coding/paas/v4",
						Path:    "/chat/completions",
						Method:  http.MethodPost,
					},
				},
				Auth: contracts.AuthPolicy{
					Enabled:  true,
					Strategy: "bearer_token",
					Params: contracts.AuthParams{
						Header:      "Authorization",
						Prefix:      "Bearer",
						ValueEnvVar: "TEAMD_ZAI_API_KEY",
					},
				},
			},
			RequestShape: contracts.RequestShapeContract{
				ID:        "request-shape-delegate-runtime",
				Model:     contracts.ModelPolicy{Enabled: true, Strategy: "static_model", Params: contracts.ModelParams{Model: "glm-5-turbo"}},
				Messages:  contracts.MessagePolicy{Enabled: true, Strategy: "raw_messages"},
				Tools:     contracts.ToolPolicy{Enabled: true, Strategy: "tools_inline"},
				Streaming: contracts.StreamingPolicy{Enabled: true, Strategy: "static_stream", Params: contracts.StreamingParams{Stream: false}},
			},
		},
		PromptAssets: contracts.PromptAssetsContract{
			ID: "prompt-assets-delegate-runtime",
			PromptAsset: contracts.PromptAssetPolicy{
				Enabled:  true,
				Strategy: "inline_assets",
				Params:   contracts.PromptAssetParams{Assets: []contracts.PromptAsset{}},
			},
		},
		ProviderTrace: contracts.ProviderTraceContract{
			ID: "provider-trace-delegate-runtime",
			Request: contracts.ProviderTracePolicy{
				Enabled:  true,
				Strategy: "inline_request",
				Params: contracts.ProviderTraceParams{
					IncludeRawBody:        true,
					IncludeDecodedPayload: true,
				},
			},
		},
		Tools: contracts.ToolContract{
			Catalog: contracts.ToolCatalogPolicy{
				Enabled:  true,
				Strategy: "static_allowlist",
				Params:   contracts.ToolCatalogParams{ToolIDs: []string{"delegate_spawn", "delegate_wait"}},
			},
			Serialization: contracts.ToolSerializationPolicy{
				Enabled:  true,
				Strategy: "openai_function_tools",
				Params:   contracts.ToolSerializationParams{IncludeDescriptions: true},
			},
		},
		DelegationTools: contracts.DelegationToolContract{
			Catalog: contracts.DelegationCatalogPolicy{
				Enabled:  true,
				Strategy: "static_allowlist",
				Params:   contracts.DelegationCatalogParams{ToolIDs: []string{"delegate_spawn", "delegate_wait"}},
			},
			Description: contracts.DelegationDescriptionPolicy{
				Enabled:  true,
				Strategy: "static_builtin_descriptions",
				Params: contracts.DelegationDescriptionParams{
					IncludeExamples:       true,
					IncludeBackendHints:   true,
					IncludeLifecycleNotes: true,
				},
			},
		},
		DelegationExecution: contracts.DelegationExecutionContract{
			Backend: contracts.DelegationBackendPolicy{
				Enabled:  true,
				Strategy: "backend_allowlist",
				Params: contracts.DelegationBackendParams{
					AllowedBackends: []string{"local_worker"},
					DefaultBackend:  "local_worker",
				},
			},
			Result: contracts.DelegationResultPolicy{
				Enabled:  true,
				Strategy: "bounded_wait_results",
				Params: contracts.DelegationResultParams{
					IncludeEvents:         true,
					IncludeArtifacts:      true,
					IncludePolicySnapshot: true,
					DefaultEventLimit:     25,
					MaxEventLimit:         100,
				},
			},
		},
		ToolExecution: contracts.ToolExecutionContract{
			Access: contracts.ToolAccessPolicy{
				Enabled:  true,
				Strategy: "static_allowlist",
				Params:   contracts.ToolAccessParams{ToolIDs: []string{"delegate_spawn", "delegate_wait"}},
			},
			Approval: contracts.ToolApprovalPolicy{Enabled: true, Strategy: "always_allow"},
			Sandbox:  contracts.ToolSandboxPolicy{Enabled: true, Strategy: "default_runtime"},
		},
	}
}

func delegateRuntimeContractsWithShellForTest() contracts.ResolvedContracts {
	out := delegateRuntimeContractsForTest()
	out.Tools = contracts.ToolContract{
		Catalog: contracts.ToolCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ToolCatalogParams{ToolIDs: []string{"shell_exec"}},
		},
		Serialization: contracts.ToolSerializationPolicy{
			Enabled:  true,
			Strategy: "openai_function_tools",
			Params:   contracts.ToolSerializationParams{IncludeDescriptions: true},
		},
	}
	out.ShellTools = contracts.ShellToolContract{
		Catalog: contracts.ShellCatalogPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ShellCatalogParams{ToolIDs: []string{"shell_exec"}},
		},
		Description: contracts.ShellDescriptionPolicy{
			Enabled:  true,
			Strategy: "static_builtin_descriptions",
			Params: contracts.ShellDescriptionParams{
				IncludeExamples: true,
			},
		},
	}
	out.ShellExecution = contracts.ShellExecutionContract{
		Command: contracts.ShellCommandPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params: contracts.ShellCommandParams{
				AllowedCommands: []string{"pwd"},
			},
		},
		Approval: contracts.ShellApprovalPolicy{
			Enabled:  true,
			Strategy: "always_allow",
		},
		Runtime: contracts.ShellRuntimePolicy{
			Enabled:  true,
			Strategy: "workspace_write",
			Params: contracts.ShellRuntimeParams{
				Cwd:            ".",
				Timeout:        "5s",
				MaxOutputBytes: 4096,
				AllowNetwork:   true,
			},
		},
	}
	out.ToolExecution = contracts.ToolExecutionContract{
		Access: contracts.ToolAccessPolicy{
			Enabled:  true,
			Strategy: "static_allowlist",
			Params:   contracts.ToolAccessParams{ToolIDs: []string{"shell_exec"}},
		},
		Approval: contracts.ToolApprovalPolicy{
			Enabled:  true,
			Strategy: "always_allow",
		},
		Sandbox: contracts.ToolSandboxPolicy{
			Enabled:  true,
			Strategy: "default_runtime",
		},
	}
	return out
}

func requestContainsToolErrorFragment(requestBody map[string]any, toolName, fragment string) bool {
	messages, ok := requestBody["messages"].([]any)
	if !ok {
		return false
	}
	for _, raw := range messages {
		message, ok := raw.(map[string]any)
		if !ok {
			continue
		}
		if message["role"] != "tool" || message["name"] != toolName {
			continue
		}
		content, _ := message["content"].(string)
		if strings.Contains(content, fragment) {
			return true
		}
	}
	return false
}
