package daemon

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"teamd/internal/config"
	"teamd/internal/contracts"
	"teamd/internal/delegation"
	"teamd/internal/filesystem"
	"teamd/internal/provider"
	"teamd/internal/runtime"
	"teamd/internal/runtime/projections"
	"teamd/internal/shell"
	"teamd/internal/tools"
)

func TestRunVersionCutover(t *testing.T) {
	t.Parallel()

	providerServer := newImmediateProviderServer(t)
	server := newRunVersionTestServer(t, providerServer.URL)
	sessionID := createRunVersionSession(t, server)

	payload, err := server.executeCommand(context.Background(), CommandRequest{
		Command: "chat.send",
		Payload: map[string]any{
			"session_id":        sessionID,
			"prompt":            "ping",
			"execution_version": string(executionVersionV2),
		},
	})
	if err != nil {
		t.Fatalf("chat.send returned error: %v", err)
	}
	snapshot := mapPayloadForRunVersionTest(t, payload)["session"].(SessionSnapshot)
	if snapshot.ExecutionVersion != string(executionVersionV2) {
		t.Fatalf("snapshot execution version = %q, want %q", snapshot.ExecutionVersion, executionVersionV2)
	}
	if snapshot.MainRun.ExecutionVersion != string(executionVersionV2) {
		t.Fatalf("main run execution version = %q, want %q", snapshot.MainRun.ExecutionVersion, executionVersionV2)
	}
	if snapshot.MainRunActive || snapshot.MainRun.Active {
		t.Fatalf("main run flags = %+v, want inactive after completion", snapshot.MainRun)
	}
}

func TestNoMixedRunVersion(t *testing.T) {
	t.Parallel()

	providerServer := newQueuedProviderServer(t)
	server := newRunVersionTestServer(t, providerServer.URL)
	sessionID := createRunVersionSession(t, server)

	firstDone := make(chan error, 1)
	go func() {
		_, err := server.executeCommand(context.Background(), CommandRequest{
			Command: "chat.send",
			Payload: map[string]any{
				"session_id": sessionID,
				"prompt":     "first",
			},
		})
		firstDone <- err
	}()

	waitForRequest(t, providerServer.firstStarted)

	secondPayload, err := server.executeCommand(context.Background(), CommandRequest{
		Command: "chat.send",
		Payload: map[string]any{
			"session_id":        sessionID,
			"prompt":            "second",
			"execution_version": string(executionVersionV2),
		},
	})
	if err != nil {
		t.Fatalf("queued chat.send returned error: %v", err)
	}
	secondSnapshot := mapPayloadForRunVersionTest(t, secondPayload)["session"].(SessionSnapshot)
	if secondSnapshot.ExecutionVersion != string(executionVersionV1) {
		t.Fatalf("queued snapshot execution version = %q, want %q", secondSnapshot.ExecutionVersion, executionVersionV1)
	}
	if queued, _ := mapPayloadForRunVersionTest(t, secondPayload)["queued"].(bool); !queued {
		t.Fatalf("queued payload = %#v, want queued=true", secondPayload)
	}

	close(providerServer.firstRelease)
	if err := <-firstDone; err != nil {
		t.Fatalf("first chat.send returned error: %v", err)
	}

	waitForRequest(t, providerServer.secondStarted)
	activeSnapshot, err := server.buildSessionSnapshot(sessionID)
	if err != nil {
		t.Fatalf("build active snapshot: %v", err)
	}
	if activeSnapshot.ExecutionVersion != string(executionVersionV1) {
		t.Fatalf("active snapshot execution version = %q, want %q", activeSnapshot.ExecutionVersion, executionVersionV1)
	}
	if activeSnapshot.MainRun.ExecutionVersion != string(executionVersionV1) {
		t.Fatalf("active main run execution version = %q, want %q", activeSnapshot.MainRun.ExecutionVersion, executionVersionV1)
	}

	close(providerServer.secondRelease)
	finishedSnapshot, err := server.buildSessionSnapshot(sessionID)
	if err != nil {
		t.Fatalf("build finished snapshot: %v", err)
	}
	if finishedSnapshot.ExecutionVersion != string(executionVersionV1) {
		t.Fatalf("finished snapshot execution version = %q, want %q", finishedSnapshot.ExecutionVersion, executionVersionV1)
	}
}

type runVersionTestProvider struct {
	URL           string
	firstStarted  chan struct{}
	firstRelease  chan struct{}
	secondStarted chan struct{}
	secondRelease chan struct{}
}

func newImmediateProviderServer(t *testing.T) runVersionTestProvider {
	t.Helper()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = fmt.Fprint(w, `{"id":"resp-1","model":"glm-5-turbo","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"pong"}}],"usage":{"prompt_tokens":8,"completion_tokens":4,"total_tokens":12}}`)
	}))
	t.Cleanup(server.Close)
	return runVersionTestProvider{URL: server.URL}
}

