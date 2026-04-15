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
		Kind:     "FilesystemToolContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"filesystem_catalog_policy_path",
			"filesystem_description_policy_path",
		},
	})
	registry.Register(ModuleType{
		Kind:     "FilesystemExecutionContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"filesystem_scope_policy_path",
			"filesystem_mutation_policy_path",
			"filesystem_io_policy_path",
		},
	})
	registry.Register(ModuleType{
		Kind:     "ShellToolContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"shell_catalog_policy_path",
			"shell_description_policy_path",
		},
	})
	registry.Register(ModuleType{
		Kind:     "ShellExecutionContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"shell_command_policy_path",
			"shell_approval_policy_path",
			"shell_runtime_policy_path",
		},
	})
	registry.Register(ModuleType{
		Kind:     "DelegationToolContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"delegation_catalog_policy_path",
			"delegation_description_policy_path",
		},
	})
	registry.Register(ModuleType{
		Kind:     "DelegationExecutionContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"delegation_backend_policy_path",
			"delegation_result_policy_path",
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
		Kind:     "OperatorSurfaceContractConfig",
		Category: ModuleCategoryContract,
		RefFields: []string{
			"daemon_server_policy_path",
			"web_assets_policy_path",
			"client_transport_policy_path",
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
		Kind:     "FilesystemCatalogPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "FilesystemDescriptionPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "FilesystemScopePolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "FilesystemMutationPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "FilesystemIOPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ShellCatalogPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ShellDescriptionPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ShellCommandPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ShellApprovalPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ShellRuntimePolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "DelegationCatalogPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "DelegationDescriptionPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "DelegationBackendPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "DelegationResultPolicyConfig",
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
	registry.Register(ModuleType{
		Kind:     "DaemonServerPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "WebAssetsPolicyConfig",
		Category: ModuleCategoryPolicy,
	})
	registry.Register(ModuleType{
		Kind:     "ClientTransportPolicyConfig",
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
