package contracts

type ResolvedContracts struct {
	ProviderRequest ProviderRequestContract
	Memory          MemoryContract
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
	MaxAttempts     int      `yaml:"max_attempts"`
	BaseDelay       string   `yaml:"base_delay"`
	MaxDelay        string   `yaml:"max_delay"`
	RetryOnStatuses []int    `yaml:"retry_on_statuses"`
	RetryOnErrors   []string `yaml:"retry_on_errors"`
}

type TimeoutPolicy struct {
	ID       string
	Enabled  bool
	Strategy string
	Params   TimeoutParams
}

type TimeoutParams struct {
	Total   string `yaml:"total"`
	Connect string `yaml:"connect"`
	Idle    string `yaml:"idle"`
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
