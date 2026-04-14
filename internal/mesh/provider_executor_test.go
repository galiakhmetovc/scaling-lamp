package mesh

import (
	"context"
	"testing"

	"teamd/internal/provider"
)

func TestProviderExecutorBuildsFinalCandidateReply(t *testing.T) {
	exec := ProviderExecutor{
		AgentID:  "agent-a",
		Provider: provider.FakeProvider{},
	}

	reply, err := exec.Execute(context.Background(), Envelope{
		Prompt: "write a script",
	})
	if err != nil {
		t.Fatalf("execute: %v", err)
	}
	if reply.AgentID != "agent-a" {
		t.Fatalf("unexpected agent id: %#v", reply)
	}
	if reply.Stage != "final" {
		t.Fatalf("unexpected stage: %#v", reply)
	}
	if reply.Text != "write a script" {
		t.Fatalf("unexpected text: %#v", reply)
	}
}
