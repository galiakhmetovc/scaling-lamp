package provider

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"

	"teamd/internal/contracts"
	itools "teamd/internal/tools"
)

type ClientInput struct {
	PromptAssetSelection []string
	Messages             []contracts.Message
	Tools                []itools.Definition
	AttemptObserver      func(AttemptTrace)
	StreamObserver       func(StreamEvent)
}

type ClientResult struct {
	RequestBody       []byte
	Transport         Response
	Provider          ProviderResponse
	TransportAttempts []AttemptTrace
	ToolDecisions     []ToolDecision
}

type Client struct {
	PromptAssets  *PromptAssetExecutor
	RequestShape  *RequestShapeExecutor
	PlanTools     *itools.PlanToolExecutor
	ToolCatalog   *itools.CatalogExecutor
	ToolExecution *itools.ExecutionGate
	Transport     *TransportExecutor
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
	ToolCalls    []ToolCall
}

type ToolCall struct {
	ID        string
	Name      string
	Arguments map[string]any
}

type ToolDecision struct {
	ToolID   string
	Decision itools.ExecutionDecision
}

func NewClient(promptAssets *PromptAssetExecutor, requestShape *RequestShapeExecutor, planTools *itools.PlanToolExecutor, toolCatalog *itools.CatalogExecutor, toolExecution *itools.ExecutionGate, transport *TransportExecutor) *Client {
	return &Client{
		PromptAssets:  promptAssets,
		RequestShape:  requestShape,
		PlanTools:     planTools,
		ToolCatalog:   toolCatalog,
		ToolExecution: toolExecution,
		Transport:     transport,
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
	if c.PlanTools == nil {
		return ClientResult{}, fmt.Errorf("provider client plan tool executor is nil")
	}
	if c.ToolCatalog == nil {
		return ClientResult{}, fmt.Errorf("provider client tool catalog executor is nil")
	}
	if c.ToolExecution == nil {
		return ClientResult{}, fmt.Errorf("provider client tool execution gate is nil")
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

	planTools, err := c.PlanTools.Build(contractSet.PlanTools)
	if err != nil {
		return ClientResult{}, fmt.Errorf("build plan tools: %w", err)
	}
	availableTools := make([]itools.Definition, 0, len(planTools)+len(input.Tools))
	availableTools = append(availableTools, planTools...)
	availableTools = append(availableTools, input.Tools...)
	visibleTools, err := c.ToolCatalog.Build(contractSet.Tools, itools.CatalogInput{
		Available: availableTools,
	})
	if err != nil {
		return ClientResult{}, fmt.Errorf("build visible tools: %w", err)
	}
	serializedTools, err := c.ToolCatalog.Serialize(contractSet.Tools, visibleTools)
	if err != nil {
		return ClientResult{}, fmt.Errorf("serialize visible tools: %w", err)
	}

	requestBody, err := c.RequestShape.Build(contractSet.ProviderRequest.RequestShape, RequestShapeInput{
		PrependPromptAssets: resolvedPromptAssets.Prepend,
		AppendPromptAssets:  resolvedPromptAssets.Append,
		Messages:            input.Messages,
		Tools:               serializedTools,
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
	decisions, err := c.evaluateToolCalls(contractSet.ToolExecution, parsed.ToolCalls)
	if err != nil {
		return ClientResult{
			RequestBody:       requestBody,
			Transport:         response,
			Provider:          parsed,
			TransportAttempts: attempts,
			ToolDecisions:     decisions,
		}, err
	}

	return ClientResult{
		RequestBody:       requestBody,
		Transport:         response,
		Provider:          parsed,
		TransportAttempts: attempts,
		ToolDecisions:     decisions,
	}, nil
}

func (c *Client) evaluateToolCalls(contract contracts.ToolExecutionContract, calls []ToolCall) ([]ToolDecision, error) {
	if len(calls) == 0 {
		return nil, nil
	}
	out := make([]ToolDecision, 0, len(calls))
	for _, call := range calls {
		decision, err := c.ToolExecution.Evaluate(contract, call.Name)
		if err != nil {
			return out, err
		}
		out = append(out, ToolDecision{ToolID: call.Name, Decision: decision})
		if !decision.Allowed {
			return out, fmt.Errorf("tool call %q denied: %s", call.Name, decision.Reason)
		}
		if decision.ApprovalRequired {
			return out, fmt.Errorf("tool call %q requires approval", call.Name)
		}
	}
	return out, nil
}

func applyProviderStreamChunk(out *ProviderResponse, data []byte, onEvent func(StreamEvent)) error {
	var raw struct {
		ID      string `json:"id"`
		Model   string `json:"model"`
		Choices []struct {
			FinishReason string `json:"finish_reason"`
			Delta        struct {
				Role             string `json:"role"`
				Content          string `json:"content"`
				ReasoningContent string `json:"reasoning_content"`
				Reasoning        string `json:"reasoning"`
			} `json:"delta"`
			Message struct {
				Role    string `json:"role"`
				Content any    `json:"content"`
			} `json:"message"`
		} `json:"choices"`
		OutputText string `json:"output_text"`
		Usage      struct {
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
			if onEvent != nil {
				onEvent(StreamEvent{Kind: StreamEventText, Text: raw.Choices[0].Delta.Content})
			}
		}
		if raw.Choices[0].Delta.ReasoningContent != "" && onEvent != nil {
			onEvent(StreamEvent{Kind: StreamEventReasoning, Text: raw.Choices[0].Delta.ReasoningContent})
		}
		if raw.Choices[0].Delta.Reasoning != "" && onEvent != nil {
			onEvent(StreamEvent{Kind: StreamEventReasoning, Text: raw.Choices[0].Delta.Reasoning})
		}
		if text := extractMessageContentText(raw.Choices[0].Message.Content); text != "" {
			out.Message.Role = raw.Choices[0].Message.Role
			out.Message.Content += text
			if onEvent != nil {
				onEvent(StreamEvent{Kind: StreamEventText, Text: text})
			}
		}
		if raw.Choices[0].FinishReason != "" {
			out.FinishReason = raw.Choices[0].FinishReason
		}
	}
	if raw.OutputText != "" {
		out.Message.Content += raw.OutputText
		if onEvent != nil {
			onEvent(StreamEvent{Kind: StreamEventText, Text: raw.OutputText})
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

func extractMessageContentText(value any) string {
	switch typed := value.(type) {
	case string:
		return typed
	case []any:
		var out strings.Builder
		for _, item := range typed {
			part, ok := item.(map[string]any)
			if !ok {
				continue
			}
			if text, ok := part["text"].(string); ok {
				out.WriteString(text)
			}
		}
		return out.String()
	default:
		return ""
	}
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
				Role      string `json:"role"`
				Content   string `json:"content"`
				ToolCalls []struct {
					ID       string `json:"id"`
					Function struct {
						Name      string          `json:"name"`
						Arguments json.RawMessage `json:"arguments"`
					} `json:"function"`
				} `json:"tool_calls"`
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
	toolCalls, err := decodeToolCalls(raw.Choices[0].Message.ToolCalls)
	if err != nil {
		return ProviderResponse{}, err
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
		ToolCalls: toolCalls,
	}, nil
}

func decodeToolCalls(rawCalls []struct {
	ID       string `json:"id"`
	Function struct {
		Name      string          `json:"name"`
		Arguments json.RawMessage `json:"arguments"`
	} `json:"function"`
}) ([]ToolCall, error) {
	if len(rawCalls) == 0 {
		return nil, nil
	}
	out := make([]ToolCall, 0, len(rawCalls))
	for _, raw := range rawCalls {
		args, err := decodeToolArguments(raw.Function.Arguments)
		if err != nil {
			return nil, fmt.Errorf("decode tool call arguments for %q: %w", raw.Function.Name, err)
		}
		out = append(out, ToolCall{
			ID:        raw.ID,
			Name:      raw.Function.Name,
			Arguments: args,
		})
	}
	return out, nil
}

func decodeToolArguments(raw json.RawMessage) (map[string]any, error) {
	if len(strings.TrimSpace(string(raw))) == 0 || strings.TrimSpace(string(raw)) == "null" {
		return nil, nil
	}
	var object map[string]any
	if err := json.Unmarshal(raw, &object); err == nil {
		return object, nil
	}
	var wrapped string
	if err := json.Unmarshal(raw, &wrapped); err != nil {
		return nil, err
	}
	if strings.TrimSpace(wrapped) == "" {
		return nil, nil
	}
	if err := json.Unmarshal([]byte(wrapped), &object); err != nil {
		return nil, err
	}
	return object, nil
}
