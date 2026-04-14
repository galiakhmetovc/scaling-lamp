package mesh

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestHTTPTransportDeliversEnvelopeToPeer(t *testing.T) {
	transport := NewHTTPTransport(http.DefaultClient)
	server := httptest.NewServer(transport.Handler(func(_ context.Context, env Envelope) (Envelope, error) {
		return Envelope{
			Version:    env.Version,
			MessageID:  env.MessageID + "-reply",
			TraceID:    env.TraceID,
			SessionID:  env.SessionID,
			OwnerAgent: env.OwnerAgent,
			FromAgent:  env.ToAgent,
			ToAgent:    env.FromAgent,
			TaskClass:  env.TaskClass,
			Kind:       "reply",
			TTL:        env.TTL,
			Prompt:     "ack:" + env.Prompt,
		}, nil
	}))
	defer server.Close()

	reply, err := transport.Send(context.Background(), server.URL, Envelope{
		Version:    "v1",
		MessageID:  "msg-1",
		TraceID:    "trace-1",
		SessionID:  "session-1",
		OwnerAgent: "owner",
		FromAgent:  "owner",
		ToAgent:    "peer-a",
		TaskClass:  "analysis",
		Kind:       "task",
		TTL:        3,
		Prompt:     "hello",
	})
	if err != nil {
		t.Fatalf("send: %v", err)
	}
	if reply.Kind != "reply" {
		t.Fatalf("unexpected reply kind: %#v", reply)
	}
	if reply.Prompt != "ack:hello" {
		t.Fatalf("unexpected reply payload: %#v", reply)
	}
}
