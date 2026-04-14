package config

type AgentConfig struct {
	Kind    string          `yaml:"kind"`
	Version string          `yaml:"version"`
	ID      string          `yaml:"id"`
	Spec    AgentConfigSpec `yaml:"spec"`
}

type AgentConfigSpec struct {
	Contracts ContractRefs `yaml:"contracts"`
}

type ContractRefs struct {
	TransportPath string `yaml:"transport"`
	MemoryPath    string `yaml:"memory"`
}

type ModuleHeader struct {
	Kind string `yaml:"kind"`
	ID   string `yaml:"id"`
}

type ModuleGraph struct {
	Contracts map[string]ModuleHeader
	Policies  map[string]ModuleHeader
}
