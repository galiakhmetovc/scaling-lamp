package mesh

import "testing"

func TestPeerDescriptorRoundTrip(t *testing.T) {
	peer := PeerDescriptor{
		AgentID: "agent-a",
		Model:   "glm-5",
		Status:  "idle",
	}
	if peer.AgentID != "agent-a" {
		t.Fatalf("unexpected peer: %#v", peer)
	}
	if peer.Status != "idle" {
		t.Fatalf("unexpected peer status: %#v", peer)
	}
}
