package provider

import (
	"context"
	"fmt"

	"teamd/internal/contracts"
)

type ClientInput struct {
	PromptAssets []contracts.Message
	Messages     []contracts.Message
	Tools        []map[string]any
}

type ClientResult struct {
	RequestBody []byte
	Transport   Response
}

type Client struct {
	RequestShape *RequestShapeExecutor
	Transport    *TransportExecutor
}

func NewClient(requestShape *RequestShapeExecutor, transport *TransportExecutor) *Client {
	return &Client{
		RequestShape: requestShape,
		Transport:    transport,
	}
}

func (c *Client) Execute(ctx context.Context, contract contracts.ProviderRequestContract, input ClientInput) (ClientResult, error) {
	if c == nil {
		return ClientResult{}, fmt.Errorf("provider client is nil")
	}
	if c.RequestShape == nil {
		return ClientResult{}, fmt.Errorf("provider client request-shape executor is nil")
	}
	if c.Transport == nil {
		return ClientResult{}, fmt.Errorf("provider client transport executor is nil")
	}

	requestBody, err := c.RequestShape.Build(contract.RequestShape, RequestShapeInput{
		PromptAssets: input.PromptAssets,
		Messages:     input.Messages,
		Tools:        input.Tools,
	})
	if err != nil {
		return ClientResult{}, fmt.Errorf("build provider request body: %w", err)
	}

	response, err := c.Transport.Execute(ctx, contract.Transport, Request{
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

