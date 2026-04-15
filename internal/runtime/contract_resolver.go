package runtime

import (
	"fmt"
	"path/filepath"
	"sort"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/policies"
)

type contractSpec[T any] struct {
	Kind string `yaml:"kind"`
	ID   string `yaml:"id"`
	Spec T      `yaml:"spec"`
}

type policySpec[T any] struct {
	Kind string `yaml:"kind"`
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool   `yaml:"enabled"`
		Strategy string `yaml:"strategy"`
		Params   T      `yaml:"params"`
	} `yaml:"spec"`
}

type transportContractBody struct {
	EndpointPolicyPath string `yaml:"endpoint_policy_path"`
	AuthPolicyPath     string `yaml:"auth_policy_path"`
	RetryPolicyPath    string `yaml:"retry_policy_path"`
	TimeoutPolicyPath  string `yaml:"timeout_policy_path"`
}

type memoryContractBody struct {
	OffloadPolicyPath string `yaml:"offload_policy_path"`
}

type promptAssetsContractBody struct {
	PromptAssetPolicyPath string `yaml:"prompt_asset_policy_path"`
}

type promptAssemblyContractBody struct {
	SystemPromptPolicyPath string `yaml:"system_prompt_policy_path"`
	SessionHeadPolicyPath  string `yaml:"session_head_policy_path"`
}

type toolContractBody struct {
	ToolCatalogPolicyPath       string `yaml:"tool_catalog_policy_path"`
	ToolSerializationPolicyPath string `yaml:"tool_serialization_policy_path"`
}

type filesystemToolContractBody struct {
	FilesystemCatalogPolicyPath     string `yaml:"filesystem_catalog_policy_path"`
	FilesystemDescriptionPolicyPath string `yaml:"filesystem_description_policy_path"`
}

type filesystemExecutionContractBody struct {
	FilesystemScopePolicyPath    string `yaml:"filesystem_scope_policy_path"`
	FilesystemMutationPolicyPath string `yaml:"filesystem_mutation_policy_path"`
	FilesystemIOPolicyPath       string `yaml:"filesystem_io_policy_path"`
}

type shellToolContractBody struct {
	ShellCatalogPolicyPath     string `yaml:"shell_catalog_policy_path"`
	ShellDescriptionPolicyPath string `yaml:"shell_description_policy_path"`
}

type shellExecutionContractBody struct {
	ShellCommandPolicyPath  string `yaml:"shell_command_policy_path"`
	ShellApprovalPolicyPath string `yaml:"shell_approval_policy_path"`
	ShellRuntimePolicyPath  string `yaml:"shell_runtime_policy_path"`
}

type delegationToolContractBody struct {
	DelegationCatalogPolicyPath     string `yaml:"delegation_catalog_policy_path"`
	DelegationDescriptionPolicyPath string `yaml:"delegation_description_policy_path"`
}

type delegationExecutionContractBody struct {
	DelegationBackendPolicyPath string `yaml:"delegation_backend_policy_path"`
	DelegationResultPolicyPath  string `yaml:"delegation_result_policy_path"`
}

type toolExecutionContractBody struct {
	ToolAccessPolicyPath   string `yaml:"tool_access_policy_path"`
	ToolApprovalPolicyPath string `yaml:"tool_approval_policy_path"`
	ToolSandboxPolicyPath  string `yaml:"tool_sandbox_policy_path"`
}

type planToolContractBody struct {
	PlanToolPolicyPath string `yaml:"plan_tool_policy_path"`
}

type providerTraceContractBody struct {
	ProviderTracePolicyPath string `yaml:"provider_trace_policy_path"`
}

type chatContractBody struct {
	InputPolicyPath   string `yaml:"input_policy_path"`
	SubmitPolicyPath  string `yaml:"submit_policy_path"`
	OutputPolicyPath  string `yaml:"output_policy_path"`
	StatusPolicyPath  string `yaml:"status_policy_path"`
	CommandPolicyPath string `yaml:"command_policy_path"`
	ResumePolicyPath  string `yaml:"resume_policy_path"`
}

type operatorSurfaceContractBody struct {
	DaemonServerPolicyPath    string `yaml:"daemon_server_policy_path"`
	WebAssetsPolicyPath       string `yaml:"web_assets_policy_path"`
	ClientTransportPolicyPath string `yaml:"client_transport_policy_path"`
}

type requestShapeContractBody struct {
	ModelPolicyPath          string `yaml:"model_policy_path"`
	MessagePolicyPath        string `yaml:"message_policy_path"`
	ToolPolicyPath           string `yaml:"tool_policy_path"`
	ResponseFormatPolicyPath string `yaml:"response_format_policy_path"`
	StreamingPolicyPath      string `yaml:"streaming_policy_path"`
	SamplingPolicyPath       string `yaml:"sampling_policy_path"`
}

type contractApplyFunc func(*contracts.ResolvedContracts, string, *policies.Registry) error

func ResolveContracts(cfg config.AgentConfig) (contracts.ResolvedContracts, error) {
	return ResolveContractsWithRegistry(cfg, policies.NewBuiltInRegistry())
}

