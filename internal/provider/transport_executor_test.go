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
