package config

import "fmt"

type ModuleRegistry struct {
	kinds map[string]struct{}
}

func NewModuleRegistry() *ModuleRegistry {
	return &ModuleRegistry{kinds: map[string]struct{}{}}
}

func (r *ModuleRegistry) Register(kind string) {
	r.kinds[kind] = struct{}{}
}

func (r *ModuleRegistry) ValidateKind(kind string) error {
	if _, ok := r.kinds[kind]; !ok {
		return fmt.Errorf("unsupported module kind %q", kind)
	}
	return nil
}
