package zai

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"teamd/internal/llmtrace"
	"teamd/internal/provider"
)

type Client struct {
	BaseURL       string
	APIKey        string
	HTTPClient    *http.Client
	Model         string
	ThinkingType  string
	ClearThinking bool
	Temperature   *float64
	TopP          *float64
	MaxTokens     *int
}

func NewClient(baseURL string, apiKey string) *Client {
	if baseURL == "" {
		baseURL = "https://api.z.ai/api/coding/paas/v4"
	}
	return &Client{
		BaseURL:      baseURL,
		APIKey:       apiKey,
		HTTPClient:   http.DefaultClient,
		Model:        "glm-5-turbo",
		ThinkingType: "enabled",
	}
}

func (c *Client) WithModel(model string) *Client {
	if model != "" {
		c.Model = model
	}
	return c
}

func (c *Client) WithThinking(thinkingType string, clearThinking bool) *Client {
	if thinkingType != "" {
		c.ThinkingType = thinkingType
	}
	c.ClearThinking = clearThinking
	return c
}

type chatCompletionRequest struct {
	Model          string                 `json:"model"`
	Messages       []chatMessageRequest   `json:"messages"`
	Thinking       thinkingRequest        `json:"thinking,omitempty"`
	Temperature    *float64               `json:"temperature,omitempty"`
	TopP           *float64               `json:"top_p,omitempty"`
	MaxTokens      *int                   `json:"max_tokens,omitempty"`
	DoSample       *bool                  `json:"do_sample,omitempty"`
	ResponseFormat *responseFormatRequest `json:"response_format,omitempty"`
	Tools          []toolRequest          `json:"tools,omitempty"`
	ToolChoice     string                 `json:"tool_choice,omitempty"`
}

type responseFormatRequest struct {
	Type string `json:"type"`
}

type chatMessageRequest struct {
	Role       string            `json:"role"`
	Content    string            `json:"content"`
	Name       string            `json:"name,omitempty"`
	ToolCallID string            `json:"tool_call_id,omitempty"`
	ToolCalls  []toolCallPayload `json:"tool_calls,omitempty"`
}

type thinkingRequest struct {
	Type          string `json:"type,omitempty"`
	ClearThinking *bool  `json:"clear_thinking,omitempty"`
}

type toolRequest struct {
	Type     string              `json:"type"`
	Function toolFunctionRequest `json:"function"`
}

type toolFunctionRequest struct {
	Name        string         `json:"name"`
	Description string         `json:"description"`
	Parameters  map[string]any `json:"parameters,omitempty"`
}

type toolCallPayload struct {
	ID       string                  `json:"id,omitempty"`
	Type     string                  `json:"type,omitempty"`
	Function toolCallFunctionPayload `json:"function"`
}

type toolCallFunctionPayload struct {
	Name      string        `json:"name"`
	Arguments toolArguments `json:"arguments"`
}

type toolArguments map[string]any

func (a *toolArguments) UnmarshalJSON(data []byte) error {
	if len(bytes.TrimSpace(data)) == 0 || bytes.Equal(bytes.TrimSpace(data), []byte("null")) {
		*a = nil
		return nil
	}

	if len(bytes.TrimSpace(data)) > 0 && bytes.TrimSpace(data)[0] == '"' {
		var raw string
		if err := json.Unmarshal(data, &raw); err != nil {
			return err
		}
		if strings.TrimSpace(raw) == "" {
			*a = nil
			return nil
		}

		var parsed map[string]any
		if err := json.Unmarshal([]byte(raw), &parsed); err != nil {
			return err
		}
		*a = parsed
		return nil
	}

	var parsed map[string]any
	if err := json.Unmarshal(data, &parsed); err != nil {
		return err
	}
	*a = parsed
	return nil
}

func (a toolArguments) MarshalJSON() ([]byte, error) {
	if a == nil {
		return []byte("null"), nil
	}
	body, err := json.Marshal(map[string]any(a))
	if err != nil {
		return nil, err
	}
	return json.Marshal(string(body))
}

