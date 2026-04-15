package runtime

import (
	"context"
	"fmt"
	"time"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/filesystem"
	"teamd/internal/promptassembly"
	"teamd/internal/provider"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
	"teamd/internal/tools"
)

type Agent struct {
	Config          config.AgentConfig
	ConfigPath      string
	MaxToolRounds   int
	Contracts       contracts.ResolvedContracts
	PromptAssembly  *promptassembly.Executor
	PromptAssets    *provider.PromptAssetExecutor
	Transport       *provider.TransportExecutor
	RequestShape    *provider.RequestShapeExecutor
	PlanTools       *tools.PlanToolExecutor
	FilesystemTools *filesystem.DefinitionExecutor
	ShellTools      *shell.DefinitionExecutor
	ShellRuntime    *shell.Executor
	ToolCatalog     *tools.CatalogExecutor
	ToolExecution   *tools.ExecutionGate
	ProviderClient  *provider.Client
	EventLog        EventLog
	Projections     []projections.Projection
	ProjectionStore projections.Store
	UIBus           *UIEventBus
	Now             func() time.Time
	NewID           func(prefix string) string
}

func (a *Agent) RecordEvent(ctx context.Context, event eventing.Event) error {
	if err := a.EventLog.Append(ctx, event); err != nil {
		return fmt.Errorf("append event: %w", err)
	}
	for _, projection := range a.Projections {
		if err := projection.Apply(event); err != nil {
			return fmt.Errorf("apply event to projection %q: %w", projection.ID(), err)
		}
	}
	if a.ProjectionStore != nil {
		if err := a.ProjectionStore.Save(a.Projections); err != nil {
			return fmt.Errorf("save projection snapshots: %w", err)
		}
	}
	return nil
}

