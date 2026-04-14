package runtime_test

import (
	"path/filepath"
	"strings"
	"testing"

	"teamd/internal/config"
	"teamd/internal/policies"
	"teamd/internal/runtime"
)

func TestResolveContractsBuildsTransportAndMemoryContracts(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n"+
		"    request_shape: ./contracts/request-shape.yaml\n"+
		"    memory: ./contracts/memory.yaml\n"+
		"    prompt_assets: ./contracts/prompt-assets.yaml\n")

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

	mustWriteFile(t, filepath.Join(dir, "contracts", "prompt-assets.yaml"), ""+
		"kind: PromptAssetsContractConfig\n"+
		"version: v1\n"+
		"id: prompt-assets-main\n"+
		"spec:\n"+
		"  prompt_asset_policy_path: ../policies/prompt-assets/inline.yaml\n")

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

	mustWriteFile(t, filepath.Join(dir, "policies", "prompt-assets", "inline.yaml"), ""+
		"kind: PromptAssetPolicyConfig\n"+
		"version: v1\n"+
		"id: prompt-assets-inline\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: inline_assets\n"+
		"  params:\n"+
		"    assets:\n"+
		"      - role: system\n"+
		"        content: You are terse.\n")

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	contracts, err := runtime.ResolveContracts(cfg)
	if err != nil {
		t.Fatalf("ResolveContracts returned error: %v", err)
	}

	if contracts.ProviderRequest.Transport.ID != "transport-main" {
		t.Fatalf("transport ID = %q, want %q", contracts.ProviderRequest.Transport.ID, "transport-main")
	}
	if contracts.ProviderRequest.Transport.Endpoint.ID != "endpoint-main" {
		t.Fatalf("endpoint ID = %q, want %q", contracts.ProviderRequest.Transport.Endpoint.ID, "endpoint-main")
	}
	if contracts.ProviderRequest.Transport.Auth.Strategy != "bearer_token" {
		t.Fatalf("auth strategy = %q, want %q", contracts.ProviderRequest.Transport.Auth.Strategy, "bearer_token")
	}
	if contracts.ProviderRequest.Transport.Retry.Params.MaxAttempts != 3 {
		t.Fatalf("max attempts = %d, want 3", contracts.ProviderRequest.Transport.Retry.Params.MaxAttempts)
	}
	if contracts.ProviderRequest.Transport.Timeout.Params.Total != "30s" {
		t.Fatalf("timeout total = %q, want %q", contracts.ProviderRequest.Transport.Timeout.Params.Total, "30s")
	}
	if contracts.ProviderRequest.RequestShape.ID != "request-shape-main" {
		t.Fatalf("request-shape ID = %q, want %q", contracts.ProviderRequest.RequestShape.ID, "request-shape-main")
	}
	if contracts.ProviderRequest.RequestShape.Messages.Strategy != "raw_messages" {
		t.Fatalf("message strategy = %q, want %q", contracts.ProviderRequest.RequestShape.Messages.Strategy, "raw_messages")
	}
	if contracts.ProviderRequest.RequestShape.ResponseFormat.Params.Type != "json_object" {
		t.Fatalf("response format type = %q, want %q", contracts.ProviderRequest.RequestShape.ResponseFormat.Params.Type, "json_object")
	}
	if contracts.Memory.ID != "memory-main" {
		t.Fatalf("memory ID = %q, want %q", contracts.Memory.ID, "memory-main")
	}
	if contracts.Memory.Offload.Params.MaxChars != 1200 {
		t.Fatalf("max chars = %d, want 1200", contracts.Memory.Offload.Params.MaxChars)
	}
	if contracts.PromptAssets.ID != "prompt-assets-main" {
		t.Fatalf("prompt-assets ID = %q, want %q", contracts.PromptAssets.ID, "prompt-assets-main")
	}
	if len(contracts.PromptAssets.PromptAsset.Params.Assets) != 1 {
		t.Fatalf("prompt asset count = %d, want 1", len(contracts.PromptAssets.PromptAsset.Params.Assets))
	}
}

