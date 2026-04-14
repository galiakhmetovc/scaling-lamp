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
		Enabled  bool `yaml:"enabled"`
		Strategy string `yaml:"strategy"`
		Params   T    `yaml:"params"`
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
