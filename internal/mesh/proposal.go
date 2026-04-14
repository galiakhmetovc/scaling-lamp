package mesh

import (
	"encoding/json"
	"fmt"
)

func ParseProposalReply(agentID, raw string) (CandidateReply, error) {
	var payload struct {
		Understanding  string   `json:"understanding"`
		PlannedChecks  []string `json:"planned_checks"`
		SuggestedTools []string `json:"suggested_tools"`
		Risks          []string `json:"risks"`
		DraftConclusion string  `json:"draft_conclusion"`
		Metadata       struct {
			EstimatedTokens int      `json:"estimated_tokens"`
			SuggestedTools  []string `json:"suggested_tools"`
			Confidence      float64  `json:"confidence"`
			RiskFlags       []string `json:"risk_flags"`
		} `json:"metadata"`
	}
	if err := json.Unmarshal([]byte(raw), &payload); err != nil {
		return CandidateReply{}, fmt.Errorf("parse proposal reply: %w", err)
	}
	return CandidateReply{
		AgentID: agentID,
		Stage:   "final",
		Proposal: Proposal{
			Understanding:  payload.Understanding,
			PlannedChecks:  payload.PlannedChecks,
			SuggestedTools: payload.SuggestedTools,
			Risks:          payload.Risks,
			DraftConclusion: payload.DraftConclusion,
		},
		ProposalMetadata: ProposalMetadata{
			EstimatedTokens: payload.Metadata.EstimatedTokens,
			SuggestedTools:  payload.Metadata.SuggestedTools,
			Confidence:      payload.Metadata.Confidence,
			RiskFlags:       payload.Metadata.RiskFlags,
		},
	}, nil
}
