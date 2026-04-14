package provider_test

import (
	"bytes"
	"context"
	"errors"
	"io"
	"net/http"
	"testing"
	"time"

	"teamd/internal/contracts"
	"teamd/internal/provider"
)

type fakeDoer struct {
	do func(*http.Request) (*http.Response, error)
}

func (f fakeDoer) Do(req *http.Request) (*http.Response, error) {
	return f.do(req)
}

func TestTransportExecutorAppliesStaticEndpointAndBearerAuth(t *testing.T) {
	t.Setenv("ZAI_API_KEY", "secret-token")

	var captured *http.Request
	executor := provider.NewTransportExecutor(fakeDoer{
		do: func(req *http.Request) (*http.Response, error) {
			captured = req
			return &http.Response{
				StatusCode: http.StatusOK,
				Header:     http.Header{"X-Test": []string{"ok"}},
				Body:       io.NopCloser(bytes.NewBufferString("response-body")),
			}, nil
		},
	})

	resp, err := executor.Execute(context.Background(), contracts.TransportContract{
		ID: "transport-main",
		Endpoint: contracts.EndpointPolicy{
			Enabled:  true,
			Strategy: "static",
			Params: contracts.EndpointParams{
				BaseURL: "https://api.z.ai",
				Path:    "/api/paas/v4/chat/completions",
				Method:  http.MethodPost,
				ExtraHeaders: map[string]string{
					"X-Extra": "1",
				},
			},
		},
		Auth: contracts.AuthPolicy{
			Enabled:  true,
			Strategy: "bearer_token",
			Params: contracts.AuthParams{
				Header:      "Authorization",
				Prefix:      "Bearer",
				ValueEnvVar: "ZAI_API_KEY",
			},
		},
	}, provider.Request{
		Body:        []byte(`{"hello":"world"}`),
		ContentType: "application/json",
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}

	if captured == nil {
		t.Fatal("request was not captured")
	}
	if got := captured.URL.String(); got != "https://api.z.ai/api/paas/v4/chat/completions" {
		t.Fatalf("url = %q", got)
	}
	if got := captured.Method; got != http.MethodPost {
		t.Fatalf("method = %q", got)
	}
	if got := captured.Header.Get("Authorization"); got != "Bearer secret-token" {
		t.Fatalf("authorization = %q", got)
	}
	if got := captured.Header.Get("X-Extra"); got != "1" {
		t.Fatalf("x-extra = %q", got)
	}
	if got := captured.Header.Get("Content-Type"); got != "application/json" {
		t.Fatalf("content-type = %q", got)
	}
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d", resp.StatusCode)
	}
	if string(resp.Body) != "response-body" {
		t.Fatalf("body = %q", string(resp.Body))
	}
}

func TestTransportExecutorRetriesRetriableStatus(t *testing.T) {
	t.Parallel()

	attempts := 0
	sleeps := 0
	executor := provider.NewTransportExecutor(fakeDoer{
		do: func(req *http.Request) (*http.Response, error) {
			attempts++
			if attempts == 1 {
				return &http.Response{
					StatusCode: http.StatusTooManyRequests,
					Body:       io.NopCloser(bytes.NewBufferString("retry")),
				}, nil
			}
			return &http.Response{
				StatusCode: http.StatusOK,
				Body:       io.NopCloser(bytes.NewBufferString("ok")),
			}, nil
		},
	})
	executor.Sleep = func(time.Duration) {
		sleeps++
	}
	executor.Jitter = func(delay time.Duration) time.Duration {
		return 0
	}

	resp, err := executor.Execute(context.Background(), contracts.TransportContract{
		Endpoint: contracts.EndpointPolicy{
			Enabled:  true,
			Strategy: "static",
			Params: contracts.EndpointParams{
				BaseURL: "https://api.z.ai",
				Path:    "/chat",
				Method:  http.MethodPost,
			},
		},
		Auth: contracts.AuthPolicy{
			Enabled:  true,
			Strategy: "none",
		},
		Retry: contracts.RetryPolicy{
			Enabled:  true,
			Strategy: "exponential_jitter",
			Params: contracts.RetryParams{
				MaxAttempts:     3,
				BaseDelay:       "10ms",
				MaxDelay:        "1s",
				RetryOnStatuses: []int{429, 500},
			},
		},
	}, provider.Request{Body: []byte(`{}`)})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}

	if attempts != 2 {
		t.Fatalf("attempts = %d, want 2", attempts)
	}
	if sleeps != 1 {
		t.Fatalf("sleeps = %d, want 1", sleeps)
	}
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status = %d, want 200", resp.StatusCode)
	}
}

