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

func mustWriteFile(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll(%q): %v", path, err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatalf("WriteFile(%q): %v", path, err)
	}
}