func ResolveContractsWithRegistry(cfg config.AgentConfig, policyRegistry *policies.Registry) (contracts.ResolvedContracts, error) {
	var out contracts.ResolvedContracts

	for _, contractPath := range sortedContractPaths(cfg.Spec.Contracts) {
		header, err := config.LoadModuleHeader(contractPath)
		if err != nil {
			return contracts.ResolvedContracts{}, fmt.Errorf("load contract header %q: %w", contractPath, err)
		}
		apply, err := resolveContractApplyFunc(header.Kind)
		if err != nil {
			return contracts.ResolvedContracts{}, err
		}
		if err := apply(&out, contractPath, policyRegistry); err != nil {
			return contracts.ResolvedContracts{}, err
		}
	}

	return out, nil
}

func resolveRequestShapeContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[requestShapeContractBody](path, "request-shape")
	if err != nil {
		return err
	}
	if contract.Spec.ModelPolicyPath == "" {
		return fmt.Errorf("request-shape contract %q missing model_policy_path", contract.ID)
	}
	if contract.Spec.MessagePolicyPath == "" {
		return fmt.Errorf("request-shape contract %q missing message_policy_path", contract.ID)
	}
	if contract.Spec.ToolPolicyPath == "" {
		return fmt.Errorf("request-shape contract %q missing tool_policy_path", contract.ID)
	}
	if contract.Spec.ResponseFormatPolicyPath == "" {
		return fmt.Errorf("request-shape contract %q missing response_format_policy_path", contract.ID)
	}
	if contract.Spec.StreamingPolicyPath == "" {
		return fmt.Errorf("request-shape contract %q missing streaming_policy_path", contract.ID)
	}
	if contract.Spec.SamplingPolicyPath == "" {
		return fmt.Errorf("request-shape contract %q missing sampling_policy_path", contract.ID)
	}

	modelPolicy, err := loadPolicy[contracts.ModelParams](path, contract.Spec.ModelPolicyPath, "ModelPolicyConfig", "model", policyRegistry)
	if err != nil {
		return err
	}
	messagePolicy, err := loadPolicy[struct{}](path, contract.Spec.MessagePolicyPath, "MessagePolicyConfig", "message", policyRegistry)
	if err != nil {
		return err
	}
	toolPolicy, err := loadPolicy[struct{}](path, contract.Spec.ToolPolicyPath, "ToolPolicyConfig", "tool", policyRegistry)
	if err != nil {
		return err
	}
	responseFormatPolicy, err := loadPolicy[contracts.ResponseFormatParams](path, contract.Spec.ResponseFormatPolicyPath, "ResponseFormatPolicyConfig", "response-format", policyRegistry)
	if err != nil {
		return err
	}
	streamingPolicy, err := loadPolicy[contracts.StreamingParams](path, contract.Spec.StreamingPolicyPath, "StreamingPolicyConfig", "streaming", policyRegistry)
	if err != nil {
		return err
	}
	samplingPolicy, err := loadPolicy[contracts.SamplingParams](path, contract.Spec.SamplingPolicyPath, "SamplingPolicyConfig", "sampling", policyRegistry)
	if err != nil {
		return err
	}

	out.ProviderRequest.RequestShape = contracts.RequestShapeContract{
		ID: contract.ID,
		Model: contracts.ModelPolicy{
			ID:       modelPolicy.ID,
			Enabled:  modelPolicy.Spec.Enabled,
			Strategy: modelPolicy.Spec.Strategy,
			Params:   modelPolicy.Spec.Params,
		},
		Messages: contracts.MessagePolicy{
			ID:       messagePolicy.ID,
			Enabled:  messagePolicy.Spec.Enabled,
			Strategy: messagePolicy.Spec.Strategy,
		},
		Tools: contracts.ToolPolicy{
			ID:       toolPolicy.ID,
			Enabled:  toolPolicy.Spec.Enabled,
			Strategy: toolPolicy.Spec.Strategy,
		},
		ResponseFormat: contracts.ResponseFormatPolicy{
			ID:       responseFormatPolicy.ID,
			Enabled:  responseFormatPolicy.Spec.Enabled,
			Strategy: responseFormatPolicy.Spec.Strategy,
			Params:   responseFormatPolicy.Spec.Params,
		},
		Streaming: contracts.StreamingPolicy{
			ID:       streamingPolicy.ID,
			Enabled:  streamingPolicy.Spec.Enabled,
			Strategy: streamingPolicy.Spec.Strategy,
			Params:   streamingPolicy.Spec.Params,
		},
		Sampling: contracts.SamplingPolicy{
			ID:       samplingPolicy.ID,
			Enabled:  samplingPolicy.Spec.Enabled,
			Strategy: samplingPolicy.Spec.Strategy,
			Params:   samplingPolicy.Spec.Params,
		},
	}

	return nil
}

func resolveTransportContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[transportContractBody](path, "transport")
	if err != nil {
		return err
	}
	if contract.Spec.EndpointPolicyPath == "" {
		return fmt.Errorf("transport contract %q missing endpoint_policy_path", contract.ID)
	}
	if contract.Spec.AuthPolicyPath == "" {
		return fmt.Errorf("transport contract %q missing auth_policy_path", contract.ID)
	}
	if contract.Spec.RetryPolicyPath == "" {
		return fmt.Errorf("transport contract %q missing retry_policy_path", contract.ID)
	}
	if contract.Spec.TimeoutPolicyPath == "" {
		return fmt.Errorf("transport contract %q missing timeout_policy_path", contract.ID)
	}

	endpointPolicy, err := loadPolicy[contracts.EndpointParams](path, contract.Spec.EndpointPolicyPath, "EndpointPolicyConfig", "endpoint", policyRegistry)
	if err != nil {
		return err
	}
	authPolicy, err := loadPolicy[contracts.AuthParams](path, contract.Spec.AuthPolicyPath, "AuthPolicyConfig", "auth", policyRegistry)
	if err != nil {
		return err
	}
	retryPolicy, err := loadPolicy[contracts.RetryParams](path, contract.Spec.RetryPolicyPath, "RetryPolicyConfig", "retry", policyRegistry)
	if err != nil {
		return err
	}
	timeoutPolicy, err := loadPolicy[contracts.TimeoutParams](path, contract.Spec.TimeoutPolicyPath, "TimeoutPolicyConfig", "timeout", policyRegistry)
	if err != nil {
		return err
	}

	out.ProviderRequest.Transport = contracts.TransportContract{
		ID: contract.ID,
		Endpoint: contracts.EndpointPolicy{
			ID:       endpointPolicy.ID,
			Enabled:  endpointPolicy.Spec.Enabled,
			Strategy: endpointPolicy.Spec.Strategy,
			Params:   endpointPolicy.Spec.Params,
		},
		Auth: contracts.AuthPolicy{
			ID:       authPolicy.ID,
			Enabled:  authPolicy.Spec.Enabled,
			Strategy: authPolicy.Spec.Strategy,
			Params:   authPolicy.Spec.Params,
		},
		Retry: contracts.RetryPolicy{
			ID:       retryPolicy.ID,
			Enabled:  retryPolicy.Spec.Enabled,
			Strategy: retryPolicy.Spec.Strategy,
			Params:   retryPolicy.Spec.Params,
		},
		Timeout: contracts.TimeoutPolicy{
			ID:       timeoutPolicy.ID,
			Enabled:  timeoutPolicy.Spec.Enabled,
			Strategy: timeoutPolicy.Spec.Strategy,
			Params:   timeoutPolicy.Spec.Params,
		},
	}

	return nil
}

func resolveMemoryContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[memoryContractBody](path, "memory")
	if err != nil {
		return err
	}
	if contract.Spec.OffloadPolicyPath == "" {
		return fmt.Errorf("memory contract %q missing offload_policy_path", contract.ID)
	}

	policy, err := loadPolicy[contracts.OffloadParams](path, contract.Spec.OffloadPolicyPath, "OffloadPolicyConfig", "offload", policyRegistry)
	if err != nil {
		return err
	}

	out.Memory = contracts.MemoryContract{
		ID: contract.ID,
		Offload: contracts.OffloadPolicy{
			ID:       policy.ID,
			Enabled:  policy.Spec.Enabled,
			Strategy: policy.Spec.Strategy,
			Params:   policy.Spec.Params,
		},
	}

	return nil
}

func resolveChatContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[chatContractBody](path, "chat")
	if err != nil {
		return err
	}
	if contract.Spec.InputPolicyPath == "" || contract.Spec.SubmitPolicyPath == "" || contract.Spec.OutputPolicyPath == "" || contract.Spec.StatusPolicyPath == "" || contract.Spec.CommandPolicyPath == "" || contract.Spec.ResumePolicyPath == "" {
		return fmt.Errorf("chat contract %q missing one or more policy paths", contract.ID)
	}
	inputPolicy, err := loadPolicy[contracts.ChatInputParams](path, contract.Spec.InputPolicyPath, "ChatInputPolicyConfig", "chat-input", policyRegistry)
	if err != nil {
		return err
	}
	submitPolicy, err := loadPolicy[contracts.ChatSubmitParams](path, contract.Spec.SubmitPolicyPath, "ChatSubmitPolicyConfig", "chat-submit", policyRegistry)
	if err != nil {
		return err
	}
	outputPolicy, err := loadPolicy[contracts.ChatOutputParams](path, contract.Spec.OutputPolicyPath, "ChatOutputPolicyConfig", "chat-output", policyRegistry)
	if err != nil {
		return err
	}
	statusPolicy, err := loadPolicy[contracts.ChatStatusParams](path, contract.Spec.StatusPolicyPath, "ChatStatusPolicyConfig", "chat-status", policyRegistry)
	if err != nil {
		return err
	}
	commandPolicy, err := loadPolicy[contracts.ChatCommandParams](path, contract.Spec.CommandPolicyPath, "ChatCommandPolicyConfig", "chat-command", policyRegistry)
	if err != nil {
		return err
	}
	resumePolicy, err := loadPolicy[contracts.ChatResumeParams](path, contract.Spec.ResumePolicyPath, "ChatResumePolicyConfig", "chat-resume", policyRegistry)
	if err != nil {
		return err
	}

	out.Chat = contracts.ChatContract{
		ID:      contract.ID,
		Input:   contracts.ChatInputPolicy{ID: inputPolicy.ID, Enabled: inputPolicy.Spec.Enabled, Strategy: inputPolicy.Spec.Strategy, Params: inputPolicy.Spec.Params},
		Submit:  contracts.ChatSubmitPolicy{ID: submitPolicy.ID, Enabled: submitPolicy.Spec.Enabled, Strategy: submitPolicy.Spec.Strategy, Params: submitPolicy.Spec.Params},
		Output:  contracts.ChatOutputPolicy{ID: outputPolicy.ID, Enabled: outputPolicy.Spec.Enabled, Strategy: outputPolicy.Spec.Strategy, Params: outputPolicy.Spec.Params},
		Status:  contracts.ChatStatusPolicy{ID: statusPolicy.ID, Enabled: statusPolicy.Spec.Enabled, Strategy: statusPolicy.Spec.Strategy, Params: statusPolicy.Spec.Params},
		Command: contracts.ChatCommandPolicy{ID: commandPolicy.ID, Enabled: commandPolicy.Spec.Enabled, Strategy: commandPolicy.Spec.Strategy, Params: commandPolicy.Spec.Params},
		Resume:  contracts.ChatResumePolicy{ID: resumePolicy.ID, Enabled: resumePolicy.Spec.Enabled, Strategy: resumePolicy.Spec.Strategy, Params: resumePolicy.Spec.Params},
	}
	return nil
}

func resolvePromptAssetsContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[promptAssetsContractBody](path, "prompt-assets")
	if err != nil {
		return err
	}
	if contract.Spec.PromptAssetPolicyPath == "" {
		return fmt.Errorf("prompt-assets contract %q missing prompt_asset_policy_path", contract.ID)
	}

	policy, err := loadPolicy[contracts.PromptAssetParams](path, contract.Spec.PromptAssetPolicyPath, "PromptAssetPolicyConfig", "prompt-asset", policyRegistry)
	if err != nil {
		return err
	}

	out.PromptAssets = contracts.PromptAssetsContract{
		ID: contract.ID,
		PromptAsset: contracts.PromptAssetPolicy{
			ID:       policy.ID,
			Enabled:  policy.Spec.Enabled,
			Strategy: policy.Spec.Strategy,
			Params:   policy.Spec.Params,
		},
	}

	return nil
}

func resolvePromptAssemblyContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[promptAssemblyContractBody](path, "prompt-assembly")
	if err != nil {
		return err
	}
	if contract.Spec.SystemPromptPolicyPath == "" || contract.Spec.SessionHeadPolicyPath == "" {
		return fmt.Errorf("prompt-assembly contract %q missing one or more policy paths", contract.ID)
	}

	systemPromptPolicy, err := loadPolicy[contracts.SystemPromptParams](path, contract.Spec.SystemPromptPolicyPath, "SystemPromptPolicyConfig", "system-prompt", policyRegistry)
	if err != nil {
		return err
	}
	systemPromptPolicyPath := resolveModulePath(path, contract.Spec.SystemPromptPolicyPath)
	if systemPromptPolicy.Spec.Params.Path != "" {
		systemPromptPolicy.Spec.Params.Path = resolveModulePath(systemPromptPolicyPath, systemPromptPolicy.Spec.Params.Path)
	}
	sessionHeadPolicy, err := loadPolicy[contracts.SessionHeadParams](path, contract.Spec.SessionHeadPolicyPath, "SessionHeadPolicyConfig", "session-head", policyRegistry)
	if err != nil {
		return err
	}

	out.PromptAssembly = contracts.PromptAssemblyContract{
		ID: contract.ID,
		SystemPrompt: contracts.SystemPromptPolicy{
			ID:       systemPromptPolicy.ID,
			Enabled:  systemPromptPolicy.Spec.Enabled,
			Strategy: systemPromptPolicy.Spec.Strategy,
			Params:   systemPromptPolicy.Spec.Params,
		},
		SessionHead: contracts.SessionHeadPolicy{
			ID:       sessionHeadPolicy.ID,
			Enabled:  sessionHeadPolicy.Spec.Enabled,
			Strategy: sessionHeadPolicy.Spec.Strategy,
			Params:   sessionHeadPolicy.Spec.Params,
		},
	}

	return nil
}

func resolveToolContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[toolContractBody](path, "tools")
	if err != nil {
		return err
	}
	if contract.Spec.ToolCatalogPolicyPath == "" || contract.Spec.ToolSerializationPolicyPath == "" {
		return fmt.Errorf("tools contract %q missing one or more policy paths", contract.ID)
	}
	catalogPolicy, err := loadPolicy[contracts.ToolCatalogParams](path, contract.Spec.ToolCatalogPolicyPath, "ToolCatalogPolicyConfig", "tool-catalog", policyRegistry)
	if err != nil {
		return err
	}
	serializationPolicy, err := loadPolicy[contracts.ToolSerializationParams](path, contract.Spec.ToolSerializationPolicyPath, "ToolSerializationPolicyConfig", "tool-serialization", policyRegistry)
	if err != nil {
		return err
	}
	out.Tools = contracts.ToolContract{
		ID: contract.ID,
		Catalog: contracts.ToolCatalogPolicy{
			ID:       catalogPolicy.ID,
			Enabled:  catalogPolicy.Spec.Enabled,
			Strategy: catalogPolicy.Spec.Strategy,
			Params:   catalogPolicy.Spec.Params,
		},
		Serialization: contracts.ToolSerializationPolicy{
			ID:       serializationPolicy.ID,
			Enabled:  serializationPolicy.Spec.Enabled,
			Strategy: serializationPolicy.Spec.Strategy,
			Params:   serializationPolicy.Spec.Params,
		},
	}
	return nil
}

func resolveFilesystemToolContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[filesystemToolContractBody](path, "filesystem-tools")
	if err != nil {
		return err
	}
	if contract.Spec.FilesystemCatalogPolicyPath == "" || contract.Spec.FilesystemDescriptionPolicyPath == "" {
		return fmt.Errorf("filesystem-tools contract %q missing one or more policy paths", contract.ID)
	}
	catalogPolicy, err := loadPolicy[contracts.FilesystemCatalogParams](path, contract.Spec.FilesystemCatalogPolicyPath, "FilesystemCatalogPolicyConfig", "filesystem-catalog", policyRegistry)
	if err != nil {
		return err
	}
	descriptionPolicy, err := loadPolicy[contracts.FilesystemDescriptionParams](path, contract.Spec.FilesystemDescriptionPolicyPath, "FilesystemDescriptionPolicyConfig", "filesystem-description", policyRegistry)
	if err != nil {
		return err
	}
	out.FilesystemTools = contracts.FilesystemToolContract{
		ID: contract.ID,
		Catalog: contracts.FilesystemCatalogPolicy{
			ID:       catalogPolicy.ID,
			Enabled:  catalogPolicy.Spec.Enabled,
			Strategy: catalogPolicy.Spec.Strategy,
			Params:   catalogPolicy.Spec.Params,
		},
		Description: contracts.FilesystemDescriptionPolicy{
			ID:       descriptionPolicy.ID,
			Enabled:  descriptionPolicy.Spec.Enabled,
			Strategy: descriptionPolicy.Spec.Strategy,
			Params:   descriptionPolicy.Spec.Params,
		},
	}
	return nil
}

func resolveFilesystemExecutionContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[filesystemExecutionContractBody](path, "filesystem-execution")
	if err != nil {
		return err
	}
	if contract.Spec.FilesystemScopePolicyPath == "" || contract.Spec.FilesystemMutationPolicyPath == "" || contract.Spec.FilesystemIOPolicyPath == "" {
		return fmt.Errorf("filesystem-execution contract %q missing one or more policy paths", contract.ID)
	}
	scopePolicy, err := loadPolicy[contracts.FilesystemScopeParams](path, contract.Spec.FilesystemScopePolicyPath, "FilesystemScopePolicyConfig", "filesystem-scope", policyRegistry)
	if err != nil {
		return err
	}
	mutationPolicy, err := loadPolicy[contracts.FilesystemMutationParams](path, contract.Spec.FilesystemMutationPolicyPath, "FilesystemMutationPolicyConfig", "filesystem-mutation", policyRegistry)
	if err != nil {
		return err
	}
	ioPolicy, err := loadPolicy[contracts.FilesystemIOParams](path, contract.Spec.FilesystemIOPolicyPath, "FilesystemIOPolicyConfig", "filesystem-io", policyRegistry)
	if err != nil {
		return err
	}
	out.FilesystemExecution = contracts.FilesystemExecutionContract{
		ID: contract.ID,
		Scope: contracts.FilesystemScopePolicy{
			ID:       scopePolicy.ID,
			Enabled:  scopePolicy.Spec.Enabled,
			Strategy: scopePolicy.Spec.Strategy,
			Params:   scopePolicy.Spec.Params,
		},
		Mutation: contracts.FilesystemMutationPolicy{
			ID:       mutationPolicy.ID,
			Enabled:  mutationPolicy.Spec.Enabled,
			Strategy: mutationPolicy.Spec.Strategy,
			Params:   mutationPolicy.Spec.Params,
		},
		IO: contracts.FilesystemIOPolicy{
			ID:       ioPolicy.ID,
			Enabled:  ioPolicy.Spec.Enabled,
			Strategy: ioPolicy.Spec.Strategy,
			Params:   ioPolicy.Spec.Params,
		},
	}
	return nil
}

func resolveShellToolContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[shellToolContractBody](path, "shell-tools")
	if err != nil {
		return err
	}
	if contract.Spec.ShellCatalogPolicyPath == "" || contract.Spec.ShellDescriptionPolicyPath == "" {
		return fmt.Errorf("shell-tools contract %q missing one or more policy paths", contract.ID)
	}
	catalogPolicy, err := loadPolicy[contracts.ShellCatalogParams](path, contract.Spec.ShellCatalogPolicyPath, "ShellCatalogPolicyConfig", "shell-catalog", policyRegistry)
	if err != nil {
		return err
	}
	descriptionPolicy, err := loadPolicy[contracts.ShellDescriptionParams](path, contract.Spec.ShellDescriptionPolicyPath, "ShellDescriptionPolicyConfig", "shell-description", policyRegistry)
	if err != nil {
		return err
	}
	out.ShellTools = contracts.ShellToolContract{
		ID: contract.ID,
		Catalog: contracts.ShellCatalogPolicy{
			ID:       catalogPolicy.ID,
			Enabled:  catalogPolicy.Spec.Enabled,
			Strategy: catalogPolicy.Spec.Strategy,
			Params:   catalogPolicy.Spec.Params,
		},
		Description: contracts.ShellDescriptionPolicy{
			ID:       descriptionPolicy.ID,
			Enabled:  descriptionPolicy.Spec.Enabled,
			Strategy: descriptionPolicy.Spec.Strategy,
			Params:   descriptionPolicy.Spec.Params,
		},
	}
	return nil
}

func resolveShellExecutionContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[shellExecutionContractBody](path, "shell-execution")
	if err != nil {
		return err
	}
	if contract.Spec.ShellCommandPolicyPath == "" || contract.Spec.ShellApprovalPolicyPath == "" || contract.Spec.ShellRuntimePolicyPath == "" {
		return fmt.Errorf("shell-execution contract %q missing one or more policy paths", contract.ID)
	}
	commandPolicy, err := loadPolicy[contracts.ShellCommandParams](path, contract.Spec.ShellCommandPolicyPath, "ShellCommandPolicyConfig", "shell-command", policyRegistry)
	if err != nil {
		return err
	}
	approvalPolicy, err := loadPolicy[contracts.ShellApprovalParams](path, contract.Spec.ShellApprovalPolicyPath, "ShellApprovalPolicyConfig", "shell-approval", policyRegistry)
	if err != nil {
		return err
	}
	runtimePolicy, err := loadPolicy[contracts.ShellRuntimeParams](path, contract.Spec.ShellRuntimePolicyPath, "ShellRuntimePolicyConfig", "shell-runtime", policyRegistry)
	if err != nil {
		return err
	}
	out.ShellExecution = contracts.ShellExecutionContract{
		ID: contract.ID,
		Command: contracts.ShellCommandPolicy{
			ID:       commandPolicy.ID,
			Enabled:  commandPolicy.Spec.Enabled,
			Strategy: commandPolicy.Spec.Strategy,
			Params:   commandPolicy.Spec.Params,
		},
		Approval: contracts.ShellApprovalPolicy{
			ID:       approvalPolicy.ID,
			Enabled:  approvalPolicy.Spec.Enabled,
			Strategy: approvalPolicy.Spec.Strategy,
			Params:   approvalPolicy.Spec.Params,
		},
		Runtime: contracts.ShellRuntimePolicy{
			ID:       runtimePolicy.ID,
			Enabled:  runtimePolicy.Spec.Enabled,
			Strategy: runtimePolicy.Spec.Strategy,
			Params:   runtimePolicy.Spec.Params,
		},
	}
	return nil
}

func resolveDelegationToolContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[delegationToolContractBody](path, "delegation-tools")
	if err != nil {
		return err
	}
	if contract.Spec.DelegationCatalogPolicyPath == "" || contract.Spec.DelegationDescriptionPolicyPath == "" {
		return fmt.Errorf("delegation-tools contract %q missing one or more policy paths", contract.ID)
	}
	catalogPolicy, err := loadPolicy[contracts.DelegationCatalogParams](path, contract.Spec.DelegationCatalogPolicyPath, "DelegationCatalogPolicyConfig", "delegation-catalog", policyRegistry)
	if err != nil {
		return err
	}
	descriptionPolicy, err := loadPolicy[contracts.DelegationDescriptionParams](path, contract.Spec.DelegationDescriptionPolicyPath, "DelegationDescriptionPolicyConfig", "delegation-description", policyRegistry)
	if err != nil {
		return err
	}
	out.DelegationTools = contracts.DelegationToolContract{
		ID: contract.ID,
		Catalog: contracts.DelegationCatalogPolicy{
			ID:       catalogPolicy.ID,
			Enabled:  catalogPolicy.Spec.Enabled,
			Strategy: catalogPolicy.Spec.Strategy,
			Params:   catalogPolicy.Spec.Params,
		},
		Description: contracts.DelegationDescriptionPolicy{
			ID:       descriptionPolicy.ID,
			Enabled:  descriptionPolicy.Spec.Enabled,
			Strategy: descriptionPolicy.Spec.Strategy,
			Params:   descriptionPolicy.Spec.Params,
		},
	}
	return nil
}

func resolveDelegationExecutionContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[delegationExecutionContractBody](path, "delegation-execution")
	if err != nil {
		return err
	}
	if contract.Spec.DelegationBackendPolicyPath == "" || contract.Spec.DelegationResultPolicyPath == "" {
		return fmt.Errorf("delegation-execution contract %q missing one or more policy paths", contract.ID)
	}
	backendPolicy, err := loadPolicy[contracts.DelegationBackendParams](path, contract.Spec.DelegationBackendPolicyPath, "DelegationBackendPolicyConfig", "delegation-backend", policyRegistry)
	if err != nil {
		return err
	}
	resultPolicy, err := loadPolicy[contracts.DelegationResultParams](path, contract.Spec.DelegationResultPolicyPath, "DelegationResultPolicyConfig", "delegation-result", policyRegistry)
	if err != nil {
		return err
	}
	out.DelegationExecution = contracts.DelegationExecutionContract{
		ID: contract.ID,
		Backend: contracts.DelegationBackendPolicy{
			ID:       backendPolicy.ID,
			Enabled:  backendPolicy.Spec.Enabled,
			Strategy: backendPolicy.Spec.Strategy,
			Params:   backendPolicy.Spec.Params,
		},
		Result: contracts.DelegationResultPolicy{
			ID:       resultPolicy.ID,
			Enabled:  resultPolicy.Spec.Enabled,
			Strategy: resultPolicy.Spec.Strategy,
			Params:   resultPolicy.Spec.Params,
		},
	}
	return nil
}

func resolveToolExecutionContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[toolExecutionContractBody](path, "tool-execution")
	if err != nil {
		return err
	}
	if contract.Spec.ToolAccessPolicyPath == "" || contract.Spec.ToolApprovalPolicyPath == "" || contract.Spec.ToolSandboxPolicyPath == "" {
		return fmt.Errorf("tool-execution contract %q missing one or more policy paths", contract.ID)
	}
	accessPolicy, err := loadPolicy[contracts.ToolAccessParams](path, contract.Spec.ToolAccessPolicyPath, "ToolAccessPolicyConfig", "tool-access", policyRegistry)
	if err != nil {
		return err
	}
	approvalPolicy, err := loadPolicy[contracts.ToolApprovalParams](path, contract.Spec.ToolApprovalPolicyPath, "ToolApprovalPolicyConfig", "tool-approval", policyRegistry)
	if err != nil {
		return err
	}
	sandboxPolicy, err := loadPolicy[contracts.ToolSandboxParams](path, contract.Spec.ToolSandboxPolicyPath, "ToolSandboxPolicyConfig", "tool-sandbox", policyRegistry)
	if err != nil {
		return err
	}
	out.ToolExecution = contracts.ToolExecutionContract{
		ID: contract.ID,
		Access: contracts.ToolAccessPolicy{
			ID:       accessPolicy.ID,
			Enabled:  accessPolicy.Spec.Enabled,
			Strategy: accessPolicy.Spec.Strategy,
			Params:   accessPolicy.Spec.Params,
		},
		Approval: contracts.ToolApprovalPolicy{
			ID:       approvalPolicy.ID,
			Enabled:  approvalPolicy.Spec.Enabled,
			Strategy: approvalPolicy.Spec.Strategy,
			Params:   approvalPolicy.Spec.Params,
		},
		Sandbox: contracts.ToolSandboxPolicy{
			ID:       sandboxPolicy.ID,
			Enabled:  sandboxPolicy.Spec.Enabled,
			Strategy: sandboxPolicy.Spec.Strategy,
			Params:   sandboxPolicy.Spec.Params,
		},
	}
	return nil
}

func resolvePlanToolContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[planToolContractBody](path, "plan-tools")
	if err != nil {
		return err
	}
	if contract.Spec.PlanToolPolicyPath == "" {
		return fmt.Errorf("plan-tools contract %q missing plan_tool_policy_path", contract.ID)
	}
	policy, err := loadPolicy[contracts.PlanToolParams](path, contract.Spec.PlanToolPolicyPath, "PlanToolPolicyConfig", "plan-tool", policyRegistry)
	if err != nil {
		return err
	}
	out.PlanTools = contracts.PlanToolContract{
		ID: contract.ID,
		PlanTool: contracts.PlanToolPolicy{
			ID:       policy.ID,
			Enabled:  policy.Spec.Enabled,
			Strategy: policy.Spec.Strategy,
			Params:   policy.Spec.Params,
		},
	}
	return nil
}

func resolveProviderTraceContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[providerTraceContractBody](path, "provider-trace")
	if err != nil {
		return err
	}
	if contract.Spec.ProviderTracePolicyPath == "" {
		return fmt.Errorf("provider-trace contract %q missing provider_trace_policy_path", contract.ID)
	}

	policy, err := loadPolicy[contracts.ProviderTraceParams](path, contract.Spec.ProviderTracePolicyPath, "ProviderTracePolicyConfig", "provider-trace", policyRegistry)
	if err != nil {
		return err
	}

	out.ProviderTrace = contracts.ProviderTraceContract{
		ID: contract.ID,
		Request: contracts.ProviderTracePolicy{
			ID:       policy.ID,
			Enabled:  policy.Spec.Enabled,
			Strategy: policy.Spec.Strategy,
			Params:   policy.Spec.Params,
		},
	}

	return nil
}