func TestTransportExecutorAppliesPerRequestTimeout(t *testing.T) {
	t.Parallel()

	executor := provider.NewTransportExecutor(fakeDoer{
		do: func(req *http.Request) (*http.Response, error) {
			if _, ok := req.Context().Deadline(); !ok {
				return nil, errors.New("missing request deadline")
			}
			return &http.Response{
				StatusCode: http.StatusOK,
				Body:       io.NopCloser(bytes.NewBufferString("ok")),
			}, nil
		},
	})

	_, err := executor.Execute(context.Background(), contracts.TransportContract{
		Endpoint: contracts.EndpointPolicy{
			Enabled:  true,
			Strategy: "static",
			Params: contracts.EndpointParams{
				BaseURL: "https://api.z.ai",
				Path:    "/chat",
				Method:  http.MethodPost,
			},
		},
		Auth: contracts.AuthPolicy{
			Enabled:  true,
			Strategy: "none",
		},
		Timeout: contracts.TimeoutPolicy{
			Enabled:  true,
			Strategy: "per_request",
			Params: contracts.TimeoutParams{
				Total: "5s",
			},
		},
	}, provider.Request{Body: []byte(`{}`)})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
}

func TestTransportExecutorUsesOperationBudgetForLongRunningNonStreaming(t *testing.T) {
	t.Parallel()

	executor := provider.NewTransportExecutor(fakeDoer{
		do: func(req *http.Request) (*http.Response, error) {
			if _, ok := req.Context().Deadline(); !ok {
				return nil, errors.New("missing operation deadline")
			}
			return &http.Response{
				StatusCode: http.StatusOK,
				Body:       io.NopCloser(bytes.NewBufferString("ok")),
			}, nil
		},
	})

	_, err := executor.Execute(context.Background(), contracts.TransportContract{
		Endpoint: contracts.EndpointPolicy{
			Enabled:  true,
			Strategy: "static",
			Params: contracts.EndpointParams{
				BaseURL: "https://api.z.ai",
				Path:    "/chat",
				Method:  http.MethodPost,
			},
		},
		Auth: contracts.AuthPolicy{
			Enabled:  true,
			Strategy: "none",
		},
		Timeout: contracts.TimeoutPolicy{
			Enabled:  true,
			Strategy: "long_running_non_streaming",
			Params: contracts.TimeoutParams{
				OperationBudget: "1m",
			},
		},
	}, provider.Request{Body: []byte(`{}`)})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
}

