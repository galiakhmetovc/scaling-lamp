package runtime

import (
	"fmt"
	"path/filepath"

	"teamd/internal/config"
)

type transportContractConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		EndpointPolicyPath string `yaml:"endpoint_policy_path"`
	} `yaml:"spec"`
}

type endpointPolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool           `yaml:"enabled"`
		Strategy string         `yaml:"strategy"`
		Params   EndpointParams `yaml:"params"`
	} `yaml:"spec"`
}

type memoryContractConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		OffloadPolicyPath string `yaml:"offload_policy_path"`
	} `yaml:"spec"`
}

type offloadPolicyConfig struct {
	ID   string `yaml:"id"`
	Spec struct {
		Enabled  bool          `yaml:"enabled"`
		Strategy string        `yaml:"strategy"`
		Params   OffloadParams `yaml:"params"`
	} `yaml:"spec"`
}

func ResolveContracts(cfg config.AgentConfig) (ResolvedContracts, error) {
	var out ResolvedContracts

	if transportPath := cfg.Spec.Contracts["transport"]; transportPath != "" {
		transport, err := resolveTransportContract(transportPath)
		if err != nil {
			return ResolvedContracts{}, err
		}
		out.ProviderRequest.Transport = transport
	}

	if memoryPath := cfg.Spec.Contracts["memory"]; memoryPath != "" {
		memory, err := resolveMemoryContract(memoryPath)
		if err != nil {
			return ResolvedContracts{}, err
		}
		out.Memory = memory
	}

	return out, nil
}

func resolveTransportContract(path string) (TransportContract, error) {
	var contract transportContractConfig
	if err := config.LoadModule(path, &contract); err != nil {
		return TransportContract{}, fmt.Errorf("load transport contract: %w", err)
	}
	if contract.Spec.EndpointPolicyPath == "" {
		return TransportContract{}, fmt.Errorf("transport contract %q missing endpoint_policy_path", contract.ID)
	}
	policyPath := resolveModulePath(path, contract.Spec.EndpointPolicyPath)

	var policy endpointPolicyConfig
	if err := config.LoadModule(policyPath, &policy); err != nil {
		return TransportContract{}, fmt.Errorf("load endpoint policy: %w", err)
	}

	return TransportContract{
		ID: contract.ID,
		Endpoint: EndpointPolicy{
			ID:       policy.ID,
			Enabled:  policy.Spec.Enabled,
			Strategy: policy.Spec.Strategy,
			Params:   policy.Spec.Params,
		},
	}, nil
}

func resolveMemoryContract(path string) (MemoryContract, error) {
	var contract memoryContractConfig
	if err := config.LoadModule(path, &contract); err != nil {
		return MemoryContract{}, fmt.Errorf("load memory contract: %w", err)
	}
	if contract.Spec.OffloadPolicyPath == "" {
		return MemoryContract{}, fmt.Errorf("memory contract %q missing offload_policy_path", contract.ID)
	}
	policyPath := resolveModulePath(path, contract.Spec.OffloadPolicyPath)

	var policy offloadPolicyConfig
	if err := config.LoadModule(policyPath, &policy); err != nil {
		return MemoryContract{}, fmt.Errorf("load offload policy: %w", err)
	}

	return MemoryContract{
		ID: contract.ID,
		Offload: OffloadPolicy{
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
