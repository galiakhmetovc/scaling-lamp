package config_test

import (
	"testing"

	"teamd/internal/config"
)

func TestModuleRegistryValidatesModuleKind(t *testing.T) {
	t.Parallel()

	registry := config.NewModuleRegistry()
	registry.Register(config.ModuleType{
		Kind:      "TransportContractConfig",
		Category:  config.ModuleCategoryContract,
		RefFields: []string{"endpoint_policy_path"},
	})

	err := registry.ValidateKind("AuthPolicyConfig")
	if err == nil {
		t.Fatal("ValidateKind error = nil, want error")
	}

	if err := registry.ValidateKind("TransportContractConfig"); err != nil {
		t.Fatalf("ValidateKind returned error for registered kind: %v", err)
	}
}

func TestBuiltInModuleRegistryExposesModuleMetadata(t *testing.T) {
	t.Parallel()

	registry := config.NewBuiltInModuleRegistry()

	moduleType, err := registry.Type("TransportContractConfig")
	if err != nil {
		t.Fatalf("Type returned error: %v", err)
	}
	if moduleType.Category != config.ModuleCategoryContract {
		t.Fatalf("category = %q, want %q", moduleType.Category, config.ModuleCategoryContract)
	}
	if len(moduleType.RefFields) != 4 {
		t.Fatalf("ref fields len = %d, want 4", len(moduleType.RefFields))
	}
	if moduleType.RefFields[0] != "endpoint_policy_path" || moduleType.RefFields[1] != "auth_policy_path" {
		t.Fatalf("ref fields = %#v, want transport policy refs", moduleType.RefFields)
	}
}