func TestTransportExecutorSkipsRetryForLateTransportError(t *testing.T) {
	t.Parallel()

	attempts := 0
	sleeps := 0
	executor := provider.NewTransportExecutor(fakeDoer{
		do: func(req *http.Request) (*http.Response, error) {
			attempts++
			<-req.Context().Done()
			return nil, req.Context().Err()
		},
	})
	executor.Sleep = func(time.Duration) {
		sleeps++
	}

	_, err := executor.Execute(context.Background(), contracts.TransportContract{
		Endpoint: contracts.EndpointPolicy{
			Enabled:  true,
			Strategy: "static",
			Params: contracts.EndpointParams{
				BaseURL: "https://api.z.ai",
				Path:    "/chat",
				Method:  http.MethodPost,
			},
		},
		Auth: contracts.AuthPolicy{
			Enabled:  true,
			Strategy: "none",
		},
		Retry: contracts.RetryPolicy{
			Enabled:  true,
			Strategy: "exponential_jitter",
			Params: contracts.RetryParams{
				MaxAttempts:       3,
				BaseDelay:         "10ms",
				MaxDelay:          "100ms",
				RetryOnErrors:     []string{"transport_error"},
				EarlyFailureWindow: "5ms",
			},
		},
		Timeout: contracts.TimeoutPolicy{
			Enabled:  true,
			Strategy: "long_running_non_streaming",
			Params: contracts.TimeoutParams{
				OperationBudget: "100ms",
				AttemptTimeout:  "20ms",
			},
		},
	}, provider.Request{Body: []byte(`{}`)})
	if err == nil {
		t.Fatal("Execute error = nil, want timeout transport error")
	}
	if attempts != 1 {
		t.Fatalf("attempts = %d, want 1", attempts)
	}
	if sleeps != 0 {
		t.Fatalf("sleeps = %d, want 0", sleeps)
	}
}

func TestTransportExecutorEmitsAttemptTrace(t *testing.T) {
	t.Parallel()

	var traces []provider.AttemptTrace
	executor := provider.NewTransportExecutor(fakeDoer{
		do: func(req *http.Request) (*http.Response, error) {
			return &http.Response{
				StatusCode: http.StatusTooManyRequests,
				Body:       io.NopCloser(bytes.NewBufferString("retry")),
			}, nil
		},
	})
	executor.Sleep = func(time.Duration) {}
	executor.Jitter = func(delay time.Duration) time.Duration { return 0 }

	resp, err := executor.Execute(context.Background(), contracts.TransportContract{
		Endpoint: contracts.EndpointPolicy{
			Enabled:  true,
			Strategy: "static",
			Params: contracts.EndpointParams{
				BaseURL: "https://api.z.ai",
				Path:    "/chat",
				Method:  http.MethodPost,
			},
		},
		Auth: contracts.AuthPolicy{
			Enabled:  true,
			Strategy: "none",
		},
		Retry: contracts.RetryPolicy{
			Enabled:  true,
			Strategy: "exponential_jitter",
			Params: contracts.RetryParams{
				MaxAttempts:     2,
				BaseDelay:       "10ms",
				MaxDelay:        "100ms",
				RetryOnStatuses: []int{429},
			},
		},
		Timeout: contracts.TimeoutPolicy{
			Enabled:  true,
			Strategy: "long_running_non_streaming",
			Params: contracts.TimeoutParams{
				OperationBudget: "1m",
			},
		},
	}, provider.Request{
		Body: []byte(`{}`),
		AttemptObserver: func(trace provider.AttemptTrace) {
			traces = append(traces, trace)
		},
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if resp.StatusCode != http.StatusTooManyRequests {
		t.Fatalf("status = %d, want 429", resp.StatusCode)
	}
	if len(traces) != 2 {
		t.Fatalf("trace count = %d, want 2", len(traces))
	}
	if traces[0].Attempt != 1 {
		t.Fatalf("trace[0].Attempt = %d, want 1", traces[0].Attempt)
	}
	if !traces[0].RetryDecision {
		t.Fatalf("trace[0].RetryDecision = false, want true")
	}
	if traces[0].RetryReason != "status:429" {
		t.Fatalf("trace[0].RetryReason = %q, want status:429", traces[0].RetryReason)
	}
	if traces[0].ComputedBackoff <= 0 {
		t.Fatalf("trace[0].ComputedBackoff = %s, want > 0", traces[0].ComputedBackoff)
	}
	if traces[1].FinalAttemptCount != 2 {
		t.Fatalf("trace[1].FinalAttemptCount = %d, want 2", traces[1].FinalAttemptCount)
	}
	if traces[1].RetryDecision {
		t.Fatalf("trace[1].RetryDecision = true, want false")
	}
}
