package main

import (
	"bytes"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
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
		"    projections: [session, run]\n"+
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

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), fmt.Sprintf("" +
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

func mustWriteFile(t *testing.T, path, body string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("MkdirAll(%q) returned error: %v", filepath.Dir(path), err)
	}
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("WriteFile(%q) returned error: %v", path, err)
	}
}
