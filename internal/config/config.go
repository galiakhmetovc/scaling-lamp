package config

import (
	"bufio"
	"os"
	"strconv"
	"strings"
	"time"
)

type Config struct {
	PostgresDSN             string
	WorkspaceRoot           string
	ArtifactRoot            string
	RuntimeStorePath        string
	MCPFSRoot               string
	OllamaBaseURL           string
	MemoryEmbeddingsEnabled bool
	MemoryEmbeddingModel    string
	MemoryEmbeddingDims     int
	MemoryPolicyProfile     string
	MemoryPromoteCheckpoint bool
	MemoryPromoteContinuity bool
	MemoryRecallKinds       []string
	MemoryMaxBodyChars      int
	MemoryMaxResolvedFacts  int
	ApprovalRequiredTools   []string
	APIEnabled              bool
	APIListenAddr           string
	APIBaseURL              string
	APIAuthToken            string
	ProviderAPIKey          string
	ZAIBaseURL              string
	ZAIModel                string
	ZAIReasoningMode        string
	ZAIClearThinking        bool
	ZAITemperature          *float64
	ZAITopP                 *float64
	ZAIMaxTokens            *int
	ProviderRoundTimeout    time.Duration
	LLMCompactionEnabled    bool
	LLMCompactionTimeout    time.Duration
	TelegramToken           string
	TelegramBaseURL         string
	ContextWindowTokens     int
	PromptBudgetTokens      int
	CompactionTriggerTokens int
	MaxToolContextChars     int
	AgentID                 string
	MeshEnabled             bool
	MeshListenAddr          string
	MeshRegistryDSN         string
	MeshColdStartFanout     int
	MeshExplorationRate     float64
	MeshPeerTimeout         time.Duration
	MeshHeartbeatInterval   time.Duration
	MeshStaleThreshold      time.Duration
	MeshClassifierModel     string
	MeshProposalModel       string
	MeshExecutionModel      string
	LLMTraceEnabled         bool
	LLMTraceDir             string
}

