package projections

import "fmt"

type Factory func() Projection

type Registry struct {
	factories map[string]Factory
}

func NewRegistry() *Registry {
	return &Registry{factories: map[string]Factory{}}
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
