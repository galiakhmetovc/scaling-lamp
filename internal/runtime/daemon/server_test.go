package daemon_test

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
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
	if payload.Settings.Revision == "" || len(payload.Settings.FormFields) == 0 {
		t.Fatalf("settings snapshot = %+v, want populated revisioned settings", payload.Settings)
	}
	if len(payload.Sessions) != 1 || payload.Sessions[0].SessionID != "session-1" {
		t.Fatalf("sessions = %+v", payload.Sessions)
	}
}

func TestBootstrapEndpointReturnsEmptySessionsArrayWhenNoSessionsExist(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
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

	var payload struct {
		Sessions []map[string]any `json:"sessions"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &payload); err != nil {
		t.Fatalf("decode bootstrap: %v", err)
	}
	if payload.Sessions == nil {
		t.Fatal("bootstrap sessions = nil, want empty array")
	}
	if len(payload.Sessions) != 0 {
		t.Fatalf("bootstrap sessions len = %d, want 0", len(payload.Sessions))
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

func TestEmbeddedAssetsServeWebAppShell(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	rec := httptest.NewRecorder()
	server.Handler().ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", rec.Code)
	}
	body := rec.Body.String()
	if !strings.Contains(body, `<div id="root"></div>`) {
		t.Fatalf("expected embedded web app shell, got: %s", body)
	}
	if strings.Contains(body, "Phase 1 control plane shell") {
		t.Fatalf("legacy daemon shell still served: %s", body)
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

func TestWebsocketStreamsUIBusEventsWithStableJSONKeys(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}
	httpServer := httptest.NewServer(server.Handler())
	defer httpServer.Close()

	conn := dialWebsocket(t, httpServer.URL, "/ws")
	defer conn.Close()
	_ = readEnvelopeJSON(t, conn)

	agent.UIBus.Publish(runtime.UIEvent{
		Kind:      runtime.UIEventStreamText,
		SessionID: "session-1",
		RunID:     "run-1",
		Text:      "partial",
	})

	envelope := readEnvelopeJSON(t, conn)
	if envelope["type"] != "ui_event" {
		t.Fatalf("type = %#v, want ui_event", envelope["type"])
	}
	event := mapPayload(t, envelope["event"])
	if event["kind"] != "stream.text" {
		t.Fatalf("event.kind = %#v, want stream.text", event["kind"])
	}
	if event["session_id"] != "session-1" {
		t.Fatalf("event.session_id = %#v, want session-1", event["session_id"])
	}
	if event["text"] != "partial" {
		t.Fatalf("event.text = %#v, want partial", event["text"])
	}
}

func TestWebsocketSessionCreateCommandReturnsSessionSnapshot(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}
	httpServer := httptest.NewServer(server.Handler())
	defer httpServer.Close()

	conn := dialWebsocket(t, httpServer.URL, "/ws")
	defer conn.Close()
	_ = readEnvelopeJSON(t, conn)

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-1",
		"command": "session.create",
	})

	accepted := readEnvelopeJSON(t, conn)
	if accepted["type"] != "command_accepted" {
		t.Fatalf("accepted type = %#v, want command_accepted", accepted["type"])
	}
	completed := readEnvelopeJSON(t, conn)
	if completed["type"] != "command_completed" {
		t.Fatalf("completed type = %#v, want command_completed", completed["type"])
	}
	payload := mapPayload(t, completed["payload"])
	session := mapPayload(t, payload["session"])
	if session["session_id"] == "" {
		t.Fatalf("session snapshot missing session_id: %+v", session)
	}
	queuedDrafts, ok := session["queued_drafts"].([]any)
	if !ok {
		t.Fatalf("queued_drafts type = %T, want []any", session["queued_drafts"])
	}
	if len(queuedDrafts) != 0 {
		t.Fatalf("queued_drafts len = %d, want 0", len(queuedDrafts))
	}
}

func TestWebsocketPlanCreateCommandUpdatesSessionSnapshot(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}
	httpServer := httptest.NewServer(server.Handler())
	defer httpServer.Close()

	conn := dialWebsocket(t, httpServer.URL, "/ws")
	defer conn.Close()
	_ = readEnvelopeJSON(t, conn)

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-1",
		"command": "session.create",
	})
	_ = readEnvelopeJSON(t, conn)
	created := readEnvelopeJSON(t, conn)
	sessionID := mapPayload(t, mapPayload(t, created["payload"])["session"])["session_id"]

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-2",
		"command": "plan.create",
		"payload": map[string]any{
			"session_id": sessionID,
			"goal":       "Ship daemon clients",
		},
	})
	_ = readEnvelopeJSON(t, conn)
	completed := readEnvelopeJSON(t, conn)
	payload := mapPayload(t, completed["payload"])
	session := mapPayload(t, payload["session"])
	planHead := mapPayload(t, session["plan"])
	plan := mapPayload(t, planHead["plan"])
	if got := plan["goal"]; got != "Ship daemon clients" {
		t.Fatalf("plan goal = %#v, want Ship daemon clients", got)
	}
}

func TestWebsocketChatSendCommandCompletesWithUpdatedSessionSnapshot(t *testing.T) {
	provider := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"id":"resp-1","model":"glm-5-turbo","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"Pong from daemon"}}],"usage":{"prompt_tokens":8,"completion_tokens":4,"total_tokens":12}}`))
	}))
	defer provider.Close()

	agent := buildChatDaemonAgent(t, provider.URL)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}
	httpServer := httptest.NewServer(server.Handler())
	defer httpServer.Close()

	conn := dialWebsocket(t, httpServer.URL, "/ws")
	defer conn.Close()
	_ = readEnvelopeJSON(t, conn)

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-1",
		"command": "session.create",
	})
	_ = readEnvelopeJSON(t, conn)
	created := readEnvelopeJSON(t, conn)
	sessionID := mapPayload(t, mapPayload(t, created["payload"])["session"])["session_id"]

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-2",
		"command": "chat.send",
		"payload": map[string]any{
			"session_id": sessionID,
			"prompt":     "ping",
		},
	})
	accepted := readEnvelopeJSON(t, conn)
	if accepted["type"] != "command_accepted" {
		t.Fatalf("accepted type = %#v, want command_accepted", accepted["type"])
	}
	completed := waitForEnvelopeType(t, conn, "command_completed")
	payload := mapPayload(t, completed["payload"])
	session := mapPayload(t, payload["session"])
	transcript, ok := session["transcript"].([]any)
	if !ok || len(transcript) != 2 {
		t.Fatalf("transcript = %#v, want 2 messages", session["transcript"])
	}
	last := mapPayload(t, transcript[1])
	if got := last["content"]; got != "Pong from daemon" {
		t.Fatalf("assistant content = %#v, want Pong from daemon", got)
	}
}

