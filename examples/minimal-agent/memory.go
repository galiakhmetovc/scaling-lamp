package main

import (
	"context"
	"fmt"
)

type MemoryStore struct {
	history []Message
}

func NewMemoryStore() *MemoryStore {
	return &MemoryStore{}
}

func (m *MemoryStore) Append(message Message) {
	m.history = append(m.history, message)
}

func (m *MemoryStore) Messages() []Message {
	out := make([]Message, len(m.history))
	copy(out, m.history)
	return out
}

type Engine struct {
	Provider Provider
	Tools    map[string]Tool
	Memory   *MemoryStore
}

func (e Engine) Handle(ctx context.Context, userText string) (string, error) {
	e.Memory.Append(Message{Role: "user", Content: userText})
	for {
		resp, err := e.Provider.Generate(ctx, e.Memory.Messages())
		if err != nil {
			return "", err
		}
		if len(resp.ToolCalls) == 0 {
			e.Memory.Append(Message{Role: "assistant", Content: resp.Text})
			return resp.Text, nil
		}
		for _, call := range resp.ToolCalls {
			tool, ok := e.Tools[call.Name]
			if !ok {
				return "", fmt.Errorf("unknown tool: %s", call.Name)
			}
			result, err := tool.Run(ctx, call.Args)
			if err != nil {
				return "", err
			}
			e.Memory.Append(Message{Role: "tool", Content: call.Name + ": " + result})
		}
	}
}
