package runtime_test

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"teamd/internal/config"
	"teamd/internal/runtime"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

func TestBuildAgentLoadsRootConfigAndBootstrapsRuntime(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  runtime:\n"+
		"    event_log: file_jsonl\n"+
		"    event_log_path: ./var/events.jsonl\n"+
		"    prompt_asset_executor: prompt_asset_default\n"+
		"    transport_executor: transport_default\n"+
		"    request_shape_executor: request_shape_default\n"+
		"    provider_client: provider_client_default\n"+
		"    projections: [session, run, transcript, active_plan, plan_archive, plan_head]\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n"+
		"    request_shape: ./contracts/request-shape.yaml\n"+
		"    memory: ./contracts/memory.yaml\n"+
		"    plan_tools: ./contracts/plan-tools.yaml\n"+
		"    provider_trace: ./contracts/provider-trace.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), ""+
		"kind: TransportContractConfig\n"+
		"version: v1\n"+
		"id: transport-main\n"+
		"spec:\n"+
		"  endpoint_policy_path: ../policies/transport/endpoint.yaml\n"+
		"  auth_policy_path: ../policies/transport/auth.yaml\n"+
		"  retry_policy_path: ../policies/transport/retry.yaml\n"+
		"  timeout_policy_path: ../policies/transport/timeout.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "memory.yaml"), ""+
		"kind: MemoryContractConfig\n"+
		"version: v1\n"+
		"id: memory-main\n"+
		"spec:\n"+
		"  offload_policy_path: ../policies/memory/offload.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "provider-trace.yaml"), ""+
		"kind: ProviderTraceContractConfig\n"+
		"version: v1\n"+
		"id: provider-trace-main\n"+
		"spec:\n"+
		"  provider_trace_policy_path: ../policies/provider-trace/request.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "plan-tools.yaml"), ""+
		"kind: PlanToolContractConfig\n"+
		"version: v1\n"+
		"id: plan-tools-main\n"+
		"spec:\n"+
		"  plan_tool_policy_path: ../policies/tools/plan-tools.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "request-shape.yaml"), ""+
		"kind: RequestShapeContractConfig\n"+
		"version: v1\n"+
		"id: request-shape-main\n"+
		"spec:\n"+
		"  model_policy_path: ../policies/request-shape/model.yaml\n"+
		"  message_policy_path: ../policies/request-shape/messages.yaml\n"+
		"  tool_policy_path: ../policies/request-shape/tools.yaml\n"+
		"  response_format_policy_path: ../policies/request-shape/response-format.yaml\n"+
		"  streaming_policy_path: ../policies/request-shape/streaming.yaml\n"+
		"  sampling_policy_path: ../policies/request-shape/sampling.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), ""+
		"kind: EndpointPolicyConfig\n"+
		"version: v1\n"+
		"id: endpoint-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static\n"+
		"  params:\n"+
		"    base_url: https://api.z.ai\n"+
		"    path: /api/paas/v4/chat/completions\n"+
		"    method: POST\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "auth.yaml"), ""+
		"kind: AuthPolicyConfig\n"+
		"version: v1\n"+
		"id: auth-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: bearer_token\n"+
		"  params:\n"+
		"    header: Authorization\n"+
		"    prefix: Bearer\n"+
		"    value_env_var: ZAI_API_KEY\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "retry.yaml"), ""+
		"kind: RetryPolicyConfig\n"+
		"version: v1\n"+
		"id: retry-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: exponential_jitter\n"+
		"  params:\n"+
		"    max_attempts: 3\n"+
		"    base_delay: 100ms\n"+
		"    max_delay: 1s\n"+
		"    retry_on_statuses: [429, 500, 502, 503]\n"+
		"    retry_on_errors: [transport_error]\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "timeout.yaml"), ""+
		"kind: TimeoutPolicyConfig\n"+
		"version: v1\n"+
		"id: timeout-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: per_request\n"+
		"  params:\n"+
		"    total: 30s\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "memory", "offload.yaml"), ""+
		"kind: OffloadPolicyConfig\n"+
		"version: v1\n"+
		"id: offload-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: old_only\n"+
		"  params:\n"+
		"    max_chars: 1200\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "provider-trace", "request.yaml"), ""+
		"kind: ProviderTracePolicyConfig\n"+
		"version: v1\n"+
		"id: provider-trace-request-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: inline_request\n"+
		"  params:\n"+
		"    include_raw_body: true\n"+
		"    include_decoded_payload: true\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "tools", "plan-tools.yaml"), ""+
		"kind: PlanToolPolicyConfig\n"+
		"version: v1\n"+
		"id: plan-tools-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: default_plan_tools\n"+
		"  params:\n"+
		"    tool_ids: [init_plan]\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "model.yaml"), ""+
		"kind: ModelPolicyConfig\n"+
		"version: v1\n"+
		"id: model-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_model\n"+
		"  params:\n"+
		"    model: glm-4.6\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "messages.yaml"), ""+
		"kind: MessagePolicyConfig\n"+
		"version: v1\n"+
		"id: messages-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: raw_messages\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "tools.yaml"), ""+
		"kind: ToolPolicyConfig\n"+
		"version: v1\n"+
		"id: tools-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: tools_inline\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "response-format.yaml"), ""+
		"kind: ResponseFormatPolicyConfig\n"+
		"version: v1\n"+
		"id: response-format-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: default\n"+
		"  params:\n"+
		"    type: json_object\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "streaming.yaml"), ""+
		"kind: StreamingPolicyConfig\n"+
		"version: v1\n"+
		"id: streaming-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_stream\n"+
		"  params:\n"+
		"    stream: false\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "sampling.yaml"), ""+
		"kind: SamplingPolicyConfig\n"+
		"version: v1\n"+
		"id: sampling-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_sampling\n"+
		"  params:\n"+
		"    temperature: 0.2\n"+
		"    max_output_tokens: 2048\n")

	agent, err := runtime.BuildAgent(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("BuildAgent returned error: %v", err)
	}

	if agent.Config.ID != "agent-test" {
		t.Fatalf("agent config ID = %q, want %q", agent.Config.ID, "agent-test")
	}
	if agent.EventLog == nil {
		t.Fatal("agent EventLog is nil")
	}
	if len(agent.Projections) != 6 {
		t.Fatalf("agent projections len = %d, want 6", len(agent.Projections))
	}
	if agent.Transport == nil {
		t.Fatal("agent Transport is nil")
	}
	if agent.PromptAssets == nil {
		t.Fatal("agent PromptAssets is nil")
	}
	if agent.RequestShape == nil {
		t.Fatal("agent RequestShape is nil")
	}
	if agent.ProviderClient == nil {
		t.Fatal("agent ProviderClient is nil")
	}
	if agent.Contracts.PlanTools.PlanTool.Strategy != "default_plan_tools" {
		t.Fatalf("plan tool strategy = %q, want default_plan_tools", agent.Contracts.PlanTools.PlanTool.Strategy)
	}
	if agent.Contracts.ProviderRequest.Transport.ID != "transport-main" {
		t.Fatalf("transport contract ID = %q, want %q", agent.Contracts.ProviderRequest.Transport.ID, "transport-main")
	}
	if agent.Contracts.ProviderRequest.Transport.Endpoint.Strategy != "static" {
		t.Fatalf("endpoint strategy = %q, want %q", agent.Contracts.ProviderRequest.Transport.Endpoint.Strategy, "static")
	}
	if agent.Contracts.ProviderRequest.Transport.Endpoint.Params.BaseURL != "https://api.z.ai" {
		t.Fatalf("endpoint base URL = %q, want %q", agent.Contracts.ProviderRequest.Transport.Endpoint.Params.BaseURL, "https://api.z.ai")
	}
	if agent.Contracts.ProviderRequest.Transport.Auth.Strategy != "bearer_token" {
		t.Fatalf("auth strategy = %q, want %q", agent.Contracts.ProviderRequest.Transport.Auth.Strategy, "bearer_token")
	}
	if agent.Contracts.ProviderRequest.Transport.Retry.Params.MaxAttempts != 3 {
		t.Fatalf("max attempts = %d, want 3", agent.Contracts.ProviderRequest.Transport.Retry.Params.MaxAttempts)
	}
	if agent.Contracts.ProviderRequest.Transport.Timeout.Params.Total != "30s" {
		t.Fatalf("timeout total = %q, want %q", agent.Contracts.ProviderRequest.Transport.Timeout.Params.Total, "30s")
	}
	if agent.Contracts.Memory.Offload.Strategy != "old_only" {
		t.Fatalf("offload strategy = %q, want %q", agent.Contracts.Memory.Offload.Strategy, "old_only")
	}
	if agent.Contracts.ProviderTrace.Request.Strategy != "inline_request" {
		t.Fatalf("provider trace strategy = %q, want %q", agent.Contracts.ProviderTrace.Request.Strategy, "inline_request")
	}
	if agent.Contracts.ProviderRequest.RequestShape.ID != "request-shape-main" {
		t.Fatalf("request-shape ID = %q, want %q", agent.Contracts.ProviderRequest.RequestShape.ID, "request-shape-main")
	}
	if agent.Contracts.ProviderRequest.RequestShape.Model.Params.Model != "glm-4.6" {
		t.Fatalf("model = %q, want %q", agent.Contracts.ProviderRequest.RequestShape.Model.Params.Model, "glm-4.6")
	}
}