func TestWebsocketDraftCommandsRoundTripQueueState(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}
	httpServer := httptest.NewServer(server.Handler())
	defer httpServer.Close()

	conn := dialWebsocket(t, httpServer.URL, "/ws")
	defer conn.Close()
	_ = readEnvelopeJSON(t, conn)

	writeCommandEnvelope(t, conn, map[string]any{"type": "command", "id": "cmd-1", "command": "session.create"})
	_ = readEnvelopeJSON(t, conn)
	created := readEnvelopeJSON(t, conn)
	sessionID := mapPayload(t, mapPayload(t, created["payload"])["session"])["session_id"]

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-2",
		"command": "draft.enqueue",
		"payload": map[string]any{"session_id": sessionID, "text": "later"},
	})
	_, completed := waitForEventAndCommandCompleted(t, conn, "draft_queued", "cmd-2")
	session := mapPayload(t, mapPayload(t, completed["payload"])["session"])
	queue, ok := session["queued_drafts"].([]any)
	if !ok || len(queue) != 1 {
		t.Fatalf("queued_drafts = %#v, want single draft", session["queued_drafts"])
	}
	draft := mapPayload(t, queue[0])

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-3",
		"command": "draft.recall",
		"payload": map[string]any{"session_id": sessionID, "draft_id": draft["id"]},
	})
	_, recalled := waitForEventAndCommandCompleted(t, conn, "draft_recalled", "cmd-3")
	session = mapPayload(t, mapPayload(t, recalled["payload"])["session"])
	queue, _ = session["queued_drafts"].([]any)
	if len(queue) != 0 {
		t.Fatalf("queued_drafts after recall = %#v, want empty", session["queued_drafts"])
	}
}

