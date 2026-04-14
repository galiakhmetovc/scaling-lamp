package config_test

import (
	"os"
	"path/filepath"
	"testing"

	"teamd/internal/config"
)

func TestLoadRootConfigLoadsExplicitModulePaths(t *testing.T) {
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
		"  endpoint_policy_path: ./policies/endpoint.yaml\n")

	got, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	if got.ID != "agent-test" {
		t.Fatalf("LoadRoot ID = %q, want %q", got.ID, "agent-test")
	}
	if got.Spec.Contracts["transport"] != filepath.Join(dir, "contracts", "transport.yaml") {
		t.Fatalf("LoadRoot transport path = %q, want resolved path", got.Spec.Contracts["transport"])
	}
}

func TestLoadRootConfigLoadsExplicitModuleGraph(t *testing.T) {
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
		"    memory: ./contracts/memory.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), ""+
		"kind: TransportContractConfig\n"+
		"version: v1\n"+
		"id: transport-main\n"+
		"spec:\n"+
		"  endpoint_policy_path: ../policies/transport/endpoint.yaml\n")

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
		"id: endpoint-main\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "memory", "offload.yaml"), ""+
		"kind: OffloadPolicyConfig\n"+
		"version: v1\n"+
		"id: offload-main\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "model.yaml"), ""+
		"kind: ModelPolicyConfig\n"+
		"version: v1\n"+
		"id: model-main\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "messages.yaml"), ""+
		"kind: MessagePolicyConfig\n"+
		"version: v1\n"+
		"id: messages-main\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "tools.yaml"), ""+
		"kind: ToolPolicyConfig\n"+
		"version: v1\n"+
		"id: tools-main\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "response-format.yaml"), ""+
		"kind: ResponseFormatPolicyConfig\n"+
		"version: v1\n"+
		"id: response-format-main\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "streaming.yaml"), ""+
		"kind: StreamingPolicyConfig\n"+
		"version: v1\n"+
		"id: streaming-main\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "sampling.yaml"), ""+
		"kind: SamplingPolicyConfig\n"+
		"version: v1\n"+
		"id: sampling-main\n")

	got, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	if got.Spec.Contracts["memory"] != filepath.Join(dir, "contracts", "memory.yaml") {
		t.Fatalf("LoadRoot memory path = %q, want resolved path", got.Spec.Contracts["memory"])
	}

	graph, err := config.LoadModuleGraph(got, config.NewBuiltInModuleRegistry())
	if err != nil {
		t.Fatalf("LoadModuleGraph returned error: %v", err)
	}

	if len(graph.Contracts) != 3 {
		t.Fatalf("contracts len = %d, want 3", len(graph.Contracts))
	}
	if len(graph.Policies) != 8 {
		t.Fatalf("policies len = %d, want 8", len(graph.Policies))
	}
	if graph.Policies["endpoint-main"].Kind != "EndpointPolicyConfig" {
		t.Fatalf("endpoint policy kind = %q, want EndpointPolicyConfig", graph.Policies["endpoint-main"].Kind)
	}
	if graph.Policies["model-main"].Kind != "ModelPolicyConfig" {
		t.Fatalf("model policy kind = %q, want ModelPolicyConfig", graph.Policies["model-main"].Kind)
	}
}

func TestLoadModuleGraphUsesRegistryReferenceMetadata(t *testing.T) {
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
		"  retry_policy_path: ../policies/transport/retry.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), ""+
		"kind: EndpointPolicyConfig\n"+
		"version: v1\n"+
		"id: endpoint-main\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "retry.yaml"), ""+
		"kind: RetryPolicyConfig\n"+
		"version: v1\n"+
		"id: retry-main\n")

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	registry := config.NewBuiltInModuleRegistry()
	registry.Register(config.ModuleType{
		Kind:      "TransportContractConfig",
		Category:  config.ModuleCategoryContract,
		RefFields: []string{"endpoint_policy_path", "retry_policy_path"},
	})
	registry.Register(config.ModuleType{
		Kind:     "RetryPolicyConfig",
		Category: config.ModuleCategoryPolicy,
	})

	graph, err := config.LoadModuleGraph(cfg, registry)
	if err != nil {
		t.Fatalf("LoadModuleGraph returned error: %v", err)
	}

	if len(graph.Policies) != 2 {
		t.Fatalf("policies len = %d, want 2", len(graph.Policies))
	}
	if graph.Policies["retry-main"].Kind != "RetryPolicyConfig" {
		t.Fatalf("retry policy kind = %q, want RetryPolicyConfig", graph.Policies["retry-main"].Kind)
	}
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
