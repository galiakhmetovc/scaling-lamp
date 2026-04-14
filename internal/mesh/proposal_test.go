package mesh

import (
	"testing"
)

func TestParseProposalReply(t *testing.T) {
	reply, err := ParseProposalReply("agent-a", `{"understanding":"Проверить память","planned_checks":["free -h"],"suggested_tools":["shell.exec"],"risks":["нет доступа"],"draft_conclusion":"память в норме","metadata":{"estimated_tokens":120,"suggested_tools":["shell.exec"],"confidence":0.8,"risk_flags":["requires_shell"]}}`)
	if err != nil {
		t.Fatalf("parse proposal: %v", err)
	}
	if reply.AgentID != "agent-a" || reply.Proposal.Understanding != "Проверить память" {
		t.Fatalf("unexpected proposal reply: %#v", reply)
	}
	if reply.ProposalMetadata.Confidence != 0.8 {
		t.Fatalf("unexpected proposal metadata: %#v", reply)
	}
}

func TestProposalCollectionPolicyHonorsPartialQuorum(t *testing.T) {
	policy := ProposalPolicy{ProposalTimeout: 15, MinQuorumSize: 2, RetryCount: 1}
	if !policy.SatisfiesQuorum(2) {
		t.Fatalf("expected quorum to be satisfied")
	}
	if policy.SatisfiesQuorum(1) {
		t.Fatalf("expected quorum to remain unsatisfied")
	}
}
