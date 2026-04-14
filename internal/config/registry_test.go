package config_test

import (
	"testing"

	"teamd/internal/config"
)

func TestModuleRegistryValidatesModuleKind(t *testing.T) {
	t.Parallel()

	registry := config.NewModuleRegistry()
	registry.Register("TransportContractConfig")

	err := registry.ValidateKind("AuthPolicyConfig")
	if err == nil {
		t.Fatal("ValidateKind error = nil, want error")
	}

	if err := registry.ValidateKind("TransportContractConfig"); err != nil {
		t.Fatalf("ValidateKind returned error for registered kind: %v", err)
	}
}
