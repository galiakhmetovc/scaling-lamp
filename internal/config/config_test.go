package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestLoadReadsZAISettingsFromEnv(t *testing.T) {
	t.Setenv("TEAMD_ZAI_BASE_URL", "https://api.z.ai/api/coding/paas/v4")
	t.Setenv("TEAMD_ZAI_MODEL", "glm-5-turbo")
	t.Setenv("TEAMD_ZAI_API_KEY", "secret")

	cfg := Load()

	if cfg.ZAIBaseURL != "https://api.z.ai/api/coding/paas/v4" {
		t.Fatalf("unexpected base url: %q", cfg.ZAIBaseURL)
	}
	if cfg.ZAIModel != "glm-5-turbo" {
		t.Fatalf("unexpected model: %q", cfg.ZAIModel)
	}
	if cfg.ProviderAPIKey != "secret" {
		t.Fatalf("unexpected api key: %q", cfg.ProviderAPIKey)
	}
}

func TestLoadReadsTelegramSettingsFromEnv(t *testing.T) {
	t.Setenv("TEAMD_TELEGRAM_TOKEN", "tg-secret")
	t.Setenv("TEAMD_TELEGRAM_BASE_URL", "https://api.telegram.org")
	t.Setenv("TEAMD_WORKSPACE_ROOT", "/tmp/teamd-workspace")
	t.Setenv("TEAMD_RUNTIME_STORE_PATH", "/tmp/teamd-runtime.db")
	t.Setenv("TEAMD_ZAI_REASONING_MODE", "enabled")
	t.Setenv("TEAMD_ZAI_CLEAR_THINKING", "true")
	t.Setenv("TEAMD_ZAI_TEMPERATURE", "0.7")
	t.Setenv("TEAMD_ZAI_TOP_P", "0.8")
	t.Setenv("TEAMD_ZAI_MAX_TOKENS", "2048")
	t.Setenv("TEAMD_PROVIDER_ROUND_TIMEOUT", "45s")
	t.Setenv("TEAMD_POSTGRES_DSN", "postgres://postgres:postgres@localhost:5432/postgres?sslmode=disable")
	t.Setenv("TEAMD_OLLAMA_BASE_URL", "http://127.0.0.1:11434")
	t.Setenv("TEAMD_MEMORY_EMBEDDINGS_ENABLED", "true")
	t.Setenv("TEAMD_MEMORY_EMBED_MODEL", "nomic-embed-text:latest")
	t.Setenv("TEAMD_MEMORY_EMBED_DIMS", "768")
	t.Setenv("TEAMD_MEMORY_POLICY_PROFILE", "standard")
	t.Setenv("TEAMD_MEMORY_PROMOTE_CHECKPOINT", "true")
	t.Setenv("TEAMD_MEMORY_PROMOTE_CONTINUITY", "false")
	t.Setenv("TEAMD_MEMORY_RECALL_KINDS", "continuity,checkpoint")
	t.Setenv("TEAMD_MEMORY_MAX_BODY_CHARS", "900")
	t.Setenv("TEAMD_MEMORY_MAX_RESOLVED_FACTS", "5")
	t.Setenv("TEAMD_APPROVAL_REQUIRED_TOOLS", "shell.exec,filesystem.write_file")

	cfg := Load()

	if cfg.TelegramToken != "tg-secret" {
		t.Fatalf("unexpected telegram token: %q", cfg.TelegramToken)
	}
	if cfg.TelegramBaseURL != "https://api.telegram.org" {
		t.Fatalf("unexpected telegram base url: %q", cfg.TelegramBaseURL)
	}
	if cfg.WorkspaceRoot != "/tmp/teamd-workspace" {
		t.Fatalf("unexpected workspace root: %q", cfg.WorkspaceRoot)
	}
	if cfg.RuntimeStorePath != "/tmp/teamd-runtime.db" {
		t.Fatalf("unexpected runtime store path: %q", cfg.RuntimeStorePath)
	}
	if cfg.PostgresDSN != "postgres://postgres:postgres@localhost:5432/postgres?sslmode=disable" {
		t.Fatalf("unexpected postgres dsn: %q", cfg.PostgresDSN)
	}
	if cfg.ZAIReasoningMode != "enabled" {
		t.Fatalf("unexpected reasoning mode: %q", cfg.ZAIReasoningMode)
	}
	if !cfg.ZAIClearThinking {
		t.Fatal("expected clear thinking enabled")
	}
	if cfg.ZAITemperature == nil || *cfg.ZAITemperature != 0.7 {
		t.Fatalf("unexpected temperature: %#v", cfg.ZAITemperature)
	}
	if cfg.ZAITopP == nil || *cfg.ZAITopP != 0.8 {
		t.Fatalf("unexpected top_p: %#v", cfg.ZAITopP)
	}
	if cfg.ZAIMaxTokens == nil || *cfg.ZAIMaxTokens != 2048 {
		t.Fatalf("unexpected max_tokens: %#v", cfg.ZAIMaxTokens)
	}
	if cfg.ProviderRoundTimeout.String() != "45s" {
		t.Fatalf("unexpected provider round timeout: %s", cfg.ProviderRoundTimeout)
	}
	if cfg.OllamaBaseURL != "http://127.0.0.1:11434" {
		t.Fatalf("unexpected ollama base url: %q", cfg.OllamaBaseURL)
	}
	if !cfg.MemoryEmbeddingsEnabled {
		t.Fatal("expected memory embeddings enabled")
	}
	if cfg.MemoryEmbeddingModel != "nomic-embed-text:latest" {
		t.Fatalf("unexpected memory embed model: %q", cfg.MemoryEmbeddingModel)
	}
	if cfg.MemoryEmbeddingDims != 768 {
		t.Fatalf("unexpected memory embed dims: %d", cfg.MemoryEmbeddingDims)
	}
	if cfg.MemoryPolicyProfile != "standard" {
		t.Fatalf("unexpected memory policy profile: %q", cfg.MemoryPolicyProfile)
	}
	if !cfg.MemoryPromoteCheckpoint {
		t.Fatal("expected checkpoint promotion enabled")
	}
	if cfg.MemoryPromoteContinuity {
		t.Fatal("expected continuity promotion disabled")
	}
	if len(cfg.MemoryRecallKinds) != 2 || cfg.MemoryRecallKinds[0] != "continuity" || cfg.MemoryRecallKinds[1] != "checkpoint" {
		t.Fatalf("unexpected recall kinds: %#v", cfg.MemoryRecallKinds)
	}
	if cfg.MemoryMaxBodyChars != 900 {
		t.Fatalf("unexpected memory max body chars: %d", cfg.MemoryMaxBodyChars)
	}
	if cfg.MemoryMaxResolvedFacts != 5 {
		t.Fatalf("unexpected memory max resolved facts: %d", cfg.MemoryMaxResolvedFacts)
	}
	if len(cfg.ApprovalRequiredTools) != 2 || cfg.ApprovalRequiredTools[0] != "shell.exec" {
		t.Fatalf("unexpected approval required tools: %#v", cfg.ApprovalRequiredTools)
	}
}