func TestBuildAgentUsesConfiguredRuntimeComposition(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  runtime:\n"+
		"    event_log: file_jsonl\n"+
		"    event_log_path: ./var/events.jsonl\n"+
		"    projection_store_path: ./var/projections.json\n"+
		"    prompt_asset_executor: prompt_asset_default\n"+
		"    transport_executor: transport_default\n"+
		"    request_shape_executor: request_shape_default\n"+
		"    provider_client: provider_client_default\n"+
		"    projections: [run]\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n"+
		"    request_shape: ./contracts/request-shape.yaml\n"+
		"    memory: ./contracts/memory.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), ""+
		"kind: TransportContractConfig\n"+
		"version: v1\n"+
		"id: transport-main\n"+
		"spec:\n"+
		"  endpoint_policy_path: ../policies/transport/endpoint.yaml\n"+
		"  auth_policy_path: ../policies/transport/auth.yaml\n"+
		"  retry_policy_path: ../policies/transport/retry.yaml\n"+
		"  timeout_policy_path: ../policies/transport/timeout.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "memory.yaml"), ""+
		"kind: MemoryContractConfig\n"+
		"version: v1\n"+
		"id: memory-main\n"+
		"spec:\n"+
		"  offload_policy_path: ../policies/memory/offload.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "request-shape.yaml"), ""+
		"kind: RequestShapeContractConfig\n"+
		"version: v1\n"+
		"id: request-shape-main\n"+
		"spec:\n"+
		"  model_policy_path: ../policies/request-shape/model.yaml\n"+
		"  message_policy_path: ../policies/request-shape/messages.yaml\n"+
		"  tool_policy_path: ../policies/request-shape/tools.yaml\n"+
		"  response_format_policy_path: ../policies/request-shape/response-format.yaml\n"+
		"  streaming_policy_path: ../policies/request-shape/streaming.yaml\n"+
		"  sampling_policy_path: ../policies/request-shape/sampling.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), ""+
		"kind: EndpointPolicyConfig\n"+
		"version: v1\n"+
		"id: endpoint-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static\n"+
		"  params:\n"+
		"    base_url: https://api.z.ai\n"+
		"    path: /api/paas/v4/chat/completions\n"+
		"    method: POST\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "auth.yaml"), ""+
		"kind: AuthPolicyConfig\n"+
		"version: v1\n"+
		"id: auth-main\n"+
		"spec:\n"+
		"  enabled: false\n"+
		"  strategy: none\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "retry.yaml"), ""+
		"kind: RetryPolicyConfig\n"+
		"version: v1\n"+
		"id: retry-main\n"+
		"spec:\n"+
		"  enabled: false\n"+
		"  strategy: none\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "timeout.yaml"), ""+
		"kind: TimeoutPolicyConfig\n"+
		"version: v1\n"+
		"id: timeout-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: per_request\n"+
		"  params:\n"+
		"    total: 30s\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "memory", "offload.yaml"), ""+
		"kind: OffloadPolicyConfig\n"+
		"version: v1\n"+
		"id: offload-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: old_only\n"+
		"  params:\n"+
		"    max_chars: 1200\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "model.yaml"), ""+
		"kind: ModelPolicyConfig\n"+
		"version: v1\n"+
		"id: model-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_model\n"+
		"  params:\n"+
		"    model: glm-4.6\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "messages.yaml"), ""+
		"kind: MessagePolicyConfig\n"+
		"version: v1\n"+
		"id: messages-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: raw_messages\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "tools.yaml"), ""+
		"kind: ToolPolicyConfig\n"+
		"version: v1\n"+
		"id: tools-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: tools_inline\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "response-format.yaml"), ""+
		"kind: ResponseFormatPolicyConfig\n"+
		"version: v1\n"+
		"id: response-format-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: default\n"+
		"  params:\n"+
		"    type: json_object\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "streaming.yaml"), ""+
		"kind: StreamingPolicyConfig\n"+
		"version: v1\n"+
		"id: streaming-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_stream\n"+
		"  params:\n"+
		"    stream: false\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "sampling.yaml"), ""+
		"kind: SamplingPolicyConfig\n"+
		"version: v1\n"+
		"id: sampling-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_sampling\n"+
		"  params:\n"+
		"    temperature: 0.2\n"+
		"    max_output_tokens: 2048\n")

	agent, err := runtime.BuildAgent(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("BuildAgent returned error: %v", err)
	}

	if len(agent.Projections) != 1 {
		t.Fatalf("agent projections len = %d, want 1", len(agent.Projections))
	}
	if agent.Projections[0].ID() != "run" {
		t.Fatalf("projection id = %q, want %q", agent.Projections[0].ID(), "run")
	}
	if agent.Transport == nil {
		t.Fatal("agent Transport is nil")
	}
	if agent.RequestShape == nil {
		t.Fatal("agent RequestShape is nil")
	}
}

