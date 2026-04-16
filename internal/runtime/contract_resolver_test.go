package runtime_test

import (
	"path/filepath"
	"strings"
	"testing"

	"teamd/internal/config"
	"teamd/internal/policies"
	"teamd/internal/runtime"
)

func TestResolveContractsBuildsTransportAndMemoryContracts(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n"+
		"    request_shape: ./contracts/request-shape.yaml\n"+
		"    memory: ./contracts/memory.yaml\n"+
		"    prompt_assets: ./contracts/prompt-assets.yaml\n"+
		"    prompt_assembly: ./contracts/prompt-assembly.yaml\n"+
		"    tools: ./contracts/tools.yaml\n"+
		"    tool_execution: ./contracts/tool-execution.yaml\n"+
		"    filesystem_tools: ./contracts/filesystem-tools.yaml\n"+
		"    filesystem_execution: ./contracts/filesystem-execution.yaml\n"+
		"    shell_tools: ./contracts/shell-tools.yaml\n"+
		"    shell_execution: ./contracts/shell-execution.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), ""+
		"kind: TransportContractConfig\n"+
		"version: v1\n"+
		"id: transport-main\n"+
		"spec:\n"+
		"  endpoint_policy_path: ../policies/transport/endpoint.yaml\n"+
		"  auth_policy_path: ../policies/transport/auth.yaml\n"+
		"  retry_policy_path: ../policies/transport/retry.yaml\n"+
		"  timeout_policy_path: ../policies/transport/timeout.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "memory.yaml"), ""+
		"kind: MemoryContractConfig\n"+
		"version: v1\n"+
		"id: memory-main\n"+
		"spec:\n"+
		"  offload_policy_path: ../policies/memory/offload.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "request-shape.yaml"), ""+
		"kind: RequestShapeContractConfig\n"+
		"version: v1\n"+
		"id: request-shape-main\n"+
		"spec:\n"+
		"  model_policy_path: ../policies/request-shape/model.yaml\n"+
		"  message_policy_path: ../policies/request-shape/messages.yaml\n"+
		"  tool_policy_path: ../policies/request-shape/tools.yaml\n"+
		"  response_format_policy_path: ../policies/request-shape/response-format.yaml\n"+
		"  streaming_policy_path: ../policies/request-shape/streaming.yaml\n"+
		"  sampling_policy_path: ../policies/request-shape/sampling.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "prompt-assets.yaml"), ""+
		"kind: PromptAssetsContractConfig\n"+
		"version: v1\n"+
		"id: prompt-assets-main\n"+
		"spec:\n"+
		"  prompt_asset_policy_path: ../policies/prompt-assets/inline.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "prompt-assembly.yaml"), ""+
		"kind: PromptAssemblyContractConfig\n"+
		"version: v1\n"+
		"id: prompt-assembly-main\n"+
		"spec:\n"+
		"  system_prompt_policy_path: ../policies/prompt-assembly/system-prompt.yaml\n"+
		"  session_head_policy_path: ../policies/prompt-assembly/session-head.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "tools.yaml"), ""+
		"kind: ToolContractConfig\n"+
		"version: v1\n"+
		"id: tools-main\n"+
		"spec:\n"+
		"  tool_catalog_policy_path: ../policies/tools/catalog.yaml\n"+
		"  tool_serialization_policy_path: ../policies/tools/serialization.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "tool-execution.yaml"), ""+
		"kind: ToolExecutionContractConfig\n"+
		"version: v1\n"+
		"id: tool-execution-main\n"+
		"spec:\n"+
		"  tool_access_policy_path: ../policies/tool-execution/access.yaml\n"+
		"  tool_approval_policy_path: ../policies/tool-execution/approval.yaml\n"+
		"  tool_sandbox_policy_path: ../policies/tool-execution/sandbox.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "filesystem-tools.yaml"), ""+
		"kind: FilesystemToolContractConfig\n"+
		"version: v1\n"+
		"id: filesystem-tools-main\n"+
		"spec:\n"+
		"  filesystem_catalog_policy_path: ../policies/filesystem-tools/catalog.yaml\n"+
		"  filesystem_description_policy_path: ../policies/filesystem-tools/description.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "filesystem-execution.yaml"), ""+
		"kind: FilesystemExecutionContractConfig\n"+
		"version: v1\n"+
		"id: filesystem-execution-main\n"+
		"spec:\n"+
		"  filesystem_scope_policy_path: ../policies/filesystem-execution/scope.yaml\n"+
		"  filesystem_mutation_policy_path: ../policies/filesystem-execution/mutation.yaml\n"+
		"  filesystem_io_policy_path: ../policies/filesystem-execution/io.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "shell-tools.yaml"), ""+
		"kind: ShellToolContractConfig\n"+
		"version: v1\n"+
		"id: shell-tools-main\n"+
		"spec:\n"+
		"  shell_catalog_policy_path: ../policies/shell-tools/catalog.yaml\n"+
		"  shell_description_policy_path: ../policies/shell-tools/description.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "shell-execution.yaml"), ""+
		"kind: ShellExecutionContractConfig\n"+
		"version: v1\n"+
		"id: shell-execution-main\n"+
		"spec:\n"+
		"  shell_command_policy_path: ../policies/shell-execution/command.yaml\n"+
		"  shell_approval_policy_path: ../policies/shell-execution/approval.yaml\n"+
		"  shell_runtime_policy_path: ../policies/shell-execution/runtime.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), ""+
		"kind: EndpointPolicyConfig\n"+
		"version: v1\n"+
		"id: endpoint-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static\n"+
		"  params:\n"+
		"    base_url: https://api.z.ai\n"+
		"    path: /api/paas/v4/chat/completions\n"+
		"    method: POST\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "auth.yaml"), ""+
		"kind: AuthPolicyConfig\n"+
		"version: v1\n"+
		"id: auth-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: bearer_token\n"+
		"  params:\n"+
		"    header: Authorization\n"+
		"    prefix: Bearer\n"+
		"    value_env_var: ZAI_API_KEY\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "retry.yaml"), ""+
		"kind: RetryPolicyConfig\n"+
		"version: v1\n"+
		"id: retry-main\n"+
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
		"id: timeout-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: per_request\n"+
		"  params:\n"+
		"    total: 30s\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "memory", "offload.yaml"), ""+
		"kind: OffloadPolicyConfig\n"+
		"version: v1\n"+
		"id: offload-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: old_only\n"+
		"  params:\n"+
		"    max_chars: 1200\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "model.yaml"), ""+
		"kind: ModelPolicyConfig\n"+
		"version: v1\n"+
		"id: model-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_model\n"+
		"  params:\n"+
		"    model: glm-4.6\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "messages.yaml"), ""+
		"kind: MessagePolicyConfig\n"+
		"version: v1\n"+
		"id: messages-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: raw_messages\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "tools.yaml"), ""+
		"kind: ToolPolicyConfig\n"+
		"version: v1\n"+
		"id: tools-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: tools_inline\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "response-format.yaml"), ""+
		"kind: ResponseFormatPolicyConfig\n"+
		"version: v1\n"+
		"id: response-format-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: default\n"+
		"  params:\n"+
		"    type: json_object\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "streaming.yaml"), ""+
		"kind: StreamingPolicyConfig\n"+
		"version: v1\n"+
		"id: streaming-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_stream\n"+
		"  params:\n"+
		"    stream: false\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "sampling.yaml"), ""+
		"kind: SamplingPolicyConfig\n"+
		"version: v1\n"+
		"id: sampling-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_sampling\n"+
		"  params:\n"+
		"    temperature: 0.2\n"+
		"    max_output_tokens: 2048\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "prompt-assets", "inline.yaml"), ""+
		"kind: PromptAssetPolicyConfig\n"+
		"version: v1\n"+
		"id: prompt-assets-inline\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: inline_assets\n"+
		"  params:\n"+
		"    assets:\n"+
		"      - role: system\n"+
		"        content: You are terse.\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "prompt-assembly", "system-prompt.yaml"), ""+
		"kind: SystemPromptPolicyConfig\n"+
		"version: v1\n"+
		"id: system-prompt-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: file_static\n"+
		"  params:\n"+
		"    path: ./prompts/system.md\n"+
		"    role: system\n"+
		"    required: true\n"+
		"    trim_trailing_whitespace: true\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "prompt-assembly", "session-head.yaml"), ""+
		"kind: SessionHeadPolicyConfig\n"+
		"version: v1\n"+
		"id: session-head-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: projection_summary\n"+
		"  params:\n"+
		"    placement: message0\n"+
		"    title: Session head\n"+
		"    max_items: 5\n"+
		"    max_user_chars: 160\n"+
		"    max_assistant_chars: 240\n"+
		"    compact_plan: true\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "tools", "catalog.yaml"), ""+
		"kind: ToolCatalogPolicyConfig\n"+
		"version: v1\n"+
		"id: tool-catalog-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_allowlist\n"+
		"  params:\n"+
		"    tool_ids: []\n"+
		"    allow_empty: true\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "tools", "serialization.yaml"), ""+
		"kind: ToolSerializationPolicyConfig\n"+
		"version: v1\n"+
		"id: tool-serialization-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: openai_function_tools\n"+
		"  params:\n"+
		"    include_descriptions: true\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "tool-execution", "access.yaml"), ""+
		"kind: ToolAccessPolicyConfig\n"+
		"version: v1\n"+
		"id: tool-access-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: deny_all\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "tool-execution", "approval.yaml"), ""+
		"kind: ToolApprovalPolicyConfig\n"+
		"version: v1\n"+
		"id: tool-approval-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: always_allow\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "tool-execution", "sandbox.yaml"), ""+
		"kind: ToolSandboxPolicyConfig\n"+
		"version: v1\n"+
		"id: tool-sandbox-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: default_runtime\n"+
		"  params:\n"+
		"    allow_network: false\n"+
		"    timeout: 30s\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "filesystem-tools", "catalog.yaml"), ""+
		"kind: FilesystemCatalogPolicyConfig\n"+
		"version: v1\n"+
		"id: filesystem-catalog-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_allowlist\n"+
		"  params:\n"+
		"    tool_ids: [fs_list, fs_read_text]\n"+
		"    allow_empty: false\n"+
		"    dedupe: true\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "filesystem-tools", "description.yaml"), ""+
		"kind: FilesystemDescriptionPolicyConfig\n"+
		"version: v1\n"+
		"id: filesystem-description-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_builtin_descriptions\n"+
		"  params:\n"+
		"    include_examples: true\n"+
		"    include_scope_hint: true\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "filesystem-execution", "scope.yaml"), ""+
		"kind: FilesystemScopePolicyConfig\n"+
		"version: v1\n"+
		"id: filesystem-scope-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: workspace_only\n"+
		"  params:\n"+
		"    root_path: .\n"+
		"    read_subpaths: [config, internal]\n"+
		"    write_subpaths: [internal]\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "filesystem-execution", "mutation.yaml"), ""+
		"kind: FilesystemMutationPolicyConfig\n"+
		"version: v1\n"+
		"id: filesystem-mutation-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: allow_writes\n"+
		"  params:\n"+
		"    allow_write: true\n"+
		"    allow_move: true\n"+
		"    allow_mkdir: true\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "filesystem-execution", "io.yaml"), ""+
		"kind: FilesystemIOPolicyConfig\n"+
		"version: v1\n"+
		"id: filesystem-io-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: bounded_text_io\n"+
		"  params:\n"+
		"    max_read_bytes: 4096\n"+
		"    max_write_bytes: 2048\n"+
		"    encoding: utf-8\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "shell-tools", "catalog.yaml"), ""+
		"kind: ShellCatalogPolicyConfig\n"+
		"version: v1\n"+
		"id: shell-catalog-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_allowlist\n"+
		"  params:\n"+
		"    tool_ids: [shell_exec]\n"+
		"    allow_empty: false\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "shell-tools", "description.yaml"), ""+
		"kind: ShellDescriptionPolicyConfig\n"+
		"version: v1\n"+
		"id: shell-description-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_builtin_descriptions\n"+
		"  params:\n"+
		"    include_examples: true\n"+
		"    include_runtime_limits: true\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "shell-execution", "command.yaml"), ""+
		"kind: ShellCommandPolicyConfig\n"+
		"version: v1\n"+
		"id: shell-command-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_allowlist\n"+
		"  params:\n"+
		"    allowed_commands: [go, git]\n"+
		"    allowed_prefixes: [go test, git status]\n"+
		"    deny_patterns: [rm -rf]\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "shell-execution", "approval.yaml"), ""+
		"kind: ShellApprovalPolicyConfig\n"+
		"version: v1\n"+
		"id: shell-approval-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: always_allow\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "shell-execution", "runtime.yaml"), ""+
		"kind: ShellRuntimePolicyConfig\n"+
		"version: v1\n"+
		"id: shell-runtime-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: workspace_write\n"+
		"  params:\n"+
		"    cwd: .\n"+
		"    timeout: 30s\n"+
		"    max_output_bytes: 16384\n"+
		"    allow_network: false\n")

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	contracts, err := runtime.ResolveContracts(cfg)
	if err != nil {
		t.Fatalf("ResolveContracts returned error: %v", err)
	}

	if contracts.ProviderRequest.Transport.ID != "transport-main" {
		t.Fatalf("transport ID = %q, want %q", contracts.ProviderRequest.Transport.ID, "transport-main")
	}
	if contracts.ProviderRequest.Transport.Endpoint.ID != "endpoint-main" {
		t.Fatalf("endpoint ID = %q, want %q", contracts.ProviderRequest.Transport.Endpoint.ID, "endpoint-main")
	}
	if contracts.ProviderRequest.Transport.Auth.Strategy != "bearer_token" {
		t.Fatalf("auth strategy = %q, want %q", contracts.ProviderRequest.Transport.Auth.Strategy, "bearer_token")
	}
	if contracts.ProviderRequest.Transport.Retry.Params.MaxAttempts != 3 {
		t.Fatalf("max attempts = %d, want 3", contracts.ProviderRequest.Transport.Retry.Params.MaxAttempts)
	}
	if contracts.ProviderRequest.Transport.Timeout.Params.Total != "30s" {
		t.Fatalf("timeout total = %q, want %q", contracts.ProviderRequest.Transport.Timeout.Params.Total, "30s")
	}
	if contracts.ProviderRequest.RequestShape.ID != "request-shape-main" {
		t.Fatalf("request-shape ID = %q, want %q", contracts.ProviderRequest.RequestShape.ID, "request-shape-main")
	}
	if contracts.ProviderRequest.RequestShape.Messages.Strategy != "raw_messages" {
		t.Fatalf("message strategy = %q, want %q", contracts.ProviderRequest.RequestShape.Messages.Strategy, "raw_messages")
	}
	if contracts.ProviderRequest.RequestShape.ResponseFormat.Params.Type != "json_object" {
		t.Fatalf("response format type = %q, want %q", contracts.ProviderRequest.RequestShape.ResponseFormat.Params.Type, "json_object")
	}
	if contracts.Memory.ID != "memory-main" {
		t.Fatalf("memory ID = %q, want %q", contracts.Memory.ID, "memory-main")
	}
	if contracts.Memory.Offload.Params.MaxChars != 1200 {
		t.Fatalf("max chars = %d, want 1200", contracts.Memory.Offload.Params.MaxChars)
	}
	if contracts.PromptAssets.ID != "prompt-assets-main" {
		t.Fatalf("prompt-assets ID = %q, want %q", contracts.PromptAssets.ID, "prompt-assets-main")
	}
	if len(contracts.PromptAssets.PromptAsset.Params.Assets) != 1 {
		t.Fatalf("prompt asset count = %d, want 1", len(contracts.PromptAssets.PromptAsset.Params.Assets))
	}
	if contracts.PromptAssembly.ID != "prompt-assembly-main" {
		t.Fatalf("prompt-assembly ID = %q, want %q", contracts.PromptAssembly.ID, "prompt-assembly-main")
	}
	if contracts.PromptAssembly.SystemPrompt.Strategy != "file_static" {
		t.Fatalf("system prompt strategy = %q, want %q", contracts.PromptAssembly.SystemPrompt.Strategy, "file_static")
	}
	if contracts.PromptAssembly.SessionHead.Params.Placement != "message0" {
		t.Fatalf("session head placement = %q, want %q", contracts.PromptAssembly.SessionHead.Params.Placement, "message0")
	}
	if contracts.PromptAssembly.SessionHead.Params.MaxUserChars != 160 {
		t.Fatalf("session head max_user_chars = %d, want 160", contracts.PromptAssembly.SessionHead.Params.MaxUserChars)
	}
	if contracts.PromptAssembly.SessionHead.Params.MaxAssistantChars != 240 {
		t.Fatalf("session head max_assistant_chars = %d, want 240", contracts.PromptAssembly.SessionHead.Params.MaxAssistantChars)
	}
	if !contracts.PromptAssembly.SessionHead.Params.CompactPlan {
		t.Fatal("expected compact_plan to be true")
	}
	if contracts.Tools.ID != "tools-main" {
		t.Fatalf("tools ID = %q, want %q", contracts.Tools.ID, "tools-main")
	}
	if contracts.Tools.Catalog.Strategy != "static_allowlist" {
		t.Fatalf("tool catalog strategy = %q, want %q", contracts.Tools.Catalog.Strategy, "static_allowlist")
	}
	if contracts.ToolExecution.ID != "tool-execution-main" {
		t.Fatalf("tool execution ID = %q, want %q", contracts.ToolExecution.ID, "tool-execution-main")
	}
	if contracts.ToolExecution.Access.Strategy != "deny_all" {
		t.Fatalf("tool access strategy = %q, want %q", contracts.ToolExecution.Access.Strategy, "deny_all")
	}
	if contracts.FilesystemTools.ID != "filesystem-tools-main" {
		t.Fatalf("filesystem tools ID = %q, want %q", contracts.FilesystemTools.ID, "filesystem-tools-main")
	}
	if got, want := len(contracts.FilesystemTools.Catalog.Params.ToolIDs), 2; got != want {
		t.Fatalf("filesystem tool ids len = %d, want %d", got, want)
	}
	if contracts.FilesystemExecution.ID != "filesystem-execution-main" {
		t.Fatalf("filesystem execution ID = %q, want %q", contracts.FilesystemExecution.ID, "filesystem-execution-main")
	}
	if contracts.FilesystemExecution.Scope.Params.RootPath != "." {
		t.Fatalf("filesystem scope root_path = %q, want %q", contracts.FilesystemExecution.Scope.Params.RootPath, ".")
	}
	if contracts.FilesystemExecution.IO.Params.MaxReadBytes != 4096 {
		t.Fatalf("filesystem max_read_bytes = %d, want 4096", contracts.FilesystemExecution.IO.Params.MaxReadBytes)
	}
	if contracts.ShellTools.ID != "shell-tools-main" {
		t.Fatalf("shell tools ID = %q, want %q", contracts.ShellTools.ID, "shell-tools-main")
	}
	if contracts.ShellTools.Description.Params.IncludeRuntimeLimits != true {
		t.Fatalf("shell include_runtime_limits = %v, want true", contracts.ShellTools.Description.Params.IncludeRuntimeLimits)
	}
	if contracts.ShellExecution.ID != "shell-execution-main" {
		t.Fatalf("shell execution ID = %q, want %q", contracts.ShellExecution.ID, "shell-execution-main")
	}
	if contracts.ShellExecution.Command.Params.AllowedCommands[0] != "go" {
		t.Fatalf("shell allowed_commands[0] = %q, want %q", contracts.ShellExecution.Command.Params.AllowedCommands[0], "go")
	}
	if contracts.ShellExecution.Runtime.Params.MaxOutputBytes != 16384 {
		t.Fatalf("shell max_output_bytes = %d, want 16384", contracts.ShellExecution.Runtime.Params.MaxOutputBytes)
	}
}

