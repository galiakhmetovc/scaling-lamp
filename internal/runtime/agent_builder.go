package runtime

import (
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

	return &Agent{
		Config:   cfg,
		EventLog: NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewRunProjection(),
		},
	}, nil
}
