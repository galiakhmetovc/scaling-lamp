package runtime

import (
	"fmt"

	"teamd/internal/config"
	"teamd/internal/runtime/projections"
)

type Agent struct {
	Config      config.AgentConfig
	EventLog    EventLog
	Projections []projections.Projection
}

func BuildAgent(configPath string) (*Agent, error) {
	cfg, err := config.LoadRoot(configPath)
	if err != nil {
		return nil, err
	}

	moduleRegistry := config.NewModuleRegistry()
	moduleRegistry.Register("TransportContractConfig")

	transportHeader, err := config.LoadModuleHeader(cfg.Spec.Contracts.TransportPath)
	if err != nil {
		return nil, err
	}
	if err := moduleRegistry.ValidateKind(transportHeader.Kind); err != nil {
		return nil, fmt.Errorf("validate transport module: %w", err)
	}

	projectionRegistry := projections.NewRegistry()
	projectionRegistry.Register("session", func() projections.Projection { return projections.NewSessionProjection() })
	projectionRegistry.Register("run", func() projections.Projection { return projections.NewRunProjection() })
	projectionSet, err := projectionRegistry.Build("session", "run")
	if err != nil {
		return nil, fmt.Errorf("build projections: %w", err)
	}

	return &Agent{
		Config:   cfg,
		EventLog: NewInMemoryEventLog(),
		Projections: projectionSet,
	}, nil
}
