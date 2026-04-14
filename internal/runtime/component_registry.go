package runtime

import (
	"fmt"

	"teamd/internal/config"
	"teamd/internal/provider"
	"teamd/internal/runtime/projections"
)

type EventLogFactory func(runtimeConfig config.AgentRuntimeConfig) (EventLog, error)
type TransportExecutorFactory func() *provider.TransportExecutor
type RequestShapeExecutorFactory func() *provider.RequestShapeExecutor

type ComponentRegistry struct {
	eventLogs             map[string]EventLogFactory
	transportExecutors    map[string]TransportExecutorFactory
	requestShapeExecutors map[string]RequestShapeExecutorFactory
	projections           *projections.Registry
}

func NewComponentRegistry() *ComponentRegistry {
	return &ComponentRegistry{
		eventLogs:             map[string]EventLogFactory{},
		transportExecutors:    map[string]TransportExecutorFactory{},
		requestShapeExecutors: map[string]RequestShapeExecutorFactory{},
		projections:           projections.NewRegistry(),
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
	registry.RegisterTransportExecutor("transport_default", func() *provider.TransportExecutor {
		return provider.NewTransportExecutor(nil)
	})
	registry.RegisterRequestShapeExecutor("request_shape_default", func() *provider.RequestShapeExecutor {
		return provider.NewRequestShapeExecutor()
	})
	registry.RegisterProjection("session", func() projections.Projection { return projections.NewSessionProjection() })
	registry.RegisterProjection("run", func() projections.Projection { return projections.NewRunProjection() })
	return registry
}

func (r *ComponentRegistry) RegisterEventLog(name string, factory EventLogFactory) {
	r.eventLogs[name] = factory
}

func (r *ComponentRegistry) RegisterTransportExecutor(name string, factory TransportExecutorFactory) {
	r.transportExecutors[name] = factory
}

func (r *ComponentRegistry) RegisterRequestShapeExecutor(name string, factory RequestShapeExecutorFactory) {
	r.requestShapeExecutors[name] = factory
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

func (r *ComponentRegistry) BuildProjections(names ...string) ([]projections.Projection, error) {
	return r.projections.Build(names...)
}