func Load() Config {
	cfg := TestConfig()

	if v := os.Getenv("TEAMD_POSTGRES_DSN"); v != "" {
		cfg.PostgresDSN = v
	}
	if v := os.Getenv("TEAMD_WORKSPACE_ROOT"); v != "" {
		cfg.WorkspaceRoot = v
	}
	if v := os.Getenv("TEAMD_ARTIFACT_ROOT"); v != "" {
		cfg.ArtifactRoot = v
	}
	if v := os.Getenv("TEAMD_RUNTIME_STORE_PATH"); v != "" {
		cfg.RuntimeStorePath = v
	}
	if v := os.Getenv("TEAMD_MCP_FS_ROOT"); v != "" {
		cfg.MCPFSRoot = v
	}
	if v := os.Getenv("TEAMD_OLLAMA_BASE_URL"); v != "" {
		cfg.OllamaBaseURL = v
	}
	if v := os.Getenv("TEAMD_MEMORY_EMBEDDINGS_ENABLED"); v != "" {
		cfg.MemoryEmbeddingsEnabled = v == "1" || v == "true" || v == "TRUE" || v == "yes" || v == "on"
	}
	if v := os.Getenv("TEAMD_MEMORY_EMBED_MODEL"); v != "" {
		cfg.MemoryEmbeddingModel = v
	}
	if v := os.Getenv("TEAMD_MEMORY_EMBED_DIMS"); v != "" {
		cfg.MemoryEmbeddingDims = atoiOrDefault(v, cfg.MemoryEmbeddingDims)
	}
	if v := os.Getenv("TEAMD_MEMORY_POLICY_PROFILE"); v != "" {
		cfg.MemoryPolicyProfile = v
	}
	if v := os.Getenv("TEAMD_MEMORY_PROMOTE_CHECKPOINT"); v != "" {
		cfg.MemoryPromoteCheckpoint = v == "1" || v == "true" || v == "TRUE" || v == "yes" || v == "on"
	}
	if v := os.Getenv("TEAMD_MEMORY_PROMOTE_CONTINUITY"); v != "" {
		cfg.MemoryPromoteContinuity = v == "1" || v == "true" || v == "TRUE" || v == "yes" || v == "on"
	}
	if v := os.Getenv("TEAMD_MEMORY_RECALL_KINDS"); v != "" {
		cfg.MemoryRecallKinds = splitCSV(v)
	}
	if v := os.Getenv("TEAMD_MEMORY_MAX_BODY_CHARS"); v != "" {
		cfg.MemoryMaxBodyChars = atoiOrDefault(v, cfg.MemoryMaxBodyChars)
	}
	if v := os.Getenv("TEAMD_MEMORY_MAX_RESOLVED_FACTS"); v != "" {
		cfg.MemoryMaxResolvedFacts = atoiOrDefault(v, cfg.MemoryMaxResolvedFacts)
	}
	if v := os.Getenv("TEAMD_APPROVAL_REQUIRED_TOOLS"); v != "" {
		if strings.EqualFold(strings.TrimSpace(v), "none") {
			cfg.ApprovalRequiredTools = []string{}
		} else {
			cfg.ApprovalRequiredTools = splitCSV(v)
		}
	}
	if v := os.Getenv("TEAMD_API_ENABLED"); v != "" {
		cfg.APIEnabled = v == "1" || v == "true" || v == "TRUE" || v == "yes" || v == "on"
	}
	if v := os.Getenv("TEAMD_API_LISTEN_ADDR"); v != "" {
		cfg.APIListenAddr = v
	}
	if v := os.Getenv("TEAMD_API_BASE_URL"); v != "" {
		cfg.APIBaseURL = v
	}
	if v := os.Getenv("TEAMD_API_AUTH_TOKEN"); v != "" {
		cfg.APIAuthToken = v
	}
	if v := os.Getenv("TEAMD_ZAI_API_KEY"); v != "" {
		cfg.ProviderAPIKey = v
	}
	if v := os.Getenv("TEAMD_ZAI_BASE_URL"); v != "" {
		cfg.ZAIBaseURL = v
	}
	if v := os.Getenv("TEAMD_ZAI_MODEL"); v != "" {
		cfg.ZAIModel = v
	}
	if v := os.Getenv("TEAMD_ZAI_REASONING_MODE"); v != "" {
		cfg.ZAIReasoningMode = v
	}
	if v := os.Getenv("TEAMD_ZAI_CLEAR_THINKING"); v != "" {
		cfg.ZAIClearThinking = v == "1" || v == "true" || v == "TRUE" || v == "yes" || v == "on"
	}
	if v := os.Getenv("TEAMD_ZAI_TEMPERATURE"); v != "" {
		if parsed, err := strconv.ParseFloat(v, 64); err == nil {
			cfg.ZAITemperature = &parsed
		}
	}
	if v := os.Getenv("TEAMD_ZAI_TOP_P"); v != "" {
		if parsed, err := strconv.ParseFloat(v, 64); err == nil {
			cfg.ZAITopP = &parsed
		}
	}
	if v := os.Getenv("TEAMD_ZAI_MAX_TOKENS"); v != "" {
		if parsed, err := strconv.Atoi(v); err == nil {
			cfg.ZAIMaxTokens = &parsed
		}
	}
	if v := os.Getenv("TEAMD_PROVIDER_ROUND_TIMEOUT"); v != "" {
		cfg.ProviderRoundTimeout = durationOrDefault(v, cfg.ProviderRoundTimeout)
	}
	if v := os.Getenv("TEAMD_LLM_COMPACTION_ENABLED"); v != "" {
		cfg.LLMCompactionEnabled = v == "1" || v == "true" || v == "TRUE" || v == "yes" || v == "on"
	}
	if v := os.Getenv("TEAMD_LLM_COMPACTION_TIMEOUT"); v != "" {
		cfg.LLMCompactionTimeout = durationOrDefault(v, cfg.LLMCompactionTimeout)
	}
	if v := os.Getenv("TEAMD_TELEGRAM_TOKEN"); v != "" {
		cfg.TelegramToken = v
	}
	if v := os.Getenv("TEAMD_TELEGRAM_BASE_URL"); v != "" {
		cfg.TelegramBaseURL = v
	}
	if v := os.Getenv("TEAMD_CONTEXT_WINDOW_TOKENS"); v != "" {
		cfg.ContextWindowTokens = atoiOrDefault(v, cfg.ContextWindowTokens)
	}
	if v := os.Getenv("TEAMD_PROMPT_BUDGET_TOKENS"); v != "" {
		cfg.PromptBudgetTokens = atoiOrDefault(v, cfg.PromptBudgetTokens)
	}
	if v := os.Getenv("TEAMD_COMPACTION_TRIGGER_TOKENS"); v != "" {
		cfg.CompactionTriggerTokens = atoiOrDefault(v, cfg.CompactionTriggerTokens)
	}
	if v := os.Getenv("TEAMD_MAX_TOOL_CONTEXT_CHARS"); v != "" {
		cfg.MaxToolContextChars = atoiOrDefault(v, cfg.MaxToolContextChars)
	}
	if v := os.Getenv("TEAMD_AGENT_ID"); v != "" {
		cfg.AgentID = v
	}
	if v := os.Getenv("TEAMD_MESH_ENABLED"); v != "" {
		cfg.MeshEnabled = v == "1" || v == "true" || v == "TRUE" || v == "yes" || v == "on"
	}
	if v := os.Getenv("TEAMD_MESH_LISTEN_ADDR"); v != "" {
		cfg.MeshListenAddr = v
	}
	if v := os.Getenv("TEAMD_MESH_REGISTRY_DSN"); v != "" {
		cfg.MeshRegistryDSN = v
	}
	if v := os.Getenv("TEAMD_MESH_COLD_START_FANOUT"); v != "" {
		cfg.MeshColdStartFanout = atoiOrDefault(v, cfg.MeshColdStartFanout)
	}
	if v := os.Getenv("TEAMD_MESH_EXPLORATION_RATE"); v != "" {
		cfg.MeshExplorationRate = atofOrDefault(v, cfg.MeshExplorationRate)
	}
	if v := os.Getenv("TEAMD_MESH_PEER_TIMEOUT"); v != "" {
		cfg.MeshPeerTimeout = durationOrDefault(v, cfg.MeshPeerTimeout)
	}
	if v := os.Getenv("TEAMD_MESH_HEARTBEAT_INTERVAL"); v != "" {
		cfg.MeshHeartbeatInterval = durationOrDefault(v, cfg.MeshHeartbeatInterval)
	}
	if v := os.Getenv("TEAMD_MESH_STALE_THRESHOLD"); v != "" {
		cfg.MeshStaleThreshold = durationOrDefault(v, cfg.MeshStaleThreshold)
	}
	if v := os.Getenv("TEAMD_MESH_CLASSIFIER_MODEL"); v != "" {
		cfg.MeshClassifierModel = v
	}
	if v := os.Getenv("TEAMD_MESH_PROPOSAL_MODEL"); v != "" {
		cfg.MeshProposalModel = v
	}
	if v := os.Getenv("TEAMD_MESH_EXECUTION_MODEL"); v != "" {
		cfg.MeshExecutionModel = v
	}
	if v := os.Getenv("TEAMD_LLM_TRACE_ENABLED"); v != "" {
		cfg.LLMTraceEnabled = v == "1" || v == "true" || v == "TRUE" || v == "yes" || v == "on"
	}
	if v := os.Getenv("TEAMD_LLM_TRACE_DIR"); v != "" {
		cfg.LLMTraceDir = v
	}

	return cfg
}

