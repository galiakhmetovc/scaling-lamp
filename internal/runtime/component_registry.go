package runtime

import (
	"fmt"

	"teamd/internal/config"
	"teamd/internal/promptassembly"
	"teamd/internal/provider"
	"teamd/internal/runtime/projections"
	"teamd/internal/tools"
)

type EventLogFactory func(runtimeConfig config.AgentRuntimeConfig) (EventLog, error)
type PromptAssemblyExecutorFactory func() *promptassembly.Executor
type PromptAssetExecutorFactory func() *provider.PromptAssetExecutor
type TransportExecutorFactory func() *provider.TransportExecutor
type RequestShapeExecutorFactory func() *provider.RequestShapeExecutor
type PlanToolExecutorFactory func() *tools.PlanToolExecutor
type ToolCatalogExecutorFactory func() *tools.CatalogExecutor
type ToolExecutionGateFactory func() *tools.ExecutionGate
type ProviderClientFactory func(*provider.PromptAssetExecutor, *provider.RequestShapeExecutor, *tools.PlanToolExecutor, *tools.CatalogExecutor, *tools.ExecutionGate, *provider.TransportExecutor) *provider.Client

type ComponentRegistry struct {
	eventLogs               map[string]EventLogFactory
	promptAssemblyExecutors map[string]PromptAssemblyExecutorFactory
	promptAssetExecutors    map[string]PromptAssetExecutorFactory
	transportExecutors      map[string]TransportExecutorFactory
	requestShapeExecutors   map[string]RequestShapeExecutorFactory
	planToolExecutors       map[string]PlanToolExecutorFactory
	toolCatalogExecutors    map[string]ToolCatalogExecutorFactory
	toolExecutionGates      map[string]ToolExecutionGateFactory
	providerClients         map[string]ProviderClientFactory
	projections             *projections.Registry
}

func NewComponentRegistry() *ComponentRegistry {
	return &ComponentRegistry{
		eventLogs:               map[string]EventLogFactory{},
		promptAssemblyExecutors: map[string]PromptAssemblyExecutorFactory{},
		promptAssetExecutors:    map[string]PromptAssetExecutorFactory{},
		transportExecutors:      map[string]TransportExecutorFactory{},
		requestShapeExecutors:   map[string]RequestShapeExecutorFactory{},
		planToolExecutors:       map[string]PlanToolExecutorFactory{},
		toolCatalogExecutors:    map[string]ToolCatalogExecutorFactory{},
		toolExecutionGates:      map[string]ToolExecutionGateFactory{},
		providerClients:         map[string]ProviderClientFactory{},
		projections:             projections.NewRegistry(),
	}
}

func NewBuiltInComponentRegistry() *ComponentRegistry {
	registry := NewComponentRegistry()
	registry.RegisterEventLog("in_memory", func(_ config.AgentRuntimeConfig) (EventLog, error) {
		return NewInMemoryEventLog(), nil
	})
	registry.RegisterEventLog("file_jsonl", func(runtimeConfig config.AgentRuntimeConfig) (EventLog, error) {
		return NewFileEventLog(runtimeConfig.EventLogPath)
	})
	registry.RegisterPromptAssemblyExecutor("prompt_assembly_default", func() *promptassembly.Executor {
		return promptassembly.NewExecutor()
	})
	registry.RegisterPromptAssetExecutor("prompt_asset_default", func() *provider.PromptAssetExecutor {
		return provider.NewPromptAssetExecutor()
	})
	registry.RegisterTransportExecutor("transport_default", func() *provider.TransportExecutor {
		return provider.NewTransportExecutor(nil)
	})
	registry.RegisterRequestShapeExecutor("request_shape_default", func() *provider.RequestShapeExecutor {
		return provider.NewRequestShapeExecutor()
	})
	registry.RegisterToolCatalogExecutor("tool_catalog_default", func() *tools.CatalogExecutor {
		return tools.NewCatalogExecutor()
	})
	registry.RegisterPlanToolExecutor("plan_tool_default", func() *tools.PlanToolExecutor {
		return tools.NewPlanToolExecutor()
	})
	registry.RegisterToolExecutionGate("tool_execution_default", func() *tools.ExecutionGate {
		return tools.NewExecutionGate()
	})
	registry.RegisterProviderClient("provider_client_default", func(promptAssets *provider.PromptAssetExecutor, requestShape *provider.RequestShapeExecutor, planTools *tools.PlanToolExecutor, toolCatalog *tools.CatalogExecutor, toolExecution *tools.ExecutionGate, transport *provider.TransportExecutor) *provider.Client {
		return provider.NewClient(promptAssets, requestShape, planTools, toolCatalog, toolExecution, transport)
	})
	registry.RegisterProjection("session", func() projections.Projection { return projections.NewSessionProjection() })
	registry.RegisterProjection("run", func() projections.Projection { return projections.NewRunProjection() })
	registry.RegisterProjection("transcript", func() projections.Projection { return projections.NewTranscriptProjection() })
	registry.RegisterProjection("active_plan", func() projections.Projection { return projections.NewActivePlanProjection() })
	registry.RegisterProjection("plan_archive", func() projections.Projection { return projections.NewPlanArchiveProjection() })
	registry.RegisterProjection("plan_head", func() projections.Projection { return projections.NewPlanHeadProjection() })
	return registry
}

