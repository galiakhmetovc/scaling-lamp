package contracts

type ResolvedContracts struct {
	ProviderRequest ProviderRequestContract
	Memory          MemoryContract
	PromptAssets    PromptAssetsContract
	Chat            ChatContract
}

type ProviderRequestContract struct {
	Transport    TransportContract
	RequestShape RequestShapeContract
}

type TransportContract struct {
	ID       string
	Endpoint EndpointPolicy
	Auth     AuthPolicy
	Retry    RetryPolicy
	Timeout  TimeoutPolicy
}

type EndpointPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   EndpointParams
}

type EndpointParams struct {
	BaseURL      string            `yaml:"base_url"`
	Path         string            `yaml:"path"`
	Method       string            `yaml:"method"`
	ExtraHeaders map[string]string `yaml:"extra_headers"`
}

type AuthPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   AuthParams
}

type AuthParams struct {
	Header      string `yaml:"header"`
	Prefix      string `yaml:"prefix"`
	ValueEnvVar string `yaml:"value_env_var"`
}

type RetryPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   RetryParams
}

type RetryParams struct {
	MaxAttempts        int      `yaml:"max_attempts"`
	BaseDelay          string   `yaml:"base_delay"`
	MaxDelay           string   `yaml:"max_delay"`
	RetryOnStatuses    []int    `yaml:"retry_on_statuses"`
	RetryOnErrors      []string `yaml:"retry_on_errors"`
	EarlyFailureWindow string   `yaml:"early_failure_window"`
}

type TimeoutPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   TimeoutParams
}

type TimeoutParams struct {
	Total           string `yaml:"total"`
	Connect         string `yaml:"connect"`
	Idle            string `yaml:"idle"`
	OperationBudget string `yaml:"operation_budget"`
	AttemptTimeout  string `yaml:"attempt_timeout"`
}

type RequestShapeContract struct {
	ID             string
	Model          ModelPolicy
	Messages       MessagePolicy
	Tools          ToolPolicy
	ResponseFormat ResponseFormatPolicy
	Streaming      StreamingPolicy
	Sampling       SamplingPolicy
}

type PromptAssetsContract struct {
	ID          string
	PromptAsset PromptAssetPolicy
}

type ChatContract struct {
	ID      string
	Input   ChatInputPolicy
	Submit  ChatSubmitPolicy
	Output  ChatOutputPolicy
	Status  ChatStatusPolicy
	Command ChatCommandPolicy
	Resume  ChatResumePolicy
}

type ChatInputPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ChatInputParams
}

type ChatSubmitPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ChatSubmitParams
}

type ChatOutputPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ChatOutputParams
}

type ChatStatusPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ChatStatusParams
}

type ChatCommandPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ChatCommandParams
}

type ChatResumePolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ChatResumeParams
}

type ChatInputParams struct {
	PrimaryPrompt      string `yaml:"primary_prompt"`
	ContinuationPrompt string `yaml:"continuation_prompt"`
}

type ChatSubmitParams struct {
	EmptyLineThreshold int `yaml:"empty_line_threshold"`
}

type ChatOutputParams struct {
	ShowFinalNewline bool `yaml:"show_final_newline"`
}

type ChatStatusParams struct {
	ShowHeader bool `yaml:"show_header"`
	ShowUsage  bool `yaml:"show_usage"`
}

type ChatCommandParams struct {
	ExitCommand    string `yaml:"exit_command"`
	HelpCommand    string `yaml:"help_command"`
	SessionCommand string `yaml:"session_command"`
}

type ChatResumeParams struct {
	RequireExplicitID bool `yaml:"require_explicit_id"`
}

type PromptAssetPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   PromptAssetParams
}

type PromptAssetParams struct {
	Assets []PromptAsset `yaml:"assets"`
}

type PromptAsset struct {
	ID      string `yaml:"id" json:"id"`
	Role    string `yaml:"role" json:"role"`
	Content string `yaml:"content" json:"content"`
	Placement string `yaml:"placement" json:"placement"`
}

type ModelPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ModelParams
}

type ModelParams struct {
	Model string `yaml:"model"`
}

type MessagePolicy struct {
	ID       string
	Enabled  bool
	Strategy string
}

type ToolPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
}

type ResponseFormatPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   ResponseFormatParams
}

type ResponseFormatParams struct {
	Type string `yaml:"type"`
}

type StreamingPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   StreamingParams
}

type StreamingParams struct {
	Stream bool `yaml:"stream"`
}

type SamplingPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   SamplingParams
}

type SamplingParams struct {
	Temperature     *float64 `yaml:"temperature"`
	TopP            *float64 `yaml:"top_p"`
	MaxOutputTokens *int     `yaml:"max_output_tokens"`
}

type Message struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

type MemoryContract struct {
	ID      string
	Offload OffloadPolicy
}

type OffloadPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   OffloadParams
}

type OffloadParams struct {
	MaxChars int `yaml:"max_chars"`
}
