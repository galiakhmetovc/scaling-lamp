package config

import (
	"fmt"
	"os"
	"path/filepath"

	"gopkg.in/yaml.v3"
)

func LoadRoot(path string) (AgentConfig, error) {
	var cfg AgentConfig

	body, err := os.ReadFile(path)
	if err != nil {
		return AgentConfig{}, fmt.Errorf("read root config: %w", err)
	}
	if err := yaml.Unmarshal(body, &cfg); err != nil {
		return AgentConfig{}, fmt.Errorf("decode root config: %w", err)
	}

	baseDir := filepath.Dir(path)
	cfg.Spec.Contracts.TransportPath = resolveModulePath(baseDir, cfg.Spec.Contracts.TransportPath)
	cfg.Spec.Contracts.MemoryPath = resolveModulePath(baseDir, cfg.Spec.Contracts.MemoryPath)

	return cfg, nil
}

func LoadModuleHeader(path string) (ModuleHeader, error) {
	var header ModuleHeader

	body, err := os.ReadFile(path)
	if err != nil {
		return ModuleHeader{}, fmt.Errorf("read module header: %w", err)
	}
	if err := yaml.Unmarshal(body, &header); err != nil {
		return ModuleHeader{}, fmt.Errorf("decode module header: %w", err)
	}
	return header, nil
}

func LoadModuleGraph(cfg AgentConfig) (ModuleGraph, error) {
	graph := ModuleGraph{
		Contracts: map[string]ModuleHeader{},
		Policies:  map[string]ModuleHeader{},
	}

	if err := loadContractAndPolicies(cfg.Spec.Contracts.TransportPath, []string{"endpoint_policy_path"}, graph); err != nil {
		return ModuleGraph{}, fmt.Errorf("load transport contract graph: %w", err)
	}
	if err := loadContractAndPolicies(cfg.Spec.Contracts.MemoryPath, []string{"offload_policy_path"}, graph); err != nil {
		return ModuleGraph{}, fmt.Errorf("load memory contract graph: %w", err)
	}

	return graph, nil
}

func resolveModulePath(baseDir, modulePath string) string {
	if modulePath == "" {
		return ""
	}
	if filepath.IsAbs(modulePath) {
		return filepath.Clean(modulePath)
	}
	return filepath.Clean(filepath.Join(baseDir, modulePath))
}

func loadContractAndPolicies(contractPath string, policyKeys []string, graph ModuleGraph) error {
	if contractPath == "" {
		return nil
	}

	header, err := LoadModuleHeader(contractPath)
	if err != nil {
		return err
	}
	graph.Contracts[header.ID] = header

	body, err := os.ReadFile(contractPath)
	if err != nil {
		return fmt.Errorf("read contract body: %w", err)
	}

	var raw struct {
		Spec map[string]string `yaml:"spec"`
	}
	if err := yaml.Unmarshal(body, &raw); err != nil {
		return fmt.Errorf("decode contract body: %w", err)
	}

	contractDir := filepath.Dir(contractPath)
	for _, key := range policyKeys {
		policyRef := raw.Spec[key]
		if policyRef == "" {
			continue
		}
		policyPath := resolveModulePath(contractDir, policyRef)
		policyHeader, err := LoadModuleHeader(policyPath)
		if err != nil {
			return fmt.Errorf("load policy header for %q: %w", key, err)
		}
		graph.Policies[policyHeader.ID] = policyHeader
	}

	return nil
}
