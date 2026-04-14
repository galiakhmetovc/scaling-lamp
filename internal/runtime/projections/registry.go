package projections

import "fmt"

type Factory func() Projection

type Registry struct {
	factories map[string]Factory
}

func NewRegistry() *Registry {
	return &Registry{factories: map[string]Factory{}}
}

func NewBuiltInRegistry() *Registry {
	registry := NewRegistry()
	registry.Register("session", func() Projection { return NewSessionProjection() })
	registry.Register("run", func() Projection { return NewRunProjection() })
	registry.Register("transcript", func() Projection { return NewTranscriptProjection() })
	return registry
}

func (r *Registry) Register(name string, factory Factory) {
	r.factories[name] = factory
}

func (r *Registry) Build(names ...string) ([]Projection, error) {
	out := make([]Projection, 0, len(names))
	for _, name := range names {
		factory, ok := r.factories[name]
		if !ok {
			return nil, fmt.Errorf("projection %q is not registered", name)
		}
		out = append(out, factory())
	}
	return out, nil
}

func (r *Registry) BuildDefaults() ([]Projection, error) {
	return r.Build("session", "run", "transcript")
}