func TestConfig() Config {
	return Config{
		PostgresDSN:             "postgres://teamd:teamd@localhost:5432/teamd_test?sslmode=disable",
		WorkspaceRoot:           ".",
		ArtifactRoot:            "var/artifacts",
		RuntimeStorePath:        "var/runtime.db",
		MCPFSRoot:               "/",
		OllamaBaseURL:           "http://127.0.0.1:11434",
		MemoryEmbeddingModel:    "nomic-embed-text:latest",
		MemoryEmbeddingDims:     768,
		MemoryPolicyProfile:     "conservative",
		MemoryPromoteCheckpoint: false,
		MemoryPromoteContinuity: true,
		MemoryRecallKinds:       []string{"continuity"},
		MemoryMaxBodyChars:      600,
		MemoryMaxResolvedFacts:  3,
		ApprovalRequiredTools:   []string{"shell.exec", "filesystem.write_file"},
		APIEnabled:              true,
		APIListenAddr:           "127.0.0.1:18081",
		APIBaseURL:              "http://127.0.0.1:18081",
		ProviderAPIKey:          "test-api-key",
		ZAIBaseURL:              "https://api.z.ai/api/coding/paas/v4",
		ZAIModel:                "glm-5-turbo",
		ZAIReasoningMode:        "enabled",
		ProviderRoundTimeout:    90 * time.Second,
		LLMCompactionTimeout:    5 * time.Minute,
		TelegramBaseURL:         "https://api.telegram.org",
		ContextWindowTokens:     200000,
		PromptBudgetTokens:      150000,
		CompactionTriggerTokens: 120000,
		MaxToolContextChars:     4096,
		MeshColdStartFanout:     2,
		MeshExplorationRate:     0.1,
		MeshPeerTimeout:         60 * time.Second,
		MeshHeartbeatInterval:   30 * time.Second,
		MeshStaleThreshold:      2 * time.Minute,
		MeshClassifierModel:     "glm-5-turbo",
		MeshProposalModel:       "glm-5-turbo",
		MeshExecutionModel:      "glm-5-turbo",
		LLMTraceDir:             "var/llm-traces",
	}
}

func LoadDotEnv(path string) error {
	file, err := os.Open(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	defer file.Close()

	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		if strings.HasPrefix(line, "export ") {
			line = strings.TrimSpace(strings.TrimPrefix(line, "export "))
		}
		key, value, ok := strings.Cut(line, "=")
		if !ok {
			continue
		}
		key = strings.TrimSpace(key)
		value = strings.TrimSpace(value)
		if key == "" {
			continue
		}
		value = strings.Trim(value, `"'`)
		if _, exists := os.LookupEnv(key); !exists {
			_ = os.Setenv(key, value)
		}
	}
	return scanner.Err()
}

func atoiOrDefault(raw string, fallback int) int {
	v, err := strconv.Atoi(raw)
	if err != nil {
		return fallback
	}
	return v
}

func atofOrDefault(raw string, fallback float64) float64 {
	v, err := strconv.ParseFloat(raw, 64)
	if err != nil {
		return fallback
	}
	return v
}

func durationOrDefault(raw string, fallback time.Duration) time.Duration {
	v, err := time.ParseDuration(raw)
	if err != nil {
		return fallback
	}
	return v
}

func splitCSV(raw string) []string {
	parts := strings.Split(raw, ",")
	out := make([]string, 0, len(parts))
	for _, part := range parts {
		part = strings.TrimSpace(part)
		if part == "" {
			continue
		}
		out = append(out, part)
	}
	return out
}
