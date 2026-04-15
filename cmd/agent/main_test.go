package main

import (
	"bytes"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"teamd/internal/runtime"
	"teamd/internal/runtime/eventing"
)

func TestRunExecutesSmokeAndPrintsAssistantMessage(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/chat/completions" {
			t.Fatalf("path = %q, want /chat/completions", r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer secret-token" {
			t.Fatalf("authorization = %q, want Bearer secret-token", got)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{
  "id":"resp-1",
  "model":"glm-5-turbo",
  "choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"pong"}}],
  "usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}
}`))
	}))
	defer server.Close()

	dir := t.TempDir()
	configPath := writeSmokeConfig(t, dir, server.URL)

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	err := run([]string{"--config", configPath, "--smoke", "ping"}, &stdout, &stderr)
	if err != nil {
		t.Fatalf("run returned error: %v", err)
	}

	if got := stdout.String(); got != "pong\n" {
		t.Fatalf("stdout = %q, want %q", got, "pong\n")
	}
}

func TestRunAutoloadsDotEnvWithoutOverridingExistingEnv(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if got := r.Header.Get("Authorization"); got != "Bearer shell-token" {
			t.Fatalf("authorization = %q, want Bearer shell-token", got)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{
  "id":"resp-1",
  "model":"glm-5-turbo",
  "choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"pong"}}],
  "usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}
}`))
	}))
	defer server.Close()

	dir := t.TempDir()
	configPath := writeSmokeConfig(t, dir, server.URL)
	mustWriteFile(t, filepath.Join(dir, ".env"), "TEAMD_ZAI_API_KEY=dotenv-token\n")

	oldWD, err := os.Getwd()
	if err != nil {
		t.Fatalf("Getwd returned error: %v", err)
	}
	if err := os.Chdir(dir); err != nil {
		t.Fatalf("Chdir returned error: %v", err)
	}
	defer func() {
		_ = os.Chdir(oldWD)
	}()

	t.Setenv("TEAMD_ZAI_API_KEY", "shell-token")

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	if err := run([]string{"--config", configPath, "--smoke", "ping"}, &stdout, &stderr); err != nil {
		t.Fatalf("run returned error: %v", err)
	}
}

func TestRunAutoloadsDotEnvWhenProcessEnvMissing(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if got := r.Header.Get("Authorization"); got != "Bearer dotenv-token" {
			t.Fatalf("authorization = %q, want Bearer dotenv-token", got)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{
  "id":"resp-1",
  "model":"glm-5-turbo",
  "choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"pong"}}],
  "usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}
}`))
	}))
	defer server.Close()

	dir := t.TempDir()
	configPath := writeSmokeConfig(t, dir, server.URL)
	mustWriteFile(t, filepath.Join(dir, ".env"), strings.Join([]string{
		"# comment",
		"TEAMD_ZAI_API_KEY=dotenv-token",
		"",
	}, "\n"))

	oldWD, err := os.Getwd()
	if err != nil {
		t.Fatalf("Getwd returned error: %v", err)
	}
	if err := os.Chdir(dir); err != nil {
		t.Fatalf("Chdir returned error: %v", err)
	}
	defer func() {
		_ = os.Chdir(oldWD)
	}()

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	if err := run([]string{"--config", configPath, "--smoke", "ping"}, &stdout, &stderr); err != nil {
		t.Fatalf("run returned error: %v", err)
	}
}

func TestRunChatStreamsReplyAndExitsOnSlashExit(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		_, _ = w.Write([]byte(strings.Join([]string{
			`data: {"id":"resp-1","model":"glm-5-turbo","choices":[{"delta":{"role":"assistant","content":"Po"},"finish_reason":""}]}`,
			"",
			`data: {"id":"resp-1","model":"glm-5-turbo","choices":[{"delta":{"content":"ng"},"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}}`,
			"",
			"data: [DONE]",
			"",
		}, "\n")))
	}))
	defer server.Close()

	dir := t.TempDir()
	configPath := writeChatConfig(t, dir, server.URL)

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	stdin := strings.NewReader("ping\n\n/exit\n")
	err := runWithIO([]string{"--config", configPath, "--chat"}, stdin, &stdout, &stderr)
	if err != nil {
		t.Fatalf("runWithIO returned error: %v", err)
	}

	got := stdout.String()
	if !strings.Contains(got, "session:") {
		t.Fatalf("stdout = %q, want session header", got)
	}
	if !strings.Contains(got, "Pong") {
		t.Fatalf("stdout = %q, want streamed Pong", got)
	}
}

