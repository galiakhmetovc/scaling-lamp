package mcp

import "context"

type Tool struct {
	Name        string
	Description string
	Parameters  map[string]any
	Call        func(context.Context, CallInput) (CallResult, error)
}

type CallInput struct {
	Arguments map[string]any
}

type CallResult struct {
	Content string
}
