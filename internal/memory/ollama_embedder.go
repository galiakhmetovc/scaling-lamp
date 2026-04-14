package memory

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
)

type OllamaEmbedder struct {
	BaseURL    string
	Model      string
	HTTPClient *http.Client
}

func NewOllamaEmbedder(baseURL, model string) *OllamaEmbedder {
	if strings.TrimSpace(baseURL) == "" {
		baseURL = "http://127.0.0.1:11434"
	}
	if strings.TrimSpace(model) == "" {
		model = "nomic-embed-text:latest"
	}
	return &OllamaEmbedder{
		BaseURL:    strings.TrimRight(baseURL, "/"),
		Model:      model,
		HTTPClient: http.DefaultClient,
	}
}

func (e *OllamaEmbedder) Embed(ctx context.Context, text string) ([]float32, error) {
	payload := map[string]any{
		"model": e.Model,
		"input": text,
	}
	body, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, e.BaseURL+"/api/embed", bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := e.HTTPClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		raw, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("ollama embeddings error: status=%d body=%s", resp.StatusCode, strings.TrimSpace(string(raw)))
	}
	var out struct {
		Embedding  []float32   `json:"embedding"`
		Embeddings [][]float32 `json:"embeddings"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return nil, err
	}
	if len(out.Embedding) > 0 {
		return out.Embedding, nil
	}
	if len(out.Embeddings) > 0 {
		return out.Embeddings[0], nil
	}
	return nil, fmt.Errorf("ollama embeddings response missing vector")
}
