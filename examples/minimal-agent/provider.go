package main

import (
	"context"
	"strings"
)

type Message struct {
	Role    string
	Content string
}

type ToolCall struct {
	Name string
	Args map[string]string
}

type ProviderResponse struct {
	Text      string
	ToolCalls []ToolCall
}

type Provider interface {
	Generate(context.Context, []Message) (ProviderResponse, error)
}

type EchoProvider struct{}

func (EchoProvider) Generate(_ context.Context, messages []Message) (ProviderResponse, error) {
	last := messages[len(messages)-1].Content
	if strings.Contains(strings.ToLower(last), "time") {
		return ProviderResponse{
			ToolCalls: []ToolCall{{Name: "time.now", Args: map[string]string{}}},
		}, nil
	}
	return ProviderResponse{Text: "echo: " + last}, nil
}