func TestWebsocketChatSendQueuesWhileActiveAndAutoDispatchesNextDraft(t *testing.T) {
	providerStarted := make(chan struct{}, 1)
	releaseFirst := make(chan struct{})
	requests := 0
	provider := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests++
		if requests == 1 {
			providerStarted <- struct{}{}
			<-releaseFirst
		}
		w.Header().Set("Content-Type", "application/json")
		content := "First response"
		if requests == 2 {
			content = "Queued response"
		}
		_, _ = w.Write([]byte(fmt.Sprintf(`{"id":"resp-%d","model":"glm-5-turbo","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"%s"}}],"usage":{"prompt_tokens":8,"completion_tokens":4,"total_tokens":12}}`, requests, content)))
	}))
	defer provider.Close()

	agent := buildChatDaemonAgent(t, provider.URL)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}
	httpServer := httptest.NewServer(server.Handler())
	defer httpServer.Close()

	conn := dialWebsocket(t, httpServer.URL, "/ws")
	defer conn.Close()
	_ = readEnvelopeJSON(t, conn)

	writeCommandEnvelope(t, conn, map[string]any{"type": "command", "id": "cmd-1", "command": "session.create"})
	_ = readEnvelopeJSON(t, conn)
	created := readEnvelopeJSON(t, conn)
	sessionID := mapPayload(t, mapPayload(t, created["payload"])["session"])["session_id"]

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-2",
		"command": "chat.send",
		"payload": map[string]any{"session_id": sessionID, "prompt": "first"},
	})
	_ = readEnvelopeJSON(t, conn)
	<-providerStarted

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-3",
		"command": "chat.send",
		"payload": map[string]any{"session_id": sessionID, "prompt": "second"},
	})
	queuedEvent, queuedCompleted := waitForDraftQueuedAndCommandCompleted(t, conn, "cmd-3")
	queuedPayload := mapPayload(t, queuedCompleted["payload"])
	if queuedPayload["queued"] != true {
		t.Fatalf("queued payload = %#v, want queued=true", queuedPayload)
	}
	queuedDraft := mapPayload(t, queuedEvent["payload"])["draft"]

	close(releaseFirst)

	firstCompleted, started, queuedRunDone := waitForQueuedRunProgress(t, conn, "cmd-2")
	_ = firstCompleted
	startedDraft := mapPayload(t, started["payload"])["draft"]
	if mapPayload(t, startedDraft)["id"] != mapPayload(t, queuedDraft)["id"] {
		t.Fatalf("started queued draft = %#v, want %#v", startedDraft, queuedDraft)
	}
	payload := mapPayload(t, queuedRunDone["payload"])
	result := mapPayload(t, payload["result"])
	if result["content"] != "Queued response" {
		t.Fatalf("queued result content = %#v, want Queued response", result["content"])
	}
	session := mapPayload(t, payload["session"])
	if session["main_run_active"] != false {
		t.Fatalf("session main_run_active = %#v, want false", session["main_run_active"])
	}
}

func TestWebsocketSettingsFormApplyReloadsAgentAndRejectsStaleRevision(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}
	httpServer := httptest.NewServer(server.Handler())
	defer httpServer.Close()

	conn := dialWebsocket(t, httpServer.URL, "/ws")
	defer conn.Close()
	_ = readEnvelopeJSON(t, conn)

	writeCommandEnvelope(t, conn, map[string]any{"type": "command", "id": "cmd-1", "command": "settings.get"})
	_ = readEnvelopeJSON(t, conn)
	getCompleted := waitForCommandCompleted(t, conn, "cmd-1")
	settings := mapPayload(t, mapPayload(t, getCompleted["payload"])["settings"])
	baseRevision := settings["revision"]
	fields, ok := settings["form_fields"].([]any)
	if !ok || len(fields) == 0 {
		t.Fatalf("form fields = %#v, want populated", settings["form_fields"])
	}

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-2",
		"command": "settings.form.apply",
		"payload": map[string]any{
			"base_revision": baseRevision,
			"values": map[string]any{
				"max_tool_rounds": 7,
				"markdown_style":  "light",
			},
		},
	})
	_ = readEnvelopeJSON(t, conn)
		applyCompleted := waitForCommandCompleted(t, conn, "cmd-2")
		_ = waitForEnvelopeType(t, conn, "settings_applied")
	applied := mapPayload(t, mapPayload(t, applyCompleted["payload"])["settings"])
	if applied["revision"] == baseRevision {
		t.Fatalf("settings revision did not change after apply")
	}

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-3",
		"command": "settings.get",
	})
	_ = readEnvelopeJSON(t, conn)
	reloaded := mapPayload(t, mapPayload(t, waitForCommandCompleted(t, conn, "cmd-3")["payload"])["settings"])
	assertSettingValue(t, reloaded["form_fields"], "max_tool_rounds", float64(7))
	assertSettingValue(t, reloaded["form_fields"], "markdown_style", "light")

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-4",
		"command": "settings.form.apply",
		"payload": map[string]any{
			"base_revision": baseRevision,
			"values": map[string]any{
				"show_tool_calls": false,
			},
		},
	})
	_ = readEnvelopeJSON(t, conn)
	failed := waitForCommandFailed(t, conn, "cmd-4")
	if !strings.Contains(fmt.Sprint(failed["error"]), "revision conflict") {
		t.Fatalf("stale apply error = %#v, want revision conflict", failed["error"])
	}
}

