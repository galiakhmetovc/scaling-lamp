package provider

import (
	"encoding/json"
	"fmt"

	"teamd/internal/contracts"
)

type RequestShapeInput struct {
	PrependPromptAssets []contracts.Message
	AppendPromptAssets  []contracts.Message
	Messages            []contracts.Message
	Tools               []map[string]any
}

type RequestShapeExecutor struct{}

func NewRequestShapeExecutor() *RequestShapeExecutor {
	return &RequestShapeExecutor{}
}

func (e *RequestShapeExecutor) Build(contract contracts.RequestShapeContract, input RequestShapeInput) ([]byte, error) {
	if e == nil {
		return nil, fmt.Errorf("request-shape executor is nil")
	}
	if !contract.Model.Enabled {
		return nil, fmt.Errorf("request-shape model policy is disabled")
	}
	if contract.Model.Strategy != "static_model" {
		return nil, fmt.Errorf("unsupported model strategy %q", contract.Model.Strategy)
	}
	if contract.Messages.Enabled && contract.Messages.Strategy != "raw_messages" {
		return nil, fmt.Errorf("unsupported message strategy %q", contract.Messages.Strategy)
	}
	if contract.Tools.Enabled && contract.Tools.Strategy != "tools_inline" {
		return nil, fmt.Errorf("unsupported tool strategy %q", contract.Tools.Strategy)
	}

	payload := map[string]any{
		"model":    contract.Model.Params.Model,
		"messages": append(append(append([]contracts.Message{}, input.PrependPromptAssets...), input.Messages...), input.AppendPromptAssets...),
	}

	if contract.Tools.Enabled {
		payload["tools"] = input.Tools
	}
	if contract.ResponseFormat.Enabled && contract.ResponseFormat.Params.Type != "" {
		payload["response_format"] = map[string]any{
			"type": contract.ResponseFormat.Params.Type,
		}
	}
	if contract.Streaming.Enabled {
		payload["stream"] = contract.Streaming.Params.Stream
	}
	if contract.Sampling.Enabled {
		if contract.Sampling.Params.Temperature != nil {
			payload["temperature"] = *contract.Sampling.Params.Temperature
		}
		if contract.Sampling.Params.TopP != nil {
			payload["top_p"] = *contract.Sampling.Params.TopP
		}
		if contract.Sampling.Params.MaxOutputTokens != nil {
			payload["max_output_tokens"] = *contract.Sampling.Params.MaxOutputTokens
		}
		if contract.Sampling.Params.ReasoningEffort != "" {
			payload["reasoning_effort"] = contract.Sampling.Params.ReasoningEffort
		}
	}

	body, err := json.Marshal(payload)
	if err != nil {
		return nil, fmt.Errorf("marshal request payload: %w", err)
	}
	return body, nil
}