type chatCompletionResponse struct {
	Choices []struct {
		FinishReason string `json:"finish_reason"`
		Message      struct {
			Content          string            `json:"content"`
			ReasoningContent string            `json:"reasoning_content"`
			ToolCalls        []toolCallPayload `json:"tool_calls"`
		} `json:"message"`
	} `json:"choices"`
	Usage struct {
		PromptTokens     int `json:"prompt_tokens"`
		CompletionTokens int `json:"completion_tokens"`
		TotalTokens      int `json:"total_tokens"`
		CachedTokens     int `json:"cached_tokens"`
	} `json:"usage"`
}

func (c *Client) Generate(ctx context.Context, req provider.PromptRequest) (provider.PromptResponse, error) {
	if req.Transport.Timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, req.Transport.Timeout)
		defer cancel()
	}

	model := c.Model
	if strings.TrimSpace(req.Config.Model) != "" {
		model = strings.TrimSpace(req.Config.Model)
	}
	thinkingType := c.ThinkingType
	if strings.TrimSpace(req.Config.ReasoningMode) != "" {
		thinkingType = strings.TrimSpace(req.Config.ReasoningMode)
	}
	clearThinking := c.ClearThinking
	if req.Config.ClearThinking != nil {
		clearThinking = *req.Config.ClearThinking
	}
	temperature := c.Temperature
	if req.Config.Temperature != nil {
		temperature = req.Config.Temperature
	}
	topP := c.TopP
	if req.Config.TopP != nil {
		topP = req.Config.TopP
	}
	maxTokens := c.MaxTokens
	if req.Config.MaxTokens != nil {
		maxTokens = req.Config.MaxTokens
	}
	doSample := req.Config.DoSample
	var responseFormat *responseFormatRequest
	if strings.TrimSpace(req.Config.ResponseFormat) != "" {
		responseFormat = &responseFormatRequest{Type: strings.TrimSpace(req.Config.ResponseFormat)}
	}

	payload := chatCompletionRequest{
		Model:    model,
		Messages: make([]chatMessageRequest, 0, len(req.Messages)),
		Thinking: thinkingRequest{
			Type:          thinkingType,
			ClearThinking: &clearThinking,
		},
		Temperature:    temperature,
		TopP:           topP,
		MaxTokens:      maxTokens,
		DoSample:       doSample,
		ResponseFormat: responseFormat,
	}
	for _, message := range req.Messages {
		role := message.Role
		if role == "" {
			role = "user"
		}
		name := message.Name
		if role == "tool" {
			name = ""
		}
		payload.Messages = append(payload.Messages, chatMessageRequest{
			Role:       role,
			Content:    message.Content,
			Name:       name,
			ToolCallID: message.ToolCallID,
			ToolCalls:  makeToolCallPayloads(message.ToolCalls),
		})
	}
	if len(req.Tools) > 0 {
		payload.ToolChoice = "auto"
		payload.Tools = make([]toolRequest, 0, len(req.Tools))
		for _, tool := range req.Tools {
			payload.Tools = append(payload.Tools, toolRequest{
				Type: "function",
				Function: toolFunctionRequest{
					Name:        tool.Name,
					Description: tool.Description,
					Parameters:  tool.Parameters,
				},
			})
		}
	}

	body, err := json.Marshal(payload)
	if err != nil {
		return provider.PromptResponse{}, err
	}

	baseURL := c.BaseURL
	if strings.TrimSpace(req.Transport.BaseURL) != "" {
		baseURL = strings.TrimSpace(req.Transport.BaseURL)
	}
	path := "/chat/completions"
	if strings.TrimSpace(req.Transport.Path) != "" {
		path = strings.TrimSpace(req.Transport.Path)
	}
	url := strings.TrimRight(baseURL, "/") + path
	var lastErr error
	for attempt := 0; attempt < 3; attempt++ {
		httpReq, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(body))
		if err != nil {
			return provider.PromptResponse{}, err
		}
		httpReq.Header.Set("Content-Type", "application/json")
		if auth := resolveAuth(c.APIKey, req.Transport.Auth); auth != nil {
			httpReq.Header.Set(auth.Header, auth.Value)
		}
		for key, value := range req.Transport.Headers {
			if strings.TrimSpace(key) == "" {
				continue
			}
			httpReq.Header.Set(key, value)
		}

		httpResp, err := c.HTTPClient.Do(httpReq)
		if err != nil {
			lastErr = err
		} else {
			defer httpResp.Body.Close()
			rawBody, readErr := io.ReadAll(httpResp.Body)
			if readErr != nil {
				return provider.PromptResponse{}, readErr
			}
			if recorder := llmtrace.ActiveCall(ctx); recorder != nil {
				recorder.RecordProviderHTTP(url, map[string][]string(httpReq.Header.Clone()), map[string][]string(httpResp.Header.Clone()), body, rawBody, httpResp.StatusCode)
			}
			if httpResp.StatusCode >= 200 && httpResp.StatusCode < 300 {
				var resp chatCompletionResponse
				if err := json.Unmarshal(rawBody, &resp); err != nil {
					return provider.PromptResponse{}, err
				}
				if len(resp.Choices) == 0 {
					return provider.PromptResponse{}, fmt.Errorf("zai api error: empty choices")
				}

				return provider.PromptResponse{
					Text:  resp.Choices[0].Message.Content,
					Model: model,
					Reasoning: provider.ReasoningSettings{
						Mode:          thinkingType,
						ClearThinking: clearThinking,
					},
					ReasoningContent: resp.Choices[0].Message.ReasoningContent,
					FinishReason:     resp.Choices[0].FinishReason,
					ToolCalls:        makeProviderToolCalls(resp.Choices[0].Message.ToolCalls),
					Usage: provider.Usage{
						PromptTokens:     resp.Usage.PromptTokens,
						CompletionTokens: resp.Usage.CompletionTokens,
						TotalTokens:      resp.Usage.TotalTokens,
						CachedTokens:     resp.Usage.CachedTokens,
					},
				}, nil
			}

			lastErr = fmt.Errorf("zai api error: status=%d body=%s", httpResp.StatusCode, strings.TrimSpace(string(rawBody)))
			if !isRetryableStatus(httpResp.StatusCode) {
				return provider.PromptResponse{}, lastErr
			}
		}

		if attempt < 2 {
			select {
			case <-ctx.Done():
				return provider.PromptResponse{}, ctx.Err()
			case <-time.After(time.Duration(attempt+1) * 100 * time.Millisecond):
			}
		}
	}

	return provider.PromptResponse{}, lastErr
}