func TestWebsocketSettingsRawApplyUsesRevisionChecks(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}
	httpServer := httptest.NewServer(server.Handler())
	defer httpServer.Close()

	conn := dialWebsocket(t, httpServer.URL, "/ws")
	defer conn.Close()
	_ = readEnvelopeJSON(t, conn)

	writeCommandEnvelope(t, conn, map[string]any{"type": "command", "id": "cmd-1", "command": "settings.raw.get", "payload": map[string]any{"path": "policies/chat/output.yaml"}})
	_ = readEnvelopeJSON(t, conn)
	filePayload := mapPayload(t, waitForCommandCompleted(t, conn, "cmd-1")["payload"])
	file := mapPayload(t, filePayload["file"])
	revision := file["revision"]
	content := fmt.Sprint(file["content"])

	updatedContent := strings.Replace(content, "markdown_style: dark", "markdown_style: light", 1)
	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-2",
		"command": "settings.raw.apply",
		"payload": map[string]any{
			"path":          "policies/chat/output.yaml",
			"base_revision": revision,
			"content":       updatedContent,
		},
	})
	_ = readEnvelopeJSON(t, conn)
	_ = waitForCommandCompleted(t, conn, "cmd-2")
	_ = waitForEnvelopeType(t, conn, "settings_applied")

	writeCommandEnvelope(t, conn, map[string]any{"type": "command", "id": "cmd-3", "command": "settings.raw.get", "payload": map[string]any{"path": "policies/chat/output.yaml"}})
	_ = readEnvelopeJSON(t, conn)
	reloaded := mapPayload(t, waitForCommandCompleted(t, conn, "cmd-3")["payload"])
	file = mapPayload(t, reloaded["file"])
	if !strings.Contains(fmt.Sprint(file["content"]), "markdown_style: light") {
		t.Fatalf("raw settings content not updated: %s", file["content"])
	}

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-4",
		"command": "settings.raw.apply",
		"payload": map[string]any{
			"path":          "policies/chat/output.yaml",
			"base_revision": revision,
			"content":       content,
		},
	})
	_ = readEnvelopeJSON(t, conn)
	failed := waitForCommandFailed(t, conn, "cmd-4")
	if !strings.Contains(fmt.Sprint(failed["error"]), "revision conflict") {
		t.Fatalf("stale raw apply error = %#v, want revision conflict", failed["error"])
	}
}