func resolveOperatorSurfaceContract(out *contracts.ResolvedContracts, path string, policyRegistry *policies.Registry) error {
	contract, err := loadContract[operatorSurfaceContractBody](path, "operator-surface")
	if err != nil {
		return err
	}
	if contract.Spec.DaemonServerPolicyPath == "" || contract.Spec.WebAssetsPolicyPath == "" || contract.Spec.ClientTransportPolicyPath == "" {
		return fmt.Errorf("operator-surface contract %q missing one or more policy paths", contract.ID)
	}

	serverPolicy, err := loadPolicy[contracts.DaemonServerParams](path, contract.Spec.DaemonServerPolicyPath, "DaemonServerPolicyConfig", "daemon-server", policyRegistry)
	if err != nil {
		return err
	}
	assetsPolicy, err := loadPolicy[contracts.WebAssetsParams](path, contract.Spec.WebAssetsPolicyPath, "WebAssetsPolicyConfig", "web-assets", policyRegistry)
	if err != nil {
		return err
	}
	clientPolicy, err := loadPolicy[contracts.ClientTransportParams](path, contract.Spec.ClientTransportPolicyPath, "ClientTransportPolicyConfig", "client-transport", policyRegistry)
	if err != nil {
		return err
	}

	out.OperatorSurface = contracts.OperatorSurfaceContract{
		ID: contract.ID,
		DaemonServer: contracts.DaemonServerPolicy{
			ID:       serverPolicy.ID,
			Enabled:  serverPolicy.Spec.Enabled,
			Strategy: serverPolicy.Spec.Strategy,
			Params:   serverPolicy.Spec.Params,
		},
		WebAssets: contracts.WebAssetsPolicy{
			ID:       assetsPolicy.ID,
			Enabled:  assetsPolicy.Spec.Enabled,
			Strategy: assetsPolicy.Spec.Strategy,
			Params:   assetsPolicy.Spec.Params,
		},
		ClientTransport: contracts.ClientTransportPolicy{
			ID:       clientPolicy.ID,
			Enabled:  clientPolicy.Spec.Enabled,
			Strategy: clientPolicy.Spec.Strategy,
			Params:   clientPolicy.Spec.Params,
		},
	}
	return nil
}

func loadContract[T any](path, label string) (contractSpec[T], error) {
	var contract contractSpec[T]
	if err := config.LoadModule(path, &contract); err != nil {
		return contractSpec[T]{}, fmt.Errorf("load %s contract: %w", label, err)
	}
	return contract, nil
}

func loadPolicy[T any](contractPath, refPath, expectedKind, label string, policyRegistry *policies.Registry) (policySpec[T], error) {
	policyPath := resolveModulePath(contractPath, refPath)

	var policy policySpec[T]
	if err := config.LoadModule(policyPath, &policy); err != nil {
		return policySpec[T]{}, fmt.Errorf("load %s policy: %w", label, err)
	}
	if expectedKind != "" && policy.Kind != expectedKind {
		return policySpec[T]{}, fmt.Errorf("%s policy %q has kind %q, want %q", label, policy.ID, policy.Kind, expectedKind)
	}
	if err := validatePolicyConfig(policyRegistry, policy.Kind, policy.Spec.Strategy); err != nil {
		return policySpec[T]{}, err
	}
	return policy, nil
}

func sortedContractPaths(contractMap map[string]string) []string {
	paths := make([]string, 0, len(contractMap))
	for _, path := range contractMap {
		if path == "" {
			continue
		}
		paths = append(paths, path)
	}
	sort.Strings(paths)
	return paths
}

func resolveContractApplyFunc(kind string) (contractApplyFunc, error) {
	switch kind {
	case "TransportContractConfig":
		return resolveTransportContract, nil
	case "RequestShapeContractConfig":
		return resolveRequestShapeContract, nil
	case "MemoryContractConfig":
		return resolveMemoryContract, nil
	case "PromptAssetsContractConfig":
		return resolvePromptAssetsContract, nil
	case "PromptAssemblyContractConfig":
		return resolvePromptAssemblyContract, nil
	case "ToolContractConfig":
		return resolveToolContract, nil
	case "FilesystemToolContractConfig":
		return resolveFilesystemToolContract, nil
	case "FilesystemExecutionContractConfig":
		return resolveFilesystemExecutionContract, nil
	case "ShellToolContractConfig":
		return resolveShellToolContract, nil
	case "ShellExecutionContractConfig":
		return resolveShellExecutionContract, nil
	case "DelegationToolContractConfig":
		return resolveDelegationToolContract, nil
	case "DelegationExecutionContractConfig":
		return resolveDelegationExecutionContract, nil
	case "ToolExecutionContractConfig":
		return resolveToolExecutionContract, nil
	case "PlanToolContractConfig":
		return resolvePlanToolContract, nil
	case "ProviderTraceContractConfig":
		return resolveProviderTraceContract, nil
	case "ChatContractConfig":
		return resolveChatContract, nil
	case "OperatorSurfaceContractConfig":
		return resolveOperatorSurfaceContract, nil
	default:
		return nil, fmt.Errorf("unsupported contract kind %q", kind)
	}
}

func resolveModulePath(modulePath, refPath string) string {
	if filepath.IsAbs(refPath) {
		return filepath.Clean(refPath)
	}
	return filepath.Clean(filepath.Join(filepath.Dir(modulePath), refPath))
}

func validatePolicyConfig(policyRegistry *policies.Registry, kind, strategy string) error {
	if err := policyRegistry.ValidateStrategy(kind, strategy); err != nil {
		return fmt.Errorf("validate policy %q strategy %q: %w", kind, strategy, err)
	}
	return nil
}
