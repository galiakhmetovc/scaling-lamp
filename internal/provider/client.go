package provider

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"

	"teamd/internal/contracts"
)

type ClientInput struct {
	PromptAssetSelection []string
	Messages             []contracts.Message
	Tools                []map[string]any
	AttemptObserver      func(AttemptTrace)
	StreamObserver       func(string)
}

type ClientResult struct {
	RequestBody      []byte
	Transport        Response
	Provider         ProviderResponse
	TransportAttempts []AttemptTrace
}

type Client struct {
	PromptAssets *PromptAssetExecutor
	RequestShape *RequestShapeExecutor
	Transport    *TransportExecutor
}

type Usage struct {
	InputTokens  int
	OutputTokens int
	TotalTokens  int
}

type ProviderResponse struct {
	ID           string
	Model        string
	Message      contracts.Message
	FinishReason string
	Usage        Usage
}

func NewClient(promptAssets *PromptAssetExecutor, requestShape *RequestShapeExecutor, transport *TransportExecutor) *Client {
	return &Client{
		PromptAssets: promptAssets,
		RequestShape: requestShape,
		Transport:    transport,
	}
}

func (c *Client) Execute(ctx context.Context, contractSet contracts.ResolvedContracts, input ClientInput) (ClientResult, error) {
	if c == nil {
		return ClientResult{}, fmt.Errorf("provider client is nil")
	}
	if c.PromptAssets == nil {
		return ClientResult{}, fmt.Errorf("provider client prompt-asset executor is nil")
	}
	if c.RequestShape == nil {
		return ClientResult{}, fmt.Errorf("provider client request-shape executor is nil")
	}
	if c.Transport == nil {
		return ClientResult{}, fmt.Errorf("provider client transport executor is nil")
	}

	resolvedPromptAssets, err := c.PromptAssets.Build(contractSet.PromptAssets, PromptAssetInput{
		SelectedIDs: input.PromptAssetSelection,
	})
	if err != nil {
		return ClientResult{}, fmt.Errorf("build prompt assets: %w", err)
	}

	requestBody, err := c.RequestShape.Build(contractSet.ProviderRequest.RequestShape, RequestShapeInput{
		PrependPromptAssets: resolvedPromptAssets.Prepend,
		AppendPromptAssets:  resolvedPromptAssets.Append,
		Messages:            input.Messages,
		Tools:               input.Tools,
	})
	if err != nil {
		return ClientResult{}, fmt.Errorf("build provider request body: %w", err)
	}

	attempts := make([]AttemptTrace, 0, 4)
	var streamed ProviderResponse
	observer := func(trace AttemptTrace) {
		attempts = append(attempts, trace)
		if input.AttemptObserver != nil {
			input.AttemptObserver(trace)
		}
	}
	streamObserver := func(data []byte) error {
		if !contractSet.ProviderRequest.RequestShape.Streaming.Enabled || !contractSet.ProviderRequest.RequestShape.Streaming.Params.Stream {
			return nil
		}
		return applyProviderStreamChunk(&streamed, data, input.StreamObserver)
	}

	response, err := c.Transport.Execute(ctx, contractSet.ProviderRequest.Transport, Request{
		Body:            requestBody,
		ContentType:     "application/json",
		AttemptObserver: observer,
		StreamObserver:  streamObserver,
	})
	if err != nil {
		return ClientResult{
			RequestBody:       requestBody,
			TransportAttempts: attempts,
		}, fmt.Errorf("execute provider transport: %w", err)
	}
	parsed := streamed
	if contractSet.ProviderRequest.RequestShape.Streaming.Enabled && contractSet.ProviderRequest.RequestShape.Streaming.Params.Stream {
		if parsed.Message.Content == "" {
			return ClientResult{}, fmt.Errorf("provider stream returned no content")
		}
	} else {
		parsed, err = parseProviderResponse(response)
		if err != nil {
			return ClientResult{}, err
		}
	}

	return ClientResult{
		RequestBody:       requestBody,
		Transport:         response,
		Provider:          parsed,
		TransportAttempts: attempts,
	}, nil
}

func applyProviderStreamChunk(out *ProviderResponse, data []byte, onDelta func(string)) error {
	var raw struct {
		ID      string `json:"id"`
		Model   string `json:"model"`
		Choices []struct {
			FinishReason string `json:"finish_reason"`
			Delta        struct {
				Role    string `json:"role"`
				Content string `json:"content"`
			} `json:"delta"`
		} `json:"choices"`
		Usage struct {
			PromptTokens     int `json:"prompt_tokens"`
			CompletionTokens int `json:"completion_tokens"`
			TotalTokens      int `json:"total_tokens"`
		} `json:"usage"`
	}
	if err := json.Unmarshal(data, &raw); err != nil {
		return fmt.Errorf("decode provider stream chunk: %w", err)
	}
	if raw.ID != "" {
		out.ID = raw.ID
	}
	if raw.Model != "" {
		out.Model = raw.Model
	}
	if len(raw.Choices) > 0 {
		if raw.Choices[0].Delta.Role != "" {
			out.Message.Role = raw.Choices[0].Delta.Role
		}
		if raw.Choices[0].Delta.Content != "" {
			out.Message.Content += raw.Choices[0].Delta.Content
			if onDelta != nil {
				onDelta(raw.Choices[0].Delta.Content)
			}
		}
		if raw.Choices[0].FinishReason != "" {
			out.FinishReason = raw.Choices[0].FinishReason
		}
	}
	if raw.Usage.TotalTokens > 0 {
		out.Usage = Usage{
			InputTokens:  raw.Usage.PromptTokens,
			OutputTokens: raw.Usage.CompletionTokens,
			TotalTokens:  raw.Usage.TotalTokens,
		}
	}
	if out.Message.Role == "" {
		out.Message.Role = "assistant"
	}
	return nil
}

func parseProviderResponse(response Response) (ProviderResponse, error) {
	if response.StatusCode < http.StatusOK || response.StatusCode >= http.StatusMultipleChoices {
		body := strings.TrimSpace(string(response.Body))
		if body == "" {
			return ProviderResponse{}, fmt.Errorf("provider returned status %d", response.StatusCode)
		}
		return ProviderResponse{}, fmt.Errorf("provider returned status %d: %s", response.StatusCode, body)
	}

	var raw struct {
		ID      string `json:"id"`
		Model   string `json:"model"`
		Choices []struct {
			FinishReason string `json:"finish_reason"`
			Message      struct {
				Role    string `json:"role"`
				Content string `json:"content"`
			} `json:"message"`
		} `json:"choices"`
		Usage struct {
			PromptTokens     int `json:"prompt_tokens"`
			CompletionTokens int `json:"completion_tokens"`
			TotalTokens      int `json:"total_tokens"`
		} `json:"usage"`
	}
	if err := json.Unmarshal(response.Body, &raw); err != nil {
		return ProviderResponse{}, fmt.Errorf("decode provider response: %w", err)
	}
	if len(raw.Choices) == 0 {
		return ProviderResponse{}, fmt.Errorf("provider response has no choices")
	}

	return ProviderResponse{
		ID:    raw.ID,
		Model: raw.Model,
		Message: contracts.Message{
			Role:    raw.Choices[0].Message.Role,
			Content: raw.Choices[0].Message.Content,
		},
		FinishReason: raw.Choices[0].FinishReason,
		Usage: Usage{
			InputTokens:  raw.Usage.PromptTokens,
			OutputTokens: raw.Usage.CompletionTokens,
			TotalTokens:  raw.Usage.TotalTokens,
		},
	}, nil
}
