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
	for contractName, contractPath := range cfg.Spec.Contracts {
		cfg.Spec.Contracts[contractName] = resolveModulePath(baseDir, contractPath)
	}

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

func LoadModuleGraph(cfg AgentConfig, registry *ModuleRegistry) (ModuleGraph, error) {
	graph := ModuleGraph{
		Contracts: map[string]ModuleHeader{},
		Policies:  map[string]ModuleHeader{},
	}

	visited := map[string]struct{}{}
	queue := make([]string, 0, len(cfg.Spec.Contracts))
	for _, contractPath := range cfg.Spec.Contracts {
		if contractPath == "" {
			continue
		}
		queue = append(queue, contractPath)
	}

	for len(queue) > 0 {
		modulePath := queue[0]
		queue = queue[1:]

		if _, ok := visited[modulePath]; ok {
			continue
		}
		visited[modulePath] = struct{}{}

		header, err := LoadModuleHeader(modulePath)
		if err != nil {
			return ModuleGraph{}, fmt.Errorf("load module header %q: %w", modulePath, err)
		}
		moduleType, err := registry.Type(header.Kind)
		if err != nil {
			return ModuleGraph{}, fmt.Errorf("load module %q: %w", modulePath, err)
		}

		switch moduleType.Category {
		case ModuleCategoryContract:
			graph.Contracts[header.ID] = header
		case ModuleCategoryPolicy:
			graph.Policies[header.ID] = header
		default:
			return ModuleGraph{}, fmt.Errorf("module %q has unsupported category %q", header.ID, moduleType.Category)
		}

		references, err := loadModuleReferences(modulePath, moduleType.RefFields)
		if err != nil {
			return ModuleGraph{}, fmt.Errorf("load module references for %q: %w", modulePath, err)
		}
		queue = append(queue, references...)
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

func loadModuleReferences(modulePath string, refFields []string) ([]string, error) {
	body, err := os.ReadFile(modulePath)
	if err != nil {
		return nil, fmt.Errorf("read module body: %w", err)
	}

	var raw struct {
		Spec map[string]string `yaml:"spec"`
	}
	if err := yaml.Unmarshal(body, &raw); err != nil {
		return nil, fmt.Errorf("decode module body: %w", err)
	}

	moduleDir := filepath.Dir(modulePath)
	references := make([]string, 0, len(refFields))
	for _, field := range refFields {
		refPath := raw.Spec[field]
		if refPath == "" {
			continue
		}
		references = append(references, resolveModulePath(moduleDir, refPath))
	}
	return references, nil
}
