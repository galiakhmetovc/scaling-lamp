package runtime_test

import (
	"path/filepath"
	"testing"

	"teamd/internal/config"
	"teamd/internal/runtime"
)

func TestResolveContractsBuildsOperatorSurfaceContract(t *testing.T) {
	t.Parallel()

	root := filepath.Join("..", "..")
	configPath := filepath.Join(root, "config", "zai-smoke", "agent.yaml")
	cfg, err := config.LoadRoot(configPath)
	if err != nil {
		t.Fatalf("load config: %v", err)
	}
	contracts, err := runtime.ResolveContracts(cfg)
	if err != nil {
		t.Fatalf("resolve contracts: %v", err)
	}
	if contracts.OperatorSurface.ID == "" {
		t.Fatalf("operator surface contract is nil")
	}
	if contracts.OperatorSurface.DaemonServer.ID == "" || contracts.OperatorSurface.WebAssets.ID == "" || contracts.OperatorSurface.ClientTransport.ID == "" || contracts.OperatorSurface.Settings.ID == "" {
		t.Fatalf("operator surface contract missing sub-policies: %+v", contracts.OperatorSurface)
	}
	if got := contracts.OperatorSurface.DaemonServer.Params.ListenHost; got != "0.0.0.0" {
		t.Fatalf("listen host = %q, want 0.0.0.0", got)
	}
	if got := contracts.OperatorSurface.DaemonServer.Params.ListenPort; got != 8080 {
		t.Fatalf("listen port = %d, want 8080", got)
	}
	if got := contracts.OperatorSurface.ClientTransport.Params.EndpointPath; got != "/api" {
		t.Fatalf("endpoint path = %q, want /api", got)
	}
	if len(contracts.OperatorSurface.Settings.Params.FormFields) == 0 {
		t.Fatalf("settings form fields missing: %+v", contracts.OperatorSurface.Settings)
	}
}
