package runtime

import (
	"fmt"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/provider"
	"teamd/internal/runtime/projections"
)

type Agent struct {
	Config       config.AgentConfig
	Contracts    contracts.ResolvedContracts
	Transport    *provider.TransportExecutor
	RequestShape *provider.RequestShapeExecutor
	EventLog     EventLog
	Projections  []projections.Projection
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
	eventLog, err := componentRegistry.BuildEventLog(cfg.Spec.Runtime.EventLog)
	if err != nil {
		return nil, fmt.Errorf("build event log: %w", err)
	}
	projectionSet, err := componentRegistry.BuildProjections(cfg.Spec.Runtime.Projections...)
	if err != nil {
		return nil, fmt.Errorf("build projections: %w", err)
	}
	transportExecutor, err := componentRegistry.BuildTransportExecutor(cfg.Spec.Runtime.TransportExecutor)
	if err != nil {
		return nil, fmt.Errorf("build transport executor: %w", err)
	}
	requestShapeExecutor, err := componentRegistry.BuildRequestShapeExecutor(cfg.Spec.Runtime.RequestShapeExecutor)
	if err != nil {
		return nil, fmt.Errorf("build request-shape executor: %w", err)
	}

	return &Agent{
		Config:       cfg,
		Contracts:    contracts,
		Transport:    transportExecutor,
		RequestShape: requestShapeExecutor,
		EventLog:     eventLog,
		Projections:  projectionSet,
	}, nil
}