func TestResolveContractsBuildsDelegationContracts(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    delegation_tools: ./contracts/delegation-tools.yaml\n"+
		"    delegation_execution: ./contracts/delegation-execution.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "delegation-tools.yaml"), ""+
		"kind: DelegationToolContractConfig\n"+
		"version: v1\n"+
		"id: delegation-tools-main\n"+
		"spec:\n"+
		"  delegation_catalog_policy_path: ../policies/delegation/catalog.yaml\n"+
		"  delegation_description_policy_path: ../policies/delegation/description.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "delegation-execution.yaml"), ""+
		"kind: DelegationExecutionContractConfig\n"+
		"version: v1\n"+
		"id: delegation-execution-main\n"+
		"spec:\n"+
		"  delegation_backend_policy_path: ../policies/delegation/backend.yaml\n"+
		"  delegation_result_policy_path: ../policies/delegation/result.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "delegation", "catalog.yaml"), ""+
		"kind: DelegationCatalogPolicyConfig\n"+
		"version: v1\n"+
		"id: delegation-catalog-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_allowlist\n"+
		"  params:\n"+
		"    tool_ids: [delegate_spawn, delegate_wait, delegate_handoff]\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "delegation", "description.yaml"), ""+
		"kind: DelegationDescriptionPolicyConfig\n"+
		"version: v1\n"+
		"id: delegation-description-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_builtin_descriptions\n"+
		"  params:\n"+
		"    include_examples: true\n"+
		"    include_backend_hints: true\n"+
		"    include_lifecycle_notes: true\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "delegation", "backend.yaml"), ""+
		"kind: DelegationBackendPolicyConfig\n"+
		"version: v1\n"+
		"id: delegation-backend-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: backend_allowlist\n"+
		"  params:\n"+
		"    allowed_backends: [local_worker, remote_mesh]\n"+
		"    default_backend: local_worker\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "delegation", "result.yaml"), ""+
		"kind: DelegationResultPolicyConfig\n"+
		"version: v1\n"+
		"id: delegation-result-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: bounded_wait_results\n"+
		"  params:\n"+
		"    include_events: true\n"+
		"    include_artifacts: true\n"+
		"    include_policy_snapshot: true\n"+
		"    default_event_limit: 25\n"+
		"    max_event_limit: 100\n")

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}
	got, err := runtime.ResolveContractsWithRegistry(cfg, policies.NewBuiltInRegistry())
	if err != nil {
		t.Fatalf("ResolveContractsWithRegistry returned error: %v", err)
	}
	if got.DelegationTools.ID != "delegation-tools-main" {
		t.Fatalf("delegation tools contract id = %q", got.DelegationTools.ID)
	}
	if got.DelegationExecution.Backend.Params.DefaultBackend != "local_worker" {
		t.Fatalf("default backend = %q", got.DelegationExecution.Backend.Params.DefaultBackend)
	}
	if len(got.DelegationTools.Catalog.Params.ToolIDs) != 3 {
		t.Fatalf("delegation tool ids = %#v", got.DelegationTools.Catalog.Params.ToolIDs)
	}
}

