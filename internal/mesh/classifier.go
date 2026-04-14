package mesh

import (
	"context"
	"encoding/json"
	"strings"

	"teamd/internal/provider"
)

var allowedTaskClasses = map[string]struct{}{
	"coding":   {},
	"shell":    {},
	"analysis": {},
	"research": {},
	"writing":  {},
}

type TaskClassifier interface {
	Classify(ctx context.Context, prompt string) (taskClass string, confidence float64, err error)
}

type LLMTaskClassifier struct {
	provider provider.Provider
}

func NewLLMTaskClassifier(p provider.Provider) *LLMTaskClassifier {
	return &LLMTaskClassifier{provider: p}
}

func (c *LLMTaskClassifier) Classify(ctx context.Context, prompt string) (string, float64, error) {
	resp, err := c.provider.Generate(ctx, provider.PromptRequest{
		WorkerID: "mesh-classifier",
		Messages: []provider.Message{
			{
				Role: "system",
				Content: "Classify the user task into exactly one of: coding, shell, analysis, research, writing. " +
					`Return strict JSON: {"task_class":"...","confidence":0.0,"reasoning":"..."}`,
			},
			{
				Role:    "user",
				Content: prompt,
			},
		},
	})
	if err != nil {
		return "", 0, err
	}

	var parsed struct {
		TaskClass  string  `json:"task_class"`
		Confidence float64 `json:"confidence"`
		Reasoning  string  `json:"reasoning"`
	}
	if err := json.Unmarshal([]byte(resp.Text), &parsed); err != nil {
		return "analysis", 0, nil
	}
	taskClass := strings.ToLower(strings.TrimSpace(parsed.TaskClass))
	if _, ok := allowedTaskClasses[taskClass]; !ok {
		return "analysis", 0, nil
	}
	return taskClass, parsed.Confidence, nil
}