func newQueuedProviderServer(t *testing.T) runVersionTestProvider {
	t.Helper()

	state := &runVersionTestProvider{
		firstStarted:  make(chan struct{}, 1),
		firstRelease:  make(chan struct{}),
		secondStarted: make(chan struct{}, 1),
		secondRelease: make(chan struct{}),
	}
	var requestCount int
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		requestCount++
		switch requestCount {
		case 1:
			select {
			case state.firstStarted <- struct{}{}:
			default:
			}
			<-state.firstRelease
		case 2:
			select {
			case state.secondStarted <- struct{}{}:
			default:
			}
			<-state.secondRelease
		default:
			t.Fatalf("unexpected provider request count %d", requestCount)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = fmt.Fprint(w, `{"id":"resp-1","model":"glm-5-turbo","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"pong"}}],"usage":{"prompt_tokens":8,"completion_tokens":4,"total_tokens":12}}`)
	}))
	t.Cleanup(server.Close)
	state.URL = server.URL
	return *state
}

func waitForRequest(t *testing.T, started <-chan struct{}) {
	t.Helper()

	select {
	case <-started:
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for provider request to start")
	}
}

func newRunVersionTestServer(t *testing.T, providerURL string) *Server {
	t.Helper()

	now := time.Date(2026, 4, 18, 20, 0, 0, 0, time.UTC)
	agent := &runtime.Agent{
		Config: config.AgentConfig{ID: "daemon-chat"},
		Contracts: contracts.ResolvedContracts{
			ProviderRequest: contracts.ProviderRequestContract{
				Transport: contracts.TransportContract{
					ID: "provider_client",
					Endpoint: contracts.EndpointPolicy{
						Enabled:  true,
						Strategy: "static",
						Params: contracts.EndpointParams{
							BaseURL: providerURL,
							Path:    "/chat/completions",
							Method:  "POST",
						},
					},
				},
				RequestShape: contracts.RequestShapeContract{
					Model: contracts.ModelPolicy{
						Enabled:  true,
						Strategy: "static_model",
						Params: contracts.ModelParams{
							Model: "glm-5-turbo",
						},
					},
				},
			},
			PromptAssembly: contracts.PromptAssemblyContract{},
			Memory:         contracts.MemoryContract{},
			Tools: contracts.ToolContract{
				Catalog:       contracts.ToolCatalogPolicy{},
				Serialization: contracts.ToolSerializationPolicy{},
			},
			PlanTools:       contracts.PlanToolContract{},
			FilesystemTools: contracts.FilesystemToolContract{},
			ShellTools:      contracts.ShellToolContract{},
			DelegationTools: contracts.DelegationToolContract{},
			ToolExecution:   contracts.ToolExecutionContract{},
		},
		EventLog: runtime.NewInMemoryEventLog(),
		Projections: []projections.Projection{
			projections.NewSessionProjection(),
			projections.NewSessionCatalogProjection(),
			projections.NewTranscriptProjection(),
			projections.NewChatTimelineProjection(),
			projections.NewContextBudgetProjection(),
			projections.NewPlanHeadProjection(),
			projections.NewShellCommandProjection(),
			projections.NewDelegateProjection(),
		},
		Now:   func() time.Time { return now },
		NewID: func(prefix string) string { return prefix + "-1" },
	}
	agent.PromptAssets = provider.NewPromptAssetExecutor()
	agent.RequestShape = provider.NewRequestShapeExecutor()
	agent.PlanTools = tools.NewPlanToolExecutor()
	agent.FilesystemTools = filesystem.NewDefinitionExecutor()
	agent.ShellTools = shell.NewDefinitionExecutor()
	agent.DelegationTools = delegation.NewDefinitionExecutor()
	agent.ToolCatalog = tools.NewCatalogExecutor()
	agent.ToolExecution = tools.NewExecutionGate()
	agent.ProviderClient = provider.NewClient(
		agent.PromptAssets,
		agent.RequestShape,
		agent.PlanTools,
		agent.FilesystemTools,
		agent.ShellTools,
		agent.DelegationTools,
		agent.ToolCatalog,
		agent.ToolExecution,
		provider.NewTransportExecutor(http.DefaultClient),
	)

	return &Server{
		agent:          agent,
		sessionRuntime: map[string]*sessionRuntimeState{},
		daemonBus:      newDaemonBus(),
	}
}

func createRunVersionSession(t *testing.T, server *Server) string {
	t.Helper()

	payload, err := server.executeCommand(context.Background(), CommandRequest{Command: "session.create"})
	if err != nil {
		t.Fatalf("session.create returned error: %v", err)
	}
	session := mapPayloadForRunVersionTest(t, payload)["session"].(SessionSnapshot)
	if session.SessionID == "" {
		t.Fatal("created session id is empty")
	}
	return session.SessionID
}

func mapPayloadForRunVersionTest(t *testing.T, payload any) map[string]any {
	t.Helper()

	m, ok := payload.(map[string]any)
	if !ok {
		t.Fatalf("payload type = %T, want map[string]any", payload)
	}
	return m
}
