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

	return cfg, nil
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
