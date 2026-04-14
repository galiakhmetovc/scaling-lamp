package runtime_test

import (
	"path/filepath"
	"testing"

	"teamd/internal/config"
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

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), ""+
		"kind: EndpointPolicyConfig\n"+
		"version: v1\n"+
		"id: endpoint-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static\n"+
		"  params:\n"+
		"    base_url: https://api.z.ai\n"+
		"    path: /api/paas/v4/chat/completions\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "memory", "offload.yaml"), ""+
		"kind: OffloadPolicyConfig\n"+
		"version: v1\n"+
		"id: offload-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: old_only\n"+
		"  params:\n"+
		"    max_chars: 1200\n")

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
	if contracts.Memory.ID != "memory-main" {
		t.Fatalf("memory ID = %q, want %q", contracts.Memory.ID, "memory-main")
	}
	if contracts.Memory.Offload.Params.MaxChars != 1200 {
		t.Fatalf("max chars = %d, want 1200", contracts.Memory.Offload.Params.MaxChars)
	}
}
