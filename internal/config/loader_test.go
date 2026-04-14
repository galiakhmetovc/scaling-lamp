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
	if got.Spec.Contracts.TransportPath != filepath.Join(dir, "contracts", "transport.yaml") {
		t.Fatalf("LoadRoot transport path = %q, want resolved path", got.Spec.Contracts.TransportPath)
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
		"id: endpoint-main\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "memory", "offload.yaml"), ""+
		"kind: OffloadPolicyConfig\n"+
		"version: v1\n"+
		"id: offload-main\n")

	got, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	if got.Spec.Contracts.MemoryPath != filepath.Join(dir, "contracts", "memory.yaml") {
		t.Fatalf("LoadRoot memory path = %q, want resolved path", got.Spec.Contracts.MemoryPath)
	}

	graph, err := config.LoadModuleGraph(got)
	if err != nil {
		t.Fatalf("LoadModuleGraph returned error: %v", err)
	}

	if len(graph.Contracts) != 2 {
		t.Fatalf("contracts len = %d, want 2", len(graph.Contracts))
	}
	if len(graph.Policies) != 2 {
		t.Fatalf("policies len = %d, want 2", len(graph.Policies))
	}
	if graph.Policies["endpoint-main"].Kind != "EndpointPolicyConfig" {
		t.Fatalf("endpoint policy kind = %q, want EndpointPolicyConfig", graph.Policies["endpoint-main"].Kind)
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
