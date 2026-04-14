package config

import "fmt"

type ModuleCategory string

const (
	ModuleCategoryContract ModuleCategory = "contract"
	ModuleCategoryPolicy   ModuleCategory = "policy"
)

type ModuleType struct {
	Kind      string
	Category  ModuleCategory
	RefFields []string
}

type ModuleRegistry struct {
	kinds map[string]ModuleType
}

func NewModuleRegistry() *ModuleRegistry {
	return &ModuleRegistry{kinds: map[string]ModuleType{}}
}

func NewBuiltInModuleRegistry() *ModuleRegistry {
	registry := NewModuleRegistry()
	registry.Register(ModuleType{
		Kind:      "TransportContractConfig",
		Category:  ModuleCategoryContract,
		RefFields: []string{"endpoint_policy_path"},
	})
	registry.Register(ModuleType{
		Kind:      "MemoryContractConfig",
		Category:  ModuleCategoryContract,
		RefFields: []string{"offload_policy_path"},
	})
	registry.Register(ModuleType{
		Kind:     "EndpointPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "OffloadPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	return registry
}

func (r *ModuleRegistry) Register(moduleType ModuleType) {
	r.kinds[moduleType.Kind] = moduleType
}

func (r *ModuleRegistry) ValidateKind(kind string) error {
	if _, ok := r.kinds[kind]; !ok {
		return fmt.Errorf("unsupported module kind %q", kind)
	}
	return nil
}

func (r *ModuleRegistry) Type(kind string) (ModuleType, error) {
	moduleType, ok := r.kinds[kind]
	if !ok {
		return ModuleType{}, fmt.Errorf("unsupported module kind %q", kind)
	}
	return moduleType, nil
}
