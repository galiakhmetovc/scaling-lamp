package mcp

import (
	"context"
	"fmt"
	"sort"
	"sync"
)

type Server struct {
	Name string
}

type Registry interface {
	List(role string) ([]Server, error)
}

type Runtime struct {
	mu    sync.RWMutex
	tools map[string]Tool
}

type StaticRegistry struct {
	Servers []Server
}

func NewRuntime() *Runtime {
	return &Runtime{tools: map[string]Tool{}}
}

func (r StaticRegistry) List(string) ([]Server, error) {
	return r.Servers, nil
}

func (r *Runtime) Register(tool Tool) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.tools[tool.Name] = tool
}

func (r *Runtime) ListTools(string) ([]Tool, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	names := make([]string, 0, len(r.tools))
	for name := range r.tools {
		names = append(names, name)
	}
	sort.Strings(names)

	tools := make([]Tool, 0, len(names))
	for _, name := range names {
		tools = append(tools, r.tools[name])
	}
	return tools, nil
}

func (r *Runtime) List(role string) ([]Server, error) {
	tools, err := r.ListTools(role)
	if err != nil {
		return nil, err
	}

	servers := make([]Server, 0, len(tools))
	for _, tool := range tools {
		servers = append(servers, Server{Name: tool.Name})
	}
	return servers, nil
}

func (r *Runtime) CallTool(ctx context.Context, name string, input CallInput) (CallResult, error) {
	r.mu.RLock()
	tool, ok := r.tools[name]
	r.mu.RUnlock()
	if !ok {
		return CallResult{}, fmt.Errorf("tool %q not registered", name)
	}
	if tool.Call == nil {
		return CallResult{}, fmt.Errorf("tool %q has no implementation", name)
	}
	return tool.Call(ctx, input)
}
