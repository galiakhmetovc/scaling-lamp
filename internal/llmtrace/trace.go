package llmtrace

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"teamd/internal/provider"
)

type RunMeta struct {
	RunID     string    `json:"run_id"`
	ChatID    int64     `json:"chat_id,omitempty"`
	Query     string    `json:"query"`
	StartedAt time.Time `json:"started_at"`
}

type CallTrace struct {
	StartedAt            time.Time               `json:"started_at"`
	FinishedAt           time.Time               `json:"finished_at"`
	DurationMs           int64                   `json:"duration_ms"`
	Request              provider.PromptRequest  `json:"request"`
	ParsedResponse       provider.PromptResponse `json:"parsed_response"`
	Error                string                  `json:"error,omitempty"`
	ProviderURL          string                  `json:"provider_url,omitempty"`
	ProviderStatusCode   int                     `json:"provider_status_code,omitempty"`
	ProviderRequestHeaders  map[string][]string  `json:"provider_request_headers,omitempty"`
	ProviderRequestBody  string                  `json:"provider_request_body,omitempty"`
	ProviderResponseHeaders map[string][]string  `json:"provider_response_headers,omitempty"`
	ProviderResponseBody string                  `json:"provider_response_body,omitempty"`
}

type FileTrace struct {
	RunID      string      `json:"run_id"`
	ChatID     int64       `json:"chat_id,omitempty"`
	Query      string      `json:"query"`
	StartedAt  time.Time   `json:"started_at"`
	FinishedAt time.Time   `json:"finished_at"`
	Calls      []CallTrace `json:"calls"`
}

type Collector struct {
	mu    sync.Mutex
	meta  RunMeta
	end   time.Time
	calls []CallTrace
}

type collectorKey struct{}
type activeCallKey struct{}

func NewCollector(meta RunMeta) *Collector {
	if meta.StartedAt.IsZero() {
		meta.StartedAt = time.Now().UTC()
	}
	return &Collector{meta: meta}
}

func WithCollector(ctx context.Context, collector *Collector) context.Context {
	if collector == nil {
		return ctx
	}
	return context.WithValue(ctx, collectorKey{}, collector)
}

func FromContext(ctx context.Context) *Collector {
	collector, _ := ctx.Value(collectorKey{}).(*Collector)
	return collector
}

type CallRecorder struct {
	mu        sync.Mutex
	collector *Collector
	call      CallTrace
}

type TracingProvider struct {
	Base provider.Provider
}

func StartCall(ctx context.Context, req provider.PromptRequest) (context.Context, *CallRecorder) {
	collector := FromContext(ctx)
	if collector == nil {
		return ctx, nil
	}
	recorder := &CallRecorder{
		collector: collector,
		call: CallTrace{
			StartedAt: time.Now().UTC(),
			Request:   clonePromptRequest(req),
		},
	}
	return context.WithValue(ctx, activeCallKey{}, recorder), recorder
}

func ActiveCall(ctx context.Context) *CallRecorder {
	recorder, _ := ctx.Value(activeCallKey{}).(*CallRecorder)
	return recorder
}

func (p TracingProvider) Generate(ctx context.Context, req provider.PromptRequest) (provider.PromptResponse, error) {
	if p.Base == nil {
		p.Base = provider.FakeProvider{}
	}
	ctx, recorder := StartCall(ctx, req)
	resp, err := p.Base.Generate(ctx, req)
	if recorder != nil {
		recorder.Finish(resp, err)
	}
	return resp, err
}

func (r *CallRecorder) RecordProviderHTTP(url string, requestHeaders, responseHeaders map[string][]string, requestBody, responseBody []byte, statusCode int) {
	if r == nil {
		return
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	r.call.ProviderURL = url
	r.call.ProviderStatusCode = statusCode
	r.call.ProviderRequestHeaders = cloneHeaders(requestHeaders)
	r.call.ProviderRequestBody = string(requestBody)
	r.call.ProviderResponseHeaders = cloneHeaders(responseHeaders)
	r.call.ProviderResponseBody = string(responseBody)
}

func (r *CallRecorder) Finish(resp provider.PromptResponse, err error) {
	if r == nil {
		return
	}
	r.mu.Lock()
	r.call.FinishedAt = time.Now().UTC()
	r.call.DurationMs = r.call.FinishedAt.Sub(r.call.StartedAt).Milliseconds()
	r.call.ParsedResponse = clonePromptResponse(resp)
	if err != nil {
		r.call.Error = err.Error()
	}
	call := r.call
	r.mu.Unlock()

	r.collector.mu.Lock()
	defer r.collector.mu.Unlock()
	r.collector.calls = append(r.collector.calls, call)
	r.collector.end = call.FinishedAt
}

func (c *Collector) Snapshot() FileTrace {
	if c == nil {
		return FileTrace{}
	}
	c.mu.Lock()
	defer c.mu.Unlock()

	calls := make([]CallTrace, len(c.calls))
	for i, call := range c.calls {
		calls[i] = cloneCallTrace(call)
	}
	finished := c.end
	if finished.IsZero() {
		finished = time.Now().UTC()
	}
	return FileTrace{
		RunID:      c.meta.RunID,
		ChatID:     c.meta.ChatID,
		Query:      c.meta.Query,
		StartedAt:  c.meta.StartedAt,
		FinishedAt: finished,
		Calls:      calls,
	}
}

func (c *Collector) WriteFile(dir string) (string, error) {
	if c == nil {
		return "", nil
	}
	if strings.TrimSpace(dir) == "" {
		return "", fmt.Errorf("trace dir is empty")
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	trace := c.Snapshot()
	body, err := json.MarshalIndent(trace, "", "  ")
	if err != nil {
		return "", err
	}
	filename := fmt.Sprintf("%s-%s.json", sanitizeFilename(trace.StartedAt.Format("20060102T150405Z")), sanitizeFilename(trace.RunID))
	path := filepath.Join(dir, filename)
	if err := os.WriteFile(path, body, 0o644); err != nil {
		return "", err
	}
	return path, nil
}

func sanitizeFilename(s string) string {
	s = strings.TrimSpace(s)
	if s == "" {
		return "trace"
	}
	return strings.NewReplacer("/", "-", "\\", "-", " ", "-", ":", "-", "\n", "-", "\t", "-").Replace(s)
}

func cloneCallTrace(call CallTrace) CallTrace {
	call.Request = clonePromptRequest(call.Request)
	call.ParsedResponse = clonePromptResponse(call.ParsedResponse)
	call.ProviderRequestHeaders = cloneHeaders(call.ProviderRequestHeaders)
	call.ProviderResponseHeaders = cloneHeaders(call.ProviderResponseHeaders)
	return call
}

func cloneHeaders(in map[string][]string) map[string][]string {
	if len(in) == 0 {
		return nil
	}
	out := make(map[string][]string, len(in))
	for key, values := range in {
		out[key] = append([]string(nil), values...)
	}
	return out
}

func clonePromptRequest(req provider.PromptRequest) provider.PromptRequest {
	clone := req
	clone.Messages = append([]provider.Message(nil), req.Messages...)
	for i := range clone.Messages {
		clone.Messages[i].ToolCalls = append([]provider.ToolCall(nil), req.Messages[i].ToolCalls...)
	}
	clone.Tools = append([]provider.ToolDefinition(nil), req.Tools...)
	return clone
}

func clonePromptResponse(resp provider.PromptResponse) provider.PromptResponse {
	clone := resp
	clone.ToolCalls = append([]provider.ToolCall(nil), resp.ToolCalls...)
	return clone
}
