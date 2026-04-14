package provider

import (
	"context"
	"time"
)

type Message struct {
	Role       string
	Content    string
	Name       string
	ToolCallID string
	ToolCalls  []ToolCall
}

type ToolDefinition struct {
	Name        string
	Description string
	Parameters  map[string]any
}

type ToolCall struct {
	ID        string
	Name      string
	Arguments map[string]any
}

type PromptRequest struct {
	WorkerID string
	Messages []Message
	Tools    []ToolDefinition
	Config   RequestConfig
	Transport TransportConfig
}

type RequestAuth struct {
	Header string
	Prefix string
	Value  string
}

type TransportConfig struct {
	BaseURL string
	Path    string
	Headers map[string]string
	Auth    *RequestAuth
	Timeout time.Duration
}

type RequestConfig struct {
	Model          string   `json:"model,omitempty"`
	ReasoningMode  string   `json:"reasoning_mode,omitempty"`
	ClearThinking  *bool    `json:"clear_thinking,omitempty"`
	Temperature    *float64 `json:"temperature,omitempty"`
	TopP           *float64 `json:"top_p,omitempty"`
	MaxTokens      *int     `json:"max_tokens,omitempty"`
	DoSample       *bool    `json:"do_sample,omitempty"`
	ResponseFormat string   `json:"response_format,omitempty"`
}

type Usage struct {
	PromptTokens     int
	CompletionTokens int
	TotalTokens      int
	CachedTokens     int
}

type ReasoningSettings struct {
	Mode          string
	ClearThinking bool
}

type PromptResponse struct {
	Text             string
	Model            string
	Reasoning        ReasoningSettings
	Usage            Usage
	ReasoningContent string
	FinishReason     string
	ToolCalls        []ToolCall
}

type Provider interface {
	Generate(context.Context, PromptRequest) (PromptResponse, error)
}

type FakeProvider struct{}

func (FakeProvider) Generate(_ context.Context, req PromptRequest) (PromptResponse, error) {
	text := "ok"
	if len(req.Messages) > 0 {
		text = req.Messages[len(req.Messages)-1].Content
	}
	return PromptResponse{
		Text:  text,
		Model: "fake",
	}, nil
}