func (r *ComponentRegistry) RegisterEventLog(name string, factory EventLogFactory) {
	r.eventLogs[name] = factory
}

func (r *ComponentRegistry) RegisterPromptAssemblyExecutor(name string, factory PromptAssemblyExecutorFactory) {
	r.promptAssemblyExecutors[name] = factory
}

func (r *ComponentRegistry) RegisterPromptAssetExecutor(name string, factory PromptAssetExecutorFactory) {
	r.promptAssetExecutors[name] = factory
}

func (r *ComponentRegistry) RegisterTransportExecutor(name string, factory TransportExecutorFactory) {
	r.transportExecutors[name] = factory
}

func (r *ComponentRegistry) RegisterRequestShapeExecutor(name string, factory RequestShapeExecutorFactory) {
	r.requestShapeExecutors[name] = factory
}

func (r *ComponentRegistry) RegisterToolCatalogExecutor(name string, factory ToolCatalogExecutorFactory) {
	r.toolCatalogExecutors[name] = factory
}

func (r *ComponentRegistry) RegisterPlanToolExecutor(name string, factory PlanToolExecutorFactory) {
	r.planToolExecutors[name] = factory
}

func (r *ComponentRegistry) RegisterToolExecutionGate(name string, factory ToolExecutionGateFactory) {
	r.toolExecutionGates[name] = factory
}

func (r *ComponentRegistry) RegisterProviderClient(name string, factory ProviderClientFactory) {
	r.providerClients[name] = factory
}

func (r *ComponentRegistry) RegisterProjection(name string, factory projections.Factory) {
	r.projections.Register(name, factory)
}

func (r *ComponentRegistry) BuildEventLog(name string, runtimeConfig config.AgentRuntimeConfig) (EventLog, error) {
	factory, ok := r.eventLogs[name]
	if !ok {
		return nil, fmt.Errorf("event log %q is not registered", name)
	}
	return factory(runtimeConfig)
}

func (r *ComponentRegistry) BuildPromptAssemblyExecutor(name string) (*promptassembly.Executor, error) {
	factory, ok := r.promptAssemblyExecutors[name]
	if !ok {
		return nil, fmt.Errorf("prompt-assembly executor %q is not registered", name)
	}
	return factory(), nil
}

func (r *ComponentRegistry) BuildPromptAssetExecutor(name string) (*provider.PromptAssetExecutor, error) {
	factory, ok := r.promptAssetExecutors[name]
	if !ok {
		return nil, fmt.Errorf("prompt-asset executor %q is not registered", name)
	}
	return factory(), nil
}

func (r *ComponentRegistry) BuildTransportExecutor(name string) (*provider.TransportExecutor, error) {
	factory, ok := r.transportExecutors[name]
	if !ok {
		return nil, fmt.Errorf("transport executor %q is not registered", name)
	}
	return factory(), nil
}

func (r *ComponentRegistry) BuildRequestShapeExecutor(name string) (*provider.RequestShapeExecutor, error) {
	factory, ok := r.requestShapeExecutors[name]
	if !ok {
		return nil, fmt.Errorf("request-shape executor %q is not registered", name)
	}
	return factory(), nil
}

func (r *ComponentRegistry) BuildToolCatalogExecutor(name string) (*tools.CatalogExecutor, error) {
	factory, ok := r.toolCatalogExecutors[name]
	if !ok {
		return nil, fmt.Errorf("tool catalog executor %q is not registered", name)
	}
	return factory(), nil
}

func (r *ComponentRegistry) BuildPlanToolExecutor(name string) (*tools.PlanToolExecutor, error) {
	factory, ok := r.planToolExecutors[name]
	if !ok {
		return nil, fmt.Errorf("plan tool executor %q is not registered", name)
	}
	return factory(), nil
}

func (r *ComponentRegistry) BuildToolExecutionGate(name string) (*tools.ExecutionGate, error) {
	factory, ok := r.toolExecutionGates[name]
	if !ok {
		return nil, fmt.Errorf("tool execution gate %q is not registered", name)
	}
	return factory(), nil
}

func (r *ComponentRegistry) BuildProviderClient(name string, promptAssets *provider.PromptAssetExecutor, requestShape *provider.RequestShapeExecutor, planTools *tools.PlanToolExecutor, toolCatalog *tools.CatalogExecutor, toolExecution *tools.ExecutionGate, transport *provider.TransportExecutor) (*provider.Client, error) {
	factory, ok := r.providerClients[name]
	if !ok {
		return nil, fmt.Errorf("provider client %q is not registered", name)
	}
	return factory(promptAssets, requestShape, planTools, toolCatalog, toolExecution, transport), nil
}

func (r *ComponentRegistry) BuildProjections(names ...string) ([]projections.Projection, error) {
	return r.projections.Build(names...)
}