func TestWebsocketSettingsRawApplyRollsBackOnInvalidConfig(t *testing.T) {
	t.Parallel()

	agent := buildAgentWithOperatorSurface(t)
	server, err := daemon.New(agent)
	if err != nil {
		t.Fatalf("new daemon server: %v", err)
	}
	httpServer := httptest.NewServer(server.Handler())
	defer httpServer.Close()

	conn := dialWebsocket(t, httpServer.URL, "/ws")
	defer conn.Close()
	_ = readEnvelopeJSON(t, conn)

	writeCommandEnvelope(t, conn, map[string]any{"type": "command", "id": "cmd-1", "command": "settings.raw.get", "payload": map[string]any{"path": "policies/chat/output.yaml"}})
	_ = readEnvelopeJSON(t, conn)
	filePayload := mapPayload(t, waitForCommandCompleted(t, conn, "cmd-1")["payload"])
	file := mapPayload(t, filePayload["file"])
	originalRevision := fmt.Sprint(file["revision"])
	originalContent := fmt.Sprint(file["content"])

	writeCommandEnvelope(t, conn, map[string]any{
		"type":    "command",
		"id":      "cmd-2",
		"command": "settings.raw.apply",
		"payload": map[string]any{
			"path":          "policies/chat/output.yaml",
			"base_revision": originalRevision,
			"content":       "kind: ChatOutputPolicyConfig\nspec:\n  params: [\n",
		},
	})
	_ = readEnvelopeJSON(t, conn)
	failed := waitForCommandFailed(t, conn, "cmd-2")
	if !strings.Contains(fmt.Sprint(failed["error"]), "yaml") {
		t.Fatalf("invalid raw apply error = %#v, want yaml parse/build failure", failed["error"])
	}

	writeCommandEnvelope(t, conn, map[string]any{"type": "command", "id": "cmd-3", "command": "settings.raw.get", "payload": map[string]any{"path": "policies/chat/output.yaml"}})
	_ = readEnvelopeJSON(t, conn)
	reloaded := mapPayload(t, waitForCommandCompleted(t, conn, "cmd-3")["payload"])
	file = mapPayload(t, reloaded["file"])
	if fmt.Sprint(file["revision"]) != originalRevision {
		t.Fatalf("raw settings revision changed after failed apply: got %s want %s", file["revision"], originalRevision)
	}
	if fmt.Sprint(file["content"]) != originalContent {
		t.Fatalf("raw settings content changed after failed apply")
	}
}