func TestRunChatResumeUsesExistingSession(t *testing.T) {
	t.Setenv("TEAMD_ZAI_API_KEY", "secret-token")

	call := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		call++
		w.Header().Set("Content-Type", "text/event-stream")
		body := strings.Join([]string{
			`data: {"id":"resp-1","model":"glm-5-turbo","choices":[{"delta":{"role":"assistant","content":"Po"},"finish_reason":""}]}`,
			"",
			`data: {"id":"resp-1","model":"glm-5-turbo","choices":[{"delta":{"content":"ng"},"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":3,"total_tokens":15}}`,
			"",
			"data: [DONE]",
			"",
		}, "\n")
		if call == 2 {
			body = strings.Join([]string{
				`data: {"id":"resp-2","model":"glm-5-turbo","choices":[{"delta":{"role":"assistant","content":"Pa"},"finish_reason":""}]}`,
				"",
				`data: {"id":"resp-2","model":"glm-5-turbo","choices":[{"delta":{"content":"th"},"finish_reason":"stop"}],"usage":{"prompt_tokens":18,"completion_tokens":4,"total_tokens":22}}`,
				"",
				"data: [DONE]",
				"",
			}, "\n")
		}
		_, _ = w.Write([]byte(body))
	}))
	defer server.Close()

	dir := t.TempDir()
	configPath := writeChatConfig(t, dir, server.URL)

	var firstOut bytes.Buffer
	if err := runWithIO([]string{"--config", configPath, "--chat"}, strings.NewReader("ping\n\n/exit\n"), &firstOut, io.Discard); err != nil {
		t.Fatalf("first chat run returned error: %v", err)
	}
	sessionID := extractSessionID(firstOut.String())
	if sessionID == "" {
		t.Fatalf("failed to extract session id from %q", firstOut.String())
	}

	var secondOut bytes.Buffer
	if err := runWithIO([]string{"--config", configPath, "--chat", "--resume", sessionID}, strings.NewReader("again\n\n/exit\n"), &secondOut, io.Discard); err != nil {
		t.Fatalf("resume chat run returned error: %v", err)
	}
	if !strings.Contains(secondOut.String(), "Path") {
		t.Fatalf("stdout = %q, want resumed reply Path", secondOut.String())
	}
}

