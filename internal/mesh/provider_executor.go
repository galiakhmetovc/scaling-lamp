package mesh

import (
	"context"
	"time"

	"teamd/internal/provider"
)

type ProviderExecutor struct {
	AgentID  string
	Provider provider.Provider
}

func (e ProviderExecutor) Execute(ctx context.Context, env Envelope) (CandidateReply, error) {
	started := time.Now()
	resp, err := e.Provider.Generate(ctx, provider.PromptRequest{
		WorkerID: "mesh:" + e.AgentID,
		Messages: []provider.Message{
			{Role: "user", Content: env.Prompt},
		},
	})
	if err != nil {
		return CandidateReply{
			AgentID:    e.AgentID,
			Stage:      "error",
			Err:        err.Error(),
			Latency:    time.Since(started),
			TokensUsed: 0,
		}, nil
	}
	return CandidateReply{
		AgentID:            e.AgentID,
		Stage:              "final",
		Text:               resp.Text,
		Latency:            time.Since(started),
		TokensUsed:         resp.Usage.TotalTokens,
		DeterministicScore: 1,
		PassedChecks:       true,
	}, nil
}
