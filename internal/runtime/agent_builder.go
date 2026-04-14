package runtime

import (
	"fmt"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/provider"
	"teamd/internal/runtime/projections"
)

type Agent struct {
	Config      config.AgentConfig
	Contracts   contracts.ResolvedContracts
	Transport   *provider.TransportExecutor
	EventLog    EventLog
	Projections []projections.Projection
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

	projectionSet, err := projections.NewBuiltInRegistry().BuildDefaults()
	if err != nil {
		return nil, fmt.Errorf("build projections: %w", err)
	}
	contracts, err := ResolveContracts(cfg)
	if err != nil {
		return nil, fmt.Errorf("resolve contracts: %w", err)
	}

	return &Agent{
		Config:      cfg,
		Contracts:   contracts,
		Transport:   provider.NewTransportExecutor(nil),
		EventLog:    NewInMemoryEventLog(),
		Projections: projectionSet,
	}, nil
}