func TestBuildAgentInitializesArtifactStoreForArtifactOffload(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  runtime:\n"+
		"    event_log: in_memory\n"+
		"    prompt_asset_executor: prompt_asset_default\n"+
		"    transport_executor: transport_default\n"+
		"    request_shape_executor: request_shape_default\n"+
		"    tool_catalog_executor: tool_catalog_default\n"+
		"    tool_execution_gate: tool_execution_default\n"+
		"    provider_client: provider_client_default\n"+
		"    projections: [session]\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n"+
		"    request_shape: ./contracts/request-shape.yaml\n"+
		"    memory: ./contracts/memory.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), ""+
		"kind: TransportContractConfig\n"+
		"version: v1\n"+
		"id: transport-main\n"+
		"spec:\n"+
		"  endpoint_policy_path: ../policies/transport/endpoint.yaml\n"+
		"  auth_policy_path: ../policies/transport/auth.yaml\n"+
		"  retry_policy_path: ../policies/transport/retry.yaml\n"+
		"  timeout_policy_path: ../policies/transport/timeout.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "request-shape.yaml"), ""+
		"kind: RequestShapeContractConfig\n"+
		"version: v1\n"+
		"id: request-shape-main\n"+
		"spec:\n"+
		"  model_policy_path: ../policies/request-shape/model.yaml\n"+
		"  message_policy_path: ../policies/request-shape/messages.yaml\n"+
		"  tool_policy_path: ../policies/request-shape/tools.yaml\n"+
		"  response_format_policy_path: ../policies/request-shape/response-format.yaml\n"+
		"  streaming_policy_path: ../policies/request-shape/streaming.yaml\n"+
		"  sampling_policy_path: ../policies/request-shape/sampling.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "memory.yaml"), ""+
		"kind: MemoryContractConfig\n"+
		"version: v1\n"+
		"id: memory-main\n"+
		"spec:\n"+
		"  offload_policy_path: ../policies/memory/offload.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), "kind: EndpointPolicyConfig\nversion: v1\nid: endpoint-main\nspec:\n  enabled: true\n  strategy: static\n  params:\n    base_url: https://api.z.ai\n    path: /api/paas/v4/chat/completions\n    method: POST\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "auth.yaml"), "kind: AuthPolicyConfig\nversion: v1\nid: auth-main\nspec:\n  enabled: true\n  strategy: bearer_token\n  params:\n    header: Authorization\n    prefix: Bearer\n    value_env_var: ZAI_API_KEY\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "retry.yaml"), "kind: RetryPolicyConfig\nversion: v1\nid: retry-main\nspec:\n  enabled: true\n  strategy: fixed\n  params:\n    max_attempts: 1\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "timeout.yaml"), "kind: TimeoutPolicyConfig\nversion: v1\nid: timeout-main\nspec:\n  enabled: true\n  strategy: per_request\n  params:\n    total: 30s\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "model.yaml"), "kind: ModelPolicyConfig\nversion: v1\nid: model-main\nspec:\n  enabled: true\n  strategy: static_model\n  params:\n    model: glm-5-turbo\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "messages.yaml"), "kind: MessagePolicyConfig\nversion: v1\nid: messages-main\nspec:\n  enabled: true\n  strategy: raw_messages\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "tools.yaml"), "kind: ToolPolicyConfig\nversion: v1\nid: tools-main\nspec:\n  enabled: true\n  strategy: tools_inline\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "response-format.yaml"), "kind: ResponseFormatPolicyConfig\nversion: v1\nid: response-format-main\nspec:\n  enabled: true\n  strategy: default\n  params:\n    type: text\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "streaming.yaml"), "kind: StreamingPolicyConfig\nversion: v1\nid: streaming-main\nspec:\n  enabled: true\n  strategy: static_stream\n  params:\n    stream: false\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "sampling.yaml"), "kind: SamplingPolicyConfig\nversion: v1\nid: sampling-main\nspec:\n  enabled: true\n  strategy: static_sampling\n  params:\n    temperature: 0.2\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "memory", "offload.yaml"), "kind: OffloadPolicyConfig\nversion: v1\nid: offload-main\nspec:\n  enabled: true\n  strategy: artifact_store\n  params:\n    max_chars: 128\n    preview_chars: 40\n    storage_path: ../../var/artifacts\n    expose_retrieval_tools: true\n    search_limit: 3\n")

	agent, err := runtime.BuildAgent(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("BuildAgent returned error: %v", err)
	}
	if agent.ArtifactStore == nil {
		t.Fatal("agent ArtifactStore is nil")
	}
}