func BuildAgent(configPath string) (*Agent, error) {
	cfg, err := config.LoadRoot(configPath)
	if err != nil {
		return nil, err
	}
	moduleRegistry := config.NewBuiltInModuleRegistry()
	graph, err := config.LoadModuleGraph(cfg, moduleRegistry)
	if err != nil {
		return nil, err
	}

	for _, contractHeader := range graph.Contracts {
		if err := moduleRegistry.ValidateKind(contractHeader.Kind); err != nil {
			return nil, fmt.Errorf("validate contract module %q: %w", contractHeader.ID, err)
		}
	}
	for _, policyHeader := range graph.Policies {
		if err := moduleRegistry.ValidateKind(policyHeader.Kind); err != nil {
			return nil, fmt.Errorf("validate policy module %q: %w", policyHeader.ID, err)
		}
	}

	componentRegistry := NewBuiltInComponentRegistry()
	contracts, err := ResolveContracts(cfg)
	if err != nil {
		return nil, fmt.Errorf("resolve contracts: %w", err)
	}
	eventLog, err := componentRegistry.BuildEventLog(cfg.Spec.Runtime.EventLog, cfg.Spec.Runtime)
	if err != nil {
		return nil, fmt.Errorf("build event log: %w", err)
	}
	projectionSet, err := componentRegistry.BuildProjections(cfg.Spec.Runtime.Projections...)
	if err != nil {
		return nil, fmt.Errorf("build projections: %w", err)
	}
	var projectionStore projections.Store
	if cfg.Spec.Runtime.ProjectionStorePath != "" {
		projectionStore, err = projections.NewJSONFileStore(cfg.Spec.Runtime.ProjectionStorePath)
		if err != nil {
			return nil, fmt.Errorf("build projection store: %w", err)
		}
		if err := projectionStore.Load(projectionSet); err != nil {
			return nil, fmt.Errorf("load projection store: %w", err)
		}
	}
	promptAssemblyName := cfg.Spec.Runtime.PromptAssemblyExecutor
	if promptAssemblyName == "" {
		promptAssemblyName = "prompt_assembly_default"
	}
	promptAssemblyExecutor, err := componentRegistry.BuildPromptAssemblyExecutor(promptAssemblyName)
	if err != nil {
		return nil, fmt.Errorf("build prompt-assembly executor: %w", err)
	}
	transportExecutor, err := componentRegistry.BuildTransportExecutor(cfg.Spec.Runtime.TransportExecutor)
	if err != nil {
		return nil, fmt.Errorf("build transport executor: %w", err)
	}
	requestShapeExecutor, err := componentRegistry.BuildRequestShapeExecutor(cfg.Spec.Runtime.RequestShapeExecutor)
	if err != nil {
		return nil, fmt.Errorf("build request-shape executor: %w", err)
	}
	toolCatalogName := cfg.Spec.Runtime.ToolCatalogExecutor
	if toolCatalogName == "" {
		toolCatalogName = "tool_catalog_default"
	}
	planToolExecutor, err := componentRegistry.BuildPlanToolExecutor("plan_tool_default")
	if err != nil {
		return nil, fmt.Errorf("build plan tool executor: %w", err)
	}
	filesystemToolExecutor, err := componentRegistry.BuildFilesystemToolExecutor("filesystem_tool_default")
	if err != nil {
		return nil, fmt.Errorf("build filesystem tool executor: %w", err)
	}
	shellToolExecutor, err := componentRegistry.BuildShellToolExecutor("shell_tool_default")
	if err != nil {
		return nil, fmt.Errorf("build shell tool executor: %w", err)
	}
	toolCatalogExecutor, err := componentRegistry.BuildToolCatalogExecutor(toolCatalogName)
	if err != nil {
		return nil, fmt.Errorf("build tool catalog executor: %w", err)
	}
	toolExecutionName := cfg.Spec.Runtime.ToolExecutionGate
	if toolExecutionName == "" {
		toolExecutionName = "tool_execution_default"
	}
	toolExecutionGate, err := componentRegistry.BuildToolExecutionGate(toolExecutionName)
	if err != nil {
		return nil, fmt.Errorf("build tool execution gate: %w", err)
	}
	promptAssetExecutor, err := componentRegistry.BuildPromptAssetExecutor(cfg.Spec.Runtime.PromptAssetExecutor)
	if err != nil {
		return nil, fmt.Errorf("build prompt-asset executor: %w", err)
	}
	providerClient, err := componentRegistry.BuildProviderClient(cfg.Spec.Runtime.ProviderClient, promptAssetExecutor, requestShapeExecutor, planToolExecutor, filesystemToolExecutor, shellToolExecutor, toolCatalogExecutor, toolExecutionGate, transportExecutor)
	if err != nil {
		return nil, fmt.Errorf("build provider client: %w", err)
	}
	maxToolRounds := cfg.Spec.Runtime.MaxToolRounds
	if maxToolRounds <= 0 {
		maxToolRounds = 4
	}

	return &Agent{
		Config:          cfg,
		ConfigPath:      configPath,
		MaxToolRounds:   maxToolRounds,
		Contracts:       contracts,
		PromptAssembly:  promptAssemblyExecutor,
		PromptAssets:    promptAssetExecutor,
		Transport:       transportExecutor,
		RequestShape:    requestShapeExecutor,
		PlanTools:       planToolExecutor,
		FilesystemTools: filesystemToolExecutor,
		ShellTools:      shellToolExecutor,
		ShellRuntime:    shell.NewExecutor(),
		ToolCatalog:     toolCatalogExecutor,
		ToolExecution:   toolExecutionGate,
		ProviderClient:  providerClient,
		EventLog:        eventLog,
		Projections:     projectionSet,
		ProjectionStore: projectionStore,
		UIBus:           NewUIEventBus(),
		Now:             time.Now,
		NewID: func(prefix string) string {
			return fmt.Sprintf("%s-%d", prefix, time.Now().UTC().UnixNano())
		},
	}, nil
}
