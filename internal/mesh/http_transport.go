package mesh

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"strings"
	"sync"
)

type EnvelopeHandler func(context.Context, Envelope) (Envelope, error)

type HTTPTransport struct {
	client  *http.Client
	mu      sync.Mutex
	replies map[string]Envelope
}

func NewHTTPTransport(client *http.Client) *HTTPTransport {
	if client == nil {
		client = http.DefaultClient
	}
	return &HTTPTransport{
		client:  client,
		replies: make(map[string]Envelope),
	}
}

func (t *HTTPTransport) Send(ctx context.Context, baseURL string, env Envelope) (Envelope, error) {
	if env.TTL <= 0 {
		return Envelope{}, errors.New("mesh transport: ttl exhausted")
	}
	env.TTL--
	body, err := json.Marshal(env)
	if err != nil {
		return Envelope{}, err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, strings.TrimRight(baseURL, "/")+"/mesh/message", bytes.NewReader(body))
	if err != nil {
		return Envelope{}, err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := t.client.Do(req)
	if err != nil {
		return Envelope{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return Envelope{}, errors.New("mesh transport: unexpected status")
	}

	var reply Envelope
	if err := json.NewDecoder(resp.Body).Decode(&reply); err != nil {
		return Envelope{}, err
	}
	return reply, nil
}

func (t *HTTPTransport) Handler(next EnvelopeHandler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != "/mesh/message" {
			http.NotFound(w, r)
			return
		}
		var env Envelope
		if err := json.NewDecoder(r.Body).Decode(&env); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}

		t.mu.Lock()
		if cached, ok := t.replies[env.MessageID]; ok {
			t.mu.Unlock()
			_ = json.NewEncoder(w).Encode(cached)
			return
		}
		t.mu.Unlock()

		reply, err := next(r.Context(), env)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}

		t.mu.Lock()
		t.replies[env.MessageID] = reply
		t.mu.Unlock()
		_ = json.NewEncoder(w).Encode(reply)
	})
}