func TestRunInspectSessionPrintsFailureSummaryAndCorrelatedEvents(t *testing.T) {
	dir := t.TempDir()
	configPath := writeSmokeConfig(t, dir, "http://127.0.0.1:1")

	agent, err := runtime.BuildAgent(configPath)
	if err != nil {
		t.Fatalf("BuildAgent returned error: %v", err)
	}
	events := []eventing.Event{
		{
			ID:            "evt-1",
			Sequence:      1,
			Kind:          eventing.EventSessionCreated,
			OccurredAt:    mustTime(t, "2026-04-15T10:50:00Z"),
			AggregateID:   "session-1",
			AggregateType: eventing.AggregateSession,
			Payload:       map[string]any{"session_id": "session-1"},
		},
		{
			ID:            "evt-2",
			Sequence:      2,
			Kind:          eventing.EventRunStarted,
			OccurredAt:    mustTime(t, "2026-04-15T10:50:01Z"),
			AggregateID:   "run-1",
			AggregateType: eventing.AggregateRun,
			Payload:       map[string]any{"session_id": "session-1", "prompt": "ping"},
		},
		{
			ID:            "evt-3",
			Sequence:      3,
			Kind:          eventing.EventToolCallCompleted,
			OccurredAt:    mustTime(t, "2026-04-15T10:50:02Z"),
			AggregateID:   "run-1",
			AggregateType: eventing.AggregateRun,
			Payload:       map[string]any{"session_id": "session-1", "tool_name": "shell_exec", "error": "command denied"},
		},
		{
			ID:            "evt-4",
			Sequence:      4,
			Kind:          eventing.EventRunFailed,
			OccurredAt:    mustTime(t, "2026-04-15T10:50:03Z"),
			AggregateID:   "run-1",
			AggregateType: eventing.AggregateRun,
			Payload:       map[string]any{"session_id": "session-1", "error": "provider tool loop exceeded 1 rounds"},
		},
	}
	for _, event := range events {
		if err := agent.RecordEvent(t.Context(), event); err != nil {
			t.Fatalf("RecordEvent %s returned error: %v", event.Kind, err)
		}
	}

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	if err := run([]string{"--config", configPath, "--inspect-session", "session-1"}, &stdout, &stderr); err != nil {
		t.Fatalf("run returned error: %v", err)
	}

	got := stdout.String()
	for _, want := range []string{
		"Inspection: session session-1",
		"Failure Summary",
		"provider tool loop exceeded 1 rounds",
		"shell_exec",
		"run.failed",
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("stdout missing %q: %q", want, got)
		}
	}
}

func TestRunInspectRunSupportsKindFilterAndLimit(t *testing.T) {
	dir := t.TempDir()
	configPath := writeSmokeConfig(t, dir, "http://127.0.0.1:1")

	agent, err := runtime.BuildAgent(configPath)
	if err != nil {
		t.Fatalf("BuildAgent returned error: %v", err)
	}
	events := []eventing.Event{
		{
			ID:            "evt-1",
			Sequence:      1,
			Kind:          eventing.EventRunStarted,
			OccurredAt:    mustTime(t, "2026-04-15T10:51:00Z"),
			AggregateID:   "run-2",
			AggregateType: eventing.AggregateRun,
			Payload:       map[string]any{"session_id": "session-2"},
		},
		{
			ID:            "evt-2",
			Sequence:      2,
			Kind:          eventing.EventToolCallCompleted,
			OccurredAt:    mustTime(t, "2026-04-15T10:51:01Z"),
			AggregateID:   "run-2",
			AggregateType: eventing.AggregateRun,
			Payload:       map[string]any{"session_id": "session-2", "tool_name": "fs_list", "result_text": "ok"},
		},
		{
			ID:            "evt-3",
			Sequence:      3,
			Kind:          eventing.EventToolCallCompleted,
			OccurredAt:    mustTime(t, "2026-04-15T10:51:02Z"),
			AggregateID:   "run-2",
			AggregateType: eventing.AggregateRun,
			Payload:       map[string]any{"session_id": "session-2", "tool_name": "shell_exec", "result_text": "done"},
		},
	}
	for _, event := range events {
		if err := agent.RecordEvent(t.Context(), event); err != nil {
			t.Fatalf("RecordEvent %s returned error: %v", event.Kind, err)
		}
	}

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	if err := run([]string{"--config", configPath, "--inspect-run", "run-2", "--inspect-kind", "tool.call.completed", "--inspect-limit", "1"}, &stdout, &stderr); err != nil {
		t.Fatalf("run returned error: %v", err)
	}

	got := stdout.String()
	if strings.Contains(got, "run.started") {
		t.Fatalf("stdout unexpectedly contains filtered-out event: %q", got)
	}
	if strings.Contains(got, "fs_list") {
		t.Fatalf("stdout unexpectedly contains older limited-out event: %q", got)
	}
	if !strings.Contains(got, "shell_exec") || !strings.Contains(got, "tool.call.completed") {
		t.Fatalf("stdout missing filtered event: %q", got)
	}
}

