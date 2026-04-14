package config

type AgentConfig struct {
	Kind    string          `yaml:"kind"`
	Version string          `yaml:"version"`
	ID      string          `yaml:"id"`
	Spec    AgentConfigSpec `yaml:"spec"`
}

type AgentConfigSpec struct {
	Runtime   AgentRuntimeConfig  `yaml:"runtime"`
	Contracts map[string]string `yaml:"contracts"`
}

type AgentRuntimeConfig struct {
	EventLog             string   `yaml:"event_log"`
	EventLogPath         string   `yaml:"event_log_path"`
	TransportExecutor    string   `yaml:"transport_executor"`
	RequestShapeExecutor string   `yaml:"request_shape_executor"`
	Projections          []string `yaml:"projections"`
}

type ModuleHeader struct {
	Kind string `yaml:"kind"`
	ID   string `yaml:"id"`
}

type ModuleGraph struct {
	Contracts map[string]ModuleHeader
	Policies  map[string]ModuleHeader
}
