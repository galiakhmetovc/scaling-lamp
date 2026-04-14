package main

import (
	"context"
	"time"
)

type Tool interface {
	Run(context.Context, map[string]string) (string, error)
}

type TimeTool struct{}

func (TimeTool) Run(_ context.Context, _ map[string]string) (string, error) {
	return time.Now().UTC().Format(time.RFC3339), nil
}