func TestRunInspectSessionPrintsDiagnosticsAndRecoveryHints(t *testing.T) {
	dir := t.TempDir()
	configPath := writeSmokeConfig(t, dir, "http://127.0.0.1:1")

	agent, err := runtime.BuildAgent(configPath)
	if err != nil {
		t.Fatalf("BuildAgent returned error: %v", err)
	}
	events := []eventing.Event{
		{
			ID:            "evt-1",
			Sequence:      1,
			Kind:          eventing.EventSessionCreated,
			OccurredAt:    mustTime(t, "2026-04-15T11:00:00Z"),
			AggregateID:   "session-3",
			AggregateType: eventing.AggregateSession,
			Payload:       map[string]any{"session_id": "session-3"},
		},
		{
			ID:            "evt-2",
			Sequence:      2,
			Kind:          eventing.EventRunStarted,
			OccurredAt:    mustTime(t, "2026-04-15T11:00:01Z"),
			AggregateID:   "run-3",
			AggregateType: eventing.AggregateRun,
			Payload:       map[string]any{"session_id": "session-3"},
		},
		{
			ID:            "evt-3",
			Sequence:      3,
			Kind:          eventing.EventShellCommandStarted,
			OccurredAt:    mustTime(t, "2026-04-15T11:00:02Z"),
			AggregateID:   "cmd-3",
			AggregateType: eventing.AggregateShellCommand,
			Payload: map[string]any{
				"session_id": "session-3",
				"run_id":     "run-3",
				"command_id": "cmd-3",
				"command":    "sleep",
				"status":     "running",
			},
		},
	}
	for _, event := range events {
		if err := agent.RecordEvent(t.Context(), event); err != nil {
			t.Fatalf("RecordEvent %s returned error: %v", event.Kind, err)
		}
	}

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	if err := run([]string{"--config", configPath, "--inspect-session", "session-3"}, &stdout, &stderr); err != nil {
		t.Fatalf("run returned error: %v", err)
	}

	got := stdout.String()
	for _, want := range []string{
		"Diagnostics",
		"stuck run: run-3",
		"shell command: cmd-3 status=running",
		"Recovery Hints",
		"press k to kill",
	} {
		if !strings.Contains(got, want) {
			t.Fatalf("stdout missing %q: %q", want, got)
		}
	}
}