func TestResolveContractsUsesContractKindsNotContractMapKeys(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    provider_transport_main: ./contracts/transport.yaml\n"+
		"    provider_request_shape_main: ./contracts/request-shape.yaml\n"+
		"    memory_main: ./contracts/memory.yaml\n"+
		"    prompt_assets_main: ./contracts/prompt-assets.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), ""+
		"kind: TransportContractConfig\n"+
		"version: v1\n"+
		"id: transport-main\n"+
		"spec:\n"+
		"  endpoint_policy_path: ../policies/transport/endpoint.yaml\n"+
		"  auth_policy_path: ../policies/transport/auth.yaml\n"+
		"  retry_policy_path: ../policies/transport/retry.yaml\n"+
		"  timeout_policy_path: ../policies/transport/timeout.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "request-shape.yaml"), ""+
		"kind: RequestShapeContractConfig\n"+
		"version: v1\n"+
		"id: request-shape-main\n"+
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
		"id: memory-main\n"+
		"spec:\n"+
		"  offload_policy_path: ../policies/memory/offload.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "prompt-assets.yaml"), ""+
		"kind: PromptAssetsContractConfig\n"+
		"version: v1\n"+
		"id: prompt-assets-main\n"+
		"spec:\n"+
		"  prompt_asset_policy_path: ../policies/prompt-assets/inline.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), ""+
		"kind: EndpointPolicyConfig\n"+
		"version: v1\n"+
		"id: endpoint-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static\n"+
		"  params:\n"+
		"    base_url: https://api.z.ai\n"+
		"    path: /api/paas/v4/chat/completions\n"+
		"    method: POST\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "auth.yaml"), ""+
		"kind: AuthPolicyConfig\n"+
		"version: v1\n"+
		"id: auth-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: bearer_token\n"+
		"  params:\n"+
		"    header: Authorization\n"+
		"    prefix: Bearer\n"+
		"    value_env_var: ZAI_API_KEY\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "retry.yaml"), ""+
		"kind: RetryPolicyConfig\n"+
		"version: v1\n"+
		"id: retry-main\n"+
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
		"id: timeout-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: per_request\n"+
		"  params:\n"+
		"    total: 30s\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "memory", "offload.yaml"), ""+
		"kind: OffloadPolicyConfig\n"+
		"version: v1\n"+
		"id: offload-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: old_only\n"+
		"  params:\n"+
		"    max_chars: 1200\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "model.yaml"), ""+
		"kind: ModelPolicyConfig\n"+
		"version: v1\n"+
		"id: model-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_model\n"+
		"  params:\n"+
		"    model: glm-4.6\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "messages.yaml"), ""+
		"kind: MessagePolicyConfig\n"+
		"version: v1\n"+
		"id: messages-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: raw_messages\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "tools.yaml"), ""+
		"kind: ToolPolicyConfig\n"+
		"version: v1\n"+
		"id: tools-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: tools_inline\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "response-format.yaml"), ""+
		"kind: ResponseFormatPolicyConfig\n"+
		"version: v1\n"+
		"id: response-format-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: default\n"+
		"  params:\n"+
		"    type: json_object\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "streaming.yaml"), ""+
		"kind: StreamingPolicyConfig\n"+
		"version: v1\n"+
		"id: streaming-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_stream\n"+
		"  params:\n"+
		"    stream: false\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "request-shape", "sampling.yaml"), ""+
		"kind: SamplingPolicyConfig\n"+
		"version: v1\n"+
		"id: sampling-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: static_sampling\n"+
		"  params:\n"+
		"    temperature: 0.2\n"+
		"    max_output_tokens: 2048\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "prompt-assets", "inline.yaml"), ""+
		"kind: PromptAssetPolicyConfig\n"+
		"version: v1\n"+
		"id: prompt-assets-inline\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: inline_assets\n"+
		"  params:\n"+
		"    assets:\n"+
		"      - role: system\n"+
		"        content: You are terse.\n")

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	got, err := runtime.ResolveContracts(cfg)
	if err != nil {
		t.Fatalf("ResolveContracts returned error: %v", err)
	}

	if got.ProviderRequest.Transport.ID != "transport-main" {
		t.Fatalf("transport ID = %q, want %q", got.ProviderRequest.Transport.ID, "transport-main")
	}
	if got.ProviderRequest.RequestShape.ID != "request-shape-main" {
		t.Fatalf("request-shape ID = %q, want %q", got.ProviderRequest.RequestShape.ID, "request-shape-main")
	}
	if got.Memory.ID != "memory-main" {
		t.Fatalf("memory ID = %q, want %q", got.Memory.ID, "memory-main")
	}
	if got.PromptAssets.ID != "prompt-assets-main" {
		t.Fatalf("prompt-assets ID = %q, want %q", got.PromptAssets.ID, "prompt-assets-main")
	}
}

