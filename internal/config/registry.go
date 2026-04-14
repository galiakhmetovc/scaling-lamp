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
		Kind:     "TransportContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"endpoint_policy_path",
			"auth_policy_path",
			"retry_policy_path",
			"timeout_policy_path",
		},
	})
	registry.Register(ModuleType{
		Kind:      "MemoryContractConfig",
		Category:  ModuleCategoryContract,
		RefFields: []string{"offload_policy_path"},
	})
	registry.Register(ModuleType{
		Kind:      "PromptAssetsContractConfig",
		Category:  ModuleCategoryContract,
		RefFields: []string{"prompt_asset_policy_path"},
	})
	registry.Register(ModuleType{
		Kind:     "PromptAssemblyContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"system_prompt_policy_path",
			"session_head_policy_path",
		},
	})
	registry.Register(ModuleType{
		Kind:     "ToolContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"tool_catalog_policy_path",
			"tool_serialization_policy_path",
		},
	})
	registry.Register(ModuleType{
		Kind:     "ToolExecutionContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"tool_access_policy_path",
			"tool_approval_policy_path",
			"tool_sandbox_policy_path",
		},
	})
	registry.Register(ModuleType{
		Kind:      "PlanToolContractConfig",
		Category:  ModuleCategoryContract,
		RefFields: []string{"plan_tool_policy_path"},
	})
	registry.Register(ModuleType{
		Kind:      "ProviderTraceContractConfig",
		Category:  ModuleCategoryContract,
		RefFields: []string{"provider_trace_policy_path"},
	})
	registry.Register(ModuleType{
		Kind:     "ChatContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"input_policy_path",
			"submit_policy_path",
			"output_policy_path",
			"status_policy_path",
			"command_policy_path",
			"resume_policy_path",
		},
	})
	registry.Register(ModuleType{
		Kind:     "RequestShapeContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"model_policy_path",
			"message_policy_path",
			"tool_policy_path",
			"response_format_policy_path",
			"streaming_policy_path",
			"sampling_policy_path",
		},
	})
	registry.Register(ModuleType{
		Kind:     "EndpointPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "AuthPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "RetryPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "TimeoutPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "OffloadPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ModelPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "MessagePolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ToolPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ResponseFormatPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "StreamingPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "SamplingPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "PromptAssetPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "SystemPromptPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "SessionHeadPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ToolCatalogPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ToolSerializationPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ToolAccessPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ToolApprovalPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ToolSandboxPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "PlanToolPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ProviderTracePolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ChatInputPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ChatSubmitPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ChatOutputPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ChatStatusPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ChatCommandPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ChatResumePolicyConfig",
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
