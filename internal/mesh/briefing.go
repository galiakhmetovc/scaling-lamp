package mesh

import (
	"fmt"
	"strings"
)

type BriefSynthesizer struct{}

func NewBriefSynthesizer() BriefSynthesizer {
	return BriefSynthesizer{}
}

func (BriefSynthesizer) Synthesize(candidates []CandidateReply) (ExecutionBrief, error) {
	if len(candidates) == 0 {
		return ExecutionBrief{}, fmt.Errorf("no candidates available")
	}
	best := candidates[0]
	for _, candidate := range candidates[1:] {
		if candidate.DeterministicScore > best.DeterministicScore {
			best = candidate
		}
	}
	brief := ExecutionBrief{
		Goal:          best.Proposal.Understanding,
		RequiredSteps: append([]string{}, best.Proposal.PlannedChecks...),
		Constraints:   append([]string{}, best.Proposal.Risks...),
		RequiredChecks: append([]string{}, best.Proposal.PlannedChecks...),
	}
	for _, candidate := range candidates {
		if candidate.AgentID == best.AgentID {
			continue
		}
		if candidate.Proposal.Understanding != "" && candidate.Proposal.Understanding != best.Proposal.Understanding {
			brief.AdoptedIdeas = append(brief.AdoptedIdeas, candidate.Proposal.Understanding)
		}
	}
	return BriefSynthesizer{}.Validate(brief)
}

func (BriefSynthesizer) Validate(brief ExecutionBrief) (ExecutionBrief, error) {
	if strings.TrimSpace(brief.Goal) == "" {
		return ExecutionBrief{}, fmt.Errorf("execution brief goal is required")
	}
	if len(brief.RequiredSteps) == 0 {
		return ExecutionBrief{}, fmt.Errorf("execution brief required_steps must not be empty")
	}
	if len(brief.ConflictsToResolve) > 0 {
		return ExecutionBrief{}, fmt.Errorf("execution brief has unresolved conflicts")
	}
	return brief, nil
}