func TestResolveContractsRejectsUnsupportedPolicyStrategy(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), ""+
		"kind: TransportContractConfig\n"+
		"version: v1\n"+
		"id: transport-main\n"+
		"spec:\n"+
		"  endpoint_policy_path: ../policies/transport/endpoint.yaml\n"+
		"  auth_policy_path: ../policies/transport/auth.yaml\n"+
		"  retry_policy_path: ../policies/transport/retry.yaml\n"+
		"  timeout_policy_path: ../policies/transport/timeout.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), ""+
		"kind: EndpointPolicyConfig\n"+
		"version: v1\n"+
		"id: endpoint-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: invalid_endpoint\n"+
		"  params:\n"+
		"    base_url: https://api.z.ai\n"+
		"    path: /api/paas/v4/chat/completions\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "auth.yaml"), ""+
		"kind: AuthPolicyConfig\n"+
		"version: v1\n"+
		"id: auth-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: bearer_token\n"+
		"  params:\n"+
		"    value_env_var: ZAI_API_KEY\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "retry.yaml"), ""+
		"kind: RetryPolicyConfig\n"+
		"version: v1\n"+
		"id: retry-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: none\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "timeout.yaml"), ""+
		"kind: TimeoutPolicyConfig\n"+
		"version: v1\n"+
		"id: timeout-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: per_request\n"+
		"  params:\n"+
		"    total: 30s\n")

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	_, err = runtime.ResolveContracts(cfg)
	if err == nil {
		t.Fatal("ResolveContracts error = nil, want invalid strategy error")
	}
	if got := err.Error(); got == "" || !containsAll(got, "EndpointPolicyConfig", "invalid_endpoint") {
		t.Fatalf("ResolveContracts error = %q, want policy kind and invalid strategy", got)
	}
}

