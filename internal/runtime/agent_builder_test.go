package runtime_test

import (
	"os"
	"path/filepath"
	"testing"

	"teamd/internal/runtime"
)

func TestBuildAgentLoadsRootConfigAndBootstrapsRuntime(t *testing.T) {
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
	if len(agent.Projections) != 2 {
		t.Fatalf("agent projections len = %d, want 2", len(agent.Projections))
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
	if agent.Contracts.Memory.Offload.Strategy != "old_only" {
		t.Fatalf("offload strategy = %q, want %q", agent.Contracts.Memory.Offload.Strategy, "old_only")
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
