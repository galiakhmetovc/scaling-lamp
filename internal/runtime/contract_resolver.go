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

func ResolveContracts(cfg config.AgentConfig) (contracts.ResolvedContracts, error) {
	var out contracts.ResolvedContracts

	if transportPath := cfg.Spec.Contracts["transport"]; transportPath != "" {
		transport, err := resolveTransportContract(transportPath)
		if err != nil {
			return contracts.ResolvedContracts{}, err
		}
		out.ProviderRequest.Transport = transport
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
