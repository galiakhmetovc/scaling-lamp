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

	providerServer := newBlockingProviderServer(t)
	server, _ := newRunVersionTestServer(t, providerServer.URL)
	sessionID := createRunVersionSession(t, server)

	done := make(chan error, 1)
	go func() {
		_, err := server.executeCommand(context.Background(), CommandRequest{
			Command: "chat.send",
			Payload: map[string]any{
				"session_id":        sessionID,
				"prompt":            "ping",
				"execution_version": string(executionVersionV2),
			},
		})
		done <- err
	}()

	waitForProviderRequest(t, providerServer.started)
	server.migrateSessionExecutionVersion(sessionID, executionVersionV1)

	snapshot, err := server.buildSessionSnapshot(sessionID)
	if err != nil {
		t.Fatalf("build session snapshot: %v", err)
	}
	if snapshot.ExecutionVersion != string(executionVersionV2) {
		t.Fatalf("snapshot execution version = %q, want %q", snapshot.ExecutionVersion, executionVersionV2)
	}
	if snapshot.MainRun.ExecutionVersion != string(executionVersionV2) {
		t.Fatalf("main run execution version = %q, want %q", snapshot.MainRun.ExecutionVersion, executionVersionV2)
	}
	if !snapshot.MainRunActive || !snapshot.MainRun.Active {
		t.Fatalf("main run flags = %+v, want active while provider request is in flight", snapshot.MainRun)
	}

	close(providerServer.release)
	if err := <-done; err != nil {
		t.Fatalf("chat.send returned error: %v", err)
	}

	finished, err := server.buildSessionSnapshot(sessionID)
	if err != nil {
		t.Fatalf("build finished session snapshot: %v", err)
	}
	if finished.ExecutionVersion != string(executionVersionV2) {
		t.Fatalf("finished snapshot execution version = %q, want %q", finished.ExecutionVersion, executionVersionV2)
	}
}

func TestNoMixedRunVersion(t *testing.T) {
	t.Parallel()

	providerServer := newImmediateProviderServer(t)
	server, _ := newRunVersionTestServer(t, providerServer.URL)
	sessionID := createRunVersionSession(t, server)

	first, err := server.executeCommand(context.Background(), CommandRequest{
		Command: "chat.send",
		Payload: map[string]any{
			"session_id": sessionID,
			"prompt":     "first",
		},
	})
	if err != nil {
		t.Fatalf("first chat.send returned error: %v", err)
	}
	firstSession := mapPayloadForRunVersionTest(t, first)["session"].(SessionSnapshot)
	if firstSession.ExecutionVersion != string(executionVersionV1) {
		t.Fatalf("first run execution version = %q, want %q", firstSession.ExecutionVersion, executionVersionV1)
	}

	server.migrateSessionExecutionVersion(sessionID, executionVersionV2)

	second, err := server.executeCommand(context.Background(), CommandRequest{
		Command: "chat.send",
		Payload: map[string]any{
			"session_id": sessionID,
			"prompt":     "second",
		},
	})
	if err != nil {
		t.Fatalf("second chat.send returned error: %v", err)
	}
	secondSession := mapPayloadForRunVersionTest(t, second)["session"].(SessionSnapshot)
	if secondSession.ExecutionVersion != string(executionVersionV2) {
		t.Fatalf("second run execution version = %q, want %q", secondSession.ExecutionVersion, executionVersionV2)
	}
	if secondSession.MainRun.ExecutionVersion != string(executionVersionV2) {
		t.Fatalf("second main run execution version = %q, want %q", secondSession.MainRun.ExecutionVersion, executionVersionV2)
	}
}

type runVersionTestProvider struct {
	URL     string
	started chan struct{}
	release chan struct{}
}

func newBlockingProviderServer(t *testing.T) runVersionTestProvider {
	t.Helper()

	started := make(chan struct{}, 1)
	release := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		select {
		case started <- struct{}{}:
		default:
		}
		<-release
		w.Header().Set("Content-Type", "application/json")
		_, _ = fmt.Fprint(w, `{"id":"resp-1","model":"glm-5-turbo","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"pong"}}],"usage":{"prompt_tokens":8,"completion_tokens":4,"total_tokens":12}}`)
	}))
	t.Cleanup(server.Close)
	return runVersionTestProvider{URL: server.URL, started: started, release: release}
}

func newImmediateProviderServer(t *testing.T) runVersionTestProvider {
	t.Helper()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = fmt.Fprint(w, `{"id":"resp-1","model":"glm-5-turbo","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"pong"}}],"usage":{"prompt_tokens":8,"completion_tokens":4,"total_tokens":12}}`)
	}))
	t.Cleanup(server.Close)
	return runVersionTestProvider{URL: server.URL, started: make(chan struct{}, 1), release: make(chan struct{})}
}

func waitForProviderRequest(t *testing.T, started <-chan struct{}) {
	t.Helper()

	select {
	case <-started:
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for provider request to start")
	}
}

func newRunVersionTestServer(t *testing.T, providerURL string) (*Server, *runtime.Agent) {
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

	server := &Server{
		agent:          agent,
		sessionRuntime: map[string]*sessionRuntimeState{},
		daemonBus:      newDaemonBus(),
	}
	return server, agent
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