func writeSmokeConfig(t *testing.T, dir, baseURL string) string {
	t.Helper()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: zai-smoke\n"+
		"spec:\n"+
		"  runtime:\n"+
		"    event_log: file_jsonl\n"+
		"    event_log_path: ./var/events.jsonl\n"+
		"    projection_store_path: ./var/projections.json\n"+
		"    prompt_asset_executor: prompt_asset_default\n"+
		"    transport_executor: transport_default\n"+
		"    request_shape_executor: request_shape_default\n"+
		"    provider_client: provider_client_default\n"+
		"    projections: [session, run, transcript]\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n"+
		"    request_shape: ./contracts/request-shape.yaml\n"+
		"    memory: ./contracts/memory.yaml\n"+
		"    prompt_assets: ./contracts/prompt-assets.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), ""+
		"kind: TransportContractConfig\n"+
		"version: v1\n"+
		"id: transport-zai-smoke\n"+
		"spec:\n"+
		"  endpoint_policy_path: ../policies/transport/endpoint.yaml\n"+
		"  auth_policy_path: ../policies/transport/auth.yaml\n"+
		"  retry_policy_path: ../policies/transport/retry.yaml\n"+
		"  timeout_policy_path: ../policies/transport/timeout.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "request-shape.yaml"), ""+
		"kind: RequestShapeContractConfig\n"+
		"version: v1\n"+
		"id: request-shape-zai-smoke\n"+
		"spec:\n"+
		"  model_policy_path: ../policies/request-shape/model.yaml\n"+
		"  message_policy_path: ../policies/request-shape/messages.yaml\n"+
		"  tool_policy_path: ../policies/request-shape/tools.yaml\n"+
		"  response_format_policy_path: ../policies/request-shape/response-format.yaml\n"+
		"  streaming_policy_path: ../policies/request-shape/streaming.yaml\n"+
		"  sampling_policy_path: ../policies/request-shape/sampling.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "memory.yaml"), ""+
		"kind: MemoryContractConfig\n"+
		"version: v1\n"+
		"id: memory-zai-smoke\n"+
		"spec:\n"+
		"  offload_policy_path: ../policies/memory/offload.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "prompt-assets.yaml"), ""+
		"kind: PromptAssetsContractConfig\n"+
		"version: v1\n"+
		"id: prompt-assets-zai-smoke\n"+
		"spec:\n"+
		"  prompt_asset_policy_path: ../policies/prompt-assets/assets.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), fmt.Sprintf(""+
		"kind: EndpointPolicyConfig\n"+
		"version: v1\n"+
		"id: endpoint-zai-smoke\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static\n"+
		"  params:\n"+
		"    base_url: %s\n"+
		"    path: /chat/completions\n"+
		"    method: POST\n", baseURL))
	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "auth.yaml"), ""+
		"kind: AuthPolicyConfig\n"+
		"version: v1\n"+
		"id: auth-zai-smoke\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: bearer_token\n"+
		"  params:\n"+
		"    header: Authorization\n"+
		"    prefix: Bearer\n"+
		"    value_env_var: TEAMD_ZAI_API_KEY\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "retry.yaml"), ""+
		"kind: RetryPolicyConfig\n"+
		"version: v1\n"+
		"id: retry-zai-smoke\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: exponential_jitter\n"+
		"  params:\n"+
		"    max_attempts: 3\n"+
		"    base_delay: 100ms\n"+
		"    max_delay: 1s\n"+
		"    retry_on_statuses: [429, 500, 502, 503]\n"+
		"    retry_on_errors: [transport_error]\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "timeout.yaml"), ""+
		"kind: TimeoutPolicyConfig\n"+
		"version: v1\n"+
		"id: timeout-zai-smoke\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: per_request\n"+
		"  params:\n"+
		"    total: 30s\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "model.yaml"), ""+
		"kind: ModelPolicyConfig\n"+
		"version: v1\n"+
		"id: model-zai-smoke\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_model\n"+
		"  params:\n"+
		"    model: glm-5-turbo\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "messages.yaml"), ""+
		"kind: MessagePolicyConfig\n"+
		"version: v1\n"+
		"id: messages-zai-smoke\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: raw_messages\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "tools.yaml"), ""+
		"kind: ToolPolicyConfig\n"+
		"version: v1\n"+
		"id: tools-zai-smoke\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: tools_inline\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "response-format.yaml"), ""+
		"kind: ResponseFormatPolicyConfig\n"+
		"version: v1\n"+
		"id: response-format-zai-smoke\n"+
		"spec:\n"+
		"  enabled: false\n"+
		"  strategy: default\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "streaming.yaml"), ""+
		"kind: StreamingPolicyConfig\n"+
		"version: v1\n"+
		"id: streaming-zai-smoke\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_stream\n"+
		"  params:\n"+
		"    stream: false\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "sampling.yaml"), ""+
		"kind: SamplingPolicyConfig\n"+
		"version: v1\n"+
		"id: sampling-zai-smoke\n"+
		"spec:\n"+
		"  enabled: false\n"+
		"  strategy: static_sampling\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "memory", "offload.yaml"), ""+
		"kind: OffloadPolicyConfig\n"+
		"version: v1\n"+
		"id: offload-zai-smoke\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: old_only\n"+
		"  params:\n"+
		"    max_chars: 1200\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "prompt-assets", "assets.yaml"), ""+
		"kind: PromptAssetPolicyConfig\n"+
		"version: v1\n"+
		"id: prompt-assets-zai-smoke\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: inline_assets\n"+
		"  params:\n"+
		"    assets: []\n")

	return filepath.Join(dir, "agent.yaml")
}

