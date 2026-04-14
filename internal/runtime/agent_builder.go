package runtime

import (
	"context"
	"fmt"
	"time"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/provider"
	"teamd/internal/runtime/eventing"
	"teamd/internal/runtime/projections"
)

type Agent struct {
	Config          config.AgentConfig
	Contracts       contracts.ResolvedContracts
	PromptAssets    *provider.PromptAssetExecutor
	Transport       *provider.TransportExecutor
	RequestShape    *provider.RequestShapeExecutor
	ProviderClient  *provider.Client
	EventLog        EventLog
	Projections     []projections.Projection
	ProjectionStore projections.Store
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
	transportExecutor, err := componentRegistry.BuildTransportExecutor(cfg.Spec.Runtime.TransportExecutor)
	if err != nil {
		return nil, fmt.Errorf("build transport executor: %w", err)
	}
	requestShapeExecutor, err := componentRegistry.BuildRequestShapeExecutor(cfg.Spec.Runtime.RequestShapeExecutor)
	if err != nil {
		return nil, fmt.Errorf("build request-shape executor: %w", err)
	}
	promptAssetExecutor, err := componentRegistry.BuildPromptAssetExecutor(cfg.Spec.Runtime.PromptAssetExecutor)
	if err != nil {
		return nil, fmt.Errorf("build prompt-asset executor: %w", err)
	}
	providerClient, err := componentRegistry.BuildProviderClient(cfg.Spec.Runtime.ProviderClient, promptAssetExecutor, requestShapeExecutor, transportExecutor)
	if err != nil {
		return nil, fmt.Errorf("build provider client: %w", err)
	}

	return &Agent{
		Config:          cfg,
		Contracts:       contracts,
		PromptAssets:    promptAssetExecutor,
		Transport:       transportExecutor,
		RequestShape:    requestShapeExecutor,
		ProviderClient:  providerClient,
		EventLog:        eventLog,
		Projections:     projectionSet,
		ProjectionStore: projectionStore,
		Now:             time.Now,
		NewID: func(prefix string) string {
			return fmt.Sprintf("%s-%d", prefix, time.Now().UTC().UnixNano())
		},
	}, nil
}
