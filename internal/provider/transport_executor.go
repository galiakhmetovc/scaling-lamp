package provider

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"math/rand"
	"net/http"
	"os"
	"slices"
	"strings"
	"time"

	"teamd/internal/contracts"
)

type HTTPDoer interface {
	Do(req *http.Request) (*http.Response, error)
}

type Request struct {
	Body            []byte
	ContentType     string
	AttemptObserver func(AttemptTrace)
}

type Response struct {
	StatusCode int
	Headers    http.Header
	Body       []byte
}

type AttemptTrace struct {
	Attempt          int
	StartedAt        time.Time
	FinishedAt       time.Time
	Duration         time.Duration
	StatusCode       int
	Error            string
	AttemptTimeout   time.Duration
	OperationBudget  time.Duration
	RetryDecision    bool
	RetryReason      string
	ComputedBackoff  time.Duration
	FinalAttemptCount int
}

type TransportExecutor struct {
	Doer   HTTPDoer
	Sleep  func(time.Duration)
	Jitter func(time.Duration) time.Duration
}

func NewTransportExecutor(doer HTTPDoer) *TransportExecutor {
	if doer == nil {
		doer = &http.Client{}
	}
	return &TransportExecutor{
		Doer:  doer,
		Sleep: time.Sleep,
		Jitter: func(delay time.Duration) time.Duration {
			if delay <= 0 {
				return 0
			}
			return time.Duration(rand.Int63n(int64(delay)))
		},
	}
}

func (e *TransportExecutor) Execute(ctx context.Context, contract contracts.TransportContract, request Request) (Response, error) {
	if e == nil {
		return Response{}, fmt.Errorf("transport executor is nil")
	}
	if !contract.Endpoint.Enabled {
		return Response{}, fmt.Errorf("transport endpoint policy is disabled")
	}
	if contract.Endpoint.Strategy != "static" {
		return Response{}, fmt.Errorf("unsupported endpoint strategy %q", contract.Endpoint.Strategy)
	}

	executionCtx := ctx
	executionCancel := func() {}
	operationBudget, err := operationBudget(contract.Timeout)
	if err != nil {
		return Response{}, err
	}
	if operationBudget > 0 {
		executionCtx, executionCancel = context.WithTimeout(ctx, operationBudget)
	}
	defer executionCancel()

	maxAttempts := retryAttempts(contract.Retry)
	var lastErr error
	for attempt := 1; attempt <= maxAttempts; attempt++ {
		startedAt := time.Now().UTC()
		attemptTimeout, err := attemptTimeout(contract.Timeout)
		if err != nil {
			return Response{}, err
		}

		response, err := e.executeOnce(executionCtx, contract, request, attemptTimeout)
		finishedAt := time.Now().UTC()
		trace := AttemptTrace{
			Attempt:         attempt,
			StartedAt:       startedAt,
			FinishedAt:      finishedAt,
			Duration:        finishedAt.Sub(startedAt),
			AttemptTimeout:  attemptTimeout,
			OperationBudget: operationBudget,
		}
		if err == nil {
			trace.StatusCode = response.StatusCode
			trace.RetryDecision = shouldRetryStatus(contract.Retry, response.StatusCode) && attempt < maxAttempts
			if trace.RetryDecision {
				trace.RetryReason = fmt.Sprintf("status:%d", response.StatusCode)
			}
			if !trace.RetryDecision {
				trace.FinalAttemptCount = attempt
				emitAttemptTrace(request, trace)
				return response, nil
			}
		} else {
			lastErr = err
			trace.Error = err.Error()
			trace.RetryDecision = shouldRetryError(contract.Retry, err, trace.Duration) && attempt < maxAttempts
			if trace.RetryDecision {
				trace.RetryReason = retryReason(err)
			}
			if !trace.RetryDecision {
				trace.FinalAttemptCount = attempt
				emitAttemptTrace(request, trace)
				return Response{}, err
			}
		}

		delay, err := retryDelay(contract.Retry, attempt)
		if err != nil {
			return Response{}, err
		}
		if contract.Retry.Strategy == "exponential_jitter" && e.Jitter != nil {
			delay += e.Jitter(delay)
			if maxDelay, parseErr := time.ParseDuration(contract.Retry.Params.MaxDelay); parseErr == nil && delay > maxDelay {
				delay = maxDelay
			}
		}
		trace.ComputedBackoff = delay
		trace.FinalAttemptCount = maxAttempts
		emitAttemptTrace(request, trace)
		e.Sleep(delay)
	}

	if lastErr != nil {
		return Response{}, lastErr
	}
	return Response{}, fmt.Errorf("transport execution exhausted retries without response")
}

