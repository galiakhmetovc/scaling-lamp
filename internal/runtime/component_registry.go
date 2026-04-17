package runtime

import (
	"fmt"

	"teamd/internal/config"
	"teamd/internal/delegation"
	"teamd/internal/filesystem"
	"teamd/internal/promptassembly"
	"teamd/internal/provider"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
	"teamd/internal/tools"
)

type EventLogFactory func(runtimeConfig config.AgentRuntimeConfig) (EventLog, error)
type PromptAssemblyExecutorFactory func() *promptassembly.Executor
type PromptAssetExecutorFactory func() *provider.PromptAssetExecutor
type TransportExecutorFactory func() *provider.TransportExecutor
type RequestShapeExecutorFactory func() *provider.RequestShapeExecutor
type PlanToolExecutorFactory func() *tools.PlanToolExecutor
type FilesystemToolExecutorFactory func() *filesystem.DefinitionExecutor
type ShellToolExecutorFactory func() *shell.DefinitionExecutor
type DelegationToolExecutorFactory func() *delegation.DefinitionExecutor
type ToolCatalogExecutorFactory func() *tools.CatalogExecutor
type ToolExecutionGateFactory func() *tools.ExecutionGate
type ProviderClientFactory func(*provider.PromptAssetExecutor, *provider.RequestShapeExecutor, *tools.PlanToolExecutor, *filesystem.DefinitionExecutor, *shell.DefinitionExecutor, *delegation.DefinitionExecutor, *tools.CatalogExecutor, *tools.ExecutionGate, *provider.TransportExecutor) *provider.Client

type ComponentRegistry struct {
	eventLogs               map[string]EventLogFactory
	promptAssemblyExecutors map[string]PromptAssemblyExecutorFactory
	promptAssetExecutors    map[string]PromptAssetExecutorFactory
	transportExecutors      map[string]TransportExecutorFactory
	requestShapeExecutors   map[string]RequestShapeExecutorFactory
	planToolExecutors       map[string]PlanToolExecutorFactory
	filesystemToolExecutors map[string]FilesystemToolExecutorFactory
	shellToolExecutors      map[string]ShellToolExecutorFactory
	delegationToolExecutors map[string]DelegationToolExecutorFactory
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
		filesystemToolExecutors: map[string]FilesystemToolExecutorFactory{},
		shellToolExecutors:      map[string]ShellToolExecutorFactory{},
		delegationToolExecutors: map[string]DelegationToolExecutorFactory{},
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
	registry.RegisterFilesystemToolExecutor("filesystem_tool_default", func() *filesystem.DefinitionExecutor {
		return filesystem.NewDefinitionExecutor()
	})
	registry.RegisterShellToolExecutor("shell_tool_default", func() *shell.DefinitionExecutor {
		return shell.NewDefinitionExecutor()
	})
	registry.RegisterDelegationToolExecutor("delegation_tool_default", func() *delegation.DefinitionExecutor {
		return delegation.NewDefinitionExecutor()
	})
	registry.RegisterToolExecutionGate("tool_execution_default", func() *tools.ExecutionGate {
		return tools.NewExecutionGate()
	})
	registry.RegisterProviderClient("provider_client_default", func(promptAssets *provider.PromptAssetExecutor, requestShape *provider.RequestShapeExecutor, planTools *tools.PlanToolExecutor, filesystemTools *filesystem.DefinitionExecutor, shellTools *shell.DefinitionExecutor, delegationTools *delegation.DefinitionExecutor, toolCatalog *tools.CatalogExecutor, toolExecution *tools.ExecutionGate, transport *provider.TransportExecutor) *provider.Client {
		return provider.NewClient(promptAssets, requestShape, planTools, filesystemTools, shellTools, delegationTools, toolCatalog, toolExecution, transport)
	})
	registry.RegisterProjection("session", func() projections.Projection { return projections.NewSessionProjection() })
	registry.RegisterProjection("session_catalog", func() projections.Projection { return projections.NewSessionCatalogProjection() })
	registry.RegisterProjection("session_prompt", func() projections.Projection { return projections.NewSessionPromptProjection() })
	registry.RegisterProjection("run", func() projections.Projection { return projections.NewRunProjection() })
	registry.RegisterProjection("transcript", func() projections.Projection { return projections.NewTranscriptProjection() })
	registry.RegisterProjection("chat_timeline", func() projections.Projection { return projections.NewChatTimelineProjection() })
	registry.RegisterProjection("context_budget", func() projections.Projection { return projections.NewContextBudgetProjection() })
	registry.RegisterProjection("context_summary", func() projections.Projection { return projections.NewContextSummaryProjection() })
	registry.RegisterProjection("filesystem_head", func() projections.Projection { return projections.NewFilesystemHeadProjection() })
	registry.RegisterProjection("delegate", func() projections.Projection { return projections.NewDelegateProjection() })
	registry.RegisterProjection("shell_command", func() projections.Projection { return projections.NewShellCommandProjection() })
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

func (r *ComponentRegistry) RegisterFilesystemToolExecutor(name string, factory FilesystemToolExecutorFactory) {
	r.filesystemToolExecutors[name] = factory
}

func (r *ComponentRegistry) RegisterShellToolExecutor(name string, factory ShellToolExecutorFactory) {
	r.shellToolExecutors[name] = factory
}

func (r *ComponentRegistry) RegisterDelegationToolExecutor(name string, factory DelegationToolExecutorFactory) {
	r.delegationToolExecutors[name] = factory
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

func (r *ComponentRegistry) BuildFilesystemToolExecutor(name string) (*filesystem.DefinitionExecutor, error) {
	factory, ok := r.filesystemToolExecutors[name]
	if !ok {
		return nil, fmt.Errorf("filesystem tool executor %q is not registered", name)
	}
	return factory(), nil
}

func (r *ComponentRegistry) BuildShellToolExecutor(name string) (*shell.DefinitionExecutor, error) {
	factory, ok := r.shellToolExecutors[name]
	if !ok {
		return nil, fmt.Errorf("shell tool executor %q is not registered", name)
	}
	return factory(), nil
}

func (r *ComponentRegistry) BuildDelegationToolExecutor(name string) (*delegation.DefinitionExecutor, error) {
	factory, ok := r.delegationToolExecutors[name]
	if !ok {
		return nil, fmt.Errorf("delegation tool executor %q is not registered", name)
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

func (r *ComponentRegistry) BuildProviderClient(name string, promptAssets *provider.PromptAssetExecutor, requestShape *provider.RequestShapeExecutor, planTools *tools.PlanToolExecutor, filesystemTools *filesystem.DefinitionExecutor, shellTools *shell.DefinitionExecutor, delegationTools *delegation.DefinitionExecutor, toolCatalog *tools.CatalogExecutor, toolExecution *tools.ExecutionGate, transport *provider.TransportExecutor) (*provider.Client, error) {
	factory, ok := r.providerClients[name]
	if !ok {
		return nil, fmt.Errorf("provider client %q is not registered", name)
	}
	return factory(promptAssets, requestShape, planTools, filesystemTools, shellTools, delegationTools, toolCatalog, toolExecution, transport), nil
}

func (r *ComponentRegistry) BuildProjections(names ...string) ([]projections.Projection, error) {
	return r.projections.Build(names...)
}
