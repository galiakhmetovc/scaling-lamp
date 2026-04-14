package llmtrace

import (
	"context"
	"errors"
	"testing"

	"teamd/internal/provider"
)

func TestTracingProviderRecordsPromptRequestAndParsedResponse(t *testing.T) {
	collector := NewCollector(RunMeta{RunID: "run-1"})
	traced := TracingProvider{Base: provider.FakeProvider{}}

	req := provider.PromptRequest{
		WorkerID: "telegram:1",
		Messages: []provider.Message{{Role: "user", Content: "hello"}},
	}
	resp, err := traced.Generate(WithCollector(context.Background(), collector), req)
	if err != nil {
		t.Fatalf("generate: %v", err)
	}
	if resp.Text != "hello" {
		t.Fatalf("unexpected resp: %#v", resp)
	}

	trace := collector.Snapshot()
	if len(trace.Calls) != 1 {
		t.Fatalf("unexpected calls: %#v", trace.Calls)
	}
	if trace.Calls[0].Request.WorkerID != "telegram:1" {
		t.Fatalf("unexpected worker id: %#v", trace.Calls[0].Request.WorkerID)
	}
	if trace.Calls[0].ParsedResponse.Text != "hello" {
		t.Fatalf("unexpected parsed response: %#v", trace.Calls[0].ParsedResponse)
	}
}

func TestTracingProviderRecordsErrors(t *testing.T) {
	errBoom := errors.New("boom")
	traced := TracingProvider{Base: traceErrorProvider{err: errBoom}}
	collector := NewCollector(RunMeta{RunID: "run-1"})

	_, err := traced.Generate(WithCollector(context.Background(), collector), provider.PromptRequest{})
	if !errors.Is(err, errBoom) {
		t.Fatalf("unexpected err: %v", err)
	}

	trace := collector.Snapshot()
	if len(trace.Calls) != 1 {
		t.Fatalf("unexpected calls: %#v", trace.Calls)
	}
	if trace.Calls[0].Error != "boom" {
		t.Fatalf("unexpected trace error: %#v", trace.Calls[0].Error)
	}
}

type traceErrorProvider struct {
	err error
}

func (p traceErrorProvider) Generate(context.Context, provider.PromptRequest) (provider.PromptResponse, error) {
	return provider.PromptResponse{}, p.err
}