func (e *TransportExecutor) executeOnce(ctx context.Context, contract contracts.TransportContract, request Request, timeout time.Duration) (Response, error) {
	requestCtx := ctx
	cancel := func() {}
	if timeout > 0 {
		requestCtx, cancel = context.WithTimeout(ctx, timeout)
	}
	defer cancel()

	method := contract.Endpoint.Params.Method
	if method == "" {
		method = http.MethodPost
	}

	req, err := http.NewRequestWithContext(
		requestCtx,
		method,
		strings.TrimRight(contract.Endpoint.Params.BaseURL, "/")+contract.Endpoint.Params.Path,
		bytes.NewReader(request.Body),
	)
	if err != nil {
		return Response{}, fmt.Errorf("build request: %w", err)
	}

	if request.ContentType != "" {
		req.Header.Set("Content-Type", request.ContentType)
	}
	for key, value := range contract.Endpoint.Params.ExtraHeaders {
		req.Header.Set(key, value)
	}
	if err := applyAuth(req, contract.Auth); err != nil {
		return Response{}, err
	}

	resp, err := e.Doer.Do(req)
	if err != nil {
		return Response{}, fmt.Errorf("perform request: %w", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return Response{}, fmt.Errorf("read response body: %w", err)
	}

	return Response{
		StatusCode: resp.StatusCode,
		Headers:    resp.Header.Clone(),
		Body:       body,
	}, nil
}

func applyAuth(req *http.Request, policy contracts.AuthPolicy) error {
	if !policy.Enabled || policy.Strategy == "" || policy.Strategy == "none" {
		return nil
	}
	if policy.Strategy != "bearer_token" {
		return fmt.Errorf("unsupported auth strategy %q", policy.Strategy)
	}

	value, ok := os.LookupEnv(policy.Params.ValueEnvVar)
	if !ok {
		return fmt.Errorf("auth env var %q is not set", policy.Params.ValueEnvVar)
	}
	header := policy.Params.Header
	if header == "" {
		header = "Authorization"
	}
	if prefix := strings.TrimSpace(policy.Params.Prefix); prefix != "" {
		value = prefix + " " + value
	}
	req.Header.Set(header, value)
	return nil
}

func retryAttempts(policy contracts.RetryPolicy) int {
	if !policy.Enabled || policy.Strategy == "" || policy.Strategy == "none" || policy.Params.MaxAttempts <= 0 {
		return 1
	}
	return policy.Params.MaxAttempts
}

func shouldRetryStatus(policy contracts.RetryPolicy, status int) bool {
	if !policy.Enabled || status == 0 {
		return false
	}
	return slices.Contains(policy.Params.RetryOnStatuses, status)
}

func shouldRetryError(policy contracts.RetryPolicy, err error, elapsed time.Duration) bool {
	if !policy.Enabled || err == nil {
		return false
	}
	if policy.Params.EarlyFailureWindow != "" {
		window, parseErr := time.ParseDuration(policy.Params.EarlyFailureWindow)
		if parseErr != nil {
			return false
		}
		if elapsed > window {
			return false
		}
	}
	return slices.Contains(policy.Params.RetryOnErrors, "transport_error")
}

func retryDelay(policy contracts.RetryPolicy, attempt int) (time.Duration, error) {
	if !policy.Enabled || policy.Strategy == "" || policy.Strategy == "none" {
		return 0, nil
	}
	baseDelay, err := time.ParseDuration(policy.Params.BaseDelay)
	if err != nil {
		return 0, fmt.Errorf("parse retry base_delay: %w", err)
	}
	maxDelay, err := time.ParseDuration(policy.Params.MaxDelay)
	if err != nil {
		return 0, fmt.Errorf("parse retry max_delay: %w", err)
	}

	delay := baseDelay
	switch policy.Strategy {
	case "fixed":
	case "exponential", "exponential_jitter":
		for i := 1; i < attempt; i++ {
			delay *= 2
			if delay >= maxDelay {
				delay = maxDelay
				break
			}
		}
	default:
		return 0, fmt.Errorf("unsupported retry strategy %q", policy.Strategy)
	}

	if delay > maxDelay {
		delay = maxDelay
	}
	return delay, nil
}

func attemptTimeout(policy contracts.TimeoutPolicy) (time.Duration, error) {
	if !policy.Enabled {
		return 0, nil
	}
	switch policy.Strategy {
	case "", "none":
		return 0, nil
	case "per_request":
		if policy.Params.Total == "" {
			return 0, nil
		}
		total, err := time.ParseDuration(policy.Params.Total)
		if err != nil {
			return 0, fmt.Errorf("parse timeout total: %w", err)
		}
		return total, nil
	case "long_running_non_streaming":
		if policy.Params.AttemptTimeout == "" {
			return 0, nil
		}
		total, err := time.ParseDuration(policy.Params.AttemptTimeout)
		if err != nil {
			return 0, fmt.Errorf("parse timeout attempt_timeout: %w", err)
		}
		return total, nil
	default:
		return 0, fmt.Errorf("unsupported timeout strategy %q", policy.Strategy)
	}
}

func operationBudget(policy contracts.TimeoutPolicy) (time.Duration, error) {
	if !policy.Enabled || policy.Strategy != "long_running_non_streaming" || policy.Params.OperationBudget == "" {
		return 0, nil
	}
	total, err := time.ParseDuration(policy.Params.OperationBudget)
	if err != nil {
		return 0, fmt.Errorf("parse timeout operation_budget: %w", err)
	}
	return total, nil
}

func retryReason(err error) string {
	if err == nil {
		return ""
	}
	return "transport_error"
}

func emitAttemptTrace(request Request, trace AttemptTrace) {
	if request.AttemptObserver != nil {
		request.AttemptObserver(trace)
	}
}
