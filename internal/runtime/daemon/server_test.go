package daemon_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"golang.org/x/net/websocket"
	"teamd/internal/runtime"
	"teamd/internal/runtime/daemon"
	"teamd/internal/runtime/eventing"
)

func TestNewFailsClosedWithoutOperatorSurfaceContract(t *testing.T) {
	t.Parallel()

	agent := &runtime.Agent{}
	_, err := daemon.New(agent)
	if err == nil || !strings.Contains(err.Error(), "operator_surface") {
		t.Fatalf("expected operator surface error, got %v", err)
	}
}

func TestBootstrapEndpointReturnsConfigDrivenSnapshot(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
	if err := agent.RecordEvent(context.Background(), eventing.Event{
		ID:               "evt-session-created",
		Kind:             eventing.EventSessionCreated,
		OccurredAt:       time.Date(2026, 4, 15, 17, 0, 0, 0, time.UTC),
		AggregateID:      "session-1",
		AggregateType:    eventing.AggregateSession,
		AggregateVersion: 1,
		Payload:          map[string]any{"session_id": "session-1"},
	}); err != nil {
		t.Fatalf("record session created: %v", err)
	}

	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}

	req := httptest.NewRequest(http.MethodGet, "/api/bootstrap", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", rec.Code)
	}

	var payload daemon.BootstrapPayload
	if err := json.Unmarshal(rec.Body.Bytes(), &payload); err != nil {
		t.Fatalf("decode bootstrap: %v", err)
	}
	if payload.ListenAddr != "0.0.0.0:8080" {
		t.Fatalf("listen addr = %q, want 0.0.0.0:8080", payload.ListenAddr)
	}
	if payload.Transport.EndpointPath != "/api" || payload.Transport.WebsocketPath != "/ws" {
		t.Fatalf("transport snapshot = %+v", payload.Transport)
	}
	if payload.Assets.Mode != "embedded_assets" {
		t.Fatalf("assets mode = %q", payload.Assets.Mode)
	}
	if len(payload.Sessions) != 1 || payload.Sessions[0].SessionID != "session-1" {
		t.Fatalf("sessions = %+v", payload.Sessions)
	}
}

func TestHealthzEndpointIsAvailable(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}

	req := httptest.NewRequest(http.MethodGet, "/healthz", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", rec.Code)
	}
	if !strings.Contains(rec.Body.String(), `"ok":true`) {
		t.Fatalf("unexpected healthz body: %s", rec.Body.String())
	}
}

func TestClientConfigScriptUsesConfiguredTransportPaths(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}

	req := httptest.NewRequest(http.MethodGet, "/config.js", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", rec.Code)
	}
	body := rec.Body.String()
	if !strings.Contains(body, `"endpointPath":"/api"`) || !strings.Contains(body, `"websocketPath":"/ws"`) {
		t.Fatalf("unexpected client config body: %s", body)
	}
}

func TestWebsocketStreamsUIBusEvents(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}
	httpServer := httptest.NewServer(server.Handler())
	defer httpServer.Close()

	origin := "http://example.test"
	wsURL := websocketURL(t, httpServer.URL, "/ws")
	conn, err := websocket.Dial(wsURL, "", origin)
	if err != nil {
		t.Fatalf("dial websocket: %v", err)
	}
	defer conn.Close()

	var hello daemon.WebsocketEnvelope
	if err := json.NewDecoder(conn).Decode(&hello); err != nil {
		t.Fatalf("decode hello: %v", err)
	}
	if hello.Type != "hello" {
		t.Fatalf("hello type = %q, want hello", hello.Type)
	}

	agent.UIBus.Publish(runtime.UIEvent{
		Kind:      runtime.UIEventStatusChanged,
		SessionID: "session-1",
		RunID:     "run-1",
		Status:    "running",
	})

	var envelope daemon.WebsocketEnvelope
	if err := json.NewDecoder(conn).Decode(&envelope); err != nil {
		t.Fatalf("decode ui event envelope: %v", err)
	}
	if envelope.Type != "ui_event" || envelope.Event == nil {
		t.Fatalf("unexpected websocket envelope: %+v", envelope)
	}
	if envelope.Event.SessionID != "session-1" || envelope.Event.Status != "running" {
		t.Fatalf("unexpected ui event payload: %+v", envelope.Event)
	}
}

func buildAgentWithOperatorSurface(t *testing.T) *runtime.Agent {
	t.Helper()

	root := filepath.Join("..", "..", "..")
	configPath := filepath.Join(root, "config", "zai-smoke", "agent.yaml")
	agent, err := runtime.BuildAgent(configPath)
	if err != nil {
		t.Fatalf("build agent: %v", err)
	}
	return agent
}

func websocketURL(t *testing.T, baseURL, route string) string {
	t.Helper()

	parsed, err := url.Parse(baseURL)
	if err != nil {
		t.Fatalf("parse base url: %v", err)
	}
	switch parsed.Scheme {
	case "http":
		parsed.Scheme = "ws"
	case "https":
		parsed.Scheme = "wss"
	default:
		t.Fatalf("unexpected base scheme %q", parsed.Scheme)
	}
	parsed.Path = route
	return parsed.String()
}
