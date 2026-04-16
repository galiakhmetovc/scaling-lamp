package policies

import "fmt"

type Family string

const (
	FamilyEndpoint                    Family = "endpoint"
	FamilyAuth                        Family = "auth"
	FamilyRetry                       Family = "retry"
	FamilyTimeout                     Family = "timeout"
	FamilyOffload                     Family = "offload"
	FamilyModel                       Family = "model"
	FamilyMessage                     Family = "message"
	FamilyTool                        Family = "tool"
	FamilyResponseFormat              Family = "response_format"
	FamilyStreaming                   Family = "streaming"
	FamilySampling                    Family = "sampling"
	FamilyPromptAsset                 Family = "prompt_asset"
	FamilySystemPrompt                Family = "system_prompt"
	FamilySessionHead                 Family = "session_head"
	FamilyContextBudgetAccounting     Family = "context_budget_accounting"
	FamilyContextBudgetEstimation     Family = "context_budget_estimation"
	FamilyContextBudgetCompaction     Family = "context_budget_compaction"
	FamilyContextBudgetSummaryDisplay Family = "context_budget_summary_display"
	FamilyToolCatalog                 Family = "tool_catalog"
	FamilyToolSerialization           Family = "tool_serialization"
	FamilyFilesystemCatalog           Family = "filesystem_catalog"
	FamilyFilesystemDescription       Family = "filesystem_description"
	FamilyFilesystemScope             Family = "filesystem_scope"
	FamilyFilesystemMutation          Family = "filesystem_mutation"
	FamilyFilesystemIO                Family = "filesystem_io"
	FamilyShellCatalog                Family = "shell_catalog"
	FamilyShellDescription            Family = "shell_description"
	FamilyShellCommand                Family = "shell_command"
	FamilyShellApproval               Family = "shell_approval"
	FamilyShellRuntime                Family = "shell_runtime"
	FamilyDelegationCatalog           Family = "delegation_catalog"
	FamilyDelegationDescription       Family = "delegation_description"
	FamilyDelegationBackend           Family = "delegation_backend"
	FamilyDelegationResult            Family = "delegation_result"
	FamilyToolAccess                  Family = "tool_access"
	FamilyToolApproval                Family = "tool_approval"
	FamilyToolSandbox                 Family = "tool_sandbox"
	FamilyPlanTool                    Family = "plan_tool"
	FamilyProviderTrace               Family = "provider_trace"
	FamilyChatInput                   Family = "chat_input"
	FamilyChatSubmit                  Family = "chat_submit"
	FamilyChatOutput                  Family = "chat_output"
	FamilyChatStatus                  Family = "chat_status"
	FamilyChatCommand                 Family = "chat_command"
	FamilyChatResume                  Family = "chat_resume"
	FamilyDaemonServer                Family = "daemon_server"
	FamilyWebAssets                   Family = "web_assets"
	FamilyClientTransport             Family = "client_transport"
	FamilySettingsSurface             Family = "settings_surface"
)

type Type struct {
	Kind     string
	Family   Family
	Strategy map[string]struct{}
}

type Registry struct {
	types map[string]Type
}

func NewRegistry() *Registry {
	return &Registry{types: map[string]Type{}}
}