func TestResolveContractsResolvesArtifactOffloadParams(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    memory: ./contracts/memory.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "contracts", "memory.yaml"), ""+
		"kind: MemoryContractConfig\n"+
		"version: v1\n"+
		"id: memory-main\n"+
		"spec:\n"+
		"  offload_policy_path: ../policies/memory/offload.yaml\n")
	mustWriteFile(t, filepath.Join(dir, "policies", "memory", "offload.yaml"), ""+
		"kind: OffloadPolicyConfig\n"+
		"version: v1\n"+
		"id: offload-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: artifact_store\n"+
		"  params:\n"+
		"    max_chars: 512\n"+
		"    preview_chars: 80\n"+
		"    storage_path: ../../var/artifacts\n"+
		"    expose_retrieval_tools: true\n"+
		"    search_limit: 4\n")

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}
	got, err := runtime.ResolveContracts(cfg)
	if err != nil {
		t.Fatalf("ResolveContracts returned error: %v", err)
	}
	if got.Memory.Offload.Strategy != "artifact_store" {
		t.Fatalf("offload strategy = %q, want artifact_store", got.Memory.Offload.Strategy)
	}
	if got.Memory.Offload.Params.StoragePath != filepath.Join(dir, "var", "artifacts") {
		t.Fatalf("storage path = %q, want %q", got.Memory.Offload.Params.StoragePath, filepath.Join(dir, "var", "artifacts"))
	}
	if !got.Memory.Offload.Params.ExposeRetrievalTools {
		t.Fatal("expose retrieval tools = false, want true")
	}
	if got.Memory.Offload.Params.SearchLimit != 4 {
		t.Fatalf("search limit = %d, want 4", got.Memory.Offload.Params.SearchLimit)
	}
}