func TestResolveContractsUsesContractKindsNotContractMapKeys(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    provider_transport_main: ./contracts/transport.yaml\n"+
		"    provider_request_shape_main: ./contracts/request-shape.yaml\n"+
		"    memory_main: ./contracts/memory.yaml\n"+
		"    prompt_assets_main: ./contracts/prompt-assets.yaml\n")

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

	mustWriteFile(t, filepath.Join(dir, "contracts", "prompt-assets.yaml"), ""+
		"kind: PromptAssetsContractConfig\n"+
		"version: v1\n"+
		"id: prompt-assets-main\n"+
		"spec:\n"+
		"  prompt_asset_policy_path: ../policies/prompt-assets/inline.yaml\n")

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

	mustWriteFile(t, filepath.Join(dir, "policies", "prompt-assets", "inline.yaml"), ""+
		"kind: PromptAssetPolicyConfig\n"+
		"version: v1\n"+
		"id: prompt-assets-inline\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: inline_assets\n"+
		"  params:\n"+
		"    assets:\n"+
		"      - role: system\n"+
		"        content: You are terse.\n")

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	got, err := runtime.ResolveContracts(cfg)
	if err != nil {
		t.Fatalf("ResolveContracts returned error: %v", err)
	}

	if got.ProviderRequest.Transport.ID != "transport-main" {
		t.Fatalf("transport ID = %q, want %q", got.ProviderRequest.Transport.ID, "transport-main")
	}
	if got.ProviderRequest.RequestShape.ID != "request-shape-main" {
		t.Fatalf("request-shape ID = %q, want %q", got.ProviderRequest.RequestShape.ID, "request-shape-main")
	}
	if got.Memory.ID != "memory-main" {
		t.Fatalf("memory ID = %q, want %q", got.Memory.ID, "memory-main")
	}
	if got.PromptAssets.ID != "prompt-assets-main" {
		t.Fatalf("prompt-assets ID = %q, want %q", got.PromptAssets.ID, "prompt-assets-main")
	}
}

func TestResolveContractsRejectsUnsupportedPolicyStrategy(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), ""+
		"kind: TransportContractConfig\n"+
		"version: v1\n"+
		"id: transport-main\n"+
		"spec:\n"+
		"  endpoint_policy_path: ../policies/transport/endpoint.yaml\n"+
		"  auth_policy_path: ../policies/transport/auth.yaml\n"+
		"  retry_policy_path: ../policies/transport/retry.yaml\n"+
		"  timeout_policy_path: ../policies/transport/timeout.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), ""+
		"kind: EndpointPolicyConfig\n"+
		"version: v1\n"+
		"id: endpoint-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: invalid_endpoint\n"+
		"  params:\n"+
		"    base_url: https://api.z.ai\n"+
		"    path: /api/paas/v4/chat/completions\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "auth.yaml"), ""+
		"kind: AuthPolicyConfig\n"+
		"version: v1\n"+
		"id: auth-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: bearer_token\n"+
		"  params:\n"+
		"    value_env_var: ZAI_API_KEY\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "retry.yaml"), ""+
		"kind: RetryPolicyConfig\n"+
		"version: v1\n"+
		"id: retry-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
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

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	_, err = runtime.ResolveContracts(cfg)
	if err == nil {
		t.Fatal("ResolveContracts error = nil, want invalid strategy error")
	}
	if got := err.Error(); got == "" || !containsAll(got, "EndpointPolicyConfig", "invalid_endpoint") {
		t.Fatalf("ResolveContracts error = %q, want policy kind and invalid strategy", got)
	}
}

func TestResolveContractsWithRegistryAllowsExtendedStrategies(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), ""+
		"kind: TransportContractConfig\n"+
		"version: v1\n"+
		"id: transport-main\n"+
		"spec:\n"+
		"  endpoint_policy_path: ../policies/transport/endpoint.yaml\n"+
		"  auth_policy_path: ../policies/transport/auth.yaml\n"+
		"  retry_policy_path: ../policies/transport/retry.yaml\n"+
		"  timeout_policy_path: ../policies/transport/timeout.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), ""+
		"kind: EndpointPolicyConfig\n"+
		"version: v1\n"+
		"id: endpoint-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: env_resolved\n"+
		"  params:\n"+
		"    base_url: https://api.z.ai\n"+
		"    path: /api/paas/v4/chat/completions\n")

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

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	registry := policies.NewBuiltInRegistry()
	endpointType, err := registry.Type("EndpointPolicyConfig")
	if err != nil {
		t.Fatalf("Type returned error: %v", err)
	}
	endpointType.Strategy["env_resolved"] = struct{}{}
	registry.Register(endpointType)

	resolved, err := runtime.ResolveContractsWithRegistry(cfg, registry)
	if err != nil {
		t.Fatalf("ResolveContractsWithRegistry returned error: %v", err)
	}
	if resolved.ProviderRequest.Transport.Endpoint.Strategy != "env_resolved" {
		t.Fatalf("endpoint strategy = %q, want %q", resolved.ProviderRequest.Transport.Endpoint.Strategy, "env_resolved")
	}
}