func TestLoadReadsContextBudgetSettingsFromEnv(t *testing.T) {
	t.Setenv("TEAMD_CONTEXT_WINDOW_TOKENS", "32000")
	t.Setenv("TEAMD_PROMPT_BUDGET_TOKENS", "24000")
	t.Setenv("TEAMD_COMPACTION_TRIGGER_TOKENS", "16000")
	t.Setenv("TEAMD_MAX_TOOL_CONTEXT_CHARS", "2048")
	t.Setenv("TEAMD_LLM_COMPACTION_ENABLED", "true")
	t.Setenv("TEAMD_LLM_COMPACTION_TIMEOUT", "12s")

	cfg := Load()

	if cfg.ContextWindowTokens != 32000 {
		t.Fatalf("unexpected context window tokens: %d", cfg.ContextWindowTokens)
	}
	if cfg.PromptBudgetTokens != 24000 {
		t.Fatalf("unexpected prompt budget tokens: %d", cfg.PromptBudgetTokens)
	}
	if cfg.CompactionTriggerTokens != 16000 {
		t.Fatalf("unexpected compaction trigger tokens: %d", cfg.CompactionTriggerTokens)
	}
	if cfg.MaxToolContextChars != 2048 {
		t.Fatalf("unexpected max tool context chars: %d", cfg.MaxToolContextChars)
	}
	if !cfg.LLMCompactionEnabled {
		t.Fatal("expected llm compaction enabled")
	}
	if cfg.LLMCompactionTimeout.String() != "12s" {
		t.Fatalf("unexpected llm compaction timeout: %s", cfg.LLMCompactionTimeout)
	}
}

func TestLoadAllowsDisablingApprovalTools(t *testing.T) {
	t.Setenv("TEAMD_APPROVAL_REQUIRED_TOOLS", "none")

	cfg := Load()

	if cfg.ApprovalRequiredTools == nil {
		t.Fatal("expected explicit empty approval tool list, got nil")
	}
	if len(cfg.ApprovalRequiredTools) != 0 {
		t.Fatalf("expected approvals disabled, got %#v", cfg.ApprovalRequiredTools)
	}
}

func TestLoadReadsAPIOperatorAuthTokenFromEnv(t *testing.T) {
	t.Setenv("TEAMD_API_AUTH_TOKEN", "operator-secret")

	cfg := Load()

	if cfg.APIAuthToken != "operator-secret" {
		t.Fatalf("unexpected api auth token: %q", cfg.APIAuthToken)
	}
}