func TestResolveContractsWithRegistryAllowsExtendedStrategies(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-test\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    transport: ./contracts/transport.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "transport.yaml"), ""+
		"kind: TransportContractConfig\n"+
		"version: v1\n"+
		"id: transport-main\n"+
		"spec:\n"+
		"  endpoint_policy_path: ../policies/transport/endpoint.yaml\n"+
		"  auth_policy_path: ../policies/transport/auth.yaml\n"+
		"  retry_policy_path: ../policies/transport/retry.yaml\n"+
		"  timeout_policy_path: ../policies/transport/timeout.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "endpoint.yaml"), ""+
		"kind: EndpointPolicyConfig\n"+
		"version: v1\n"+
		"id: endpoint-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: env_resolved\n"+
		"  params:\n"+
		"    base_url: https://api.z.ai\n"+
		"    path: /api/paas/v4/chat/completions\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "auth.yaml"), ""+
		"kind: AuthPolicyConfig\n"+
		"version: v1\n"+
		"id: auth-main\n"+
		"spec:\n"+
		"  enabled: false\n"+
		"  strategy: none\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "retry.yaml"), ""+
		"kind: RetryPolicyConfig\n"+
		"version: v1\n"+
		"id: retry-main\n"+
		"spec:\n"+
		"  enabled: false\n"+
		"  strategy: none\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "transport", "timeout.yaml"), ""+
		"kind: TimeoutPolicyConfig\n"+
		"version: v1\n"+
		"id: timeout-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: per_request\n"+
		"  params:\n"+
		"    total: 30s\n")

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	registry := policies.NewBuiltInRegistry()
	endpointType, err := registry.Type("EndpointPolicyConfig")
	if err != nil {
		t.Fatalf("Type returned error: %v", err)
	}
	endpointType.Strategy["env_resolved"] = struct{}{}
	registry.Register(endpointType)

	resolved, err := runtime.ResolveContractsWithRegistry(cfg, registry)
	if err != nil {
		t.Fatalf("ResolveContractsWithRegistry returned error: %v", err)
	}
	if resolved.ProviderRequest.Transport.Endpoint.Strategy != "env_resolved" {
		t.Fatalf("endpoint strategy = %q, want %q", resolved.ProviderRequest.Transport.Endpoint.Strategy, "env_resolved")
	}
}