func resolveAuth(apiKey string, auth *provider.RequestAuth) *provider.RequestAuth {
	if auth != nil && strings.TrimSpace(auth.Value) != "" {
		header := strings.TrimSpace(auth.Header)
		if header == "" {
			header = "Authorization"
		}
		value := strings.TrimSpace(auth.Value)
		if prefix := strings.TrimSpace(auth.Prefix); prefix != "" {
			value = prefix + " " + value
		}
		return &provider.RequestAuth{
			Header: header,
			Value:  value,
		}
	}
	if strings.TrimSpace(apiKey) == "" {
		return nil
	}
	return &provider.RequestAuth{
		Header: "Authorization",
		Value:  "Bearer " + strings.TrimSpace(apiKey),
	}
}

func isRetryableStatus(status int) bool {
	return status == http.StatusTooManyRequests || status >= 500
}

func makeToolCallPayloads(calls []provider.ToolCall) []toolCallPayload {
	if len(calls) == 0 {
		return nil
	}

	out := make([]toolCallPayload, 0, len(calls))
	for _, call := range calls {
		out = append(out, toolCallPayload{
			ID:   call.ID,
			Type: "function",
			Function: toolCallFunctionPayload{
				Name:      providerToolName(call.Name),
				Arguments: toolArguments(call.Arguments),
			},
		})
	}
	return out
}

func makeProviderToolCalls(calls []toolCallPayload) []provider.ToolCall {
	if len(calls) == 0 {
		return nil
	}

	out := make([]provider.ToolCall, 0, len(calls))
	for _, call := range calls {
		out = append(out, provider.ToolCall{
			ID:        call.ID,
			Name:      call.Function.Name,
			Arguments: call.Function.Arguments,
		})
	}
	return out
}

func providerToolName(name string) string {
	var b strings.Builder
	b.Grow(len(name))
	for _, r := range name {
		switch {
		case r >= 'a' && r <= 'z', r >= 'A' && r <= 'Z', r >= '0' && r <= '9', r == '_', r == '-':
			b.WriteRune(r)
		default:
			b.WriteRune('_')
		}
	}
	return b.String()
}