func TestResolveContractsBuildsChatContractWithParams(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-chat\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    chat: ./contracts/chat.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "chat.yaml"), ""+
		"kind: ChatContractConfig\n"+
		"version: v1\n"+
		"id: chat-main\n"+
		"spec:\n"+
		"  input_policy_path: ../policies/chat/input.yaml\n"+
		"  submit_policy_path: ../policies/chat/submit.yaml\n"+
		"  output_policy_path: ../policies/chat/output.yaml\n"+
		"  status_policy_path: ../policies/chat/status.yaml\n"+
		"  command_policy_path: ../policies/chat/command.yaml\n"+
		"  resume_policy_path: ../policies/chat/resume.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "input.yaml"), ""+
		"kind: ChatInputPolicyConfig\n"+
		"version: v1\n"+
		"id: chat-input\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: multiline_buffer\n"+
		"  params:\n"+
		"    primary_prompt: \"> \"\n"+
		"    continuation_prompt: \". \"\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "submit.yaml"), ""+
		"kind: ChatSubmitPolicyConfig\n"+
		"version: v1\n"+
		"id: chat-submit\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: double_enter\n"+
		"  params:\n"+
		"    empty_line_threshold: 1\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "output.yaml"), ""+
		"kind: ChatOutputPolicyConfig\n"+
		"version: v1\n"+
		"id: chat-output\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: streaming_text\n"+
		"  params:\n"+
		"    show_final_newline: true\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "status.yaml"), ""+
		"kind: ChatStatusPolicyConfig\n"+
		"version: v1\n"+
		"id: chat-status\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: inline_terminal\n"+
		"  params:\n"+
		"    show_header: true\n"+
		"    show_usage: true\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "command.yaml"), ""+
		"kind: ChatCommandPolicyConfig\n"+
		"version: v1\n"+
		"id: chat-command\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: slash_commands\n"+
		"  params:\n"+
		"    exit_command: /exit\n"+
		"    help_command: /help\n"+
		"    session_command: /session\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "resume.yaml"), ""+
		"kind: ChatResumePolicyConfig\n"+
		"version: v1\n"+
		"id: chat-resume\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: explicit_resume_only\n"+
		"  params:\n"+
		"    require_explicit_id: true\n")

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	got, err := runtime.ResolveContracts(cfg)
	if err != nil {
		t.Fatalf("ResolveContracts returned error: %v", err)
	}

	if got.Chat.ID != "chat-main" {
		t.Fatalf("chat ID = %q, want chat-main", got.Chat.ID)
	}
	if got.Chat.Input.Params.PrimaryPrompt != "> " || got.Chat.Input.Params.ContinuationPrompt != ". " {
		t.Fatalf("chat input params = %#v", got.Chat.Input.Params)
	}
	if got.Chat.Submit.Params.EmptyLineThreshold != 1 {
		t.Fatalf("empty line threshold = %d, want 1", got.Chat.Submit.Params.EmptyLineThreshold)
	}
	if !got.Chat.Status.Params.ShowHeader || !got.Chat.Status.Params.ShowUsage {
		t.Fatalf("chat status params = %#v", got.Chat.Status.Params)
	}
	if got.Chat.Command.Params.ExitCommand != "/exit" || got.Chat.Command.Params.HelpCommand != "/help" || got.Chat.Command.Params.SessionCommand != "/session" {
		t.Fatalf("chat command params = %#v", got.Chat.Command.Params)
	}
	if !got.Chat.Resume.Params.RequireExplicitID {
		t.Fatalf("chat resume params = %#v", got.Chat.Resume.Params)
	}
}

func containsAll(s string, want ...string) bool {
	for _, fragment := range want {
		if !strings.Contains(s, fragment) {
			return false
		}
	}
	return true
}
