package mesh

import "testing"

func TestSynthesizerBuildsExecutionBriefFromWinningProposal(t *testing.T) {
	synth := NewBriefSynthesizer()
	brief, err := synth.Synthesize([]CandidateReply{
		{
			AgentID: "agent-a",
			Proposal: Proposal{
				Understanding:  "Проверить память на сервере",
				PlannedChecks:  []string{"выполнить free -h"},
				SuggestedTools: []string{"shell.exec"},
				Risks:          []string{"неправильный интерпретатор"},
			},
			ProposalMetadata: ProposalMetadata{Confidence: 0.6},
			DeterministicScore: 1,
		},
		{
			AgentID: "agent-b",
			Proposal: Proposal{
				Understanding:  "Проверить память и кратко описать состояние",
				PlannedChecks:  []string{"выполнить free -h", "проверить swap"},
				SuggestedTools: []string{"shell.exec"},
				Risks:          []string{"нет доступа к shell"},
			},
			ProposalMetadata: ProposalMetadata{Confidence: 0.9},
			DeterministicScore: 5,
		},
	})
	if err != nil {
		t.Fatalf("synthesize: %v", err)
	}
	if brief.Goal == "" || len(brief.RequiredSteps) == 0 {
		t.Fatalf("unexpected brief: %#v", brief)
	}
	if len(brief.ConflictsToResolve) != 0 {
		t.Fatalf("expected no unresolved conflicts, got %#v", brief)
	}
}

func TestExecutionBriefValidationRejectsEmptyRequiredSteps(t *testing.T) {
	synth := NewBriefSynthesizer()
	_, err := synth.Validate(ExecutionBrief{Goal: "do work"})
	if err == nil {
		t.Fatal("expected validation error")
	}
}