func buildAgentWithOperatorSurface(t *testing.T) *runtime.Agent {
	t.Helper()
	return buildChatDaemonAgent(t, "http://127.0.0.1:1")
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

func dialWebsocket(t *testing.T, baseURL, route string) *websocket.Conn {
	t.Helper()
	conn, err := websocket.Dial(websocketURL(t, baseURL, route), "", "http://example.test")
	if err != nil {
		t.Fatalf("dial websocket: %v", err)
	}
	return conn
}

func writeCommandEnvelope(t *testing.T, conn *websocket.Conn, payload map[string]any) {
	t.Helper()
	if err := conn.SetDeadline(time.Now().Add(3 * time.Second)); err != nil {
		t.Fatalf("set deadline: %v", err)
	}
	if err := json.NewEncoder(conn).Encode(payload); err != nil {
		t.Fatalf("encode websocket payload: %v", err)
	}
}

func readEnvelopeJSON(t *testing.T, conn *websocket.Conn) map[string]any {
	t.Helper()
	if err := conn.SetDeadline(time.Now().Add(3 * time.Second)); err != nil {
		t.Fatalf("set deadline: %v", err)
	}
	var payload map[string]any
	if err := json.NewDecoder(conn).Decode(&payload); err != nil {
		t.Fatalf("decode websocket payload: %v", err)
	}
	return payload
}

func waitForEnvelopeType(t *testing.T, conn *websocket.Conn, want string) map[string]any {
	t.Helper()
	for i := 0; i < 16; i++ {
		payload := readEnvelopeJSON(t, conn)
		if payload["type"] == "command_failed" {
			t.Fatalf("received command_failed envelope: %+v", payload)
		}
		if payload["type"] == want {
			return payload
		}
	}
	t.Fatalf("did not receive websocket envelope type %q", want)
	return nil
}

func waitForCommandCompleted(t *testing.T, conn *websocket.Conn, commandID string) map[string]any {
	t.Helper()
	for i := 0; i < 16; i++ {
		payload := readEnvelopeJSON(t, conn)
		if payload["type"] == "command_failed" {
			t.Fatalf("received command_failed envelope: %+v", payload)
		}
		if payload["type"] != "command_completed" {
			continue
		}
		if payload["id"] == commandID {
			return payload
		}
	}
	t.Fatalf("did not receive command_completed envelope for %q", commandID)
	return nil
}

func waitForCommandFailed(t *testing.T, conn *websocket.Conn, commandID string) map[string]any {
	t.Helper()
	for i := 0; i < 16; i++ {
		payload := readEnvelopeJSON(t, conn)
		if payload["type"] != "command_failed" {
			continue
		}
		if payload["id"] == commandID {
			return payload
		}
	}
	t.Fatalf("did not receive command_failed envelope for %q", commandID)
	return nil
}

func waitForDraftQueuedAndCommandCompleted(t *testing.T, conn *websocket.Conn, commandID string) (map[string]any, map[string]any) {
	return waitForEventAndCommandCompleted(t, conn, "draft_queued", commandID)
}

func waitForQueuedRunProgress(t *testing.T, conn *websocket.Conn, commandID string) (map[string]any, map[string]any, map[string]any) {
	t.Helper()
	var completed map[string]any
	var started map[string]any
	var queuedDone map[string]any
	for i := 0; i < 24; i++ {
		payload := readEnvelopeJSON(t, conn)
		if payload["type"] == "command_failed" {
			t.Fatalf("received command_failed envelope: %+v", payload)
		}
		switch payload["type"] {
		case "command_completed":
			if payload["id"] == commandID {
				completed = payload
			}
		case "queue_draft_started":
			started = payload
		case "queue_draft_completed":
			queuedDone = payload
		}
		if completed != nil && started != nil && queuedDone != nil {
			return completed, started, queuedDone
		}
	}
	t.Fatalf("did not receive queued run progress for %q", commandID)
	return nil, nil, nil
}

func waitForEventAndCommandCompleted(t *testing.T, conn *websocket.Conn, eventType, commandID string) (map[string]any, map[string]any) {
	t.Helper()
	var event map[string]any
	var completed map[string]any
	for i := 0; i < 16; i++ {
		payload := readEnvelopeJSON(t, conn)
		if payload["type"] == "command_failed" {
			t.Fatalf("received command_failed envelope: %+v", payload)
		}
		switch payload["type"] {
		case eventType:
			event = payload
		case "command_completed":
			if payload["id"] == commandID {
				completed = payload
			}
		}
		if event != nil && completed != nil {
			return event, completed
		}
	}
	t.Fatalf("did not receive both %q and command_completed for %q", eventType, commandID)
	return nil, nil
}

func mapPayload(t *testing.T, value any) map[string]any {
	t.Helper()
	out, ok := value.(map[string]any)
	if !ok {
		t.Fatalf("value = %#v, want map[string]any", value)
	}
	return out
}

func assertSettingValue(t *testing.T, fieldsValue any, key string, want any) {
	t.Helper()
	fields, ok := fieldsValue.([]any)
	if !ok {
		t.Fatalf("form fields = %#v, want []any", fieldsValue)
	}
	for _, item := range fields {
		field := mapPayload(t, item)
		if field["key"] != key {
			continue
		}
		if field["value"] != want {
			t.Fatalf("field %q value = %#v, want %#v", key, field["value"], want)
		}
		return
	}
	t.Fatalf("field %q not found in %#v", key, fieldsValue)
}

func buildChatDaemonAgent(t *testing.T, baseURL string) *runtime.Agent {
	t.Helper()
	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: daemon-chat\n"+
		"spec:\n"+
		"  runtime:\n"+
		"    max_tool_rounds: 4\n"+
		"    event_log: file_jsonl\n"+
		"    event_log_path: ./var/events.jsonl\n"+
		"    projection_store_path: ./var/projections.json\n"+
		"    prompt_asset_executor: prompt_asset_default\n"+
		"    transport_executor: transport_default\n"+
		"    request_shape_executor: request_shape_default\n"+
		"    provider_client: provider_client_default\n"+
		"    projections: [session, session_catalog, run, transcript, chat_timeline, active_plan, plan_head, shell_command]\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n"+
		"    request_shape: ./contracts/request-shape.yaml\n"+
		"    memory: ./contracts/memory.yaml\n"+
		"    prompt_assets: ./contracts/prompt-assets.yaml\n"+
		"    chat: ./contracts/chat.yaml\n"+
		"    operator_surface: ./contracts/operator-surface.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), "kind: TransportContractConfig\nversion: v1\nid: transport-main\nspec:\n  endpoint_policy_path: ../policies/transport/endpoint.yaml\n  auth_policy_path: ../policies/transport/auth.yaml\n  retry_policy_path: ../policies/transport/retry.yaml\n  timeout_policy_path: ../policies/transport/timeout.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "request-shape.yaml"), "kind: RequestShapeContractConfig\nversion: v1\nid: request-shape-main\nspec:\n  model_policy_path: ../policies/request-shape/model.yaml\n  message_policy_path: ../policies/request-shape/messages.yaml\n  tool_policy_path: ../policies/request-shape/tools.yaml\n  response_format_policy_path: ../policies/request-shape/response-format.yaml\n  streaming_policy_path: ../policies/request-shape/streaming.yaml\n  sampling_policy_path: ../policies/request-shape/sampling.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "memory.yaml"), "kind: MemoryContractConfig\nversion: v1\nid: memory-main\nspec:\n  offload_policy_path: ../policies/memory/offload.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "prompt-assets.yaml"), "kind: PromptAssetsContractConfig\nversion: v1\nid: prompt-assets-main\nspec:\n  prompt_asset_policy_path: ../policies/prompt-assets/assets.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "chat.yaml"), "kind: ChatContractConfig\nversion: v1\nid: chat-main\nspec:\n  input_policy_path: ../policies/chat/input.yaml\n  submit_policy_path: ../policies/chat/submit.yaml\n  output_policy_path: ../policies/chat/output.yaml\n  status_policy_path: ../policies/chat/status.yaml\n  command_policy_path: ../policies/chat/command.yaml\n  resume_policy_path: ../policies/chat/resume.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "operator-surface.yaml"), "kind: OperatorSurfaceContractConfig\nversion: v1\nid: operator-surface-main\nspec:\n  daemon_server_policy_path: ../policies/operator-surface/daemon-server.yaml\n  web_assets_policy_path: ../policies/operator-surface/web-assets.yaml\n  client_transport_policy_path: ../policies/operator-surface/client-transport.yaml\n  settings_policy_path: ../policies/operator-surface/settings.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), fmt.Sprintf("kind: EndpointPolicyConfig\nversion: v1\nid: endpoint-main\nspec:\n  enabled: true\n  strategy: static\n  params:\n    base_url: %s\n    path: /chat/completions\n    method: POST\n", baseURL))
	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "auth.yaml"), "kind: AuthPolicyConfig\nversion: v1\nid: auth-main\nspec:\n  enabled: false\n  strategy: bearer_token\n  params:\n    header: Authorization\n    prefix: Bearer\n    value_env_var: TEAMD_ZAI_API_KEY\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "retry.yaml"), "kind: RetryPolicyConfig\nversion: v1\nid: retry-main\nspec:\n  enabled: true\n  strategy: exponential_jitter\n  params:\n    max_attempts: 1\n    base_delay: 50ms\n    max_delay: 50ms\n    retry_on_statuses: [500]\n    retry_on_errors: [transport_error]\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "timeout.yaml"), "kind: TimeoutPolicyConfig\nversion: v1\nid: timeout-main\nspec:\n  enabled: true\n  strategy: per_request\n  params:\n    total: 30s\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "model.yaml"), "kind: ModelPolicyConfig\nversion: v1\nid: model-main\nspec:\n  enabled: true\n  strategy: static_model\n  params:\n    model: glm-5-turbo\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "messages.yaml"), "kind: MessagePolicyConfig\nversion: v1\nid: messages-main\nspec:\n  enabled: true\n  strategy: raw_messages\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "tools.yaml"), "kind: ToolPolicyConfig\nversion: v1\nid: tools-main\nspec:\n  enabled: false\n  strategy: tools_inline\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "response-format.yaml"), "kind: ResponseFormatPolicyConfig\nversion: v1\nid: response-format-main\nspec:\n  enabled: false\n  strategy: default\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "streaming.yaml"), "kind: StreamingPolicyConfig\nversion: v1\nid: streaming-main\nspec:\n  enabled: true\n  strategy: static_stream\n  params:\n    stream: false\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "sampling.yaml"), "kind: SamplingPolicyConfig\nversion: v1\nid: sampling-main\nspec:\n  enabled: false\n  strategy: static_sampling\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "memory", "offload.yaml"), "kind: OffloadPolicyConfig\nversion: v1\nid: offload-main\nspec:\n  enabled: true\n  strategy: old_only\n  params:\n    max_chars: 1200\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "prompt-assets", "assets.yaml"), "kind: PromptAssetPolicyConfig\nversion: v1\nid: prompt-assets-main\nspec:\n  enabled: true\n  strategy: inline_assets\n  params:\n    assets: []\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "input.yaml"), "kind: ChatInputPolicyConfig\nversion: v1\nid: chat-input\nspec:\n  enabled: true\n  strategy: multiline_buffer\n  params:\n    primary_prompt: \"> \"\n    continuation_prompt: \". \"\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "submit.yaml"), "kind: ChatSubmitPolicyConfig\nversion: v1\nid: chat-submit\nspec:\n  enabled: true\n  strategy: double_enter\n  params:\n    empty_line_threshold: 1\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "output.yaml"), "kind: ChatOutputPolicyConfig\nversion: v1\nid: chat-output\nspec:\n  enabled: true\n  strategy: streaming_text\n  params:\n    show_final_newline: true\n    render_markdown: true\n    markdown_style: dark\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "status.yaml"), "kind: ChatStatusPolicyConfig\nversion: v1\nid: chat-status\nspec:\n  enabled: true\n  strategy: inline_terminal\n  params:\n    show_header: true\n    show_usage: true\n    show_tool_calls: true\n    show_tool_results: true\n    show_plan_after_plan_tools: true\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "command.yaml"), "kind: ChatCommandPolicyConfig\nversion: v1\nid: chat-command\nspec:\n  enabled: true\n  strategy: slash_commands\n  params:\n    exit_command: /exit\n    help_command: /help\n    session_command: /session\n    btw_command: /btw\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "resume.yaml"), "kind: ChatResumePolicyConfig\nversion: v1\nid: chat-resume\nspec:\n  enabled: true\n  strategy: explicit_resume_only\n  params:\n    require_explicit_id: true\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "operator-surface", "daemon-server.yaml"), "kind: DaemonServerPolicyConfig\nversion: v1\nid: daemon-server-main\nspec:\n  enabled: true\n  strategy: websocket_http\n  params:\n    listen_host: 0.0.0.0\n    listen_port: 8080\n    enable_websocket: true\n    public_base_url: \"\"\n    allowed_origins: []\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "operator-surface", "web-assets.yaml"), "kind: WebAssetsPolicyConfig\nversion: v1\nid: web-assets-main\nspec:\n  enabled: true\n  strategy: embedded_assets\n  params:\n    mode: embedded_assets\n    dev_proxy_url: \"\"\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "operator-surface", "client-transport.yaml"), "kind: ClientTransportPolicyConfig\nversion: v1\nid: client-transport-main\nspec:\n  enabled: true\n  strategy: websocket_http\n  params:\n    endpoint_path: /api\n    websocket_path: /ws\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "operator-surface", "settings.yaml"), "kind: SettingsSurfacePolicyConfig\nversion: v1\nid: settings-main\nspec:\n  enabled: true\n  strategy: revisioned_yaml_files\n  params:\n    require_idle_for_apply: true\n    form_fields:\n      - key: max_tool_rounds\n        label: Max Tool Rounds\n        type: int\n        file_path: agent.yaml\n        yaml_path: [spec, runtime, max_tool_rounds]\n      - key: render_markdown\n        label: Render Markdown\n        type: bool\n        file_path: policies/chat/output.yaml\n        yaml_path: [spec, params, render_markdown]\n      - key: markdown_style\n        label: Markdown Style\n        type: string\n        file_path: policies/chat/output.yaml\n        yaml_path: [spec, params, markdown_style]\n        enum: [dark, light]\n      - key: show_tool_calls\n        label: Show Tool Calls\n        type: bool\n        file_path: policies/chat/status.yaml\n        yaml_path: [spec, params, show_tool_calls]\n      - key: show_tool_results\n        label: Show Tool Results\n        type: bool\n        file_path: policies/chat/status.yaml\n        yaml_path: [spec, params, show_tool_results]\n      - key: show_plan_after_plan_tools\n        label: Show Plan After Plan Tools\n        type: bool\n        file_path: policies/chat/status.yaml\n        yaml_path: [spec, params, show_plan_after_plan_tools]\n    raw_file_globs:\n      - agent.yaml\n      - contracts/*.yaml\n      - policies/**/*.yaml\n")

	agent, err := runtime.BuildAgent(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("build chat daemon agent: %v", err)
	}
	return agent
}

func mustWriteFile(t *testing.T, path, body string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdirall(%q): %v", filepath.Dir(path), err)
	}
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("writefile(%q): %v", path, err)
	}
}