func NewBuiltInRegistry() *Registry {
	registry := NewRegistry()
	registry.Register(Type{
		Kind:   "EndpointPolicyConfig",
		Family: FamilyEndpoint,
		Strategy: setOf(
			"static",
		),
	})
	registry.Register(Type{
		Kind:   "AuthPolicyConfig",
		Family: FamilyAuth,
		Strategy: setOf(
			"none",
			"bearer_token",
		),
	})
	registry.Register(Type{
		Kind:   "RetryPolicyConfig",
		Family: FamilyRetry,
		Strategy: setOf(
			"none",
			"fixed",
			"exponential",
			"exponential_jitter",
		),
	})
	registry.Register(Type{
		Kind:   "TimeoutPolicyConfig",
		Family: FamilyTimeout,
		Strategy: setOf(
			"per_request",
			"long_running_non_streaming",
		),
	})
	registry.Register(Type{
		Kind:   "OffloadPolicyConfig",
		Family: FamilyOffload,
		Strategy: setOf(
			"old_only",
			"artifact_store",
		),
	})
	registry.Register(Type{
		Kind:   "ModelPolicyConfig",
		Family: FamilyModel,
		Strategy: setOf(
			"static_model",
		),
	})
	registry.Register(Type{
		Kind:   "MessagePolicyConfig",
		Family: FamilyMessage,
		Strategy: setOf(
			"raw_messages",
		),
	})
	registry.Register(Type{
		Kind:   "ToolPolicyConfig",
		Family: FamilyTool,
		Strategy: setOf(
			"tools_inline",
		),
	})
	registry.Register(Type{
		Kind:   "ResponseFormatPolicyConfig",
		Family: FamilyResponseFormat,
		Strategy: setOf(
			"default",
		),
	})
	registry.Register(Type{
		Kind:   "StreamingPolicyConfig",
		Family: FamilyStreaming,
		Strategy: setOf(
			"static_stream",
		),
	})
	registry.Register(Type{
		Kind:   "SamplingPolicyConfig",
		Family: FamilySampling,
		Strategy: setOf(
			"static_sampling",
		),
	})
	registry.Register(Type{
		Kind:   "PromptAssetPolicyConfig",
		Family: FamilyPromptAsset,
		Strategy: setOf(
			"inline_assets",
		),
	})
	registry.Register(Type{
		Kind:   "SystemPromptPolicyConfig",
		Family: FamilySystemPrompt,
		Strategy: setOf(
			"file_static",
		),
	})
	registry.Register(Type{
		Kind:   "SessionHeadPolicyConfig",
		Family: FamilySessionHead,
		Strategy: setOf(
			"off",
			"projection_summary",
		),
	})
	registry.Register(Type{
		Kind:   "ContextBudgetAccountingPolicyConfig",
		Family: FamilyContextBudgetAccounting,
		Strategy: setOf(
			"provider_usage_v1",
		),
	})
	registry.Register(Type{
		Kind:   "ContextBudgetEstimationPolicyConfig",
		Family: FamilyContextBudgetEstimation,
		Strategy: setOf(
			"chars_div4",
		),
	})
	registry.Register(Type{
		Kind:   "ContextBudgetCompactionPolicyConfig",
		Family: FamilyContextBudgetCompaction,
		Strategy: setOf(
			"threshold_state_only",
			"rolling_summary_v1",
		),
	})
	registry.Register(Type{
		Kind:   "ContextBudgetSummaryDisplayPolicyConfig",
		Family: FamilyContextBudgetSummaryDisplay,
		Strategy: setOf(
			"counter_only",
		),
	})
	registry.Register(Type{
		Kind:   "ToolCatalogPolicyConfig",
		Family: FamilyToolCatalog,
		Strategy: setOf(
			"static_allowlist",
		),
	})
	registry.Register(Type{
		Kind:   "ToolSerializationPolicyConfig",
		Family: FamilyToolSerialization,
		Strategy: setOf(
			"openai_function_tools",
		),
	})
	registry.Register(Type{
		Kind:   "FilesystemCatalogPolicyConfig",
		Family: FamilyFilesystemCatalog,
		Strategy: setOf(
			"static_allowlist",
		),
	})
	registry.Register(Type{
		Kind:   "FilesystemDescriptionPolicyConfig",
		Family: FamilyFilesystemDescription,
		Strategy: setOf(
			"static_builtin_descriptions",
		),
	})
	registry.Register(Type{
		Kind:   "FilesystemScopePolicyConfig",
		Family: FamilyFilesystemScope,
		Strategy: setOf(
			"workspace_only",
			"allowlist_paths",
		),
	})
	registry.Register(Type{
		Kind:   "FilesystemMutationPolicyConfig",
		Family: FamilyFilesystemMutation,
		Strategy: setOf(
			"allow_writes",
			"require_approval_for_writes",
			"trash_only_delete",
		),
	})
	registry.Register(Type{
		Kind:   "FilesystemIOPolicyConfig",
		Family: FamilyFilesystemIO,
		Strategy: setOf(
			"bounded_text_io",
		),
	})
	registry.Register(Type{
		Kind:   "ShellCatalogPolicyConfig",
		Family: FamilyShellCatalog,
		Strategy: setOf(
			"static_allowlist",
		),
	})
	registry.Register(Type{
		Kind:   "ShellDescriptionPolicyConfig",
		Family: FamilyShellDescription,
		Strategy: setOf(
			"static_builtin_descriptions",
		),
	})
	registry.Register(Type{
		Kind:   "ShellCommandPolicyConfig",
		Family: FamilyShellCommand,
		Strategy: setOf(
			"static_allowlist",
			"deny_all",
		),
	})
	registry.Register(Type{
		Kind:   "ShellApprovalPolicyConfig",
		Family: FamilyShellApproval,
		Strategy: setOf(
			"always_allow",
			"always_require",
			"require_for_patterns",
		),
	})
	registry.Register(Type{
		Kind:   "ShellRuntimePolicyConfig",
		Family: FamilyShellRuntime,
		Strategy: setOf(
			"workspace_write",
			"read_only",
			"deny_exec",
		),
	})
	registry.Register(Type{
		Kind:   "DelegationCatalogPolicyConfig",
		Family: FamilyDelegationCatalog,
		Strategy: setOf(
			"static_allowlist",
		),
	})
	registry.Register(Type{
		Kind:   "DelegationDescriptionPolicyConfig",
		Family: FamilyDelegationDescription,
		Strategy: setOf(
			"static_builtin_descriptions",
		),
	})
	registry.Register(Type{
		Kind:   "DelegationBackendPolicyConfig",
		Family: FamilyDelegationBackend,
		Strategy: setOf(
			"backend_allowlist",
		),
	})
	registry.Register(Type{
		Kind:   "DelegationResultPolicyConfig",
		Family: FamilyDelegationResult,
		Strategy: setOf(
			"bounded_wait_results",
		),
	})
	registry.Register(Type{
		Kind:   "ToolAccessPolicyConfig",
		Family: FamilyToolAccess,
		Strategy: setOf(
			"static_allowlist",
			"deny_all",
		),
	})
	registry.Register(Type{
		Kind:   "ToolApprovalPolicyConfig",
		Family: FamilyToolApproval,
		Strategy: setOf(
			"always_allow",
			"always_require",
			"require_for_destructive",
		),
	})
	registry.Register(Type{
		Kind:   "ToolSandboxPolicyConfig",
		Family: FamilyToolSandbox,
		Strategy: setOf(
			"default_runtime",
			"read_only",
			"workspace_write",
			"deny_exec",
		),
	})
	registry.Register(Type{
		Kind:   "PlanToolPolicyConfig",
		Family: FamilyPlanTool,
		Strategy: setOf(
			"default_plan_tools",
		),
	})
	registry.Register(Type{
		Kind:   "ProviderTracePolicyConfig",
		Family: FamilyProviderTrace,
		Strategy: setOf(
			"none",
			"inline_request",
		),
	})
	registry.Register(Type{
		Kind:   "ChatInputPolicyConfig",
		Family: FamilyChatInput,
		Strategy: setOf(
			"multiline_buffer",
		),
	})
	registry.Register(Type{
		Kind:   "ChatSubmitPolicyConfig",
		Family: FamilyChatSubmit,
		Strategy: setOf(
			"double_enter",
		),
	})
	registry.Register(Type{
		Kind:   "ChatOutputPolicyConfig",
		Family: FamilyChatOutput,
		Strategy: setOf(
			"streaming_text",
		),
	})
	registry.Register(Type{
		Kind:   "ChatStatusPolicyConfig",
		Family: FamilyChatStatus,
		Strategy: setOf(
			"inline_terminal",
		),
	})
	registry.Register(Type{
		Kind:   "ChatCommandPolicyConfig",
		Family: FamilyChatCommand,
		Strategy: setOf(
			"slash_commands",
		),
	})
	registry.Register(Type{
		Kind:   "ChatResumePolicyConfig",
		Family: FamilyChatResume,
		Strategy: setOf(
			"explicit_resume_only",
		),
	})
	registry.Register(Type{
		Kind:   "DaemonServerPolicyConfig",
		Family: FamilyDaemonServer,
		Strategy: setOf(
			"websocket_http",
		),
	})
	registry.Register(Type{
		Kind:   "WebAssetsPolicyConfig",
		Family: FamilyWebAssets,
		Strategy: setOf(
			"embedded_assets",
			"dev_proxy",
		),
	})
	registry.Register(Type{
		Kind:   "ClientTransportPolicyConfig",
		Family: FamilyClientTransport,
		Strategy: setOf(
			"websocket_http",
		),
	})
	registry.Register(Type{
		Kind:   "SettingsSurfacePolicyConfig",
		Family: FamilySettingsSurface,
		Strategy: setOf(
			"revisioned_yaml_files",
		),
	})
	return registry
}

func (r *Registry) Register(policyType Type) {
	r.types[policyType.Kind] = policyType
}

func (r *Registry) Type(kind string) (Type, error) {
	policyType, ok := r.types[kind]
	if !ok {
		return Type{}, fmt.Errorf("unsupported policy kind %q", kind)
	}
	return policyType, nil
}

func (r *Registry) ValidateStrategy(kind, strategy string) error {
	policyType, err := r.Type(kind)
	if err != nil {
		return err
	}
	if _, ok := policyType.Strategy[strategy]; !ok {
		return fmt.Errorf("unsupported strategy %q for policy kind %q", strategy, kind)
	}
	return nil
}

func setOf(values ...string) map[string]struct{} {
	out := make(map[string]struct{}, len(values))
	for _, value := range values {
		out[value] = struct{}{}
	}
	return out
}