func TestResolveContractsBuildsChatContractWithParams(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-chat\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    chat: ./contracts/chat.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "chat.yaml"), ""+
		"kind: ChatContractConfig\n"+
		"version: v1\n"+
		"id: chat-main\n"+
		"spec:\n"+
		"  input_policy_path: ../policies/chat/input.yaml\n"+
		"  submit_policy_path: ../policies/chat/submit.yaml\n"+
		"  output_policy_path: ../policies/chat/output.yaml\n"+
		"  status_policy_path: ../policies/chat/status.yaml\n"+
		"  command_policy_path: ../policies/chat/command.yaml\n"+
		"  resume_policy_path: ../policies/chat/resume.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "input.yaml"), ""+
		"kind: ChatInputPolicyConfig\n"+
		"version: v1\n"+
		"id: chat-input\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: multiline_buffer\n"+
		"  params:\n"+
		"    primary_prompt: \"> \"\n"+
		"    continuation_prompt: \". \"\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "submit.yaml"), ""+
		"kind: ChatSubmitPolicyConfig\n"+
		"version: v1\n"+
		"id: chat-submit\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: double_enter\n"+
		"  params:\n"+
		"    empty_line_threshold: 1\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "output.yaml"), ""+
		"kind: ChatOutputPolicyConfig\n"+
		"version: v1\n"+
		"id: chat-output\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: streaming_text\n"+
		"  params:\n"+
		"    show_final_newline: true\n"+
		"    render_markdown: true\n"+
		"    markdown_style: dark\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "status.yaml"), ""+
		"kind: ChatStatusPolicyConfig\n"+
		"version: v1\n"+
		"id: chat-status\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: inline_terminal\n"+
		"  params:\n"+
		"    show_header: true\n"+
		"    show_usage: true\n"+
		"    show_tool_calls: true\n"+
		"    show_tool_results: true\n"+
		"    show_plan_after_plan_tools: true\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "command.yaml"), ""+
		"kind: ChatCommandPolicyConfig\n"+
		"version: v1\n"+
		"id: chat-command\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: slash_commands\n"+
		"  params:\n"+
		"    exit_command: /exit\n"+
		"    help_command: /help\n"+
		"    session_command: /session\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "chat", "resume.yaml"), ""+
		"kind: ChatResumePolicyConfig\n"+
		"version: v1\n"+
		"id: chat-resume\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: explicit_resume_only\n"+
		"  params:\n"+
		"    require_explicit_id: true\n")

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	got, err := runtime.ResolveContracts(cfg)
	if err != nil {
		t.Fatalf("ResolveContracts returned error: %v", err)
	}

	if got.Chat.ID != "chat-main" {
		t.Fatalf("chat ID = %q, want chat-main", got.Chat.ID)
	}
	if got.Chat.Input.Params.PrimaryPrompt != "> " || got.Chat.Input.Params.ContinuationPrompt != ". " {
		t.Fatalf("chat input params = %#v", got.Chat.Input.Params)
	}
	if got.Chat.Submit.Params.EmptyLineThreshold != 1 {
		t.Fatalf("empty line threshold = %d, want 1", got.Chat.Submit.Params.EmptyLineThreshold)
	}
	if !got.Chat.Status.Params.ShowHeader || !got.Chat.Status.Params.ShowUsage || !got.Chat.Status.Params.ShowToolCalls || !got.Chat.Status.Params.ShowToolResults || !got.Chat.Status.Params.ShowPlanAfterPlanTools {
		t.Fatalf("chat status params = %#v", got.Chat.Status.Params)
	}
	if got.Chat.Command.Params.ExitCommand != "/exit" || got.Chat.Command.Params.HelpCommand != "/help" || got.Chat.Command.Params.SessionCommand != "/session" {
		t.Fatalf("chat command params = %#v", got.Chat.Command.Params)
	}
	if !got.Chat.Resume.Params.RequireExplicitID {
		t.Fatalf("chat resume params = %#v", got.Chat.Resume.Params)
	}
}

