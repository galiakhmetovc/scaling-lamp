package mesh

import (
	"context"
	"encoding/json"
	"strings"

	"teamd/internal/provider"
)

type Clarifier struct {
	provider provider.Provider
}

func NewClarifier(provider provider.Provider) Clarifier {
	return Clarifier{provider: provider}
}

func (c Clarifier) Clarify(ctx context.Context, input ClarificationInput) (ClarifiedTask, error) {
	if strings.TrimSpace(input.Mode) == "" || input.Mode == "off" {
		return ClarifiedTask{
			Goal:      input.Prompt,
			TaskClass: "analysis",
			TaskShape: string(TaskShapeSingle),
		}, nil
	}
	resp, err := c.provider.Generate(ctx, provider.PromptRequest{
		WorkerID: "mesh:clarifier",
		Messages: []provider.Message{
			{
				Role: "system",
				Content: "Clarify the user's task for a multi-agent runtime. " +
					`Return strict JSON only: {"goal":"...","deliverables":["..."],"constraints":["..."],"assumptions":["..."],"missing_info":["..."],"task_class":"coding|shell|analysis|research|writing","task_shape":"single|composite"}`,
			},
			{Role: "user", Content: input.Prompt},
		},
	})
	if err != nil {
		return ClarifiedTask{}, err
	}

	var task ClarifiedTask
	if err := json.Unmarshal([]byte(resp.Text), &task); err != nil {
		return ClarifiedTask{
			Goal:          input.Prompt,
			TaskClass:     "analysis",
			TaskShape:     string(TaskShapeSingle),
			Assumptions:   []string{"clarifier returned non-JSON output; falling back to original prompt"},
			LowConfidence: true,
		}, nil
	}
	if task.TaskShape == "" {
		task.TaskShape = string(TaskShapeSingle)
	}
	if task.TaskClass == "" {
		task.TaskClass = "analysis"
	}
	if input.CriticalMissingInfo && len(task.MissingInfo) > 0 && input.CurrentClarificationRound < input.MaxClarificationRounds {
		task.RequiresFollowUp = true
		task.FollowUpQuestion = task.MissingInfo[0]
		return task, nil
	}
	if input.CriticalMissingInfo && len(task.MissingInfo) > 0 && input.MaxClarificationRounds > 0 && input.CurrentClarificationRound >= input.MaxClarificationRounds {
		task.LowConfidence = true
		task.RequiresFollowUp = false
	}
	return task, nil
}
