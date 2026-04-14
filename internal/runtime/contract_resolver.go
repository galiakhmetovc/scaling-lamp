package runtime

import (
	"fmt"
	"path/filepath"

	"teamd/internal/config"
	"teamd/internal/contracts"
)

type transportContractConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		EndpointPolicyPath string `yaml:"endpoint_policy_path"`
		AuthPolicyPath     string `yaml:"auth_policy_path"`
		RetryPolicyPath    string `yaml:"retry_policy_path"`
		TimeoutPolicyPath  string `yaml:"timeout_policy_path"`
	} `yaml:"spec"`
}

type endpointPolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool                     `yaml:"enabled"`
		Strategy string                   `yaml:"strategy"`
		Params   contracts.EndpointParams `yaml:"params"`
	} `yaml:"spec"`
}

type memoryContractConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		OffloadPolicyPath string `yaml:"offload_policy_path"`
	} `yaml:"spec"`
}

type requestShapeContractConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		ModelPolicyPath          string `yaml:"model_policy_path"`
		MessagePolicyPath        string `yaml:"message_policy_path"`
		ToolPolicyPath           string `yaml:"tool_policy_path"`
		ResponseFormatPolicyPath string `yaml:"response_format_policy_path"`
		StreamingPolicyPath      string `yaml:"streaming_policy_path"`
		SamplingPolicyPath       string `yaml:"sampling_policy_path"`
	} `yaml:"spec"`
}

type authPolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool                 `yaml:"enabled"`
		Strategy string               `yaml:"strategy"`
		Params   contracts.AuthParams `yaml:"params"`
	} `yaml:"spec"`
}

type retryPolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool                  `yaml:"enabled"`
		Strategy string                `yaml:"strategy"`
		Params   contracts.RetryParams `yaml:"params"`
	} `yaml:"spec"`
}

type timeoutPolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool                    `yaml:"enabled"`
		Strategy string                  `yaml:"strategy"`
		Params   contracts.TimeoutParams `yaml:"params"`
	} `yaml:"spec"`
}

type offloadPolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool                    `yaml:"enabled"`
		Strategy string                  `yaml:"strategy"`
		Params   contracts.OffloadParams `yaml:"params"`
	} `yaml:"spec"`
}

type modelPolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool                  `yaml:"enabled"`
		Strategy string                `yaml:"strategy"`
		Params   contracts.ModelParams `yaml:"params"`
	} `yaml:"spec"`
}

type messagePolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool   `yaml:"enabled"`
		Strategy string `yaml:"strategy"`
	} `yaml:"spec"`
}

type toolShapePolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool   `yaml:"enabled"`
		Strategy string `yaml:"strategy"`
	} `yaml:"spec"`
}

type responseFormatPolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool                           `yaml:"enabled"`
		Strategy string                         `yaml:"strategy"`
		Params   contracts.ResponseFormatParams `yaml:"params"`
	} `yaml:"spec"`
}

type streamingPolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool                      `yaml:"enabled"`
		Strategy string                    `yaml:"strategy"`
		Params   contracts.StreamingParams `yaml:"params"`
	} `yaml:"spec"`
}

type samplingPolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool                     `yaml:"enabled"`
		Strategy string                   `yaml:"strategy"`
		Params   contracts.SamplingParams `yaml:"params"`
	} `yaml:"spec"`
}

func ResolveContracts(cfg config.AgentConfig) (contracts.ResolvedContracts, error) {
	var out contracts.ResolvedContracts

	if transportPath := cfg.Spec.Contracts["transport"]; transportPath != "" {
		transport, err := resolveTransportContract(transportPath)
		if err != nil {
			return contracts.ResolvedContracts{}, err
		}
		out.ProviderRequest.Transport = transport
	}
	if requestShapePath := cfg.Spec.Contracts["request_shape"]; requestShapePath != "" {
		requestShape, err := resolveRequestShapeContract(requestShapePath)
		if err != nil {
			return contracts.ResolvedContracts{}, err
		}
		out.ProviderRequest.RequestShape = requestShape
	}

	if memoryPath := cfg.Spec.Contracts["memory"]; memoryPath != "" {
		memory, err := resolveMemoryContract(memoryPath)
		if err != nil {
			return contracts.ResolvedContracts{}, err
		}
		out.Memory = memory
	}

	return out, nil
}