func mustTime(t *testing.T, value string) time.Time {
	t.Helper()
	out, err := time.Parse(time.RFC3339, value)
	if err != nil {
		t.Fatalf("time.Parse(%q) returned error: %v", value, err)
	}
	return out
}

func writeChatConfig(t *testing.T, dir, baseURL string) string {
	t.Helper()
	configPath := writeSmokeConfig(t, dir, baseURL)

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: zai-chat\n"+
		"spec:\n"+
		"  runtime:\n"+
		"    event_log: file_jsonl\n"+
		"    event_log_path: ./var/events.jsonl\n"+
		"    projection_store_path: ./var/projections.json\n"+
		"    prompt_asset_executor: prompt_asset_default\n"+
		"    transport_executor: transport_default\n"+
		"    request_shape_executor: request_shape_default\n"+
		"    provider_client: provider_client_default\n"+
		"    projections: [session, run, transcript]\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n"+
		"    request_shape: ./contracts/request-shape.yaml\n"+
		"    memory: ./contracts/memory.yaml\n"+
		"    prompt_assets: ./contracts/prompt-assets.yaml\n"+
		"    chat: ./contracts/chat.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "chat.yaml"), ""+
		"kind: ChatContractConfig\nversion: v1\nid: chat-main\nspec:\n"+
		"  input_policy_path: ../policies/chat/input.yaml\n"+
		"  submit_policy_path: ../policies/chat/submit.yaml\n"+
		"  output_policy_path: ../policies/chat/output.yaml\n"+
		"  status_policy_path: ../policies/chat/status.yaml\n"+
		"  command_policy_path: ../policies/chat/command.yaml\n"+
		"  resume_policy_path: ../policies/chat/resume.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "streaming.yaml"), ""+
		"kind: StreamingPolicyConfig\nversion: v1\nid: streaming-zai-chat\nspec:\n  enabled: true\n  strategy: static_stream\n  params:\n    stream: true\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "input.yaml"), "kind: ChatInputPolicyConfig\nversion: v1\nid: chat-input\nspec:\n  enabled: true\n  strategy: multiline_buffer\n  params:\n    primary_prompt: \"> \"\n    continuation_prompt: \". \"\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "submit.yaml"), "kind: ChatSubmitPolicyConfig\nversion: v1\nid: chat-submit\nspec:\n  enabled: true\n  strategy: double_enter\n  params:\n    empty_line_threshold: 1\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "output.yaml"), "kind: ChatOutputPolicyConfig\nversion: v1\nid: chat-output\nspec:\n  enabled: true\n  strategy: streaming_text\n  params:\n    show_final_newline: true\n    render_markdown: true\n    markdown_style: dark\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "status.yaml"), "kind: ChatStatusPolicyConfig\nversion: v1\nid: chat-status\nspec:\n  enabled: true\n  strategy: inline_terminal\n  params:\n    show_header: true\n    show_usage: true\n    show_tool_calls: true\n    show_tool_results: true\n    show_plan_after_plan_tools: true\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "command.yaml"), "kind: ChatCommandPolicyConfig\nversion: v1\nid: chat-command\nspec:\n  enabled: true\n  strategy: slash_commands\n  params:\n    exit_command: /exit\n    help_command: /help\n    session_command: /session\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "resume.yaml"), "kind: ChatResumePolicyConfig\nversion: v1\nid: chat-resume\nspec:\n  enabled: true\n  strategy: explicit_resume_only\n  params:\n    require_explicit_id: true\n")

	return configPath
}

func extractSessionID(output string) string {
	for _, line := range strings.Split(output, "\n") {
		if strings.HasPrefix(line, "session: ") {
			return strings.TrimSpace(strings.TrimPrefix(line, "session: "))
		}
	}
	return ""
}

func mustWriteFile(t *testing.T, path, body string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll(%q) returned error: %v", filepath.Dir(path), err)
	}
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("WriteFile(%q) returned error: %v", path, err)
	}
}