func TestBuildAgentLoadsShippedFilesystemCompatibilityApprovalGuard(t *testing.T) {
	t.Parallel()

	cfg, err := config.LoadRoot(filepath.Join("..", "..", "config", "zai-smoke", "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}
	contracts, err := runtime.ResolveContracts(cfg)
	if err != nil {
		t.Fatalf("ResolveContracts returned error: %v", err)
	}
	got := contracts.ToolExecution.Approval
	if got.Strategy != "require_for_destructive" {
		t.Fatalf("approval strategy = %q, want require_for_destructive", got.Strategy)
	}
	for _, toolID := range []string{"fs_read_text", "fs_write_text", "fs_patch_text", "fs_replace_in_files", "fs_move", "fs_trash"} {
		found := false
		for _, candidate := range got.Params.DestructiveToolIDs {
			if candidate == toolID {
				found = true
				break
			}
		}
		if !found {
			t.Fatalf("destructive tool ids = %#v, want %q included", got.Params.DestructiveToolIDs, toolID)
		}
	}
}

func TestBuildAgentLoadsShippedContextBudgetProjections(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	sourceRoot := filepath.Join("..", "..", "config", "zai-smoke")
	targetRoot := filepath.Join(dir, "zai-smoke")
	copyDir(t, sourceRoot, targetRoot)

	agentPath := filepath.Join(targetRoot, "agent.yaml")
	agentYAML, err := os.ReadFile(agentPath)
	if err != nil {
		t.Fatalf("ReadFile(%q): %v", agentPath, err)
	}
	patched := strings.Replace(
		string(agentYAML),
		"    projection_store_path: ../../var/zai-smoke/projections.json\n",
		"    projection_store_path: ./var/projections.json\n",
		1,
	)
	if patched == string(agentYAML) {
		t.Fatalf("failed to patch projection_store_path in %q", agentPath)
	}
	mustWriteFile(t, agentPath, patched)

	agent, err := runtime.BuildAgent(agentPath)
	if err != nil {
		t.Fatalf("BuildAgent returned error: %v", err)
	}

	var sawBudget bool
	var sawSummary bool
	for _, projection := range agent.Projections {
		switch projection.ID() {
		case "context_budget":
			sawBudget = true
		case "context_summary":
			sawSummary = true
		}
	}
	if !sawBudget {
		t.Fatal("context_budget projection is missing from shipped BuildAgent")
	}
	if !sawSummary {
		t.Fatal("context_summary projection is missing from shipped BuildAgent")
	}
}

func TestResolveContractsLoadsShippedArtifactStorePathAtRepoVar(t *testing.T) {
	t.Parallel()

	cfg, err := config.LoadRoot(filepath.Join("..", "..", "config", "zai-smoke", "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}
	contracts, err := runtime.ResolveContracts(cfg)
	if err != nil {
		t.Fatalf("ResolveContracts returned error: %v", err)
	}
	want, err := filepath.Abs(filepath.Join("..", "..", "var", "zai-smoke", "artifacts"))
	if err != nil {
		t.Fatalf("Abs returned error: %v", err)
	}
	if contracts.Memory.Offload.Params.StoragePath != want {
		t.Fatalf("storage path = %q, want %q", contracts.Memory.Offload.Params.StoragePath, want)
	}
}

func TestBuildAgentRestoresProjectionSnapshotsFromStore(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	storePath := filepath.Join(dir, "var", "projections.json")

	registry := projections.NewBuiltInRegistry()
	projectionSet, err := registry.Build("session", "run")
	if err != nil {
		t.Fatalf("Build projection set returned error: %v", err)
	}
	now := time.Date(2026, 4, 14, 12, 20, 0, 0, time.UTC)
	if err := projectionSet[0].Apply(eventing.Event{
		Kind:          eventing.EventSessionCreated,
		OccurredAt:    now,
		AggregateID:   "session-1",
		AggregateType: eventing.AggregateSession,
	}); err != nil {
		t.Fatalf("session Apply returned error: %v", err)
	}
	if err := projectionSet[1].Apply(eventing.Event{
		Kind:          eventing.EventRunStarted,
		OccurredAt:    now,
		AggregateID:   "run-1",
		AggregateType: eventing.AggregateRun,
		Payload: map[string]any{
			"session_id": "session-1",
		},
	}); err != nil {
		t.Fatalf("run Apply returned error: %v", err)
	}
	store, err := projections.NewJSONFileStore(storePath)
	if err != nil {
		t.Fatalf("NewJSONFileStore returned error: %v", err)
	}
	if err := store.Save(projectionSet); err != nil {
		t.Fatalf("Save returned error: %v", err)
	}

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  runtime:\n"+
		"    event_log: file_jsonl\n"+
		"    event_log_path: ./var/events.jsonl\n"+
		"    projection_store_path: ./var/projections.json\n"+
		"    prompt_asset_executor: prompt_asset_default\n"+
		"    transport_executor: transport_default\n"+
		"    request_shape_executor: request_shape_default\n"+
		"    provider_client: provider_client_default\n"+
		"    projections: [session, run]\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n"+
		"    request_shape: ./contracts/request-shape.yaml\n"+
		"    memory: ./contracts/memory.yaml\n")

	mustWriteMinimalContracts(t, dir)

	agent, err := runtime.BuildAgent(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("BuildAgent returned error: %v", err)
	}
	if agent.ProjectionStore == nil {
		t.Fatal("agent ProjectionStore is nil")
	}
	sessionProjection, ok := agent.Projections[0].(*projections.SessionProjection)
	if !ok {
		t.Fatalf("projection type = %T, want *SessionProjection", agent.Projections[0])
	}
	if sessionProjection.Snapshot().SessionID != "session-1" {
		t.Fatalf("SessionID = %q, want %q", sessionProjection.Snapshot().SessionID, "session-1")
	}
	runProjection, ok := agent.Projections[1].(*projections.RunProjection)
	if !ok {
		t.Fatalf("projection type = %T, want *RunProjection", agent.Projections[1])
	}
	if runProjection.Snapshot().RunID != "run-1" {
		t.Fatalf("RunID = %q, want %q", runProjection.Snapshot().RunID, "run-1")
	}
}

func TestAgentRecordEventPersistsProjectionSnapshotsAutomatically(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  runtime:\n"+
		"    event_log: file_jsonl\n"+
		"    event_log_path: ./var/events.jsonl\n"+
		"    projection_store_path: ./var/projections.json\n"+
		"    prompt_asset_executor: prompt_asset_default\n"+
		"    transport_executor: transport_default\n"+
		"    request_shape_executor: request_shape_default\n"+
		"    provider_client: provider_client_default\n"+
		"    projections: [session, run]\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n"+
		"    request_shape: ./contracts/request-shape.yaml\n"+
		"    memory: ./contracts/memory.yaml\n")

	mustWriteMinimalContracts(t, dir)

	agent, err := runtime.BuildAgent(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("BuildAgent returned error: %v", err)
	}

	now := time.Date(2026, 4, 14, 12, 45, 0, 0, time.UTC)
	if err := agent.RecordEvent(context.Background(), eventing.Event{
		ID:               "evt-session-1",
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       now,
		AggregateID:      "session-1",
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
	}); err != nil {
		t.Fatalf("RecordEvent returned error: %v", err)
	}

	reloaded, err := runtime.BuildAgent(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("BuildAgent reload returned error: %v", err)
	}

	sessionProjection, ok := reloaded.Projections[0].(*projections.SessionProjection)
	if !ok {
		t.Fatalf("projection type = %T, want *SessionProjection", reloaded.Projections[0])
	}
	if sessionProjection.Snapshot().SessionID != "session-1" {
		t.Fatalf("SessionID = %q, want %q", sessionProjection.Snapshot().SessionID, "session-1")
	}
}

func TestAgentSmokeUsesProviderClient(t *testing.T) {
	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  runtime:\n"+
		"    event_log: file_jsonl\n"+
		"    event_log_path: ./var/events.jsonl\n"+
		"    prompt_asset_executor: prompt_asset_default\n"+
		"    transport_executor: transport_default\n"+
		"    request_shape_executor: request_shape_default\n"+
		"    provider_client: provider_client_default\n"+
		"    projections: [session, run]\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n"+
		"    request_shape: ./contracts/request-shape.yaml\n"+
		"    memory: ./contracts/memory.yaml\n")

	mustWriteMinimalContracts(t, dir)
	t.Setenv("ZAI_API_KEY", "secret-token")

	agent, err := runtime.BuildAgent(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("BuildAgent returned error: %v", err)
	}

	agent.Transport.Doer = fakeDoer{
		do: func(req *http.Request) (*http.Response, error) {
			return &http.Response{
				StatusCode: 200,
				Header:     http.Header{},
				Body: io.NopCloser(strings.NewReader(`{
  "id":"resp-smoke-1",
  "model":"glm-4.6",
  "choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"pong"}}],
  "usage":{"prompt_tokens":5,"completion_tokens":1,"total_tokens":6}
}`)),
			}, nil
		},
	}

	result, err := agent.Smoke(context.Background(), runtime.SmokeInput{Prompt: "ping"})
	if err != nil {
		t.Fatalf("Smoke returned error: %v", err)
	}
	if result.Provider.Message.Content != "pong" {
		t.Fatalf("provider message = %q, want pong", result.Provider.Message.Content)
	}

	var payload map[string]any
	if err := json.Unmarshal(result.RequestBody, &payload); err != nil {
		t.Fatalf("Unmarshal returned error: %v", err)
	}
	messages, ok := payload["messages"].([]any)
	if !ok || len(messages) != 1 {
		t.Fatalf("messages = %#v", payload["messages"])
	}
	msg, ok := messages[0].(map[string]any)
	if !ok || msg["content"] != "ping" {
		t.Fatalf("message = %#v, want user ping", messages[0])
	}
}

func TestRepositoryZaiSmokeConfigLoadsAndResolvesContracts(t *testing.T) {
	t.Parallel()

	configPath := filepath.Join("..", "..", "config", "zai-smoke", "agent.yaml")
	cfg, err := config.LoadRoot(configPath)
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	if cfg.ID != "zai-smoke" {
		t.Fatalf("config ID = %q, want %q", cfg.ID, "zai-smoke")
	}
	if cfg.Spec.Runtime.ProviderClient != "provider_client_default" {
		t.Fatalf("provider client = %q, want %q", cfg.Spec.Runtime.ProviderClient, "provider_client_default")
	}

	resolved, err := runtime.ResolveContracts(cfg)
	if err != nil {
		t.Fatalf("ResolveContracts returned error: %v", err)
	}

	if resolved.ProviderRequest.Transport.Endpoint.Params.BaseURL != "https://api.z.ai/api/coding/paas/v4" {
		t.Fatalf("base URL = %q, want %q", resolved.ProviderRequest.Transport.Endpoint.Params.BaseURL, "https://api.z.ai/api/coding/paas/v4")
	}
	if resolved.ProviderRequest.Transport.Auth.Params.ValueEnvVar != "TEAMD_ZAI_API_KEY" {
		t.Fatalf("auth env var = %q, want %q", resolved.ProviderRequest.Transport.Auth.Params.ValueEnvVar, "TEAMD_ZAI_API_KEY")
	}
	if resolved.ProviderRequest.Transport.Timeout.Strategy != "long_running_non_streaming" {
		t.Fatalf("timeout strategy = %q, want %q", resolved.ProviderRequest.Transport.Timeout.Strategy, "long_running_non_streaming")
	}
	if resolved.ProviderRequest.Transport.Timeout.Params.OperationBudget != "1h" {
		t.Fatalf("operation budget = %q, want %q", resolved.ProviderRequest.Transport.Timeout.Params.OperationBudget, "1h")
	}
	if resolved.ProviderRequest.Transport.Retry.Params.EarlyFailureWindow != "5s" {
		t.Fatalf("early failure window = %q, want %q", resolved.ProviderRequest.Transport.Retry.Params.EarlyFailureWindow, "5s")
	}
	if resolved.ProviderRequest.RequestShape.Model.Params.Model != "glm-5-turbo" {
		t.Fatalf("model = %q, want %q", resolved.ProviderRequest.RequestShape.Model.Params.Model, "glm-5-turbo")
	}
	if resolved.Chat.Submit.Strategy != "double_enter" {
		t.Fatalf("chat submit strategy = %q, want %q", resolved.Chat.Submit.Strategy, "double_enter")
	}
	if resolved.Chat.Output.Strategy != "streaming_text" {
		t.Fatalf("chat output strategy = %q, want %q", resolved.Chat.Output.Strategy, "streaming_text")
	}
	if resolved.ContextBudget.ID != "context-budget-zai-smoke" {
		t.Fatalf("context budget ID = %q, want %q", resolved.ContextBudget.ID, "context-budget-zai-smoke")
	}
	if resolved.ContextBudget.Estimation.Params.CharsPerToken != 4 {
		t.Fatalf("context budget chars_per_token = %d, want %d", resolved.ContextBudget.Estimation.Params.CharsPerToken, 4)
	}
	if !resolved.ContextBudget.SummaryDisplay.Params.IncludeSummaryCount {
		t.Fatal("context budget summary display include_summary_count = false, want true")
	}
	if cfg.Spec.Runtime.MaxToolRounds != 100 {
		t.Fatalf("runtime max_tool_rounds = %d, want %d", cfg.Spec.Runtime.MaxToolRounds, 100)
	}
	if len(cfg.Spec.Runtime.Projections) == 0 || cfg.Spec.Runtime.Projections[1] != "session_catalog" {
		t.Fatalf("runtime projections = %#v, want session_catalog in shipped config", cfg.Spec.Runtime.Projections)
	}
	if resolved.FilesystemTools.Catalog.Params.ToolIDs[0] != "fs_list" {
		t.Fatalf("first filesystem tool = %q, want %q", resolved.FilesystemTools.Catalog.Params.ToolIDs[0], "fs_list")
	}
	if resolved.FilesystemExecution.Scope.Strategy != "workspace_only" {
		t.Fatalf("filesystem scope strategy = %q, want %q", resolved.FilesystemExecution.Scope.Strategy, "workspace_only")
	}
	if resolved.FilesystemExecution.IO.Params.MaxReadBytes != 131072 {
		t.Fatalf("filesystem max_read_bytes = %d, want %d", resolved.FilesystemExecution.IO.Params.MaxReadBytes, 131072)
	}
	if resolved.ShellTools.Catalog.Params.ToolIDs[0] != "shell_exec" {
		t.Fatalf("first shell tool = %q, want %q", resolved.ShellTools.Catalog.Params.ToolIDs[0], "shell_exec")
	}
	if resolved.ShellExecution.Command.Strategy != "static_allowlist" {
		t.Fatalf("shell command strategy = %q, want %q", resolved.ShellExecution.Command.Strategy, "static_allowlist")
	}
	wantCommands := []string{"pwd", "ls", "cat", "rg", "go", "git", "echo", "printf", "head", "sed", "wc", "find", "curl", "powershell", "pwsh", "cmd"}
	if got, want := len(resolved.ShellExecution.Command.Params.AllowedCommands), len(wantCommands); got != want {
		t.Fatalf("shell allowed commands len = %d, want %d", got, want)
	}
	for i, want := range wantCommands {
		if got := resolved.ShellExecution.Command.Params.AllowedCommands[i]; got != want {
			t.Fatalf("shell allowed command[%d] = %q, want %q", i, got, want)
		}
	}
	if resolved.ShellExecution.Runtime.Params.Timeout != "30s" {
		t.Fatalf("shell runtime timeout = %q, want %q", resolved.ShellExecution.Runtime.Params.Timeout, "30s")
	}
	if !resolved.ShellExecution.Runtime.Params.AllowNetwork {
		t.Fatal("shell runtime allow_network = false, want true for shipped zai-smoke config")
	}
}

func mustWriteMinimalContracts(t *testing.T, dir string) {
	t.Helper()

	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), ""+
		"kind: TransportContractConfig\n"+
		"version: v1\n"+
		"id: transport-main\n"+
		"spec:\n"+
		"  endpoint_policy_path: ../policies/transport/endpoint.yaml\n"+
		"  auth_policy_path: ../policies/transport/auth.yaml\n"+
		"  retry_policy_path: ../policies/transport/retry.yaml\n"+
		"  timeout_policy_path: ../policies/transport/timeout.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "memory.yaml"), ""+
		"kind: MemoryContractConfig\n"+
		"version: v1\n"+
		"id: memory-main\n"+
		"spec:\n"+
		"  offload_policy_path: ../policies/memory/offload.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "request-shape.yaml"), ""+
		"kind: RequestShapeContractConfig\n"+
		"version: v1\n"+
		"id: request-shape-main\n"+
		"spec:\n"+
		"  model_policy_path: ../policies/request-shape/model.yaml\n"+
		"  message_policy_path: ../policies/request-shape/messages.yaml\n"+
		"  tool_policy_path: ../policies/request-shape/tools.yaml\n"+
		"  response_format_policy_path: ../policies/request-shape/response-format.yaml\n"+
		"  streaming_policy_path: ../policies/request-shape/streaming.yaml\n"+
		"  sampling_policy_path: ../policies/request-shape/sampling.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), ""+
		"kind: EndpointPolicyConfig\nversion: v1\nid: endpoint-main\nspec:\n  enabled: true\n  strategy: static\n  params:\n    base_url: https://api.z.ai\n    path: /api/paas/v4/chat/completions\n    method: POST\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "auth.yaml"), ""+
		"kind: AuthPolicyConfig\nversion: v1\nid: auth-main\nspec:\n  enabled: false\n  strategy: none\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "retry.yaml"), ""+
		"kind: RetryPolicyConfig\nversion: v1\nid: retry-main\nspec:\n  enabled: false\n  strategy: none\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "timeout.yaml"), ""+
		"kind: TimeoutPolicyConfig\nversion: v1\nid: timeout-main\nspec:\n  enabled: true\n  strategy: per_request\n  params:\n    total: 30s\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "memory", "offload.yaml"), ""+
		"kind: OffloadPolicyConfig\nversion: v1\nid: offload-main\nspec:\n  enabled: true\n  strategy: old_only\n  params:\n    max_chars: 1200\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "model.yaml"), ""+
		"kind: ModelPolicyConfig\nversion: v1\nid: model-main\nspec:\n  enabled: true\n  strategy: static_model\n  params:\n    model: glm-4.6\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "messages.yaml"), ""+
		"kind: MessagePolicyConfig\nversion: v1\nid: messages-main\nspec:\n  enabled: true\n  strategy: raw_messages\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "tools.yaml"), ""+
		"kind: ToolPolicyConfig\nversion: v1\nid: tools-main\nspec:\n  enabled: true\n  strategy: tools_inline\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "response-format.yaml"), ""+
		"kind: ResponseFormatPolicyConfig\nversion: v1\nid: response-format-main\nspec:\n  enabled: true\n  strategy: default\n  params:\n    type: json_object\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "streaming.yaml"), ""+
		"kind: StreamingPolicyConfig\nversion: v1\nid: streaming-main\nspec:\n  enabled: true\n  strategy: static_stream\n  params:\n    stream: false\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "sampling.yaml"), ""+
		"kind: SamplingPolicyConfig\nversion: v1\nid: sampling-main\nspec:\n  enabled: true\n  strategy: static_sampling\n  params:\n    temperature: 0.2\n    max_output_tokens: 2048\n")
}

func mustWriteFile(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll(%q): %v", path, err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatalf("WriteFile(%q): %v", path, err)
	}
}

func copyDir(t *testing.T, sourceRoot, targetRoot string) {
	t.Helper()

	if err := filepath.Walk(sourceRoot, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		relative, err := filepath.Rel(sourceRoot, path)
		if err != nil {
			return err
		}
		targetPath := filepath.Join(targetRoot, relative)
		if info.IsDir() {
			return os.MkdirAll(targetPath, info.Mode())
		}
		content, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		if err := os.MkdirAll(filepath.Dir(targetPath), 0o755); err != nil {
			return err
		}
		return os.WriteFile(targetPath, content, info.Mode())
	}); err != nil {
		t.Fatalf("copyDir(%q, %q): %v", sourceRoot, targetRoot, err)
	}
}
