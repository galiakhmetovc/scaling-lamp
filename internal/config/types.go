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
}

type ModuleHeader struct {
	Kind string `yaml:"kind"`
	ID   string `yaml:"id"`
}
