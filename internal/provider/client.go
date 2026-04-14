package provider

import (
	"context"
	"fmt"

	"teamd/internal/contracts"
)

type ClientInput struct {
	PromptAssetSelection []string
	Messages             []contracts.Message
	Tools                []map[string]any
}

type ClientResult struct {
	RequestBody []byte
	Transport   Response
}

type Client struct {
	PromptAssets *PromptAssetExecutor
	RequestShape *RequestShapeExecutor
	Transport    *TransportExecutor
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

	response, err := c.Transport.Execute(ctx, contractSet.ProviderRequest.Transport, Request{
		Body:        requestBody,
		ContentType: "application/json",
	})
	if err != nil {
		return ClientResult{}, fmt.Errorf("execute provider transport: %w", err)
	}

	return ClientResult{
		RequestBody: requestBody,
		Transport:   response,
	}, nil
}
