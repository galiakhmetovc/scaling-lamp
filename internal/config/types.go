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
	ProjectionStorePath  string   `yaml:"projection_store_path"`
	PromptAssemblyExecutor string `yaml:"prompt_assembly_executor"`
	PromptAssetExecutor  string   `yaml:"prompt_asset_executor"`
	TransportExecutor    string   `yaml:"transport_executor"`
	RequestShapeExecutor string   `yaml:"request_shape_executor"`
	ToolCatalogExecutor  string   `yaml:"tool_catalog_executor"`
	ToolExecutionGate    string   `yaml:"tool_execution_gate"`
	ProviderClient       string   `yaml:"provider_client"`
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