func TestResolveContractsBuildsProviderTraceContract(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	mustWriteFile(t, filepath.Join(dir, "agent.yaml"), ""+
		"kind: AgentConfig\n"+
		"version: v1\n"+
		"id: agent-trace\n"+
		"spec:\n"+
		"  contracts:\n"+
		"    provider_trace: ./contracts/provider-trace.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "contracts", "provider-trace.yaml"), ""+
		"kind: ProviderTraceContractConfig\n"+
		"version: v1\n"+
		"id: provider-trace-main\n"+
		"spec:\n"+
		"  provider_trace_policy_path: ../policies/provider-trace/request.yaml\n")

	mustWriteFile(t, filepath.Join(dir, "policies", "provider-trace", "request.yaml"), ""+
		"kind: ProviderTracePolicyConfig\n"+
		"version: v1\n"+
		"id: provider-trace-request-main\n"+
		"spec:\n"+
		"  enabled: true\n"+
		"  strategy: inline_request\n"+
		"  params:\n"+
		"    include_raw_body: true\n"+
		"    include_decoded_payload: true\n")

	cfg, err := config.LoadRoot(filepath.Join(dir, "agent.yaml"))
	if err != nil {
		t.Fatalf("LoadRoot returned error: %v", err)
	}

	got, err := runtime.ResolveContracts(cfg)
	if err != nil {
		t.Fatalf("ResolveContracts returned error: %v", err)
	}

	if got.ProviderTrace.ID != "provider-trace-main" {
		t.Fatalf("provider trace ID = %q, want provider-trace-main", got.ProviderTrace.ID)
	}
	if got.ProviderTrace.Request.Strategy != "inline_request" {
		t.Fatalf("provider trace strategy = %q, want inline_request", got.ProviderTrace.Request.Strategy)
	}
	if !got.ProviderTrace.Request.Params.IncludeRawBody || !got.ProviderTrace.Request.Params.IncludeDecodedPayload {
		t.Fatalf("provider trace params = %#v", got.ProviderTrace.Request.Params)
	}
}

func containsAll(s string, want ...string) bool {
	for _, fragment := range want {
		if !strings.Contains(s, fragment) {
			return false
		}
	}
	return true
}