func resolveRequestShapeContract(path string) (contracts.RequestShapeContract, error) {
	var contract requestShapeContractConfig
	if err := config.LoadModule(path, &contract); err != nil {
		return contracts.RequestShapeContract{}, fmt.Errorf("load request-shape contract: %w", err)
	}
	if contract.Spec.ModelPolicyPath == "" {
		return contracts.RequestShapeContract{}, fmt.Errorf("request-shape contract %q missing model_policy_path", contract.ID)
	}
	if contract.Spec.MessagePolicyPath == "" {
		return contracts.RequestShapeContract{}, fmt.Errorf("request-shape contract %q missing message_policy_path", contract.ID)
	}
	if contract.Spec.ToolPolicyPath == "" {
		return contracts.RequestShapeContract{}, fmt.Errorf("request-shape contract %q missing tool_policy_path", contract.ID)
	}
	if contract.Spec.ResponseFormatPolicyPath == "" {
		return contracts.RequestShapeContract{}, fmt.Errorf("request-shape contract %q missing response_format_policy_path", contract.ID)
	}
	if contract.Spec.StreamingPolicyPath == "" {
		return contracts.RequestShapeContract{}, fmt.Errorf("request-shape contract %q missing streaming_policy_path", contract.ID)
	}
	if contract.Spec.SamplingPolicyPath == "" {
		return contracts.RequestShapeContract{}, fmt.Errorf("request-shape contract %q missing sampling_policy_path", contract.ID)
	}

	var modelPolicy modelPolicyConfig
	if err := config.LoadModule(resolveModulePath(path, contract.Spec.ModelPolicyPath), &modelPolicy); err != nil {
		return contracts.RequestShapeContract{}, fmt.Errorf("load model policy: %w", err)
	}
	var messagePolicy messagePolicyConfig
	if err := config.LoadModule(resolveModulePath(path, contract.Spec.MessagePolicyPath), &messagePolicy); err != nil {
		return contracts.RequestShapeContract{}, fmt.Errorf("load message policy: %w", err)
	}
	var toolPolicy toolShapePolicyConfig
	if err := config.LoadModule(resolveModulePath(path, contract.Spec.ToolPolicyPath), &toolPolicy); err != nil {
		return contracts.RequestShapeContract{}, fmt.Errorf("load tool policy: %w", err)
	}
	var responseFormatPolicy responseFormatPolicyConfig
	if err := config.LoadModule(resolveModulePath(path, contract.Spec.ResponseFormatPolicyPath), &responseFormatPolicy); err != nil {
		return contracts.RequestShapeContract{}, fmt.Errorf("load response-format policy: %w", err)
	}
	var streamingPolicy streamingPolicyConfig
	if err := config.LoadModule(resolveModulePath(path, contract.Spec.StreamingPolicyPath), &streamingPolicy); err != nil {
		return contracts.RequestShapeContract{}, fmt.Errorf("load streaming policy: %w", err)
	}
	var samplingPolicy samplingPolicyConfig
	if err := config.LoadModule(resolveModulePath(path, contract.Spec.SamplingPolicyPath), &samplingPolicy); err != nil {
		return contracts.RequestShapeContract{}, fmt.Errorf("load sampling policy: %w", err)
	}

	return contracts.RequestShapeContract{
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
	}, nil
}

func resolveTransportContract(path string) (contracts.TransportContract, error) {
	var contract transportContractConfig
	if err := config.LoadModule(path, &contract); err != nil {
		return contracts.TransportContract{}, fmt.Errorf("load transport contract: %w", err)
	}
	if contract.Spec.EndpointPolicyPath == "" {
		return contracts.TransportContract{}, fmt.Errorf("transport contract %q missing endpoint_policy_path", contract.ID)
	}
	if contract.Spec.AuthPolicyPath == "" {
		return contracts.TransportContract{}, fmt.Errorf("transport contract %q missing auth_policy_path", contract.ID)
	}
	if contract.Spec.RetryPolicyPath == "" {
		return contracts.TransportContract{}, fmt.Errorf("transport contract %q missing retry_policy_path", contract.ID)
	}
	if contract.Spec.TimeoutPolicyPath == "" {
		return contracts.TransportContract{}, fmt.Errorf("transport contract %q missing timeout_policy_path", contract.ID)
	}
	policyPath := resolveModulePath(path, contract.Spec.EndpointPolicyPath)

	var policy endpointPolicyConfig
	if err := config.LoadModule(policyPath, &policy); err != nil {
		return contracts.TransportContract{}, fmt.Errorf("load endpoint policy: %w", err)
	}
	authPath := resolveModulePath(path, contract.Spec.AuthPolicyPath)
	var authPolicy authPolicyConfig
	if err := config.LoadModule(authPath, &authPolicy); err != nil {
		return contracts.TransportContract{}, fmt.Errorf("load auth policy: %w", err)
	}
	retryPath := resolveModulePath(path, contract.Spec.RetryPolicyPath)
	var retryPolicy retryPolicyConfig
	if err := config.LoadModule(retryPath, &retryPolicy); err != nil {
		return contracts.TransportContract{}, fmt.Errorf("load retry policy: %w", err)
	}
	timeoutPath := resolveModulePath(path, contract.Spec.TimeoutPolicyPath)
	var timeoutPolicy timeoutPolicyConfig
	if err := config.LoadModule(timeoutPath, &timeoutPolicy); err != nil {
		return contracts.TransportContract{}, fmt.Errorf("load timeout policy: %w", err)
	}

	return contracts.TransportContract{
		ID: contract.ID,
		Endpoint: contracts.EndpointPolicy{
			ID:       policy.ID,
			Enabled:  policy.Spec.Enabled,
			Strategy: policy.Spec.Strategy,
			Params:   policy.Spec.Params,
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
	}, nil
}

func resolveMemoryContract(path string) (contracts.MemoryContract, error) {
	var contract memoryContractConfig
	if err := config.LoadModule(path, &contract); err != nil {
		return contracts.MemoryContract{}, fmt.Errorf("load memory contract: %w", err)
	}
	if contract.Spec.OffloadPolicyPath == "" {
		return contracts.MemoryContract{}, fmt.Errorf("memory contract %q missing offload_policy_path", contract.ID)
	}
	policyPath := resolveModulePath(path, contract.Spec.OffloadPolicyPath)

	var policy offloadPolicyConfig
	if err := config.LoadModule(policyPath, &policy); err != nil {
		return contracts.MemoryContract{}, fmt.Errorf("load offload policy: %w", err)
	}

	return contracts.MemoryContract{
		ID: contract.ID,
		Offload: contracts.OffloadPolicy{
			ID:       policy.ID,
			Enabled:  policy.Spec.Enabled,
			Strategy: policy.Spec.Strategy,
			Params:   policy.Spec.Params,
		},
	}, nil
}

func resolveModulePath(modulePath, refPath string) string {
	if filepath.IsAbs(refPath) {
		return filepath.Clean(refPath)
	}
	return filepath.Clean(filepath.Join(filepath.Dir(modulePath), refPath))
}