func TestConfigLoadsMeshSettings(t *testing.T) {
	t.Setenv("TEAMD_AGENT_ID", "agent-a")
	t.Setenv("TEAMD_MESH_ENABLED", "true")
	t.Setenv("TEAMD_MESH_LISTEN_ADDR", "127.0.0.1:18081")
	t.Setenv("TEAMD_MESH_REGISTRY_DSN", "postgres://postgres:postgres@localhost:5432/postgres?sslmode=disable")
	t.Setenv("TEAMD_MESH_COLD_START_FANOUT", "2")
	t.Setenv("TEAMD_MESH_EXPLORATION_RATE", "0.15")
	t.Setenv("TEAMD_MESH_PEER_TIMEOUT", "45s")
	t.Setenv("TEAMD_MESH_HEARTBEAT_INTERVAL", "30s")
	t.Setenv("TEAMD_MESH_STALE_THRESHOLD", "2m")

	cfg := Load()

	if cfg.AgentID != "agent-a" {
		t.Fatalf("unexpected agent id: %q", cfg.AgentID)
	}
	if !cfg.MeshEnabled {
		t.Fatal("expected mesh enabled")
	}
	if cfg.MeshListenAddr != "127.0.0.1:18081" {
		t.Fatalf("unexpected mesh listen addr: %q", cfg.MeshListenAddr)
	}
	if cfg.MeshRegistryDSN != "postgres://postgres:postgres@localhost:5432/postgres?sslmode=disable" {
		t.Fatalf("unexpected mesh registry dsn: %q", cfg.MeshRegistryDSN)
	}
	if cfg.MeshColdStartFanout != 2 {
		t.Fatalf("unexpected cold start fanout: %d", cfg.MeshColdStartFanout)
	}
	if cfg.MeshExplorationRate != 0.15 {
		t.Fatalf("unexpected exploration rate: %v", cfg.MeshExplorationRate)
	}
	if cfg.MeshPeerTimeout.String() != "45s" {
		t.Fatalf("unexpected peer timeout: %s", cfg.MeshPeerTimeout)
	}
	if cfg.MeshHeartbeatInterval.String() != "30s" {
		t.Fatalf("unexpected heartbeat interval: %s", cfg.MeshHeartbeatInterval)
	}
	if cfg.MeshStaleThreshold.String() != "2m0s" {
		t.Fatalf("unexpected stale threshold: %s", cfg.MeshStaleThreshold)
	}
}

func TestConfigMeshDisabledByDefault(t *testing.T) {
	cfg := Load()

	if cfg.MeshEnabled {
		t.Fatal("expected mesh disabled by default")
	}
}

func TestConfigLoadsMeshModelOverrides(t *testing.T) {
	t.Setenv("TEAMD_ZAI_MODEL", "glm-4.5")
	t.Setenv("TEAMD_MESH_CLASSIFIER_MODEL", "glm-4.5-air")
	t.Setenv("TEAMD_MESH_PROPOSAL_MODEL", "glm-4.6")
	t.Setenv("TEAMD_MESH_EXECUTION_MODEL", "glm-5")

	cfg := Load()

	if cfg.ZAIModel != "glm-4.5" {
		t.Fatalf("unexpected base zai model: %q", cfg.ZAIModel)
	}
	if cfg.MeshClassifierModel != "glm-4.5-air" {
		t.Fatalf("unexpected classifier model: %q", cfg.MeshClassifierModel)
	}
	if cfg.MeshProposalModel != "glm-4.6" {
		t.Fatalf("unexpected proposal model: %q", cfg.MeshProposalModel)
	}
	if cfg.MeshExecutionModel != "glm-5" {
		t.Fatalf("unexpected execution model: %q", cfg.MeshExecutionModel)
	}
}

func TestConfigLoadsLLMTraceSettings(t *testing.T) {
	t.Setenv("TEAMD_LLM_TRACE_ENABLED", "true")
	t.Setenv("TEAMD_LLM_TRACE_DIR", "/tmp/teamd-traces")

	cfg := Load()

	if !cfg.LLMTraceEnabled {
		t.Fatal("expected llm trace enabled")
	}
	if cfg.LLMTraceDir != "/tmp/teamd-traces" {
		t.Fatalf("unexpected trace dir: %q", cfg.LLMTraceDir)
	}
}

func TestLoadDotEnvLoadsMissingProcessEnvOnly(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, ".env")
	if err := os.WriteFile(path, []byte("TEAMD_ZAI_MODEL=glm-4.5\nTEAMD_LLM_TRACE_ENABLED=true\n"), 0o644); err != nil {
		t.Fatalf("write .env: %v", err)
	}

	t.Setenv("TEAMD_ZAI_MODEL", "glm-5-turbo")
	if err := LoadDotEnv(path); err != nil {
		t.Fatalf("load dotenv: %v", err)
	}
	if got := os.Getenv("TEAMD_ZAI_MODEL"); got != "glm-5-turbo" {
		t.Fatalf("expected existing env to win, got %q", got)
	}
	if got := os.Getenv("TEAMD_LLM_TRACE_ENABLED"); got != "true" {
		t.Fatalf("expected trace env loaded, got %q", got)
	}
}
